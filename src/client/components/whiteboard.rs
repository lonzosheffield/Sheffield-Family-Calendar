use dioxus::prelude::*;

use crate::client::realtime::{now_millis, use_realtime, StrokeBatcher};
use crate::server::api::undo_last_stroke;
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

    // Everything currently painted, in the order it was applied — the local
    // record `install_resize_observer`'s callback repaints from (R-22b). A
    // fresh `Snapshot` replaces it wholesale; every other source (a locally
    // drawn flush, a drained inbound `Draw`) appends to it; a clear empties
    // it.
    let mut stroke_log = use_signal(Vec::<Stroke>::new);

    // R-22b: a `ResizeObserver` on the canvas re-syncs the device pixel ratio
    // and repaints from `stroke_log` on *every* size change — not just once
    // at mount, the way a plain `use_effect(sync_device_pixel_ratio)` would.
    // Resizing a `<canvas>`'s backing store clears its bitmap, so without
    // this, maximising or restoring the whiteboard panel silently wipes it.
    use_effect(move || {
        canvas::install_resize_observer(move || {
            canvas::clear();
            for stroke in stroke_log() {
                canvas::draw_stroke(&stroke);
            }
        });
    });

    // Protocol v2 (T1.2): drain every stroke that arrived since the last
    // render — the single-slot v1 signal silently dropped the rest (R-22a).
    // T2.3 owns the persistence/repaint half of this: every drained stroke is
    // also appended to `stroke_log` so a later resize can repaint it.
    use_effect(move || {
        for stroke in bus.drain_inbound_strokes() {
            canvas::draw_stroke(&stroke);
            stroke_log.write().push(stroke);
        }
    });

    // A `Snapshot` replaces the whole canvas — and `stroke_log` — in one go.
    // This is also how the server tells everyone the board changed after an
    // undo (`api::whiteboard::undo_last_stroke` republishes one): there is no
    // bespoke "undo" wire message, `Snapshot` already says everything needed.
    use_effect(move || {
        if let Some(strokes) = (bus.snapshot)() {
            canvas::clear();
            for stroke in &strokes {
                canvas::draw_stroke(stroke);
            }
            stroke_log.set(strokes);
        }
    });

    use_effect(move || {
        let _version = (bus.clear_version)();
        canvas::clear();
        stroke_log.write().clear();
    });

    // Protocol v2: `pointermove` paints locally and appends to the open
    // stroke; the batcher emits at most one `Draw` per 34 ms frame (R-06).
    // Each flush is also this client's own record of what it drew, so it is
    // recorded in `stroke_log` immediately rather than waiting for the
    // server's echo (which the client discards anyway — W2).
    let mut flush = move |stroke: Option<Stroke>| {
        if let Some(stroke) = stroke {
            stroke_log.write().push(stroke.clone());
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
                    class: "ml-auto rounded-xl bg-white px-4 py-2 font-bold text-sheffield-dark ring-1 ring-slate-200 disabled:opacity-50",
                    disabled: bus.client_id.read().is_none(),
                    onclick: move |_| async move {
                        // Undo-own-last-stroke (R-22 / PURPLE T2.3c): a plain
                        // `#[server]` fn, not a WS message — see
                        // `api::whiteboard::undo_last_stroke`'s own doc
                        // comment. On an actual removal the server republishes
                        // a `Snapshot`, which the effect above already
                        // repaints from; there is nothing else to do here.
                        let Some(client_id) = (bus.client_id)() else { return };
                        let _ = undo_last_stroke(client_id.as_str().to_string()).await;
                    },
                    "Undo"
                }
                button {
                    class: "rounded-xl bg-sheffield-accent px-4 py-2 font-bold text-white",
                    onclick: move |_| {
                        canvas::clear();
                        stroke_log.write().clear();
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
    use std::cell::RefCell;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    // Kept alive for the life of the process: a `ResizeObserver` (and the
    // `Closure` backing its callback) that nothing on the Rust side still
    // references is eligible for the JS garbage collector, which would
    // silently stop firing repaints (R-22b). There is exactly one whiteboard
    // canvas (`CANVAS_ID` is a constant), so one slot is enough; installing
    // again (e.g. the component remounts) disconnects the previous observer
    // first so it never double-fires.
    type ResizeObserverHandle = (web_sys::ResizeObserver, Closure<dyn FnMut()>);

    thread_local! {
        static RESIZE_OBSERVER: RefCell<Option<ResizeObserverHandle>> =
            const { RefCell::new(None) };
    }

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

    /// Install (or reinstall) a `ResizeObserver` on the canvas that re-syncs
    /// the device pixel ratio and then calls `on_resize` — R-22b.
    ///
    /// `ResizeObserver` invokes its callback once immediately after
    /// `observe()` starts, so this also performs the initial DPR sync a plain
    /// `use_effect(sync_device_pixel_ratio)` used to do at mount; no separate
    /// call is needed.
    ///
    /// The callback is declared `FnMut()` — zero JS arguments — rather than
    /// the browser's actual `(entries, observer)` signature: wasm-bindgen's
    /// generated trampoline only reads what the Rust closure's arity
    /// declares, and needing the `ResizeObserverEntry` payload here (a stream
    /// of new content-box sizes we would only use to re-derive the same CSS
    /// size `css_size()` already reads straight from the DOM) would add
    /// `js_sys` and the `ResizeObserverEntry` web-sys feature for no benefit.
    pub fn install_resize_observer(mut on_resize: impl FnMut() + 'static) {
        let Some(canvas) = element() else { return };
        let closure = Closure::<dyn FnMut()>::new(move || {
            sync_device_pixel_ratio();
            on_resize();
        });
        let Ok(observer) = web_sys::ResizeObserver::new(closure.as_ref().unchecked_ref()) else {
            return;
        };
        observer.observe(&canvas);
        RESIZE_OBSERVER.with(|cell| {
            if let Some((old, _)) = cell.borrow_mut().take() {
                old.disconnect();
            }
            *cell.borrow_mut() = Some((observer, closure));
        });
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
    pub fn install_resize_observer(mut on_resize: impl FnMut() + 'static) {
        // No DOM on the server/non-wasm target; mirror the web
        // implementation's call order anyway so this stub can't silently
        // drift from it, and so neither fn is flagged as dead code.
        sync_device_pixel_ratio();
        on_resize();
    }
    pub fn draw_segment(_segment: &StrokeSegment) {}
    pub fn draw_stroke(_stroke: &Stroke) {}
    pub fn clear() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::realtime::RealtimeBus;
    use crate::shared::types::{ClientId, ServerMessage};
    use std::cell::Cell;
    use std::rc::Rc;

    /// PURPLE §P3 T2.3(d): "A unit test feeds 50 `Draw` messages into the
    /// client queue between two render ticks and asserts all 50 are drawn."
    ///
    /// `RealtimeBus::inbound_strokes` is a `Signal`, which needs a live
    /// Dioxus scope to construct (`Signal::new` panics outside one) — so this
    /// runs inside a real, minimal `VirtualDom` rather than a bare `#[test]`.
    /// The component body applies 50 `Draw`s (standing in for 50 messages
    /// arriving between renders — nothing here calls `drain_inbound_strokes`
    /// in between, exactly the R-22a gap the v1 single-slot signal used to
    /// lose) and drains once, all synchronously inside `rebuild_in_place`.
    #[derive(Clone)]
    struct HarnessProps {
        drained_count: Rc<Cell<usize>>,
    }

    fn harness(props: HarnessProps) -> Element {
        let mut bus = RealtimeBus {
            client_id: Signal::new(None),
            today: Signal::new(None),
            routine_version: Signal::new(0),
            routine_updated_for: Signal::new(None),
            tasks_version: Signal::new(0),
            tasks_updated_for: Signal::new(None),
            profiles_version: Signal::new(0),
            calendar_version: Signal::new(0),
            inbound_strokes: Signal::new(Vec::new()),
            snapshot: Signal::new(None),
            clear_version: Signal::new(0),
            resync_version: Signal::new(0),
            requested_view: Signal::new(None),
            requested_profile: Signal::new(None),
            connected: Signal::new(false),
            stale: Signal::new(false),
        };

        let other = ClientId("someone-else".into());
        for i in 0..50u64 {
            bus.apply(ServerMessage::Draw {
                board_id: DEFAULT_BOARD_ID,
                seq: i as i64 + 1,
                origin: other.clone(),
                stroke: Stroke {
                    points: vec![StrokePoint { x: 0.1, y: 0.2 }],
                    color: "#000000".into(),
                    width: 2.0,
                },
            });
        }

        props.drained_count.set(bus.drain_inbound_strokes().len());

        rsx! { div {} }
    }

    #[test]
    fn fifty_queued_draws_between_two_render_ticks_are_all_drained() {
        let drained_count = Rc::new(Cell::new(0));
        let mut dom = dioxus::dioxus_core::VirtualDom::new_with_props(
            harness,
            HarnessProps {
                drained_count: Rc::clone(&drained_count),
            },
        );
        dom.rebuild_in_place();

        assert_eq!(
            drained_count.get(),
            50,
            "every Draw applied between two drains must survive to be painted (R-22a)"
        );
    }

    /// PURPLE §P3 T2.3(e): "A unit test resizes the canvas model and asserts
    /// a repaint-from-log is issued." The non-web `canvas` stub mirrors the
    /// real `ResizeObserver`-backed one's call order (`sync_device_pixel_ratio`
    /// then the caller's repaint) without a DOM, and — like a real
    /// `ResizeObserver` — invokes the callback once immediately on install,
    /// standing in for "the canvas was just resized" (R-22b).
    #[test]
    fn resize_triggers_a_repaint_from_the_stroke_log() {
        let repainted = Rc::new(Cell::new(false));
        let flag = Rc::clone(&repainted);
        canvas::install_resize_observer(move || flag.set(true));

        assert!(
            repainted.get(),
            "a resize must trigger a repaint-from-log callback, not silently leave the canvas blank"
        );
    }
}
