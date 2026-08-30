# Realtime protocol v2

**Status:** normative for `/ws`. Landed by **T1.2**; the specification it
implements is `docs/reviews/PURPLE_TEAM.md` §P2c (decision **D5** in
`docs/PLAN.md` §2). T2.3 (whiteboard), T2.1 (kiosk), T2.2 (phone PWA), T1.4
(sessions), T1.5 (dates/authorization), T1.7 (`/health`) and T2.4 (calendar)
all implement against this document.

Everything here is enforced by `tests/realtime_tests.rs`, the T1.2 acceptance
suite. Two tests in that file also assert **this document** stays in step: one
does a compile-time exhaustive match over both enums and requires every variant
to be named below, the other requires the numeric limits to appear.

---

## 1. Shape

Protocol v1 had a single `WsMessage` enum that travelled in both directions,
and the server rebroadcast whatever a client sent, verbatim, to everybody else.
That is why a phone could forge a `CalendarUpdated` (**G13**), why every client
drew its own strokes twice (**W2**), and why one `pointermove`-per-message
whiteboard could bury the broadcast channel and get the TV's socket closed
(**G20**).

v2 splits the enum in two and makes the server the only author of anything that
is fanned out.

```
                      broadcast(capacity 1024)          OutboundQueue(256, drop-oldest)
  publish(ServerMessage) ───────────────► fan-out ──────────────────────────────► writer ──► client
                                             │ Lagged ⇒ resubscribe + Resync         ▲
                                             └───────────────────────────────────────┘

  client ──► reader ──► token bucket (40/s, burst 80) ──► validate ──► authorise ──► publish()
```

* **`ClientMessage`** — the only thing a browser may send. Anything that does
  not parse as one is dropped with a `tracing::warn` and reaches nobody.
* **`ServerMessage`** — the only thing the server sends. It is always minted by
  the server from validated inputs; client bytes are never forwarded.

Both enums are serialised with serde's internal tag `"type"` in
`snake_case`, e.g. `{"type":"ping","nonce":7}`.

### Protocol version

`PROTOCOL_VERSION = 2`, carried in both `Hello` messages. A mismatch is logged
on both sides but is not fatal — a stale kiosk keeps working with whatever it
understands until it is reloaded.

---

## 2. `ClientId`

* Minted **by the server** at upgrade — a v4 UUID, rendered as text.
* Returned to the client in `ServerMessage::Hello` as `client_id`.
* Stamped by the server onto every `Draw` and `BoardCleared` it fans out, as
  `origin`.
* A client discards any message whose `origin` equals its own id: it has
  already painted that stroke locally. This is `client::realtime::is_own_echo`.

Because the id is minted server-side and never read from the wire, one client
cannot claim another's identity — which is the difference between v2's echo
suppression and a client-supplied "sender" field.

`ClientId` is a `String` rather than a `uuid::Uuid` because `uuid` is a
server-only optional dependency and `shared/types.rs` also compiles to wasm.
The wire representation is identical.

The same reasoning applies to timestamps: `server_time` is an RFC 3339 string
and every `date`/`today` field is a `YYYY-MM-DD` string, which is byte-for-byte
what `chrono`'s `DateTime<Local>` and `NaiveDate` serialise to.

---

## 3. `ClientMessage`

| Variant | Meaning | Authorisation |
| --- | --- | --- |
| `ClientMessage::Hello { protocol }` | Announce the client and ask for its id. Sent on every (re)connection. | none |
| `ClientMessage::Ping { nonce }` | Heartbeat, every **20 s**. | none |
| `ClientMessage::Draw { board_id, stroke }` | One **whole stroke** (many points) — see §5. | none |
| `ClientMessage::ClearBoard { board_id }` | Move the board's `cleared_at` watermark. | none |
| `ClientMessage::SetView { view, auth }` | Ask the TV to show a panel. | valid parent session |
| `ClientMessage::SetActiveProfile { user_id, auth }` | Ask the TV to switch profile. | valid parent session, **or** `user_id` equals the profile this connection already owns |
| `ClientMessage::RequestSnapshot { board_id, since_seq }` | Ask for the board state after `since_seq`. | none |

`board_id` is always `1`: there is exactly one whiteboard (PURPLE §P5.5 default
15; named boards are cut). Any other value is dropped with a warning.

A `Draw` with zero points, or with more than **4,096**, is dropped.

### Authorisation

`SetView` and `SetActiveProfile` are the only client messages that can change
what the TV displays, so they are the only ones that need a credential. The
server checks `auth` against `server::api::realtime::session::is_valid`.

`session` is a seam: today it is a process-local set of issued tokens, and
**T1.4 replaces `is_valid` with the argon2id-backed 30-day parent session**
from `src/server/auth.rs`. Nothing else in the hub changes when it does.

A connection's "own profile" is set only by an **authorised**
`SetActiveProfile`, so an unauthenticated client can re-assert a profile it was
granted but can never take one it was not (R-23b).

---

## 4. `ServerMessage`

| Variant | Meaning |
| --- | --- |
| `ServerMessage::Hello { client_id, protocol, server_time, today }` | First frame on every connection, and the reply to `ClientMessage::Hello`. `today` is the server's local date — the client never uses the device clock. |
| `ServerMessage::Pong { nonce }` | Heartbeat reply, echoing the nonce. |
| `ServerMessage::Resync { reason }` | "You have missed messages; refetch." `reason` is `Lagged`, `ServerRestart` or `ClientRequested`. **Never** accompanied by a close. |
| `ServerMessage::Draw { board_id, seq, origin, stroke }` | A stroke, stamped with the server's sequence number and its author. |
| `ServerMessage::BoardCleared { board_id, seq, origin }` | The board was cleared at `seq`. |
| `ServerMessage::Snapshot { board_id, seq, strokes }` | Board state in `seq` order, in reply to `RequestSnapshot`. |
| `ServerMessage::RoutineUpdated { user_id, date }` | A routine item changed. Scoped, so a client refetches **only** that profile and day (W7). |
| `ServerMessage::TasksUpdated { user_id, date }` | A custom task changed. Same scoping (W1/G22). |
| `ServerMessage::ProfilesUpdated` | The profile roster changed (W6). Unscoped by design: it affects everybody. |
| `ServerMessage::CalendarUpdated { date }` | Calendar data for `date` changed; clients refetch. The payload itself is **not** pushed. |
| `ServerMessage::DayRolled { date }` | Local midnight passed; `date` is the new today (F29/G4). |
| `ServerMessage::SetView { view }` | The TV should show this panel. |
| `ServerMessage::SetActiveProfile { user_id }` | The TV should switch to this profile. |
| `ServerMessage::Health { stale, last_update }` | Freshness signal for the TV's badge (T1.7). |

### What a client does on `Resync`

1. Bump every version signal (routine, tasks, profiles, calendar).
2. Re-send `RequestSnapshot { board_id: 1, since_seq: 0 }`.
3. Refetch routine/tasks/calendar for the current date.

It does **not** tear down the socket.

---

## 5. Strokes and batching

A `Stroke` is `{ points: [{x, y}, …], color, width }`, with coordinates
normalised to `0.0..=1.0` so a 4 K television and a phone agree on geometry.
One stroke is one message and, once T2.3 lands, one database row.

Client behaviour (`client::realtime::StrokeBatcher`):

* `pointerdown` opens a stroke.
* `pointermove` paints locally and appends to a buffer. It **does not send**.
* Points closer than **0.002** normalised units to the previous accepted point
  are dropped.
* A flush emits at most one `ClientMessage::Draw` per **34 ms** frame — a hard
  cap of **30 messages/second** per client. Frames that arrive sooner coalesce
  into the next flush.
* Each flush is anchored to the last point of the previous flush, so the
  rendered line is continuous across flush boundaries.
* `pointerup`/`pointerleave` force a final flush and close the stroke.

Ten seconds of scribbling therefore produce ≤ 300 messages of ~20 points, not
~6,000 single-segment messages.

### Sequencing

The server assigns a monotonically increasing `seq` to every `Draw`.
`Snapshot` returns live strokes with `seq > since_seq`, in order. The live
board retains the most recent **2,000** strokes (PURPLE §P5.5 default 15);
`ClearBoard` empties it by stamping every live row's `cleared_at` watermark
(below).

**T2.3 replaced the in-memory store** in `server::api::realtime`
(`record_stroke`, `clear_board`, `snapshot`) with the `whiteboard_strokes`
table from T1.1's `0002_core` migration (`docs/HANDOFF.md` H-10). The three
signatures and the `seq` contract above are preserved, with two deliberate
refinements, both explained in full in `record_stroke`'s own doc comment in
`realtime.rs` (and `docs/HANDOFF.md` H-21):

* **A `Draw`'s `seq` is minted from an in-process counter, not the database,
  and the row is written by a detached task the publish does not wait for.**
  T1.2's own load test (8 clients × 30 msg/s × 30 s, p99 fan-out latency under
  250 ms) measured 759 ms p99 when publishing waited on a transactional
  insert over the single write connection — SQLite has exactly one writer,
  and 240 durable transactions/second leaves no room for that budget. `seq`
  is still unique and strictly increasing the instant it is minted; a reader
  (`snapshot`) only ever reports what has actually committed, so nothing
  fabricates or reorders a stroke — a client just might see one painted a
  few milliseconds before it is durable, which a live whiteboard's fan-out
  was always going to do anyway (R-06/G20).
* **`ClearBoard`'s `seq` is the board's current high-water mark, not a
  freshly minted one.** A clear does not draw anything, so it has nothing to
  allocate a sequence number *for*. The alternative (inserting a tombstone
  row just to bump a counter) would add a stroke that does not exist for no
  behavioural gain: a client reacts to receiving `BoardCleared` itself, not
  to its `seq` field changing.

### `cleared_at` and undo

`ClearBoard` never deletes a row; it stamps `cleared_at` on every currently
live one (`whiteboard_strokes.cleared_at`, `db::clear_board`). `board_snapshot`
only ever returns rows where `cleared_at IS NULL`, so the very next `Snapshot`
is empty while the cleared rows still exist on disk — for T1.6's retention
sweep (`db::compact_board`, exposed as `realtime::compact_board`) to hard-
delete on the midnight tick, per `docs/HANDOFF.md` H-10's note that undo and
compaction share one rule: **only ever remove what is provably safe**, and
prefer a watermark to a delete wherever a client might still be relying on
what "used to be there."

Undo-own-last-stroke (PURPLE §P3 T2.3c) follows the same instinct but has no
wire message of its own: `ClientMessage` carries no `Undo` variant. It is a
plain `#[server]` fn instead — `api::whiteboard::undo_last_stroke(client_id)`,
calling `db::undo_last_stroke` (T1.1), which deletes **only** the most recent
live row whose `client_id` column matches the caller's own server-minted
[`ClientId`](#2-clientid) — never another connection's. On an actual removal
the fn republishes a fresh `ServerMessage::Snapshot` of the whole board, which
every already-connected client (the undoer included) applies exactly the way
it applies a `Resync`'s re-requested one: clear the canvas, replay in `seq`
order. No new message shape was worth adding for something this infrequent.

---

## 6. Flow control

| Limit | Value | Where |
| --- | --- | --- |
| Broadcast channel capacity | **1024** | `realtime::BROADCAST_CAPACITY` |
| Per-connection outbound queue | **256**, **drop-oldest** | `realtime::OUTBOUND_QUEUE_CAPACITY` |
| Dropped frames before the backlog collapses to one `Resync` | **32** | `realtime::OUTBOUND_DROPPED_BEFORE_RESYNC` |
| Inbound token bucket | **40** msg/s, burst **80** | `realtime::RATE_LIMIT_PER_SECOND` / `RATE_LIMIT_BURST` |
| Consecutive over-budget seconds before the offender is closed | **3** | `realtime::RATE_LIMIT_OVER_BUDGET_SECONDS` |
| Client flush cap | **30** msg/s (one per 34 ms frame) | `client::realtime::FLUSH_INTERVAL_MS` |
| Heartbeat | client `Ping` every **20 s**; dead after **2** missed `Pong`s (≥ 45 s) | `client::realtime::HEARTBEAT_INTERVAL` |
| Server idle timeout | **90 s** with nothing received | `realtime::CLIENT_IDLE_TIMEOUT` |
| Retained strokes | **2,000** | `realtime::MAX_RETAINED_STROKES` |

### `RecvError::Lagged` is never fatal

This is the single most important rule in the document, and the direct fix for
**G20** (v1 closed the socket, so a scribbling child bricked the TV until it
was power-cycled):

```rust
Err(RecvError::Lagged(n)) => {
    tracing::warn!("client {client_id} lagged {n} messages; resyncing");
    receiver = sender().subscribe();          // drop the stale backlog
    outbound.push(Resync { reason: Lagged }); // …and tell the client
    continue;                                 // NEVER break
}
```

The bounded outbound queue reaches the same conclusion from the other side: a
client that is not draining its socket loses its oldest frames rather than
blocking the fan-out task, and once 32 have been dropped the whole backlog is
replaced by one `Resync`.

Over-budget **inbound** traffic is the one case that does close a socket — and
only the offender's. Three consecutive seconds over the token bucket earn a
`Resync` and a close for that connection alone; every other client is
untouched.

---

## 7. Reconnection

The send half of the socket is owned by a **supervisor** that outlives any one
connection, so the outbound queue can no longer be orphaned when the read half
dies (**G3**).

Backoff schedule, `client::realtime::backoff`:

```
1 s, 2 s, 4 s, 8 s, 15 s, 30 s, 30 s, …      ±20 % jitter, capped at 30 s
```

Jitter is seeded per client, so a room full of devices coming back after a
power cut does not stampede. A connection that stays healthy for **60 s**
resets the attempt counter to zero.

On every successful reconnect the client sends `ClientMessage::Hello`, receives
its new `ClientId`, sends `ClientMessage::RequestSnapshot`, and bumps every
version signal.

---

## 8. The midnight tick

```rust
loop {
    let now  = Local::now();                       // recomputed every iteration
    sleep(duration_until_midnight(&now)).await;    // .earliest() — DST-safe
    let today = Local::now().date_naive();
    run_day_rolled(today).await;                   // DayRolled + registered hooks
}
```

`next_midnight` resolves local midnight with chrono's `.earliest()`, which is
deterministic when a transition skips or repeats it, and recomputing `now`
every iteration means no drift accumulates and 23- and 25-hour local days are
handled by construction (**F29**, **G4**).

Beyond broadcasting `ServerMessage::DayRolled`, the tick runs every hook
registered through `realtime::on_day_rolled`:

* **T2.4** registers a forced calendar poll (W4).
* **T1.6** registers the retention sweep — stroke compaction, photo retention,
  log rotation, `wal_checkpoint(TRUNCATE)`.

The tick is started once per process by `realtime::ensure_background_tasks`,
called from the `/ws` handler. `docs/HANDOFF.md` asks the Boss to also call
`realtime::spawn_midnight_tick()` explicitly from `server::router::run`, which
T1.2 does not own.

---

## 9. Module map

| File | Contents |
| --- | --- |
| `src/shared/types.rs` | `ClientMessage`, `ServerMessage`, `Stroke`, `ClientId`, `ResyncReason`, `PROTOCOL_VERSION`, `DEFAULT_BOARD_ID` |
| `src/server/api/realtime.rs` | the hub: fan-out, outbound queue, token bucket, board store, session seam, midnight tick |
| `src/client/realtime.rs` | `RealtimeBus`, the reconnect supervisor, `backoff`, `StrokeBatcher`, `is_own_echo` |
| `tests/realtime_tests.rs` | the T1.2 acceptance suite |
