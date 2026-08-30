pub mod calendar;
// T2.2: the phone PWA — manifest, service worker, offline queue, bottom tabs.
pub mod mobile;
// T3.4: the Sheffield palette as a contract (WCAG maths, token allowlist,
// ink/ground pair table). Whole-hub: both surfaces read it. Moved up from
// tv/ by Boss at the wave 3 close per T3.4's HANDOFF request 1.
pub mod palette;
// T1.3: the kiosk join QR (fast_qr SVG). Compiled for both targets - the
// server renders it during SSR, the browser re-renders it on hydration.
pub mod qr;
pub mod routine;
pub mod screensaver;
// T2.1: the Fire TV kiosk — the 10-foot, D-pad-only surface (PLAN v2 D8).
pub mod tv;
pub mod whiteboard;
