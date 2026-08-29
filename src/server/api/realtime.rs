//! Realtime protocol **v2** — the server half.
//!
//! Normative spec: `docs/reviews/PURPLE_TEAM.md` §P2c, landed as
//! `docs/PROTOCOL.md` by this task (T1.2). The v1 hub this replaces closed the
//! socket on `broadcast::RecvError::Lagged` (G20) and rebroadcast client JSON
//! verbatim (G13); neither is possible here.
//!
//! Shape of one connection:
//!
//! ```text
//!            broadcast(1024)                 OutboundQueue(256, drop-oldest)
//! publish() ────────────────► fan-out task ─────────────────────────────► writer task ──► sink
//!                                  │ Lagged ⇒ resubscribe + Resync            ▲
//!                                  └──────────────────────────────────────────┘
//!   stream ──► reader task ──► token bucket (40/s, burst 80) ──► validate/authorise ──► publish()
//! ```
//!
//! Every rule below is a constant with the PURPLE reference next to it, so a
//! future reader can check the implementation against the spec by grep.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use chrono::{DateTime, Days, Local, TimeZone};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, Notify};

use crate::shared::types::{
    ClientId, ClientMessage, ResyncReason, ServerMessage, Stroke, DEFAULT_BOARD_ID,
    PROTOCOL_VERSION,
};

/// Broadcast channel capacity — raised 256 → 1024 (§P2c "Stroke batching and
/// rate limiting", PURPLE default 28).
pub const BROADCAST_CAPACITY: usize = 1024;
/// Per-connection bounded outbound queue, **drop-oldest** (§P2c).
pub const OUTBOUND_QUEUE_CAPACITY: usize = 256;
/// Once this many frames have been dropped from one outbound queue, the
/// backlog is replaced by a single `Resync` (§P2c).
pub const OUTBOUND_DROPPED_BEFORE_RESYNC: usize = 32;
/// Server-side token bucket: 40 messages/second, burst 80 (§P2c).
pub const RATE_LIMIT_PER_SECOND: f64 = 40.0;
pub const RATE_LIMIT_BURST: f64 = 80.0;
/// Three consecutive seconds over budget ⇒ `Resync` and close *that* client
/// only (§P2c).
pub const RATE_LIMIT_OVER_BUDGET_SECONDS: u32 = 3;
/// The server drops a connection that has sent nothing for 90 s (§P2c
/// "Heartbeat and reconnect").
pub const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Strokes retained in the live board snapshot (PURPLE default 15). T2.3
/// replaces this in-memory store with the `whiteboard_strokes` table.
pub const MAX_RETAINED_STROKES: usize = 2_000;
/// Defensive cap on the points a single batched stroke may carry.
pub const MAX_STROKE_POINTS: usize = 4_096;

/// One already-encoded `ServerMessage`. Encoding once and sharing the `Arc`
/// keeps fan-out to N clients O(N) pointer copies instead of O(N) `serde_json`
/// runs.
pub type Frame = Arc<str>;

static CHANNEL: OnceLock<broadcast::Sender<Frame>> = OnceLock::new();

pub fn sender() -> &'static broadcast::Sender<Frame> {
    CHANNEL.get_or_init(|| broadcast::channel(BROADCAST_CAPACITY).0)
}

/// Number of connections currently subscribed to the broadcast channel.
/// Consumed by T1.7's `/health`.
pub fn connected_clients() -> usize {
    sender().receiver_count()
}

/// Encode a `ServerMessage` into a shareable frame.
pub fn encode(message: &ServerMessage) -> Option<Frame> {
    match serde_json::to_string(message) {
        Ok(payload) => Some(Arc::from(payload.as_str())),
        Err(err) => {
            tracing::error!("failed to encode websocket message: {err}");
            None
        }
    }
}

/// Fan a **server-minted** message out to every connected kiosk and phone.
///
/// There is deliberately no way to publish a client's bytes: callers hand over
/// a typed [`ServerMessage`] and the server encodes it (G13).
pub fn publish(message: &ServerMessage) {
    if let Some(frame) = encode(message) {
        let _ = sender().send(frame);
    }
}

fn resync_frame(reason: ResyncReason) -> Frame {
    encode(&ServerMessage::Resync { reason })
        .unwrap_or_else(|| Arc::from(r#"{"type":"resync","reason":"lagged"}"#))
}

fn new_client_id() -> ClientId {
    ClientId(uuid::Uuid::new_v4().to_string())
}

// ---------------------------------------------------------------------------
// Bounded, drop-oldest outbound queue
// ---------------------------------------------------------------------------

/// Per-connection outbound buffer: at most [`OUTBOUND_QUEUE_CAPACITY`] frames,
/// **drop-oldest** on overflow. Once [`OUTBOUND_DROPPED_BEFORE_RESYNC`] frames
/// have been dropped the whole backlog is thrown away and replaced by a single
/// `Resync`, because a client that far behind is better off refetching than
/// replaying a truncated history (§P2c).
///
/// Pure and synchronous so it can be unit-tested without a socket.
#[derive(Debug)]
pub struct OutboundQueue {
    queue: VecDeque<Frame>,
    capacity: usize,
    dropped: usize,
    total_dropped: usize,
    resyncs_injected: usize,
}

impl OutboundQueue {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity.min(64)),
            capacity: capacity.max(1),
            dropped: 0,
            total_dropped: 0,
            resyncs_injected: 0,
        }
    }

    pub fn push(&mut self, frame: Frame) {
        if self.queue.len() >= self.capacity {
            self.queue.pop_front();
            self.dropped += 1;
            self.total_dropped += 1;
        }
        self.queue.push_back(frame);

        if self.dropped >= OUTBOUND_DROPPED_BEFORE_RESYNC {
            self.queue.clear();
            self.queue.push_back(resync_frame(ResyncReason::Lagged));
            self.dropped = 0;
            self.resyncs_injected += 1;
        }
    }

    pub fn pop(&mut self) -> Option<Frame> {
        self.queue.pop_front()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Frames discarded over the life of the queue.
    pub fn total_dropped(&self) -> usize {
        self.total_dropped
    }

    /// How many times the backlog was collapsed into a single `Resync`.
    pub fn resyncs_injected(&self) -> usize {
        self.resyncs_injected
    }
}

impl Default for OutboundQueue {
    fn default() -> Self {
        Self::with_capacity(OUTBOUND_QUEUE_CAPACITY)
    }
}

/// [`OutboundQueue`] plus the wakeup plumbing the writer task waits on.
struct Outbound {
    queue: Mutex<OutboundQueue>,
    notify: Notify,
    closed: AtomicBool,
}

impl Outbound {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(OutboundQueue::with_capacity(capacity)),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
        }
    }

    fn push(&self, frame: Frame) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut queue) = self.queue.lock() {
            queue.push(frame);
        }
        self.notify.notify_one();
    }

    fn push_message(&self, message: &ServerMessage) {
        if let Some(frame) = encode(message) {
            self.push(frame);
        }
    }

    /// Next frame, or `None` once the queue is both closed and drained.
    async fn pop(&self) -> Option<Frame> {
        loop {
            if let Ok(mut queue) = self.queue.lock() {
                if let Some(frame) = queue.pop() {
                    return Some(frame);
                }
            }
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            self.notify.notified().await;
        }
    }

    /// Stop accepting frames; the writer still drains what is already queued
    /// (so a final `Resync` reaches the client before the socket closes).
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_one();
    }
}

// ---------------------------------------------------------------------------
// Token bucket / rate limiter
// ---------------------------------------------------------------------------

/// Classic token bucket. `capacity` is the burst allowance, `refill_per_second`
/// the steady-state rate.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_per_second: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(refill_per_second: f64, capacity: f64, now: Instant) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_per_second,
            last: now,
        }
    }

    pub fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_second).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    pub fn tokens(&self) -> f64 {
        self.tokens
    }
}

/// What the connection should do with an inbound message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    /// Within budget — handle it.
    Allow,
    /// Over budget — drop this one message, warn, keep the socket.
    Throttle,
    /// Over budget for [`RATE_LIMIT_OVER_BUDGET_SECONDS`] consecutive seconds
    /// — send `Resync` and close **this** client only.
    Resync,
}

/// Per-connection inbound rate limiter (§P2c).
#[derive(Debug)]
pub struct RateLimiter {
    bucket: TokenBucket,
    window_start: Instant,
    dropped_this_window: u32,
    consecutive_over_budget_seconds: u32,
    throttled: u64,
}

impl RateLimiter {
    pub fn new(now: Instant) -> Self {
        Self {
            bucket: TokenBucket::new(RATE_LIMIT_PER_SECOND, RATE_LIMIT_BURST, now),
            window_start: now,
            dropped_this_window: 0,
            consecutive_over_budget_seconds: 0,
            throttled: 0,
        }
    }

    pub fn admit(&mut self, now: Instant) -> RateDecision {
        while now.saturating_duration_since(self.window_start) >= Duration::from_secs(1) {
            if self.dropped_this_window > 0 {
                self.consecutive_over_budget_seconds += 1;
            } else {
                self.consecutive_over_budget_seconds = 0;
            }
            self.dropped_this_window = 0;
            self.window_start += Duration::from_secs(1);
        }

        if self.consecutive_over_budget_seconds >= RATE_LIMIT_OVER_BUDGET_SECONDS {
            return RateDecision::Resync;
        }

        if self.bucket.try_consume(now) {
            RateDecision::Allow
        } else {
            self.dropped_this_window += 1;
            self.throttled += 1;
            RateDecision::Throttle
        }
    }

    pub fn throttled(&self) -> u64 {
        self.throttled
    }
}

// ---------------------------------------------------------------------------
// Board state (in-memory until T2.3 persists it)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BoardState {
    seq: i64,
    strokes: VecDeque<(i64, Stroke)>,
}

static BOARD: OnceLock<Mutex<BoardState>> = OnceLock::new();

fn board() -> &'static Mutex<BoardState> {
    BOARD.get_or_init(|| Mutex::new(BoardState::default()))
}

/// Append a stroke and return the sequence number the server stamped on it.
///
/// **T2.3 replaces this body** with a row in `whiteboard_strokes` (one row per
/// stroke, `seq`, `board_id`, `cleared_at` watermark). The signature and the
/// `seq` contract are what `docs/PROTOCOL.md` freezes.
pub fn record_stroke(stroke: &Stroke) -> i64 {
    let mut state = match board().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state.seq += 1;
    let seq = state.seq;
    state.strokes.push_back((seq, stroke.clone()));
    while state.strokes.len() > MAX_RETAINED_STROKES {
        state.strokes.pop_front();
    }
    seq
}

/// Move the `cleared_at` watermark: everything before the returned `seq` is
/// gone from future snapshots.
pub fn clear_board() -> i64 {
    let mut state = match board().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state.seq += 1;
    state.strokes.clear();
    state.seq
}

/// `(latest_seq, strokes with seq > since_seq)`, in `seq` order.
pub fn snapshot(since_seq: i64) -> (i64, Vec<Stroke>) {
    let state = match board().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    let strokes = state
        .strokes
        .iter()
        .filter(|(seq, _)| *seq > since_seq)
        .map(|(_, stroke)| stroke.clone())
        .collect();
    (state.seq, strokes)
}

/// Test helper: forget every stroke and reset the sequence.
pub fn reset_board() {
    let mut state = match board().lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state.seq = 0;
    state.strokes.clear();
}

// ---------------------------------------------------------------------------
// Parent session seam
// ---------------------------------------------------------------------------

/// Parent-session validation seam.
///
/// **T1.4 owns the real implementation** (argon2id PIN → 30-day session
/// token in `src/server/auth.rs`); this module's doc comment reserved
/// exactly this seam for it. Every function here is now a thin delegate to
/// `crate::server::auth`, which holds the actual token store (with real
/// 30-day expiry, replacing the plain `HashSet` this module started with) —
/// signatures are unchanged, so every existing caller (the WS authorisation
/// checks below, `tests/realtime_tests.rs`) needs no update
/// (`docs/PROTOCOL.md` §Authorisation).
pub mod session {
    /// Mint and register a parent session token.
    pub fn issue() -> String {
        crate::server::auth::issue_session()
    }

    pub fn insert(token: &str) {
        crate::server::auth::insert_session(token);
    }

    pub fn revoke(token: &str) {
        crate::server::auth::revoke_session(token);
    }

    pub fn revoke_all() {
        crate::server::auth::revoke_all_sessions();
    }

    /// The one predicate the realtime hub calls.
    pub fn is_valid(token: &str) -> bool {
        crate::server::auth::is_valid_session(token)
    }
}

fn authorised(auth: Option<&str>) -> bool {
    auth.is_some_and(session::is_valid)
}

// ---------------------------------------------------------------------------
// Midnight tick (DST-safe)
// ---------------------------------------------------------------------------

/// The instant of the next local midnight after `now`.
///
/// `.earliest()` makes this deterministic when local midnight is skipped by a
/// spring-forward transition or repeated by a fall-back one (F29); the 24-hour
/// fallback only fires for a zone in which the whole day is unrepresentable,
/// which cannot happen for a real IANA zone.
///
/// Recomputed from a fresh `now` on **every** iteration of the tick loop, so
/// no drift accumulates and 23- and 25-hour local days are handled by
/// construction.
pub fn next_midnight<Tz: TimeZone>(now: &DateTime<Tz>) -> DateTime<Tz> {
    let midnight = (now.date_naive() + Days::new(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always a valid time of day");
    now.timezone()
        .from_local_datetime(&midnight)
        .earliest()
        .unwrap_or_else(|| now.clone() + chrono::Duration::hours(24))
}

/// How long the tick loop should sleep before the next local midnight.
pub fn duration_until_midnight<Tz: TimeZone>(now: &DateTime<Tz>) -> Duration {
    (next_midnight(now) - now.clone())
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(60))
}

/// Work other modules want run when the day rolls over: T2.4's forced calendar
/// poll (W4) and T1.6's retention sweep (strokes, photos, logs, WAL
/// checkpoint). Registered rather than called directly so this module keeps no
/// dependency on either.
pub type DayRolledHook =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

static DAY_ROLLED_HOOKS: OnceLock<Mutex<Vec<DayRolledHook>>> = OnceLock::new();

fn day_rolled_hooks() -> &'static Mutex<Vec<DayRolledHook>> {
    DAY_ROLLED_HOOKS.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn on_day_rolled(hook: DayRolledHook) {
    let mut hooks = match day_rolled_hooks().lock() {
        Ok(hooks) => hooks,
        Err(poisoned) => poisoned.into_inner(),
    };
    hooks.push(hook);
}

/// Broadcast `DayRolled` and run every registered hook.
pub async fn run_day_rolled(date: String) {
    publish(&ServerMessage::DayRolled { date: date.clone() });
    let hooks = {
        let guard = match day_rolled_hooks().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    };
    for hook in hooks {
        hook(date.clone()).await;
    }
}

static BACKGROUND_TASKS: OnceLock<()> = OnceLock::new();

/// Start the midnight tick exactly once per process.
///
/// Called from [`ws_handler`] (which always runs inside the tokio runtime) so
/// the hub is self-starting; `docs/HANDOFF.md` asks the Boss to also call it
/// explicitly from `server::router::run`, which this task does not own.
pub fn ensure_background_tasks() {
    BACKGROUND_TASKS.get_or_init(|| {
        if tokio::runtime::Handle::try_current().is_ok() {
            spawn_midnight_tick();
        }
    });
}

/// The DST-safe midnight loop (§P2c "Midnight tick").
pub fn spawn_midnight_tick() {
    tokio::spawn(async move {
        loop {
            let now = Local::now();
            tokio::time::sleep(duration_until_midnight(&now)).await;
            let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
            tracing::info!("day rolled to {today}");
            run_day_rolled(today).await;
            // D4 / T1.1 H-7: fold the WAL back into the main file once a day
            // so `family.db-wal` cannot grow without bound on a box that is
            // never rebooted.
            if let Err(err) = crate::server::db::on_midnight_tick().await {
                tracing::warn!(%err, "midnight WAL checkpoint failed");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Connection handling
// ---------------------------------------------------------------------------

pub async fn ws_handler(upgrade: WebSocketUpgrade) -> Response {
    ensure_background_tasks();
    upgrade.on_upgrade(|socket| handle_socket(socket, new_client_id()))
}

fn hello(client_id: &ClientId) -> ServerMessage {
    let now = Local::now();
    ServerMessage::Hello {
        client_id: client_id.clone(),
        protocol: PROTOCOL_VERSION,
        server_time: now.to_rfc3339(),
        today: now.date_naive().format("%Y-%m-%d").to_string(),
    }
}

async fn handle_socket(socket: WebSocket, client_id: ClientId) {
    let (mut sink, mut stream) = socket.split();
    let outbound = Arc::new(Outbound::new(OUTBOUND_QUEUE_CAPACITY));

    // Subscribe *before* anything else so no message published between the
    // upgrade and the fan-out task's first poll is missed.
    let receiver = sender().subscribe();

    // The server mints the id and tells the client immediately; the client does
    // not have to send `Hello` first to learn who it is.
    outbound.push_message(&hello(&client_id));

    let writer_outbound = Arc::clone(&outbound);
    let writer = tokio::spawn(async move {
        while let Some(frame) = writer_outbound.pop().await {
            if sink
                .send(Message::Text(frame.as_ref().into()))
                .await
                .is_err()
            {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let fan_out_outbound = Arc::clone(&outbound);
    let fan_out_id = client_id.clone();
    let fan_out = tokio::spawn(async move {
        let mut receiver = receiver;
        loop {
            match receiver.recv().await {
                Ok(frame) => fan_out_outbound.push(frame),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // NEVER break here (G20). `tokio` has already repositioned
                    // the receiver; resubscribing drops the stale backlog so
                    // the client restarts from "now" plus one `Resync`.
                    tracing::warn!("client {fan_out_id} lagged {n} messages; resyncing");
                    receiver = sender().subscribe();
                    fan_out_outbound.push(resync_frame(ResyncReason::Lagged));
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    read_loop(&mut stream, &outbound, &client_id).await;

    outbound.close();
    let _ = writer.await;
    fan_out.abort();
}

/// Per-connection state the reader mutates.
struct Connection {
    client_id: ClientId,
    limiter: RateLimiter,
    /// The profile this connection has been granted. Set only by an
    /// **authorised** `SetActiveProfile`; an unauthenticated client may
    /// therefore only re-assert a profile it already owns (R-23b).
    active_profile: Option<i64>,
}

async fn read_loop<S>(stream: &mut S, outbound: &Outbound, client_id: &ClientId)
where
    S: futures_util::Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let mut conn = Connection {
        client_id: client_id.clone(),
        limiter: RateLimiter::new(Instant::now()),
        active_profile: None,
    };

    loop {
        let next = match tokio::time::timeout(CLIENT_IDLE_TIMEOUT, stream.next()).await {
            Ok(next) => next,
            Err(_) => {
                tracing::warn!("client {client_id} idle for 90s; closing");
                return;
            }
        };

        let message = match next {
            Some(Ok(message)) => message,
            Some(Err(err)) => {
                tracing::debug!("client {client_id} socket error: {err}");
                return;
            }
            None => return,
        };

        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => return,
            // Binary frames are not part of protocol v2; ping/pong at the
            // WebSocket layer is handled by axum.
            _ => continue,
        };

        match conn.limiter.admit(Instant::now()) {
            RateDecision::Allow => {}
            RateDecision::Throttle => {
                tracing::warn!("client {client_id} over the 40 msg/s budget; dropping a message");
                continue;
            }
            RateDecision::Resync => {
                tracing::warn!(
                    "client {client_id} over budget for {RATE_LIMIT_OVER_BUDGET_SECONDS}s; \
                     resyncing and closing this client"
                );
                outbound.push(resync_frame(ResyncReason::Lagged));
                return;
            }
        }

        let Ok(client_message) = serde_json::from_str::<ClientMessage>(text.as_str()) else {
            // G13: unknown or malformed client JSON is dropped here. It is
            // never forwarded, so `{"CalendarUpdated": …}` reaches nobody.
            tracing::warn!("client {client_id} sent an unknown message; dropped");
            continue;
        };

        handle_client_message(&mut conn, outbound, client_message);
    }
}

fn handle_client_message(conn: &mut Connection, outbound: &Outbound, message: ClientMessage) {
    match message {
        ClientMessage::Hello { protocol } => {
            if protocol != PROTOCOL_VERSION {
                tracing::warn!(
                    "client {} speaks protocol {protocol}, server speaks {PROTOCOL_VERSION}",
                    conn.client_id
                );
            }
            outbound.push_message(&hello(&conn.client_id));
        }
        ClientMessage::Ping { nonce } => {
            outbound.push_message(&ServerMessage::Pong { nonce });
        }
        ClientMessage::Draw { board_id, stroke } => {
            if !valid_board(board_id, &conn.client_id) {
                return;
            }
            if stroke.points.is_empty() || stroke.points.len() > MAX_STROKE_POINTS {
                tracing::warn!(
                    "client {} sent a stroke with {} points; dropped",
                    conn.client_id,
                    stroke.points.len()
                );
                return;
            }
            let seq = record_stroke(&stroke);
            publish(&ServerMessage::Draw {
                board_id,
                seq,
                origin: conn.client_id.clone(),
                stroke,
            });
        }
        ClientMessage::ClearBoard { board_id } => {
            if !valid_board(board_id, &conn.client_id) {
                return;
            }
            let seq = clear_board();
            publish(&ServerMessage::BoardCleared {
                board_id,
                seq,
                origin: conn.client_id.clone(),
            });
        }
        ClientMessage::RequestSnapshot {
            board_id,
            since_seq,
        } => {
            if !valid_board(board_id, &conn.client_id) {
                return;
            }
            let (seq, strokes) = snapshot(since_seq);
            outbound.push_message(&ServerMessage::Snapshot {
                board_id,
                seq,
                strokes,
            });
        }
        ClientMessage::SetView { view, auth } => {
            if !authorised(auth.as_deref()) {
                tracing::warn!(
                    "client {} sent SetView without a valid parent session; dropped",
                    conn.client_id
                );
                return;
            }
            publish(&ServerMessage::SetView { view });
        }
        ClientMessage::SetActiveProfile { user_id, auth } => {
            let permitted = authorised(auth.as_deref()) || conn.active_profile == Some(user_id);
            if !permitted {
                tracing::warn!(
                    "client {} may not switch to profile {user_id}; dropped",
                    conn.client_id
                );
                return;
            }
            conn.active_profile = Some(user_id);
            publish(&ServerMessage::SetActiveProfile { user_id });
        }
    }
}

fn valid_board(board_id: i64, client_id: &ClientId) -> bool {
    if board_id == DEFAULT_BOARD_ID {
        true
    } else {
        tracing::warn!("client {client_id} addressed unknown board {board_id}; dropped");
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(text: &str) -> Frame {
        Arc::from(text)
    }

    #[test]
    fn outbound_queue_drops_the_oldest_frame_at_capacity() {
        let mut queue = OutboundQueue::with_capacity(4);
        for i in 0..4 {
            queue.push(frame(&i.to_string()));
        }
        queue.push(frame("4"));
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.total_dropped(), 1);
        assert_eq!(queue.pop().as_deref(), Some("1"), "oldest must be dropped");
    }

    #[test]
    fn outbound_queue_collapses_into_one_resync_after_32_drops() {
        let mut queue = OutboundQueue::with_capacity(8);
        for i in 0..(8 + OUTBOUND_DROPPED_BEFORE_RESYNC) {
            queue.push(frame(&i.to_string()));
        }
        assert_eq!(queue.resyncs_injected(), 1);
        assert_eq!(queue.len(), 1);
        let only = queue.pop().expect("one frame");
        assert!(only.contains("resync"), "collapsed backlog: {only}");
        assert!(only.contains("lagged"), "collapsed backlog: {only}");
    }

    #[test]
    fn token_bucket_allows_the_burst_then_refills_at_the_configured_rate() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(RATE_LIMIT_PER_SECOND, RATE_LIMIT_BURST, start);
        for _ in 0..(RATE_LIMIT_BURST as usize) {
            assert!(bucket.try_consume(start));
        }
        assert!(
            !bucket.try_consume(start),
            "burst of 80 is the whole budget"
        );
        assert!(bucket.try_consume(start + Duration::from_millis(25)));
    }

    #[test]
    fn rate_limiter_resyncs_after_three_consecutive_over_budget_seconds() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(start);
        let mut decision = RateDecision::Allow;
        // 200 msg/s for four seconds.
        for step in 0..800u32 {
            let now = start + Duration::from_micros(u64::from(step) * 5_000);
            decision = limiter.admit(now);
            if decision == RateDecision::Resync {
                break;
            }
        }
        assert_eq!(decision, RateDecision::Resync);
        assert!(limiter.throttled() > 0);
    }

    #[test]
    fn rate_limiter_never_resyncs_a_client_inside_its_budget() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(start);
        // 30 msg/s for 30 s — the client-side cap from §P2c.
        for step in 0..900u32 {
            let now = start + Duration::from_micros(u64::from(step) * 33_333);
            assert_eq!(limiter.admit(now), RateDecision::Allow, "step {step}");
        }
        assert_eq!(limiter.throttled(), 0);
    }

    #[test]
    fn unknown_client_json_does_not_parse_as_a_client_message() {
        // The spoof path: a `ServerMessage`-shaped payload is not a
        // `ClientMessage`, so the reader drops it before anything is published.
        assert!(serde_json::from_str::<ClientMessage>(
            r#"{"type":"calendar_updated","date":"2026-08-29"}"#
        )
        .is_err());
        assert!(
            serde_json::from_str::<ClientMessage>(r#"{"CalendarUpdated":{"events":[]}}"#).is_err()
        );
    }
}
