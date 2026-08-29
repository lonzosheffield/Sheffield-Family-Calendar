use dioxus::prelude::*;

use crate::client::realtime::{now_millis, use_realtime, StrokeBatcher};
use crate::shared::types::{ClientMessage, Stroke, StrokePoint, StrokeSegment, DEFAULT_BOARD_ID};

pub const CANVAS_ID: &str = "sheffield-whiteboard";

const PALETTE: [(&str, &str); 5] = [
    ("Navy", "#2672B3"),
    ("Sky", "#8BB5DA"),
    ("Coral", "#E86A58"),
    ("Sun", "#F4D03F"),
    ("Ink", "#1F2933"),
];

#[component]
pub fn Whiteboard() -> Element {
    let (mut bus, sender) = use_realtime();
    let mut color = use_signal(|| PALETTE[0].1.to_string());
    let mut width = use_signal(|| 4.0_f64);
    let mut batcher = use_signal(StrokeBatcher::new);

    // Keep the backing store in sync with the device pixel ratio so strokes
    // stay crisp on the Fire tablet and on retina phones.
    use_effect(canvas::sync_device_pixel_ratio);

    // Protocol v2 (T1.2): drain every stroke that arrived since the last
    // render — the single-slot v1 signal silently dropped the rest (R-22a).
    // T2.3 owns the persistence/repaint half of this.
    use_effect(move || {
        for stroke in bus.drain_inbound_strokes() {
            canvas::draw_stroke(&stroke);
        }
    });

    // A `Snapshot` replaces the whole canvas: clear, then replay in seq order.
    use_effect(move || {
        if let Some(strokes) = (bus.snapshot)() {
            canvas::clear();
            for stroke in &strokes {
                canvas::draw_stroke(stroke);
            }
        }
    });

    use_effect(move || {
        let _version = (bus.clear_version)();
        canvas::clear();
    });

    // Protocol v2: `pointermove` paints locally and appends to the open
    // stroke; the batcher emits at most one `Draw` per 34 ms frame (R-06).
    let flush = move |stroke: Option<Stroke>| {
        if let Some(stroke) = stroke {
            sender.send(ClientMessage::Draw {
                board_id: DEFAULT_BOARD_ID,
                stroke,
            });
        }
    };

    rsx! {
        div { class: "flex h-full flex-col gap-3",
            div { class: "flex flex-wrap items-center gap-3",
                for (name, hex) in PALETTE {
                    button {
                        key: "{hex}",
                        class: if color() == hex {
                            "h-9 w-9 rounded-full ring-4 ring-sheffield-dark"
                        } else {
                            "h-9 w-9 rounded-full ring-1 ring-slate-200"
                        },
                        style: "background-color: {hex}",
                        aria_label: "{name} pen",
                        onclick: move |_| color.set(hex.to_string()),
                    }
                }
                input {
                    class: "w-32",
                    r#type: "range",
                    min: "1",
                    max: "24",
                    step: "1",
                    value: "{width}",
                    aria_label: "Stroke width",
                    oninput: move |event| {
                        if let Ok(value) = event.value().parse::<f64>() {
                            width.set(value);
                        }
                    },
                }
                button {
                    class: "ml-auto rounded-xl bg-sheffield-accent px-4 py-2 font-bold text-white",
                    onclick: move |_| {
                        canvas::clear();
                        sender.send(ClientMessage::ClearBoard { board_id: DEFAULT_BOARD_ID });
                    },
                    "Clear Canvas"
                }
            }

            canvas {
                id: CANVAS_ID,
                class: "h-full min-h-[16rem] w-full flex-1 touch-none rounded-2xl bg-white ring-1 ring-slate-200",
                onpointerdown: move |event| {
                    let Some(point) = normalize(&event) else { return };
                    batcher.write().begin(color(), width(), point);
                },
                onpointermove: move |event| {
                    if !batcher.peek().is_open() {
                        return;
                    }
                    let Some(to) = normalize(&event) else { return };
                    let (accepted, previous) = {
                        let mut open = batcher.write();
                        let previous = open.last_point();
                        (open.push(to), previous)
                    };
                    if accepted {
                        if let Some(from) = previous {
                            canvas::draw_segment(&StrokeSegment {
                                from,
                                to,
                                color: color(),
                                width: width(),
                            });
                        }
                    }
                    let due = batcher.write().flush_if_due(now_millis());
                    flush(due);
                },
                onpointerup: move |_| {
                    let final_stroke = batcher.write().end();
                    flush(final_stroke);
                },
                onpointerleave: move |_| {
                    let final_stroke = batcher.write().end();
                    flush(final_stroke);
                },
            }
        }
    }
}

/// Convert a pointer position into resolution independent 0..1 coordinates.
fn normalize(event: &Event<PointerData>) -> Option<StrokePoint> {
    let (css_width, css_height) = canvas::css_size()?;
    if css_width <= 0.0 || css_height <= 0.0 {
        return None;
    }
    let point = event.data().element_coordinates();
    Some(StrokePoint {
        x: (point.x / css_width).clamp(0.0, 1.0),
        y: (point.y / css_height).clamp(0.0, 1.0),
    })
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
mod canvas {
    use super::CANVAS_ID;
    use crate::shared::types::{Stroke, StrokeSegment};
    use wasm_bindgen::JsCast;

    fn element() -> Option<web_sys::HtmlCanvasElement> {
        web_sys::window()?
            .document()?
            .get_element_by_id(CANVAS_ID)?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()
    }

    fn context() -> Option<web_sys::CanvasRenderingContext2d> {
        element()?
            .get_context("2d")
            .ok()??
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .ok()
    }

    pub fn css_size() -> Option<(f64, f64)> {
        let rect = element()?.get_bounding_client_rect();
        Some((rect.width(), rect.height()))
    }

    /// Resize the backing store to `css size * devicePixelRatio` and scale the
    /// context so drawing coordinates stay in CSS pixels.
    pub fn sync_device_pixel_ratio() {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(canvas) = element() else { return };
        let Some(context) = context() else { return };
        let (width, height) = match css_size() {
            Some(size) => size,
            None => return,
        };
        let ratio = window.device_pixel_ratio().max(1.0);
        canvas.set_width((width * ratio) as u32);
        canvas.set_height((height * ratio) as u32);
        let _ = context.reset_transform();
        let _ = context.scale(ratio, ratio);
        context.set_line_cap("round");
        context.set_line_join("round");
    }

    pub fn draw_segment(segment: &StrokeSegment) {
        let Some(context) = context() else { return };
        let Some((width, height)) = css_size() else {
            return;
        };
        context.set_stroke_style_str(&segment.color);
        context.set_line_width(segment.width);
        context.begin_path();
        context.move_to(segment.from.x * width, segment.from.y * height);
        context.line_to(segment.to.x * width, segment.to.y * height);
        context.stroke();
    }

    /// Paint one batched stroke (protocol v2: one message, many points).
    pub fn draw_stroke(stroke: &Stroke) {
        let Some(context) = context() else { return };
        let Some((width, height)) = css_size() else {
            return;
        };
        let mut points = stroke.points.iter();
        let Some(first) = points.next() else { return };
        context.set_stroke_style_str(&stroke.color);
        context.set_line_width(stroke.width);
        context.begin_path();
        context.move_to(first.x * width, first.y * height);
        for point in points {
            context.line_to(point.x * width, point.y * height);
        }
        context.stroke();
    }

    pub fn clear() {
        let Some(context) = context() else { return };
        let Some((width, height)) = css_size() else {
            return;
        };
        context.clear_rect(0.0, 0.0, width, height);
    }
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
mod canvas {
    use crate::shared::types::{Stroke, StrokeSegment};

    pub fn css_size() -> Option<(f64, f64)> {
        None
    }
    pub fn sync_device_pixel_ratio() {}
    pub fn draw_segment(_segment: &StrokeSegment) {}
    pub fn draw_stroke(_stroke: &Stroke) {}
    pub fn clear() {}
}
