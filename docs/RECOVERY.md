# Recovery runbook — when something breaks

**Audience:** whoever is standing in the kitchen when the television is blank.
**Assumed knowledge:** none. Every command below is copy-pasteable.

The hub is deliberately boring to recover: SQLite is the only source of truth, it is
backed up nightly with `VACUUM INTO`, the certificates regenerate themselves, and the
television is a browser pointed at a URL. Nothing here needs a rebuild.

## First, the four things worth knowing

| Thing | Where |
| --- | --- |
| Everything the hub owns | `%ProgramData%\FamilyHub\` — `family.db`, `uploads\`, `screensaver\`, `backups\`, `pki\`, `logs\`, `setup-code.txt` (overridden by `FAMILY_HUB_DATA_DIR`; the service logs the resolved path at startup) |
| Is the hub alive? | `http://<hub-ip>:8080/health` — JSON: `db`, `last_google_poll`, `cert_not_after`, `days_to_expiry`, `disk_free_bytes`, `ws_clients`, `uptime_seconds`, `migration_version` |
| What it has been doing | `%ProgramData%\FamilyHub\logs\familyhub.log` (rotated at 10 MB × 5), mirrored to Event Viewer → Windows Logs → Application, source `FamilyHub` |
| Service controls (elevated) | `family-hub.exe status` · `start` · `stop` · `install` · `uninstall` · `run` (foreground) · `tv-probe` · `import-curriculum <path> [--replace]` (HS1) |
| Log level | `info` by default; `$env:FAMILY_HUB_LOG` (`run` only — a shell's environment is not inherited by the installed service) or `[log]\nlevel = "debug"` in *familyhub.toml* (installed service — `stop`/`start` to apply) |

**Triage in one move:** open `http://<hub-ip>:8080/health` from a phone or laptop.
It answers → the server is fine, the problem is the television or the network
([mode 1](#failure-mode-1--the-television-is-blank-or-shows-the-fire-tv-home-screen),
[mode 5](#failure-mode-5--realtime-updates-stop-arriving)). It does not answer → the
server is the problem
([mode 2](#failure-mode-2--the-hub-is-unreachable)).

---

## Failure mode 1 — The television is blank or shows the Fire TV home screen

**Symptoms.** Dark panel, or the Fire TV launcher, or a Fully Kiosk error page.
Phones still work.

**Get it back now.**

1. Press any button on the remote. A dark panel that lights up was the hub's own
   screensaver (10 minutes idle) or a sleep timer, not a failure.
2. If Fire OS's home screen is showing: open **Fully Kiosk** from the app row. On a
   Branch B (Vega OS / Silk) device the equivalent is **"Alexa, open Silk"**, then the
   bookmark — Vega OS cannot auto-launch anything after a power cut, by design
   (`docs/PLAN.md` §7 RR-3).
3. If Fully Kiosk shows a connection error, check the server first:
   `http://<hub-ip>:8080/health`. No answer →
   [mode 2](#failure-mode-2--the-hub-is-unreachable).
4. Power-cycle the television. It should come back on the kiosk unaided.

**Then find out why.**

* **It goes dark again after 5–15 minutes.** One of the three sleep timers was never
  cleared, or an OTA update reset it. Re-apply `docs/FIRE_TV.md` Branch A steps 3 and
  5: `adb shell settings get secure sleep_timeout` must print `0`,
  `adb shell settings get system screen_off_timeout` must print `2147483647`, and the
  television's own power-saver menu must be Never. All three are independent.
* **It boots to the Fire TV home screen instead of the kiosk.** Fully Kiosk's
  launch-on-boot is vendor-disclaimed on Fire OS and fails intermittently
  (`docs/PLAN.md` §7 RR-2). Re-check Launch-on-boot and the two `appops` grants
  (`SYSTEM_ALERT_WINDOW`, `GET_USAGE_STATS`); if three consecutive reboots still fail,
  the documented answer is Branch B′ — a ~$35 Google TV box running Fully Kiosk, same
  URL, no other change (`docs/FIRE_TV.md`).
* **A hub screensaver never appears, or the wrong one does.** Fully Kiosk's own
  Screensaver is enabled and outranks the hub's. Set it to **Never**.

**Verify:** the kiosk shows today's routine, and `/health`'s `ws_clients` is at least
1 (the television's own socket).

* **The kiosk shows unstyled text, or never reacts to the remote at all.** This is not
  the same failure as "blank" above — the page loaded, but `public\` (the `dx build`
  client bundle) is missing beside `family-hub.exe`: no CSS variables resolve without
  the wasm client's own DOM setup, and with no wasm there is no WebSocket and no D-pad
  handler either. See `docs/OWNER_CHECKLIST.md` step 3 — reinstall with **both**
  `family-hub.exe` and its `public\` folder in place (`install` now refuses to run at
  all without `public\assets\*.wasm` next to the executable, so this specific failure
  can only reach the television if `public\` was deleted or moved *after* a working
  install).

---

## Failure mode 2 — The hub is unreachable

**Symptoms.** `http://<hub-ip>:8080/health` times out or refuses. The television shows
the red disconnected badge; both phones say Offline.

**Get it back now.** On the hub PC, from an **elevated** PowerShell:

```powershell
cd "C:\Program Files\FamilyHub"
.\family-hub.exe status
.\family-hub.exe start
Get-Content "$env:ProgramData\FamilyHub\logs\familyhub.log" -Tail 50
```

Then work down this list — it is ordered by how often each cause actually happens:

1. **The PC is asleep or off.** Wake it. `family-hub.exe install` sets the AC power
   plan to never sleep; if the PC slept anyway, re-run `install` from an elevated
   prompt, or set it by hand in Windows power options.
2. **The service is stopped.** `family-hub.exe start`. If it starts and immediately
   stops, the log's last 50 lines name the reason — most often a data directory that
   is missing or not writable, or port 8080/8443 already taken
   (`Get-NetTCPConnection -LocalPort 8080`).
3. **The address moved.** `ipconfig` on the PC. If the address is not the reserved
   one, the DHCP reservation (`docs/OWNER_CHECKLIST.md` step 2) was lost — restore it,
   or re-point the television's Start URL at the new address. The certificate re-issues
   itself for a new address automatically; the TV's bookmark does not.
4. **The firewall rules are gone** (a Windows feature update can drop them). Re-run
   `family-hub.exe install` elevated; it re-adds TCP 8080, TCP 8443 and UDP 5353.
   Check with `netsh advfirewall firewall show rule name=FamilyHub*`.
5. **Nothing in the log at all since the last boot.** The service is not installed
   (`sc query FamilyHub` says "does not exist"). Re-do `docs/OWNER_CHECKLIST.md`
   step 3 — and remember the binary must already be at its permanent path before
   `install` runs.

**It should recover on its own first.** `install` configures the service's SCM
failure actions (restart after 5 s, then 30 s, then 60 s, resetting after 24 h with no
further failures) and turns them on for non-crash failures too — a startup failure
(a bad bind, a corrupt database, an unreadable PKI directory) now makes the service
report `Stopped` with a non-zero exit code instead of staying `RUNNING` while serving
nothing, so a transient problem (the address briefly unavailable at boot, say) usually
clears itself within a minute or two before you need to do anything below.

**More detail than the default log shows:** for `family-hub.exe run` set
`$env:FAMILY_HUB_LOG = "debug"` (or `trace`) in that shell — see `docs/DEV_WINDOWS.md`.
For the **installed service** the shell's environment is not inherited — put
`[log]\nlevel = "debug"` in *familyhub.toml* next to `family-hub.exe` (or under the
data directory) and `family-hub.exe stop` / `start`. The default `info` level
intentionally drops `dioxus_core`/`hyper`/`sqlx` internals to keep the log from
churning its 10 MB × 5 rotation ring in under an hour of real use.

**Bridge while you debug:** `family-hub.exe run` from a normal console starts the same
server in the foreground, logging to that console. The family gets the hub back while
you sort out the service.

**Verify:** `/health` returns `"db": true` from another machine, with nobody logged in
on the PC.

---

## Failure mode 3 — The phones stop trusting the hub

**Symptoms.** `https://<hub-ip>:8443/m` shows a certificate warning; the installed PWA
opens to an error; the camera or the install prompt has stopped working. The
television is unaffected — it is deliberately HTTP-only and holds no certificate.

**Diagnose in one line:** `http://<hub-ip>:8080/health` reports `cert_not_after` and
`days_to_expiry`.

* **`days_to_expiry` is still comfortably positive.** The certificate is fine; the
  phone is the problem. The CA was never installed, or iOS installed the profile
  without the second step — *Settings → General → About → Certificate Trust
  Settings* must have the hub's root switched **on**. Re-do
  `docs/OWNER_CHECKLIST.md` step 7.
* **`days_to_expiry` is small or negative.** The leaf lives 397 days and re-issues
  itself automatically at 30 days remaining, with a hot reload and no restart — so a
  negative number means the hub was off for a long stretch or the re-issue failed.
  Restart the service (`family-hub.exe stop`, then `start`); it re-issues on startup
  when the window has passed, and logs the new validity window.
* **The hub's address changed.** The address is one of the certificate's subject
  names, so a moved hub gets a fresh leaf on the next start — restart the service and
  reload the phone.
* **Nothing above works.** Delete the four files in
  `%ProgramData%\FamilyHub\pki\` (`ca.crt`, `ca.key`, `leaf.crt`, `leaf.key`) and
  restart the service. It mints a brand-new 10-year CA and a fresh leaf. **This
  invalidates the certificate on every phone** — each one must reinstall from
  `http://<hub-ip>:8080/ca.crt`. Do this last, and never as a first guess.

The PKI directory is deliberately **excluded from backups**, so a restore never
resurrects an old key. If iOS refuses the private root outright, the planned way out
is a public certificate over DNS-01 (`docs/OWNER_CHECKLIST.md` step 13).

**Verify:** a padlock, no warning, on `https://<hub-ip>:8443/m` on each phone.

---

## Failure mode 4 — The database is corrupt, or data has gone missing

**Symptoms.** `/health` reports `"db": false`; or the log repeats
`database disk image is malformed`; or a day's routine, the calendar or the
whiteboard has vanished.

The hub takes a `VACUUM INTO` backup every night at the midnight tick — a single
self-contained file, safe to take while the database is being written to — plus a copy
of `uploads\`, keeping the **last 14** of each.

**Restore.** From an elevated PowerShell:

```powershell
cd "C:\Program Files\FamilyHub"
.\family-hub.exe stop                                   # nothing may hold the DB open

$data    = "$env:ProgramData\FamilyHub"
$backups = Join-Path $data "backups"
Get-ChildItem $backups -Filter "family-*.db" | Sort-Object Name    # newest is last

# Keep the broken file for diagnosis rather than deleting it:
Rename-Item "$data\family.db" "family.db.corrupt-$(Get-Date -Format yyyyMMdd-HHmm)"
Remove-Item "$data\family.db-wal","$data\family.db-shm" -ErrorAction SilentlyContinue

Copy-Item "$backups\family-YYYYMMDD-HHMM.db" "$data\family.db"
Copy-Item "$backups\family-YYYYMMDD-HHMM_uploads\*" "$data\uploads\" -Force

.\family-hub.exe start
```

Removing the `-wal` and `-shm` sidecars is not optional: a leftover write-ahead log
from the replaced database can otherwise resurrect rows the backup never had.

**Do not** recover by copying `family.db` out of a running hub with Explorer — a plain
file copy of a live WAL database is inconsistent, which is exactly why the nightly job
uses `VACUUM INTO`.

**Verify:** `/health` returns `"db": true` and a `migration_version`; the television
shows today's routine; the photos on it still load. You have lost at most one day —
the window between the last nightly backup and the failure.

**If every backup is bad** (a failing disk will do that): stop the service, keep the
whole `%ProgramData%\FamilyHub` folder, and start the hub with an empty data
directory. It migrates a fresh database from scratch and comes up with default
profiles; the family loses history but the hub works this morning.

---

## Failure mode 5 — Realtime updates stop arriving

**Symptoms.** The television keeps showing yesterday's — or an hour-old — state and a
red disconnected badge after ~90 seconds of silence. Phones say Offline, or a tick on
a phone never reaches the TV. `/health` still answers.

1. **Check `ws_clients` in `/health`.** Zero with a lit television means the page is
   loaded but its socket is not connected. One or more means the socket is up and the
   problem is elsewhere.
2. **Wait 30 seconds.** The client reconnects on its own with backoff
   (1, 2, 4, 8, 15, 30 s), and on reconnect the server sends a full snapshot. A badge
   that clears itself is the system working, not failing.
3. **Reload the kiosk** — in Fully Kiosk, the remote's menu → Reload, or
   `adb shell input keyevent 82`. This is safe: all state lives on the server.
4. **Check the network path.** Wi-Fi dropouts on the television are the most common
   cause by a distance; `adb shell ping -c 3 <hub-ip>` from the TV settles it.
5. **A phone shows Offline but the hub is up.** Its queued changes are safe — Settings
   lists exactly what is waiting and for which day, with *Try sending now*. On iPhone,
   replay happens on next app open (`docs/PWA.md`).
6. **Still stuck.** Restart the service (`family-hub.exe stop`, `start`). Every client
   reconnects and resyncs within 30 seconds without anyone touching them.

**Verify:** tick an item on a phone; it appears on the television within a second or
two, and `/health`'s `ws_clients` counts every connected surface.

---

## Failure mode 6 — The disk is filling up

**Symptoms.** `/health`'s `disk_free_bytes` is small; uploads start failing; backups
stop appearing in `backups\`.

The hub prunes itself — 14 backups, 30 days of photos, 2,000 whiteboard strokes,
logs at 10 MB × 5 — so a filling disk is nearly always something *else* on the PC.
Check first with `Get-PSDrive C`.

If the hub really is the culprit:

* `backups\` holds the 14 newest database files and their uploads snapshots; older
  ones are deleted after each nightly run. Deleting the oldest by hand is safe.
* `uploads\` holds photo-task images for 30 days. Deleting a task from a phone removes
  its file too.
* `logs\familyhub.log*` is capped at five 10 MB files. If it is bigger than that, the
  service has not restarted since a very chatty day — restart it.
* Screensaver images in `screensaver\` are **never** pruned; they are yours, and only
  you should remove them.

**Verify:** `disk_free_bytes` in `/health` climbs, and a photo upload succeeds again.

---

## Failure mode 7 — The parent PIN is lost

**Symptoms.** Nobody can sign in on Settings; the TV Remote tab and every parent-only
action are refused. Children's routines on the television are unaffected — the
television holds no session and needs none.

The setup code is single-use, so it cannot simply be read again. On the hub PC:

1. Stop the service (`family-hub.exe stop`, elevated).
2. Take a copy of `family.db` first — this is a data edit.
3. Clear the stored PIN so the hub falls back to first-run behaviour: with any SQLite
   client, `DELETE FROM settings WHERE key = 'parent_pin_hash';` and
   `DELETE FROM settings WHERE key = 'parent_setup_code';`.
4. Start the service. It mints a fresh setup code and writes it to the log and to
   `%ProgramData%\FamilyHub\setup-code.txt`.
5. Re-do `docs/OWNER_CHECKLIST.md` step 4 with the new code.

There is **no lockout** to wait out — repeated wrong PINs only slow the next attempt
down, because a hard lockout on a wall display would be a self-inflicted outage.

**Verify:** the new PIN signs in, and the TV Remote tab moves the television.

---

## Failure mode 8 — Photos or screensaver images do not load

**Symptoms.** Broken image placeholders on the television or a phone; `/uploads/...`
or the screensaver URLs return 404; the screensaver shows only the three shipped
placeholder pictures.

1. **404 on `/uploads/<file>`.** The row survived but the file did not — a restore
   that brought back `family.db` without its `*_uploads` snapshot does exactly this.
   Copy the matching snapshot back (mode 4), or delete the task from the phone, which
   removes the row cleanly.
2. **Screensaver shows only placeholders.** The images you dropped into
   `%ProgramData%\FamilyHub\screensaver\` are not JPEG/PNG/WebP, or are not readable
   by the account the service runs as (`LocalSystem` by default). Confirm with the
   hub's own list: `http://<hub-ip>:8080/health` proves the server is up, then open one
   image URL directly in a browser.
3. **A phone upload fails.** A **413** means the request never reached the raised
   25 MiB upload route (a proxy, or the wrong URL); a **415** means the file was not an
   allowed image type. Both are logged with the reason. The server re-encodes every
   accepted upload, so a file that arrives is always a real image.
4. **Images load on the phone but not the television.** That is a caching or network
   problem on the TV, not a server one — reload the kiosk
   ([mode 5](#failure-mode-5--realtime-updates-stop-arriving), step 3).

**Verify:** the photo opens directly in a browser at
`http://<hub-ip>:8080/uploads/<file>`, and the screensaver rotates your own pictures
after 10 idle minutes.

---

## If none of this helps

Collect, in this order: the last 200 lines of
`%ProgramData%\FamilyHub\logs\familyhub.log`; the full JSON from
`http://<hub-ip>:8080/health`; `family-hub.exe status`; and, for a television problem,
`adb shell dumpsys window | Select-String mCurrentFocus`. Those four answer nearly
every question about what the hub was doing when it stopped.

Related: `docs/OWNER_CHECKLIST.md` (the one-time install) · `docs/FIRE_TV.md` (the
television) · `docs/PWA.md` (the phones) · `docs/DEV_WINDOWS.md` (building and
running from source) · `docs/PROTOCOL.md` (what the realtime messages mean) ·
`docs/PLAN.md` (the design decisions behind all of it).
