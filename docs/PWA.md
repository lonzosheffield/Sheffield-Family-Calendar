# The phone app (PWA) — what it promises, per platform

**Owner:** task T2.2 (PLAN v2 §3 / `docs/reviews/PURPLE_TEAM.md` §P3 row T2.2).
**Audience:** the two parents, and whoever maintains this next.
**Scope line (§P5.5 default 35):** the television is self-sufficient for a
child's routine. The phone is the parents' controller and admin surface —
drawing, photo capture, calendar editing and administration are phone-only
(assumption A3: only mom and dad have phones).

---

## Installing it

1. Join the home Wi-Fi.
2. Install the hub's certificate once, from `http://<hub-ip>:8080/ca.crt`
   (`docs/OWNER_CHECKLIST.md` step 7; on iPhone also switch it on under
   *Settings → General → About → Certificate Trust Settings*).
   Without it the phone has no secure context, and with no secure context
   there is no install prompt, no service worker and no camera.
3. Open `https://<hub-ip>:8443/m`.
4. **Android (Chrome):** *⋮ → Add to Home screen / Install app.*
   **iPhone (Safari):** *Share → Add to Home Screen.*

The install is driven by two files served from the **root** of the origin, not
from the hashed asset pipeline:

| URL | What it is |
| --- | --- |
| `/manifest.webmanifest` | `scope: "/"`, `start_url: "/m"`, `display: standalone`, four icons (192 and 512, plain and maskable) |
| `/sw.js` | the service worker: app-shell precache, network-first for server functions, cache-first for photos and screensaver images |

Those paths never carry a content hash. That is not cosmetic: a manifest
served from `/assets/<hash>-manifest.json` has an implicit scope of
`/assets/`, which does not contain `start_url: "/m"`, and the browser then
refuses to offer the install at all — the exact defect (G6/R-16) this task
fixed. `tests/pwa_tests.rs` fails if it is ever reintroduced.

---

## What works with the hub unreachable

The app shell, the last-seen routine, calendar and photos render from the
service worker's cache. Anything you **change** while offline is not lost and
not guessed at: it is written to a queue in the phone's own storage, and each
queued change carries two things.

* **The date it was meant for.** A step ticked at 23:58 belongs to the 29th
  even if it reaches the hub at 00:05 on the 30th. The hub re-checks that date
  and refuses anything more than a day away from its own clock.
* **An idempotency key,** minted once when the change is made and never
  regenerated. The hub claims each key exactly once, so a change that gets
  delivered twice still only happens once.

Queued changes are kept for **48 hours**. Past that they are discarded — a
two-day-old tick is about a day the family has moved on from, and the hub
would reject it anyway — and the app tells you, by name of the day, that it
discarded them. It never drops one quietly.

Only the two routine toggles are queued. A photo upload is not: a photo is too
large to hold in phone storage and is a foreground action you can simply
repeat.

---

## Android

**Promise: changes made offline are sent again automatically on reconnect**,
as soon as the phone is back on the home Wi-Fi with the app open or in the
foreground — usually within a second or two. You do not have to do anything.

Chrome does support Background Sync, which could in principle push the queue
out with the app fully closed. The hub deliberately does **not** use it. The
queue is a Rust structure in the app's own storage, which a service worker
cannot read; using Background Sync would mean a second copy of the queue, in
IndexedDB, with a second copy of the replay rules written in JavaScript — two
implementations to keep in step, and a different guarantee for each parent
depending on which phone they carry. One replay path, driven by the Rust
client, behaves identically on both platforms and is the one covered by tests.

---

## iOS

**Promise: changes made offline are replayed on next app open** — the next
time you open the app, not while it is closed.

iOS Safari does not implement Background Sync at all, so nothing on an iPhone
can replay a queued change with the app shut. Opening the app reconnects the
socket, which is what triggers the replay, so in practice the delay is however
long the phone stays in your pocket. The 48-hour window is generous enough
that an overnight gap is never a problem.

Two other iOS specifics worth knowing:

* The private root certificate must be trusted (step 2 above) or Safari gives
  the page no secure context, and without that there is no service worker and
  no *Add to Home Screen* prompt. If iOS ever refuses the private root
  outright, the escape hatch is a public certificate via DNS-01 — planned as
  P4.2, and the `CertSource` trait is already in place for it.
* Installed on the home screen, the app runs without browser chrome; the tab
  bar sits above the home indicator via `viewport-fit=cover` and the safe-area
  inset.

---

## The five tabs

| Tab | What it does |
| --- | --- |
| **Routine** | The selected child's morning routine and extra tasks; add a photo task |
| **Calendar** | Today and the week ahead, in the hub's local time |
| **Board** | The shared whiteboard, the same one the TV shows |
| **TV Remote** | Puts a panel or a profile on the television (`SetView`, `SetActiveProfile`) |
| **Settings** | Parent sign-in, what is queued offline, install help |

The TV Remote needs a parent session: sign in with the six-digit PIN under
Settings first. The hub enforces that itself — an unsigned phone's remote
button reaches the server and is dropped there, never applied.

---

## If something looks wrong

* **No install prompt.** The certificate is not trusted yet, or the page was
  opened over `http://` rather than `https://`. The HTTP origin deliberately
  redirects `/m`, `/manifest.webmanifest` and `/sw.js` to HTTPS.
* **Old content after an update.** The service worker is served with
  `Cache-Control: no-cache` and takes over immediately, but a fully closed
  installed app may need one relaunch.
* **A change did not appear on the TV.** Check the connection dot at the top
  right of the phone. If it says Offline, the change is queued — Settings
  shows exactly what is waiting and for which day, with a *Try sending now*
  button.

Related: `docs/FIRE_TV.md` (the television), `docs/OWNER_CHECKLIST.md`
(one-time setup on real devices — steps 7–9 are the phone ones),
`docs/RECOVERY.md` (what to do when a phone stops trusting the hub, or a
queued change never arrives), `docs/PROTOCOL.md` (the realtime messages the
remote sends), `docs/DEV_WINDOWS.md` (building and serving it).
