//! Realtime protocol **v2** — the client half.
//!
//! Normative spec: `docs/reviews/PURPLE_TEAM.md` §P2c, landed as
//! `docs/PROTOCOL.md` by T1.2. Three v1 defects are closed here:
//!
//! * **G3** — the socket never reconnected and the outbound half died with it.
//!   The send half is now owned by a *supervisor* ([`pump`]) that re-creates
//!   the connection on every drop with the jittered [`backoff`] schedule, and
//!   the coroutine's receiver survives across reconnects.
//! * **R-06** — `onpointermove` no longer sends. [`StrokeBatcher`] accumulates
//!   points and emits **at most one** `Draw` per animation frame
//!   ([`FLUSH_INTERVAL_MS`], ≤ 30 messages/second).
//! * **W2** — the server stamps every `Draw` with the originating
//!   [`ClientId`], so [`is_own_echo`] lets a client skip its own strokes
//!   instead of drawing them twice.

use std::time::Duration;

use dioxus::prelude::*;

#[cfg(all(feature = "web", target_arch = "wasm32"))]
use crate::shared::types::DEFAULT_BOARD_ID;
use crate::shared::types::{
    ClientId, ClientMessage, ServerMessage, Stroke, StrokePoint, View, PROTOCOL_VERSION,
};

// ---------------------------------------------------------------------------
// Timing constants (§P2c "Heartbeat and reconnect")
// ---------------------------------------------------------------------------

/// One `Ping` every 20 s.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
/// Two consecutive missed `Pong`s (≥ 45 s) mean the socket is dead.
pub const HEARTBEAT_MISSES_BEFORE_DEAD: u32 = 2;
/// Reconnect delays before jitter, capped at the last entry.
pub const BACKOFF_SCHEDULE_SECS: [u64; 6] = [1, 2, 4, 8, 15, 30];
/// ±20 % jitter so a room full of clients does not reconnect in lockstep.
pub const BACKOFF_JITTER: f64 = 0.20;
/// A connection healthy for this long resets the attempt counter to zero.
pub const BACKOFF_RESET_AFTER: Duration = Duration::from_secs(60);
/// One flush per animation frame. §P2c suggests 33 ms, but 1000/33 = 30.3
/// messages a second, which is *over* the stated hard cap; 34 ms gives 29.4/s
/// and keeps "≤ 30 messages/second per client" literally true.
pub const FLUSH_INTERVAL_MS: u64 = 34;
/// Points closer together than this (in normalised 0..1 units) are dropped.
pub const POINT_SIMPLIFY_THRESHOLD: f64 = 0.002;

/// Reconnect delay for `attempt` (0-based) **before** jitter.
pub fn backoff_base(attempt: u32) -> Duration {
    let index = (attempt as usize).min(BACKOFF_SCHEDULE_SECS.len() - 1);
    Duration::from_secs(BACKOFF_SCHEDULE_SECS[index])
}

/// Reconnect delay for `attempt`, jittered by ±[`BACKOFF_JITTER`].
pub fn backoff(attempt: u32) -> Duration {
    let base = backoff_base(attempt).as_secs_f64();
    let factor = 1.0 - BACKOFF_JITTER + 2.0 * BACKOFF_JITTER * unit_random();
    Duration::from_secs_f64(base * factor)
}

/// A pseudo-random number in `0.0..1.0`.
///
/// Seeded from a per-process entropy source rather than a constant, otherwise
/// every kiosk and phone would pick the *same* jitter and reconnect in
/// lockstep — which is exactly what jitter exists to prevent.
///
/// `pub(crate)` since QA round 1 (Q1-08): `components::routine::client_nonce`
/// mixes this into a per-page-load idempotency-key nonce, the same way this
/// module already uses it for reconnect jitter.
pub(crate) fn unit_random() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);

    let mut state = STATE.load(Ordering::Relaxed);
    if state == 0 {
        state = entropy_seed() | 1;
    }
    // xorshift64*
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    STATE.store(state, Ordering::Relaxed);
    let value = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
    (value >> 11) as f64 / (1u64 << 53) as f64
}

/// `pub(crate)` since QA round 1 (Q1-08): also seeds
/// `components::routine::client_nonce`'s per-page-load idempotency-key
/// nonce.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub(crate) fn entropy_seed() -> u64 {
    let millis = web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or(1.0);
    (millis * 1_000.0) as u64 ^ 0x9E37_79B9_7F4A_7C15
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
pub(crate) fn entropy_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos() as u64 ^ since.as_secs())
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ 0x9E37_79B9_7F4A_7C15
}

// ---------------------------------------------------------------------------
// Echo suppression
// ---------------------------------------------------------------------------

/// `true` when `origin` is this client's own id, i.e. the message is the
/// server's echo of something we already painted locally (W2).
pub fn is_own_echo(self_id: Option<&ClientId>, origin: &ClientId) -> bool {
    self_id.is_some_and(|id| id == origin)
}

// ---------------------------------------------------------------------------
// Stroke batching
// ---------------------------------------------------------------------------

/// Accumulates pointer samples and emits **at most one** [`Stroke`] per flush
/// interval, so a child scribbling for ten seconds produces ≤ 300 messages
/// instead of ~6,000 (§P2c).
///
/// Deliberately time-injected and free of any DOM or Dioxus dependency so the
/// ≤ 30 msg/s guarantee can be unit-tested.
#[derive(Debug, Clone)]
pub struct StrokeBatcher {
    color: String,
    width: f64,
    pending: Vec<StrokePoint>,
    /// Last point already sent, prepended to the next flush so the rendered
    /// line stays continuous across flush boundaries.
    anchor: Option<StrokePoint>,
    open: bool,
    last_flush_ms: Option<u64>,
    flushes: u64,
}

impl StrokeBatcher {
    pub fn new() -> Self {
        Self {
            color: String::new(),
            width: 0.0,
            pending: Vec::new(),
            anchor: None,
            open: false,
            last_flush_ms: None,
            flushes: 0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn flushes(&self) -> u64 {
        self.flushes
    }

    /// The most recent accepted point, so the canvas can draw the new segment
    /// locally without waiting for the server's echo.
    pub fn last_point(&self) -> Option<StrokePoint> {
        self.pending.last().copied().or(self.anchor)
    }

    /// `pointerdown`: start a new stroke.
    pub fn begin(&mut self, color: String, width: f64, point: StrokePoint) {
        self.color = color;
        self.width = width;
        self.pending.clear();
        self.pending.push(point);
        self.anchor = None;
        self.open = true;
    }

    /// `pointermove`: paint locally and remember the point. Returns `true` if
    /// the sample survived simplification.
    pub fn push(&mut self, point: StrokePoint) -> bool {
        if !self.open {
            return false;
        }
        let previous = self.pending.last().copied().or(self.anchor);
        if let Some(previous) = previous {
            let dx = point.x - previous.x;
            let dy = point.y - previous.y;
            if (dx * dx + dy * dy).sqrt() < POINT_SIMPLIFY_THRESHOLD {
                return false;
            }
        }
        self.pending.push(point);
        true
    }

    /// Emit a `Stroke` if the flush interval has elapsed and anything is
    /// pending. This is the ≤ 30 msg/s cap.
    pub fn flush_if_due(&mut self, now_ms: u64) -> Option<Stroke> {
        if let Some(last) = self.last_flush_ms {
            if now_ms.saturating_sub(last) < FLUSH_INTERVAL_MS {
                return None;
            }
        }
        let stroke = self.take()?;
        self.last_flush_ms = Some(now_ms);
        Some(stroke)
    }

    /// `pointerup`/`pointerleave`: force a final flush and close the stroke.
    pub fn end(&mut self) -> Option<Stroke> {
        let stroke = self.take();
        self.open = false;
        self.anchor = None;
        stroke
    }

    fn take(&mut self) -> Option<Stroke> {
        if self.pending.is_empty() {
            return None;
        }
        let mut points = Vec::with_capacity(self.pending.len() + 1);
        if let Some(anchor) = self.anchor {
            points.push(anchor);
        }
        points.append(&mut self.pending);
        self.anchor = points.last().copied();
        self.flushes += 1;
        Some(Stroke {
            points,
            color: self.color.clone(),
            width: self.width,
        })
    }
}

impl Default for StrokeBatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// The bus
// ---------------------------------------------------------------------------

/// Shared realtime state, provided as a context by [`crate::client::app::App`].
///
/// Version signals are reactive dependencies: a component that reads one is
/// re-run when the matching `ServerMessage` arrives. `RoutineUpdated` and
/// `TasksUpdated` also record *which* profile changed so a view can refetch
/// only that one (W7).
#[derive(Clone, Copy)]
pub struct RealtimeBus {
    /// This connection's server-minted id; `None` until `Hello` arrives.
    pub client_id: Signal<Option<ClientId>>,
    /// The server's idea of today, from `Hello` and `DayRolled` (F29/G4).
    pub today: Signal<Option<String>>,
    /// Bumped whenever a routine change arrives.
    pub routine_version: Signal<u64>,
    /// Profile the last `RoutineUpdated` referred to.
    pub routine_updated_for: Signal<Option<i64>>,
    /// Bumped whenever a custom-task change arrives.
    pub tasks_version: Signal<u64>,
    /// Profile the last `TasksUpdated` referred to.
    pub tasks_updated_for: Signal<Option<i64>>,
    /// Bumped by `ProfilesUpdated`.
    pub profiles_version: Signal<u64>,
    /// Bumped by `CalendarUpdated` and by the midnight `DayRolled`.
    pub calendar_version: Signal<u64>,
    /// **HS3** — bumped by `HomeschoolUpdated` and by `CurriculumUpdated`
    /// (`docs/homeschool/PLAN_HOMESCHOOL.md` H6 "Realtime"). One signal for
    /// both, because a curriculum edit changes what every boy's Today list
    /// holds just as a tick does, and the phone and TV both key their
    /// `use_resource` on it.
    pub homeschool_version: Signal<u64>,
    /// Remote strokes not yet painted. A **queue**, drained by the canvas —
    /// the single-slot v1 signal dropped everything that arrived between two
    /// renders (R-22a).
    pub inbound_strokes: Signal<Vec<Stroke>>,
    /// Latest `Snapshot` payload, replayed onto a fresh canvas.
    pub snapshot: Signal<Option<Vec<Stroke>>>,
    /// Bumped whenever another client cleared the board.
    pub clear_version: Signal<u64>,
    /// Bumped on every `Resync`; a view that caches anything refetches.
    pub resync_version: Signal<u64>,
    /// Latest `SetView` pushed from a phone.
    pub requested_view: Signal<Option<View>>,
    /// Latest `SetActiveProfile` pushed from a phone.
    pub requested_profile: Signal<Option<i64>>,
    /// Whether the socket is currently up (drives the TV's red badge).
    pub connected: Signal<bool>,
    /// Server `Health.stale` flag.
    pub stale: Signal<bool>,
}

impl RealtimeBus {
    fn bump(signal: &mut Signal<u64>) {
        let next = signal.peek().wrapping_add(1);
        signal.set(next);
    }

    /// Everything a `Resync` (or a fresh connection) invalidates.
    pub fn invalidate_all(&mut self) {
        Self::bump(&mut self.routine_version);
        Self::bump(&mut self.tasks_version);
        Self::bump(&mut self.profiles_version);
        Self::bump(&mut self.calendar_version);
        Self::bump(&mut self.homeschool_version);
        Self::bump(&mut self.resync_version);
    }

    pub fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Hello {
                client_id,
                protocol,
                today,
                ..
            } => {
                if protocol != PROTOCOL_VERSION {
                    tracing_warn(&format!(
                        "server speaks protocol {protocol}, client speaks {PROTOCOL_VERSION}"
                    ));
                }
                self.client_id.set(Some(client_id));
                self.today.set(Some(today));
                self.connected.set(true);
                self.invalidate_all();
            }
            ServerMessage::Pong { .. } => {}
            ServerMessage::Resync { .. } => self.invalidate_all(),
            ServerMessage::Draw { origin, stroke, .. } => {
                // Our own stroke came back: it is already on the canvas (W2).
                if is_own_echo(self.client_id.peek().as_ref(), &origin) {
                    return;
                }
                self.inbound_strokes.write().push(stroke);
            }
            ServerMessage::BoardCleared { origin, .. } => {
                if is_own_echo(self.client_id.peek().as_ref(), &origin) {
                    return;
                }
                self.inbound_strokes.write().clear();
                Self::bump(&mut self.clear_version);
            }
            ServerMessage::Snapshot { strokes, .. } => {
                self.inbound_strokes.write().clear();
                self.snapshot.set(Some(strokes));
            }
            ServerMessage::RoutineUpdated { user_id, .. } => {
                self.routine_updated_for.set(Some(user_id));
                Self::bump(&mut self.routine_version);
            }
            ServerMessage::TasksUpdated { user_id, .. } => {
                self.tasks_updated_for.set(Some(user_id));
                Self::bump(&mut self.tasks_version);
            }
            ServerMessage::ProfilesUpdated => Self::bump(&mut self.profiles_version),
            ServerMessage::CalendarUpdated { .. } => Self::bump(&mut self.calendar_version),
            ServerMessage::DayRolled { date } => {
                self.today.set(Some(date));
                self.invalidate_all();
            }
            ServerMessage::SetView { view } => self.requested_view.set(Some(view)),
            ServerMessage::SetActiveProfile { user_id } => {
                self.requested_profile.set(Some(user_id))
            }
            ServerMessage::Health { stale, .. } => self.stale.set(stale),
            ServerMessage::HomeschoolUpdated { .. } => Self::bump(&mut self.homeschool_version),
            ServerMessage::CurriculumUpdated { .. } => Self::bump(&mut self.homeschool_version),
        }
    }

    /// Drain every stroke the canvas has not painted yet (R-22a).
    pub fn drain_inbound_strokes(&mut self) -> Vec<Stroke> {
        let mut queue = self.inbound_strokes.write();
        std::mem::take(&mut *queue)
    }

    /// The socket dropped: clear the badge and forget the server-minted id
    /// (a new one is issued on the next `Hello`).
    pub fn on_disconnected(&mut self) {
        self.connected.set(false);
        self.client_id.set(None);
    }
}

#[cfg(feature = "server")]
fn tracing_warn(message: &str) {
    tracing::warn!("{message}");
}

#[cfg(not(feature = "server"))]
fn tracing_warn(_message: &str) {}

/// Handle used by components to publish messages to the server.
pub type RealtimeSender = Coroutine<ClientMessage>;

pub fn use_realtime() -> (RealtimeBus, RealtimeSender) {
    (
        use_context::<RealtimeBus>(),
        use_context::<RealtimeSender>(),
    )
}

/// Create the bus and start the reconnecting WebSocket supervisor.
pub fn use_realtime_provider() -> RealtimeBus {
    let bus = use_context_provider(|| RealtimeBus {
        client_id: Signal::new(None),
        today: Signal::new(None),
        routine_version: Signal::new(0),
        routine_updated_for: Signal::new(None),
        tasks_version: Signal::new(0),
        tasks_updated_for: Signal::new(None),
        profiles_version: Signal::new(0),
        calendar_version: Signal::new(0),
        homeschool_version: Signal::new(0),
        inbound_strokes: Signal::new(Vec::new()),
        snapshot: Signal::new(None),
        clear_version: Signal::new(0),
        resync_version: Signal::new(0),
        requested_view: Signal::new(None),
        requested_profile: Signal::new(None),
        connected: Signal::new(false),
        stale: Signal::new(false),
    });

    let _sender = use_coroutine(move |rx: UnboundedReceiver<ClientMessage>| async move {
        pump(bus, rx).await;
    });

    bus
}

// ---------------------------------------------------------------------------
// The reconnect supervisor
// ---------------------------------------------------------------------------

#[cfg(all(feature = "web", target_arch = "wasm32"))]
enum ConnectionOutcome {
    /// The socket lived long enough to count as healthy; reset the backoff.
    Healthy,
    /// The socket failed or died early; escalate the backoff.
    Failed,
    /// There is no URL to connect to; stop trying.
    Fatal,
}

/// Owns the coroutine's receiver for the whole lifetime of the component and
/// re-creates the socket after every drop, so the send half can never be
/// orphaned the way it was in v1 (G3).
#[cfg(all(feature = "web", target_arch = "wasm32"))]
async fn pump(bus: RealtimeBus, mut rx: UnboundedReceiver<ClientMessage>) {
    let mut attempt = 0u32;
    loop {
        match connect_once(bus, &mut rx).await {
            ConnectionOutcome::Fatal => return,
            ConnectionOutcome::Healthy => attempt = 0,
            ConnectionOutcome::Failed => attempt = attempt.saturating_add(1),
        }
        gloo_timers::future::TimeoutFuture::new(backoff(attempt).as_millis() as u32).await;
    }
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
enum SocketEvent {
    Text(String),
    Outgoing(ClientMessage),
    Beat,
    Ignore,
    Closed,
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
async fn connect_once(
    mut bus: RealtimeBus,
    rx: &mut UnboundedReceiver<ClientMessage>,
) -> ConnectionOutcome {
    use futures_util::{SinkExt, StreamExt};
    use gloo_net::websocket::{futures::WebSocket, Message};

    let Some(url) = socket_url() else {
        return ConnectionOutcome::Fatal;
    };
    let Ok(socket) = WebSocket::open(&url) else {
        return ConnectionOutcome::Failed;
    };

    let (mut sink, stream) = socket.split();

    async fn send(
        sink: &mut futures_util::stream::SplitSink<
            gloo_net::websocket::futures::WebSocket,
            Message,
        >,
        message: &ClientMessage,
    ) -> bool {
        let Ok(payload) = serde_json::to_string(message) else {
            return true;
        };
        sink.send(Message::Text(payload)).await.is_ok()
    }

    // Every (re)connection announces itself and asks for the board back.
    for message in [
        ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
        },
        ClientMessage::RequestSnapshot {
            board_id: DEFAULT_BOARD_ID,
            since_seq: 0,
        },
    ] {
        if !send(&mut sink, &message).await {
            return ConnectionOutcome::Failed;
        }
    }

    let opened_at = now_millis();

    let inbound = stream
        .map(|frame| match frame {
            Ok(Message::Text(text)) => SocketEvent::Text(text),
            Ok(Message::Bytes(_)) => SocketEvent::Ignore,
            Err(_) => SocketEvent::Closed,
        })
        // `stream::iter` rather than `stream::once(async …)`: the latter is
        // not `Unpin`, which `stream::select` needs.
        .chain(futures_util::stream::iter(std::iter::once(
            SocketEvent::Closed,
        )));
    let outgoing = rx.map(SocketEvent::Outgoing);
    let beats = gloo_timers::future::IntervalStream::new(HEARTBEAT_INTERVAL.as_millis() as u32)
        .map(|_| SocketEvent::Beat);

    let mut events =
        futures_util::stream::select(futures_util::stream::select(inbound, outgoing), beats);

    let mut nonce = 0u64;
    let mut missed_pongs = 0u32;

    while let Some(event) = events.next().await {
        match event {
            SocketEvent::Ignore => {}
            SocketEvent::Closed => break,
            SocketEvent::Text(text) => {
                let Ok(message) = serde_json::from_str::<ServerMessage>(&text) else {
                    continue;
                };
                let resync = matches!(message, ServerMessage::Resync { .. });
                if matches!(message, ServerMessage::Pong { .. }) {
                    missed_pongs = 0;
                }
                bus.apply(message);
                // A `Resync` never tears the socket down: the client just
                // re-requests the board and refetches (§P2c).
                if resync
                    && !send(
                        &mut sink,
                        &ClientMessage::RequestSnapshot {
                            board_id: DEFAULT_BOARD_ID,
                            since_seq: 0,
                        },
                    )
                    .await
                {
                    break;
                }
            }
            SocketEvent::Outgoing(message) => {
                if !send(&mut sink, &message).await {
                    break;
                }
            }
            SocketEvent::Beat => {
                if missed_pongs >= HEARTBEAT_MISSES_BEFORE_DEAD {
                    break;
                }
                missed_pongs += 1;
                nonce = nonce.wrapping_add(1);
                if !send(&mut sink, &ClientMessage::Ping { nonce }).await {
                    break;
                }
            }
        }
    }

    drop(events);
    bus.on_disconnected();

    if now_millis().saturating_sub(opened_at) >= BACKOFF_RESET_AFTER.as_millis() as u64 {
        ConnectionOutcome::Healthy
    } else {
        ConnectionOutcome::Failed
    }
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
async fn pump(_bus: RealtimeBus, _rx: UnboundedReceiver<ClientMessage>) {}

/// Milliseconds from the highest-resolution clock the target offers. Used for
/// the stroke flush timer and the healthy-connection threshold; only
/// differences matter, never the absolute value.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub fn now_millis() -> u64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now() as u64)
        .unwrap_or_default()
}

#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn socket_url() -> Option<String> {
    let location = web_sys::window()?.location();
    let protocol = if location.protocol().ok()?.starts_with("https") {
        "wss"
    } else {
        "ws"
    };
    Some(format!("{protocol}://{}/ws", location.host().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> StrokePoint {
        StrokePoint { x, y }
    }

    #[test]
    fn backoff_base_follows_the_documented_schedule_and_caps_at_thirty() {
        let expected = [1, 2, 4, 8, 15, 30, 30, 30, 30, 30];
        for (attempt, seconds) in expected.iter().enumerate() {
            assert_eq!(
                backoff_base(attempt as u32),
                Duration::from_secs(*seconds),
                "attempt {attempt}"
            );
        }
    }

    #[test]
    fn backoff_stays_within_twenty_percent_of_the_base() {
        for attempt in 0..10u32 {
            let base = backoff_base(attempt).as_secs_f64();
            for _ in 0..200 {
                let jittered = backoff(attempt).as_secs_f64();
                assert!(
                    jittered >= base * 0.8 - 1e-9 && jittered <= base * 1.2 + 1e-9,
                    "attempt {attempt}: {jittered} outside ±20% of {base}"
                );
            }
        }
    }

    #[test]
    fn echo_suppression_only_skips_our_own_origin() {
        let mine = ClientId("mine".into());
        let theirs = ClientId("theirs".into());
        assert!(is_own_echo(Some(&mine), &mine));
        assert!(!is_own_echo(Some(&mine), &theirs));
        assert!(!is_own_echo(None, &mine));
    }

    #[test]
    fn stroke_batcher_emits_at_most_thirty_messages_per_second() {
        let mut batcher = StrokeBatcher::new();
        batcher.begin("#000".into(), 4.0, point(0.0, 0.0));

        // Ten seconds of pointer samples at 1 kHz, all far enough apart to
        // survive simplification.
        let mut emitted = 0u64;
        for step in 1..=10_000u64 {
            let x = (step as f64 % 100.0) / 100.0;
            let y = (step as f64 / 10_000.0) % 1.0;
            batcher.push(point(x, y));
            if batcher.flush_if_due(step).is_some() {
                emitted += 1;
            }
        }
        if batcher.end().is_some() {
            emitted += 1;
        }

        assert!(
            emitted <= 10 * 30 + 1,
            "10 s of drawing produced {emitted} messages, above the 30/s cap"
        );
        assert!(emitted > 100, "the batcher must still send: {emitted}");
    }

    #[test]
    fn stroke_batcher_simplifies_points_closer_than_the_threshold() {
        let mut batcher = StrokeBatcher::new();
        batcher.begin("#000".into(), 4.0, point(0.5, 0.5));
        assert!(!batcher.push(point(0.5005, 0.5)), "below the threshold");
        assert!(batcher.push(point(0.51, 0.5)), "above the threshold");
        let stroke = batcher.end().expect("a stroke");
        assert_eq!(stroke.points, vec![point(0.5, 0.5), point(0.51, 0.5)]);
    }

    #[test]
    fn stroke_batcher_anchors_each_flush_to_the_previous_one() {
        let mut batcher = StrokeBatcher::new();
        batcher.begin("#000".into(), 4.0, point(0.0, 0.0));
        batcher.push(point(0.1, 0.0));
        let first = batcher.flush_if_due(0).expect("first flush");
        assert_eq!(first.points.len(), 2);

        batcher.push(point(0.2, 0.0));
        let second = batcher
            .flush_if_due(FLUSH_INTERVAL_MS)
            .expect("second flush");
        assert_eq!(
            second.points.first().copied(),
            Some(point(0.1, 0.0)),
            "the second flush must continue from the first"
        );
    }

    #[test]
    fn a_stroke_expands_into_pairwise_segments() {
        let stroke = Stroke {
            points: vec![point(0.0, 0.0), point(0.5, 0.5), point(1.0, 1.0)],
            color: "#123456".into(),
            width: 3.0,
        };
        let segments = stroke.segments();
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].to, point(0.5, 0.5));
        assert_eq!(segments[1].from, point(0.5, 0.5));
        assert_eq!(segments[1].color, "#123456");
    }
}
