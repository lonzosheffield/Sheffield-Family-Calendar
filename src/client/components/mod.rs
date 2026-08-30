pub mod calendar;
pub mod dashboard;
// T1.3: the kiosk join QR (fast_qr SVG). Compiled for both targets - the
// server renders it during SSR, the browser re-renders it on hydration.
pub mod qr;
pub mod routine;
pub mod screensaver;
// T2.1: the Fire TV kiosk — the 10-foot, D-pad-only surface (PLAN v2 D8).
pub mod tv;
pub mod whiteboard;
