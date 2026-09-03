# Owner verification checklist

This is **Appendix A of `docs/PLAN.md`**, delivered as its own file by T3.2 and
executed by the owner *after* the autonomous run. Every step here needs a human, a
phone, the television, a reboot or an elevated prompt — nothing an agent can do — so
**no task's automated acceptance test depends on any of it**. The software is
finished and tested; this is the physical install.

Work top to bottom: each step assumes the ones above it passed. Every step states an
explicit **pass criterion** — if it is not met, stop there rather than continuing,
and follow the *If it fails* pointer.

| Ids | This checklist maps 1→A1, 2→A2, 3→A3, 4→(new: parent PIN), 5→A4/A4′, 6→A5, 7→A6, 8→A7, 9→A8, 10→A9, 11→A10, 12→A11, 13→A12 of `docs/PLAN.md` Appendix A. |
| --- | --- |

**You will need:** the TV remote · a Windows admin prompt on the hub PC · the router's
admin page · both parents' phones · `adb` on the PC (`scoop install adb`, already
present) · about 45 minutes.

---

### 1. Confirm the Device identity

Read the television's own record: **Settings → My Fire TV → About**. Compare model
and OS to what the T0.0 probe already recorded live over `adb` on 2026-08-29:
Insignia **NS-50F301NA22**, **Fire OS 7.7.1.5** (Android 9 / API 28), IP
`10.0.0.178`, ADB debugging on, Fully Kiosk not yet installed — see `docs/device.toml`
and the `STATUS: FIRE_OS` line of `docs/FIRE_TV.md`.

**Pass criterion:** the model and OS shown in TV Settings match `docs/device.toml`,
so `docs/FIRE_TV.md`'s promoted **Branch A** is the right branch. Only re-do this row
if the physical television is ever replaced.

*If it fails:* the TV has been replaced or updated to a different family. Re-run
`family-hub.exe tv-probe` (or set `FAMILY_HUB_TV_IP` to the new address), then pick
the branch in `docs/FIRE_TV.md` that matches what About reports — Branch A for any
Fire OS with ADB, Branch B for a Vega OS stick. Treat the promoted branch as stale
until the probe and About agree.

### 2. Reserve the hub PC's IP address on the router

On the router's admin page, add a **DHCP reservation** binding the hub PC's MAC
address to a fixed address on `10.0.0.0/24`. Write that address into
`docs/device.toml` under `[server] ip`. Everything else in this checklist uses it as
`<hub-ip>`.

This is a prerequisite, not a nicety: the kiosk URL, the QR code and the certificate's
subject names are all built from this address. The certificate is re-issued
automatically if the address ever changes, but the television's bookmark is not.

**Pass criterion:** reboot the router; the PC comes back on the **same** address
(`ipconfig` on the PC, or the router's client list).

*If it fails:* some consumer routers only honour a reservation after the existing
lease expires — release it (`ipconfig /release` then `/renew` on the PC) and re-check.

### 3. Install the hub as a Windows service — **elevated**

Put **both** of these at the hub's **permanent** location (for example
`C:\Program Files\FamilyHub\`) before installing:

* `family-hub.exe`, from `cargo build --release --features server --bin family-hub`.
* The `public\` folder from `dx build --platform web --release`
  (`target\dx\family-calendar\release\web\public`), copied in **beside** the exe so
  the result is `C:\Program Files\FamilyHub\public\assets\*.wasm`.

`family-hub.exe` alone renders `/tv` and `/m` unstyled and inert — no wasm client, no
WebSocket, no D-pad handling — because it has no `dx`-rewritten manifest of its own;
`install` **refuses to run** (a `NotFound` error, logged) unless
`public\assets\*.wasm` already exists next to the executable it is registering, so a
missing bundle is caught here rather than discovered later on the television. The
prebuilt CI artifact (`FamilyHub-windows-x64`) already contains both pieces laid out
this way.

The installer registers the service against `std::env::current_exe()`, so moving the
file afterwards leaves a service pointing at a path that no longer exists.

Then, from an **elevated** PowerShell (Run as administrator):

```powershell
cd "C:\Program Files\FamilyHub"
.\family-hub.exe install
.\family-hub.exe start
.\family-hub.exe status
```

`install` also opens three inbound firewall rules (TCP 8080, TCP 8443, UDP 5353 for
mDNS) via `netsh`, and sets the AC power plan to never sleep, hibernate or turn the
display off via `powercfg`. Both are best-effort: a failure is written to the log and
does not abort the install.

Now **reboot the PC** and do not log in.

**Pass criterion:** with nobody logged in, `http://<hub-ip>:8080/health` from another
machine returns JSON containing `"db": true` and a `migration_version`. `sc query
FamilyHub` shows `STATE: 4 RUNNING`, and `netsh advfirewall firewall show rule
name=FamilyHub*` lists the three rules.

*If it fails:* read `%ProgramData%\FamilyHub\logs\familyhub.log` (and Event Viewer →
Windows Logs → Application, source `FamilyHub`) — the service logs its resolved data
directory and every startup failure. Then `docs/RECOVERY.md`, "The hub is
unreachable".

### 4. Set the parent PIN

The hub ships with no PIN. On first run it writes a one-time **setup code** to the
log and to `%ProgramData%\FamilyHub\setup-code.txt`. Open
`https://<hub-ip>:8443/m` on a phone (step 7 installs the certificate that makes this
pleasant; before that the browser will warn), go to **Settings**, enter the setup code
and choose a **six-digit** PIN.

**Pass criterion:** the PIN is accepted; Settings then shows the parent as signed in,
and the **TV Remote** tab's buttons work. A wrong PIN is refused with an increasing
delay and never locks the family out.

*If it fails:* the setup code is single-use. If it has already been claimed and the
PIN is lost, see `docs/RECOVERY.md`, "The parent PIN is lost".

### 5. Put the television into kiosk mode

Follow `docs/FIRE_TV.md` **Branch A** end to end: `adb connect`, sideload Fully Kiosk
Browser ≥ 1.61.2 with the PLUS licence (€8.90 / $10.99 one-off), grant
`SYSTEM_ALERT_WINDOW` and `GET_USAGE_STATS` over `adb`, `settings put secure
sleep_timeout 0`, Fully Kiosk's own **Screensaver → Never**, Start URL
`http://<hub-ip>:8080/tv`, Launch-on-boot on — and, because this is a *television*
rather than a stick, disable the **TV's own sleep / power-saver timers** (the step
that replaces the usual "turn HDMI-CEC off"; CEC only applies under Branch B′).

*Branch B alternative (a Vega OS device, no sideloading):* open **Silk**, browse to
`http://<hub-ip>:8080/tv`, bookmark it and set it as the start page. Pass criterion
becomes: "Alexa, open Silk" reaches the kiosk in one step, and you accept a manual
relaunch after a power cut.

**Pass criterion (Branch A):** **three consecutive TV reboots** each land on the
kiosk showing today's routine, with **no remote interaction**, and the screen is still
lit 20 minutes later.

*If it fails:* `docs/FIRE_TV.md` → Troubleshooting. A repeated boot-launch failure is
the known vendor risk RR-2; the documented answer is Branch B′, not more tuning.

### 6. Drive the whole TV UI with the real remote

With the remote only: change profile (Up/Down), change panel (Left/Right), tick and
untick routine items (Enter), go Back (Backspace), dismiss the screensaver, open the
join-QR overlay. Then hand the remote to a child. Append `?keys=1` to the kiosk URL
to display the live key-code overlay, and note any button that reports a code the
shipped map does not handle.

**Pass criterion:** a child completes an entire morning routine using only the remote,
without help and without a phone. Every focusable thing shows the thick focus ring
when it is selected.

*If it fails:* record the codes shown by `?keys=1` for the buttons that misbehave —
that report is exactly what a fix needs. Drawing, photo capture and calendar editing
are *deliberately* not on the remote (`docs/PWA.md`).

### 7. Install the hub's certificate on each phone

Browse to `http://<hub-ip>:8080/ca.crt` on the phone and install it.
**Android:** the download prompts to install a CA certificate.
**iPhone:** Safari downloads a profile — install it under *Settings → Profile
Downloaded*, then additionally enable it under *Settings → General → About →
Certificate Trust Settings*. The second step is separate and easy to miss; without it
nothing changes.

**Pass criterion:** `https://<hub-ip>:8443/m` loads with a padlock and no warning, on
every phone, and the browser offers to add it to the home screen.

*If it fails:* `docs/RECOVERY.md`, "The phones stop trusting the hub". If iOS refuses
the private root outright, the planned escape hatch is a public certificate over
DNS-01 — step 13.

### 8. Install the PWA on both phones

**Android (Chrome):** ⋮ → *Install app* / *Add to Home screen*.
**iPhone (Safari):** Share → *Add to Home Screen*. Launch it from the home-screen
icon, not from the browser.

**Pass criterion:** the app opens standalone — no address bar, no browser chrome —
and shows the six tabs (Routine, School, Calendar, Board, Remote, Settings).

*If it fails:* the manifest is served from the site root on purpose; a warning about
scope means something is proxying the request. See `docs/PWA.md`, "No install prompt".

### 9. Prove the offline queue with airplane mode

Open the PWA, turn airplane mode **on**, tick a routine item, wait a few seconds, turn
airplane mode **off**, and watch the television.

**Pass criterion:** the tick appears on the TV within 10 seconds of reconnecting, and
it is recorded against **the day you made it**, not the day it arrived. Doing it twice
with the same tick changes nothing the second time.

*If it fails:* on iPhone, replay happens on **next app open** — reopen the app and
watch again; that is the documented platform difference (`docs/PWA.md`, iOS). If a
queued change is older than 48 hours the app discards it and tells you which day it
belonged to.

### 10. Upload a real photo from a phone camera

In Routine, add a photo task and take a full-resolution photo (12 MP or better) with
the phone's own camera on 5 GHz Wi-Fi.

**Pass criterion:** the upload completes in **under 3 seconds**, the stored file in
`%ProgramData%\FamilyHub\uploads\` is **≤ 400 KB**, and the photo appears on the
television without a reload.

*If it fails:* a 413 means the request never reached the raised-limit upload route; a
415 means the file was not a JPEG/PNG/WebP. Both are logged with the reason.

### 11. Drop real screensaver photos in

Copy family photos into `%ProgramData%\FamilyHub\screensaver\` (or upload them from
the phone), then leave the television alone.

**Pass criterion:** after **10 minutes** of no remote activity the screensaver starts
and rotates through the new photos; any remote press dismisses it and returns to the
routine.

*If it fails:* the three shipped placeholder images prove the path itself works — if
they rotate and yours do not, the new files are not JPEG/PNG/WebP or are not readable
by the service account.

### 12. Pull the network cable for five minutes

Unplug the hub PC's network cable (or the router). Watch the television for a few
minutes, then plug it back in.

**Pass criterion:** the TV shows the red disconnected badge after about 90 seconds,
keeps displaying the last known routine rather than an error page, and **recovers
within 30 seconds** of the network returning — with no reload and no remote press.

*If it fails:* `docs/RECOVERY.md`, "The hub is unreachable" and "Realtime updates stop
arriving".

### 13. *(Optional)* Switch to a public certificate over DNS-01

Only if an iPhone refuses the private root, or you would rather not install a
certificate on every new phone: buy a domain, add a DNS API token, and set
`certs.mode = "acme_dns01"` in the hub's *familyhub.toml* configuration file (it
lives in the data directory and is created by hand — the hub runs happily without
one). This is Phase 4 work (P4.2) — the
`CertSource` seam is already in place, but the feature is not part of this run.

**Pass criterion:** phones trust `https://<hub-ip>:8443/m` with **no CA installed**,
after which the hub's CA can be removed from every phone.

*If it fails:* nothing is lost — the private CA path from step 7 keeps working; leave
`certs.mode` as it was.

### 14. Update to the School build and load the year's curriculum

The School tab (🏠) ships in a later build than the one step 3 installed. Rebuild
and re-copy both pieces exactly as step 3 describes (`family-hub.exe` from
`cargo build --release --features server --bin family-hub`, and the `public\`
folder from `dx build --platform web --release`, laid out beside it), then
place the year's curriculum file — transcribed from your own printed plan, per
`docs/homeschool/CURRICULUM_FORMAT.md` — at
`%ProgramData%\FamilyHub\curricula\ao-year-1.toml` (create the `curricula\`
folder if it is not already there; `FAMILY_HUB_CURRICULA_DIR` overrides this
default location if you would rather keep it elsewhere). The loader only reads
this folder at boot, so the file must be in place *before* the reinstall below
restarts the service — copying it in afterwards needs another restart.

From an **elevated** PowerShell (Run as administrator), the same re-install
procedure step 3 above used:

```powershell
cd "C:\Program Files\FamilyHub"
.\family-hub.exe stop
# copy the new family-hub.exe and public\ over the old ones here
.\family-hub.exe install
.\family-hub.exe start
```

**Pass criterion:** `https://<hub-ip>:8443/health` (or `http://<hub-ip>:8080/health`)
returns JSON containing `"curricula": 1`, and opening `/m` on a phone shows a
new **School** tab between Routine and Calendar with this year's plan on it.

*If it fails:* `"curricula": 0` means the loader never saw the file — check it
really landed at the path above (not a subfolder, not renamed) and that the
service was restarted after it arrived; `%ProgramData%\FamilyHub\logs\familyhub.log`
logs the resolved curricula directory and any file it rejected, with a reason,
at startup. See `docs/RECOVERY.md` failure mode 1 if the reinstall itself
leaves `/tv` or `/m` unstyled — the same "both pieces beside each other"
requirement step 3 already covers.

**One glance while you are at the TV:** open the whiteboard and check whether the
strokes the family had drawn are still there. Before the HS9 build, three of the
project's own test suites, when run in a shell that had not set `FAMILY_HUB_DATA_DIR`,
wrote to and then cleared the *live* `whiteboard_strokes` table (`docs/HANDOFF.md`
H-HS9-1). Nothing else — calendar, routines, profiles, homeschool rows — was touched, so
a blank board just means redrawing it; the HS9 build closes the hole.

---

## When all of it passes

The household is live: the television runs itself, both parents have the app, and the
hub survives a reboot and a network outage unattended. Keep this file — steps 1, 3, 5
and 7 are the ones you will repeat if the TV, the PC, or a phone is ever replaced.

Related: `docs/FIRE_TV.md` (the television, in detail) · `docs/PWA.md` (the phones) ·
`docs/RECOVERY.md` (when something breaks later) · `docs/DEV_WINDOWS.md` (rebuilding
from source) · `docs/PLAN.md` (why any of it is the way it is).
