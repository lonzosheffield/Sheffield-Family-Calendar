//! The join QR the TV shows so a parent's phone can reach the PWA
//! (PLAN v2 D3′ / D8, task T1.3).
//!
//! The payload is the **raw-IP HTTPS phone URL**, deliberately not the
//! `.local` name: Fire OS 7/8 does not resolve `.local` (RR-7), and the
//! phone must land on the HTTPS origin or it gets no service worker, no
//! install prompt and no camera capture (G7).
//!
//! Rendered as an SVG by `fast_qr` (PURPLE_TEAM.md §P5.4 picks it over
//! `qrcode`, which is two years stale). SVG rather than a raster because
//! the kiosk is a 50" screen at 10 feet: the overlay has to stay crisp at
//! whatever size the TV layout gives it, and an SVG string costs nothing to
//! re-render when the IP changes.

use dioxus::prelude::*;

/// Port the phone PWA is served on (PLAN v2 D3′; `FAMILY_HUB_TLS_ADDR`
/// overrides the bind address, and [`phone_join_url`] takes the effective
/// port as an argument rather than assuming this one).
pub const DEFAULT_PHONE_PORT: u16 = 8443;

/// The URL the QR encodes: the phone PWA on the HTTPS origin.
///
/// One function, used by the component, by the server's startup log and by
/// the acceptance test, so there is exactly one definition of what a phone
/// is asked to open.
pub fn phone_join_url(host: &str, port: u16) -> String {
    format!("https://{host}:{port}/m")
}

/// Encode `payload` as a QR code and return a standalone SVG document.
///
/// `size_px` is stamped as `width`/`height` on the root element (`fast_qr`
/// emits only a `viewBox`), which is what lets the same string be dropped
/// into the kiosk overlay at one size and rasterised at another.
pub fn qr_svg(payload: &str, size_px: u32) -> Result<String, String> {
    use fast_qr::convert::svg::SvgBuilder;
    use fast_qr::QRBuilder;

    let code = QRBuilder::new(payload)
        .build()
        .map_err(|err| format!("could not encode {payload:?} as a QR code: {err:?}"))?;

    // `SvgBuilder`'s default 4-module margin is the quiet zone the QR spec
    // requires; narrowing it is what makes codes undecodable against a
    // busy background, so it is left alone.
    let svg = SvgBuilder::default().to_str(&code);

    // `fast_qr` opens with `<svg viewBox="...">`; give it intrinsic
    // dimensions so browsers and rasterisers agree on how big it is.
    let opening = "<svg ";
    let rendered = match svg.strip_prefix(opening) {
        Some(rest) => format!(r#"<svg width="{size_px}" height="{size_px}" {rest}"#),
        None => svg,
    };
    Ok(rendered)
}

/// The kiosk's join-QR block: the code itself plus the URL in plain text,
/// because a parent standing at the TV may well just type it.
///
/// `url` is passed in rather than derived here so the component stays a
/// pure function of its props — the server knows the reachable IP, the
/// client does not.
#[component]
pub fn JoinQr(url: String, size_px: u32) -> Element {
    match qr_svg(&url, size_px) {
        Ok(svg) => rsx! {
            div { class: "flex flex-col items-center gap-4",
                div {
                    class: "bg-white p-4 rounded-2xl",
                    dangerous_inner_html: "{svg}",
                }
                p { class: "text-2xl font-semibold tracking-wide", "{url}" }
            }
        },
        // A QR that will not encode must not blank the kiosk panel; the
        // typed URL is the fallback and is the more useful half anyway.
        Err(err) => rsx! {
            div { class: "flex flex-col items-center gap-4",
                p { class: "text-2xl font-semibold tracking-wide", "{url}" }
                p { class: "text-lg opacity-70", "QR unavailable: {err}" }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_join_url_is_the_https_phone_origin() {
        assert_eq!(
            phone_join_url("10.0.0.42", DEFAULT_PHONE_PORT),
            "https://10.0.0.42:8443/m"
        );
    }

    #[test]
    fn the_rendered_svg_carries_explicit_dimensions() {
        let svg = qr_svg("https://10.0.0.42:8443/m", 512).expect("encodes");
        assert!(svg.starts_with(r#"<svg width="512" height="512" viewBox="#));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn encoding_is_deterministic() {
        // The overlay re-renders on every kiosk repaint; a QR that changed
        // shape each time would be a flickering mess on a 50" screen.
        let url = phone_join_url("10.0.0.42", DEFAULT_PHONE_PORT);
        assert_eq!(
            qr_svg(&url, 320).expect("encodes"),
            qr_svg(&url, 320).expect("encodes")
        );
    }
}
