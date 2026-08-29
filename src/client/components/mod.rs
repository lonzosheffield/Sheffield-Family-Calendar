pub mod calendar;
pub mod dashboard;
// T1.3: the kiosk join QR (fast_qr SVG). Compiled for both targets - the
// server renders it during SSR, the browser re-renders it on hydration.
pub mod qr;
pub mod routine;
pub mod screensaver;
pub mod whiteboard;
