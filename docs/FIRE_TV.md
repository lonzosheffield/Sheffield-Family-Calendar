STATUS: FIRE_OS

# Fire TV kiosk runbook

**Probe by T0.0 (2026-08-29, live `adb`). Finalised by T3.2 at the end of the run.**
T3.2 added the end-to-end runbook, the verification and troubleshooting sections and
the cross-links; it did not change the `STATUS` line, the probe table, or remove any
branch. All three kiosk branches stay published so the runbook survives the
television being replaced.

The hub does not care which branch you are on. Because of decision D3′
(`docs/PLAN.md` §2) the television is a **plain HTTP client**: it loads one URL,
opens one WebSocket and draws on one canvas. No certificate, no service worker, no
install, no app. Choosing a kiosk shell is therefore a runbook decision with **zero
code impact** — switching branches costs a bookmark.

**Kiosk URL of record:** `http://<hub-ip>:8080/tv`
(`<hub-ip>` = the server PC's DHCP-reserved address — `docs/OWNER_CHECKLIST.md`
step 2. `http://familyhub.local:8080/tv` also works from phones, but **not** from
Fire OS 7/8, which does not resolve mDNS names; always use the raw IP on the TV.)

| I want to… | Go to |
| --- | --- |
| Set the TV up for the first time | [Branch A](#branch-a--fire-os-fully-kiosk-browser--detected-promoted) below, then `docs/OWNER_CHECKLIST.md` steps 1 and 4–6 |
| Fix a TV that is blank, frozen or stuck on the Fire TV home screen | `docs/RECOVERY.md` |
| Understand what the remote can and cannot do | [Scope line](#what-the-remote-can-do) below, `docs/PWA.md` |
| Set a phone up instead | `docs/PWA.md` |
| Work on the code | `docs/DEV_WINDOWS.md` |

---

## Probe result (2026-08-29, live `adb`)

Source of the IP: `docs/device.toml` (`10.0.0.178`, DHCP lease at time of writing —
`docs/OWNER_CHECKLIST.md` step 2 asks the owner to reserve the **server's** address;
the TV's own lease may float harmlessly). `adb connect 10.0.0.178:5555` succeeded
(ADB debugging was already enabled on the device per the owner). Properties pulled
with `adb shell getprop` / `adb shell dumpsys package` / `adb shell settings get`:

| Property | Value |
| --- | --- |
| `ro.product.manufacturer` | `Amazon` |
| `ro.product.model` | `AFTDCT31` |
| `ro.product.device` | `duckie` |
| `ro.build.version.release` (Android) | `9` |
| `ro.build.version.sdk` (API) | `28` |
| `ro.build.version.name` | `Fire OS 7.7.1.5 (PS7715/5585)` |
| `ro.build.id` | `PS7715.5585N` |
| `com.amazon.webview.chromium` versionName | `138.amazon-webview-v138-7204-tv-r29_06_09_2026.7204.244.214` |
| `com.amazon.cloud9` (Silk) versionName | `138.16.11.0.7204.244.30` |
| `secure sleep_timeout` (current) | `840000` ms (14 min) — runbook step sets this to `0` |
| `system screen_off_timeout` (current) | `300000` ms (5 min) — runbook step sets this to never |
| Fully Kiosk Browser installed? | No — `pm list packages` has no `fullykiosk` entry; sideload is a Branch A runbook step, not yet performed |

**Classification logic:** `ro.build.version.name` (and `ro.build.version.release` /
`ro.product.manufacturer`) contain `Fire OS` and Android 9 → **FIRE_OS**. A device
that instead reports a Vega OS build fingerprint (no adb, no Fire OS build props,
Silk-only) classifies as **VEGA_OS**. If the probe cannot reach any device at all
(no IP configured, `adb connect` refused/times out, `adb devices` empty after
`FAMILY_HUB_TV_IP` env, `docs/device.toml`, and `adb devices` paired-list are all
tried) the classification is **UNKNOWN** and Branch A is left un-promoted; the owner
resolves it via `docs/OWNER_CHECKLIST.md` step 1.

This device matches `docs/device.toml` exactly (same model, OS build, IP, MAC) — the
values recorded there by the owner on 2026-08-29 are confirmed live, not stale.
`family-hub.exe tv-probe` (T3.1) re-runs the same probe at any time and prints the
same classification, so this table can be refreshed without an agent.

**Result: FIRE_OS. Branch A is promoted below.**

### What the display is, and why it changes one step

The device is an **Insignia NS-50F301NA22 — a 50" Fire TV Edition *television***,
not a Fire TV stick plugged into a television. There is therefore **no separate HDMI
source device**, and the generic Fire-TV-stick step *"turn HDMI-CEC off on the
television so the TV does not power the stick down"* has nothing to act on: **HDMI-CEC
is not part of this device's runbook.** Its replacement — Branch A step 5 — is
**disabling the television's own sleep / power-saver timers**, which is the setting
that would otherwise blank this panel behind a perfectly healthy kiosk. HDMI-CEC
returns as a real step only under Branch B′, where a separate box *is* plugged into
an HDMI input.

### What the remote can do

Stated scope (`docs/PLAN.md` §2 D1/D8, default 35): **routine completion, profile
switching, panel navigation, screensaver dismiss and the join-QR overlay are fully
operable from the TV remote.** Drawing, photo capture, calendar editing and all
administration are phone-only (`docs/PWA.md`). Key map: `ArrowUp`/`ArrowDown` change
profile, `ArrowLeft`/`ArrowRight` change panel, `Enter` toggles, `Backspace` is Back,
`MediaPlayPause` toggles the screensaver. There is **no Esc** — Fire TV remotes do
not have one. Append `?keys=1` to the kiosk URL to show a live key-code overlay if a
button on your remote reports something unexpected.

---

## Branch A — Fire OS (Fully Kiosk Browser) — **detected, promoted**

Applies to Fire OS 5/6/7/8/14/16 devices with ADB debugging available (this TV:
Insignia NS-50F301NA22, Fire OS 7.7.1.5, Android 9 / API 28, IP `10.0.0.178`).

**Before you start:** the hub must already be running and reachable —
`docs/OWNER_CHECKLIST.md` steps 2–3. Test from any browser on the LAN:
`http://<hub-ip>:8080/health` returns JSON with `"db": true`.

### 0. Connect adb (once per session)

```powershell
adb connect 10.0.0.178:5555        # TV: Settings -> My Fire TV -> Developer Options -> ADB debugging ON
adb devices                        # expect: 10.0.0.178:5555   device
```

If it says `unauthorized`, look at the television — Fire OS shows an "Allow USB
debugging?" dialog that must be accepted with the remote, once.

### 1. Sideload Fully Kiosk Browser

Fully Kiosk Browser **≥ 1.61.2**, with the **PLUS licence — one-off €8.90 / $10.99**
(the PLUS key is what unlocks the kiosk lock-down, motion detection and the
launch-on-boot behaviour; the free build is not sufficient). No Play Store is
involved on Fire OS — download the APK from the vendor on the PC and push it:

```powershell
adb install .\Fully-Kiosk-Browser-v1.61.2.apk
```

**There is no separate "Plus" APK.** Fully Kiosk Browser is one app; PLUS is a
**licence key** activated inside it, and the key is bought against a **Device ID that
the app only generates once it is installed and has been run on the device**. So the
order is: sideload -> launch -> read the Device ID -> buy -> activate. An earlier
version of this step said `adb install .\fully-kiosk-plus.apk`, which reads as though
the licence arrives in the download and sends you hunting for a build that does not
exist (the owner hit exactly this, 2026-09-03).

Do **not** install `Fully-Single-App-Kiosk`. It is a different vendor product that
locks the device to one *app*; it is not a browser and none of the settings below
apply to it.

**Done on this TV, 2026-09-03:** Fully Kiosk Browser 1.61.2 installed over adb, both
appops below granted and verified `allow`. Its **Device ID is `8df9c4e3-9e8ee63c`**
(shown bottom-left of Fully's own menu, beside the IP) - that is the string the
vendor's purchase page asks for. It is *not* the Android ID (`7f7c24cdb24b32b2`);
Fully derives its own.

Fully Kiosk is a declared non-Rust component with a stated exit criterion —
`docs/NON_RUST.md`.

### 2. Grant the two permissions over adb

Fully Kiosk cannot raise these dialogs itself on a Fire TV (there is no Settings UI
for them on Fire OS), so they are granted from the PC:

```powershell
adb shell appops set de.ozerov.fully SYSTEM_ALERT_WINDOW allow
adb shell appops set de.ozerov.fully GET_USAGE_STATS allow
```

* `SYSTEM_ALERT_WINDOW` — lets Fully Kiosk draw over whatever Fire OS puts on
  screen, which is how it returns to the kiosk after an Amazon overlay, an update
  prompt or the screensaver.
* `GET_USAGE_STATS` — feeds its "another app came to the foreground" watchdog. Without
  it the kiosk stays behind anything Fire OS launches.

Verify: `adb shell appops get de.ozerov.fully` lists both as `allow`.

### 3. Stop the device sleeping

```powershell
adb shell settings put secure sleep_timeout 0
adb shell settings put system screen_off_timeout 2147483647
```

`sleep_timeout` is the Fire OS inactivity timer (probed at `840000` ms = 14 min on
this TV); `0` means never. `screen_off_timeout` is Android's display timer (probed at
`300000` ms = 5 min). Verify both:

```powershell
adb shell settings get secure sleep_timeout            # expect 0
adb shell settings get system screen_off_timeout       # expect 2147483647
```

### 4. Configure Fully Kiosk itself

On the TV, open Fully Kiosk → Settings:

| Setting | Value |
| --- | --- |
| Start URL | `http://<hub-ip>:8080/tv` - **plain `http`, port 8080.** See the two warnings below |
| Launch on boot | **on** |
| Keep screen on | **on** |
| **Screensaver** (Fully Kiosk's own) | **Never** — the hub has its own screensaver (idle 10 min, `docs/PLAN.md` D5), and two competing screensavers means the hub's never wins |
| Screensaver timer / Daydream | 0 / disabled |
| Kiosk mode / lock-down | on (PLUS) |
| Web Content Settings → "Use Wide Viewport" (if present) | on — the two settings in this group are the ones that can defeat the `/tv` viewport meta (B-1); exact labels in Fully Kiosk 1.61.2 are unverified, so check by menu wording, not this row alone |
| Web Content Settings → "Desktop Mode" | off |
| Web Content Settings → "Initial Scale" | 0 / default |

#### `http`, never `https`, for the kiosk (the owner hit this 2026-09-03)

The hub listens on **two** ports and they are not interchangeable:

| Port | Speaks | For |
| --- | --- | --- |
| 8080 | plain HTTP | the TV kiosk - `GET /tv` answers **200 directly**, with no redirect to TLS |
| 8443 | TLS, private-CA certificate | the phone PWA, whose service worker needs a secure context |

Pointing the Start URL at `https://<hub-ip>:8080/tv` makes the WebView attempt a TLS
handshake against the plain-HTTP listener. It fails as `ERR_SSL_PROTOCOL_ERROR`,
visible in `adb logcat` as:

```
E chromium: [ERROR:net/socket/ssl_client_socket_impl.cc:896] handshake failed; returned -1, SSL error code 1, net_error -107
```

`net_error -107` **is** that error. It is not a certificate-trust failure: an untrusted
private CA gives `ERR_CERT_AUTHORITY_INVALID` and a "your connection is not private"
interstitial instead. `https://<hub-ip>:8443/tv` would work, but only after installing
the hub's private CA on the TV, and the kiosk gains nothing from TLS.

#### Viewport scale - the confirmed cause of `docs/BACKLOG.md` B-1

Probed 2026-09-03 over adb: `wm size` -> physical `3840x2160`, **override `1920x1080`**;
`wm density` -> **320** (= 2.0 dppx). The WebView therefore lays out at roughly
**960 CSS px** wide while the kiosk is designed for `TV_RENDER_WIDTH_PX` = 1920 CSS px
(`src/client/components/tv/style.rs`), so every `px`-sized element paints at 2x and
overflows. Evidence:
`docs/design/current-state/tv-clipped-fullykiosk-adb-2026-09-03.png` (the hub, clipped
exactly as the owner's photo) and
`tv-fullykiosk-welcome-clipped-2026-09-03.png` - **Fully Kiosk's own welcome page is
clipped the same way**, which rules out anything in the hub's own markup as the cause.

Fully Kiosk is **not** implicated: it was not installed at all when the owner took the
B-1 photo (only Silk and the Chromium WebView were present), so B-1's original
"Fully Kiosk's web content scale is not 100 %" hypothesis is dead.

**The fix (TV1, `docs/design/PLAN_TV_VIEWPORT.md` §1.1):** `/tv` now serves its own
viewport meta — `width=1920, user-scalable=no` (`tv::style::TV_VIEWPORT_META`) — as
the last `name="viewport"` tag Dioxus writes into `<head>`, in place of the old global
`width=device-width` meta every route used to share. With `width=1920` and no
`initial-scale`, Chromium computes the scale that fits 1920 CSS px into the panel: on
this TV that is 960 / 1920 = 0.5, so the layout viewport becomes 1920 × 1080 CSS px —
exactly the size the poster card, the rail budget and the golden files are built for —
while the WebView still rasterises the same 960 physical CSS px it always did (device
scale factor 2.0 × page scale 0.5 = one framebuffer pixel per CSS pixel). `curl
http://<hub-ip>:8080/tv` will still show the `dx` build's own template meta,
`width=device-width, initial-scale=1`, first — that one ships inside
`public/index.html` ahead of anything Dioxus renders — followed by ours. Chromium
applies viewport metas in document order and **the last one wins**, so the template's
earlier tag is harmless; it is not a sign the fix failed. The phone routes (`/m`,
`/mobile`) are untouched: they keep serving
`width=device-width, initial-scale=1, viewport-fit=cover, user-scalable=no`,
byte-identical to before.

### 5. Disable the **television's** sleep / power-saver timers

*(This is the step that replaces "turn HDMI-CEC off" — see
[What the display is](#what-the-display-is-and-why-it-changes-one-step). This panel
is the Fire TV; there is no second device for CEC to power down.)*

On the TV: **Settings → Display & Sounds → (Display / Power) → sleep, standby,
auto-power-off, "Power Saving" or "Energy Saver" timers → Never / Off.** Fire OS
Edition televisions expose one or two of these under slightly different names by
firmware; turn off every one of them. Also set **Settings → Display & Sounds →
Screensaver → Start after → Never** so the Fire OS screensaver cannot take the
foreground back from the kiosk.

If the panel still blanks after all three (`sleep_timeout`, `screen_off_timeout`, TV
power saver), see `docs/RECOVERY.md` — "The television is blank or shows the Fire TV
home screen".

### 6. Prove it survives a power cut

Reboot the TV **three times** in a row (`adb reboot`, or pull the mains lead) and
confirm each boot lands on the kiosk showing today's routine, with **no remote
interaction at all**. Three is deliberate: Fully Kiosk's boot-launch on Fire OS is
vendor-disclaimed and fails intermittently rather than consistently (`docs/PLAN.md`
§7 RR-2). This is the pass criterion of `docs/OWNER_CHECKLIST.md` step 5.

If one of the three fails, do not tune it in place — the documented fallback is
Branch B (Silk bookmark, manual relaunch) or Branch B′ (a dedicated box), both below.

Fully Kiosk / Fire OS is vendor-disclaimed by Amazon (not an officially supported
combination) — declared as a non-Rust dependency with this exit criterion in
`docs/NON_RUST.md`: if Amazon ever blocks sideloading on a future Fire OS update,
fall back to Branch B (Silk bookmark) with reduced boot-resilience, or move to
Branch B′ (a dedicated Fully-Kiosk-capable box).

### Unlock and lock the kiosk from a parent phone (B-6)

Not a setup step — a shortcut for after setup (`docs/BACKLOG.md` B-6,
`docs/design/PLAN_KIOSK_REMOTE.md`). Instead of opening Fully's Remote Admin page
(`http://10.0.0.178:2323/home`) and signing in there, a signed-in parent opens the hub
app's **Settings** → **TV kiosk** (just below *Offline & install*) and taps **Unlock
Kiosk** or **Lock Kiosk**.

* **Once, on either parent's phone:** enter the TV address (prefilled `10.0.0.178:2323`)
  and the password set under Fully's Settings → Remote Administration, then **Save**. The
  hub checks the pair against the TV (`deviceInfo`) before it keeps it; a wrong password
  is not saved. The password is never sent back to any phone.
* **Shared:** there is one saved TV address and password for the whole family. The second
  parent's phone shows the buttons with no setup, and changing the address or password
  from either phone changes it for both.
* **One tap, no confirm dialog** (the owner's choice). After an unlock the phone says
  *"Kiosk unlocked on the TV. Tap Lock Kiosk when you're finished."* — nothing re-locks
  the TV automatically.
* **How it travels:** phone → hub (HTTPS `:8443`) → TV (plain HTTP `:2323`, Fully's
  `?cmd=unlockKiosk|lockKiosk|deviceInfo&password=…&type=json`). The hub only ever talks
  to a private (RFC 1918) IPv4 address typed with an optional port, never a host name,
  and never follows a redirect. The code is `src/server/api/kiosk.rs`.
* **Only over HTTPS.** The parent session cookie is `Secure`, so over the plain
  `http://…:8080` origin these buttons answer *"Your sign-in expired. Enter the PIN
  again."* rather than reaching the TV.
* **Where the password lives:** in the hub's database (`settings` rows `kiosk_admin_url`
  and `kiosk_admin_password`), in plain text because it has to be replayed to Fully on
  every tap. The nightly backups (`src/server/backup.rs`) therefore contain it too —
  treat the backup folder like the password itself.
* **If the TV's address changes** (a DHCP move after a router restart), the phone says
  *"Can't reach the TV. Is it on? If it was restarted, its address may have changed."* —
  tap **Change TV address or password** and enter the new one (the password is always
  asked for again with a new address).

**Recorded on device (K-A7, pending the owner's reinstall):** the three real Fully
response bodies for `deviceInfo`, `unlockKiosk` and `lockKiosk`, password redacted, go
here once the hub has been reinstalled and the buttons tried on the Insignia.

## Branch B — Vega OS (no sideloading)

Applies to newer Fire TV Stick 4K Select (2025) / Stick HD (2026) hardware running
Vega OS, which has no ADB access and no app sideloading at all. **Not this device** —
kept because a replacement bought in 2026 or later is likely to be Vega.

1. Open **Amazon Silk** (pre-installed) and navigate to `http://<hub-ip>:8080/tv`.
   Silk on this generation is Chromium 138, which has everything the kiosk needs
   (canvas, WebSocket, pointer events, `ResizeObserver`).
2. Bookmark the page / pin it to the Vega OS home row for one-tap access, and set it
   as Silk's start page.
3. **No boot auto-launch is possible on Vega OS.** After a power cut the TV returns
   to the Vega OS home screen; the recovery step is "Alexa, open Silk" (or a remote
   press) followed by selecting the bookmark. This is a known, accepted limitation
   (`docs/PLAN.md` §7 RR-3) and the first line of `docs/RECOVERY.md`'s blank-TV entry.
4. Vega OS still ships a device-level **Screensaver** setting
   (Settings → Display → Screensaver) — set it to the longest available value, and
   its sleep/power-saver timers to Never, so neither obscures the kiosk during a
   routine. There is no `adb`, so both are remote-only changes.

## Branch B′ — Vega + boot resilience wanted

For a household that lands on Branch B but wants Branch-A-style unattended
boot-launch without Fire OS sideloading:

1. Buy a small **Google TV box** (~$35, e.g. Chromecast with Google TV) or repurpose
   a retired **Android tablet**, either of which supports sideloading.
2. Install **Fully Kiosk Browser** on that device (same PLUS licence, €8.90 / $10.99
   one-off) and follow Branch A steps 2–4 on it instead of the television. `adb` over
   Wi-Fi works the same way.
3. Connect the box to the television over its own HDMI input and set that input as
   the TV's default-on source (most TVs expose a "Power On behavior" / default input
   setting).
4. **HDMI-CEC matters here** — this is the one branch with a separate source device.
   Turn CEC *off* on the television (Settings → HDMI-CEC / "device control" / the
   vendor's brand name for it: Anynet+, Bravia Sync, SimpLink) so the television
   cannot put the box to sleep with it, and so the box cannot switch the TV's input
   away from the kiosk.
5. This is a **priced upgrade, not a dependency** — the household is fully
   functional on plain Branch B (Silk bookmark) without buying anything.

---

## Verifying the kiosk from the PC

None of these need the remote:

```powershell
adb connect 10.0.0.178:5555
adb shell dumpsys window | Select-String -Pattern "mCurrentFocus"   # expect de.ozerov.fully
adb shell settings get secure sleep_timeout                          # expect 0
adb shell appops get de.ozerov.fully                                 # both ops allow
adb shell input keyevent 20                                          # D-pad Down -> next profile
```

`http://<hub-ip>:8080/health` reports what the *server* thinks: `ws_clients` includes
the television whenever the kiosk is connected, so a `ws_clients` of 0 with a lit
screen means the page is up but the socket is not (`docs/RECOVERY.md`).

**The viewport readout.** Open `http://<hub-ip>:8080/tv?keys=1` (Fully Kiosk's remote
admin `loadUrl`, or temporarily as the Start URL) and read the HUD's first line: it
must say `viewport 1920×1080 css px, dpr 2.00 — as designed (target 1920×1080)`.
Anything else — most likely `960×540` — means the WebView is still laying out at
`device-width`: check the two Web Content Settings rows in step 4, and whether the
running binary predates TV1 (`docs/design/PLAN_TV_VIEWPORT.md`).

**Pixel probe** (`docs/design/PLAN_TV_VIEWPORT.md` §4.2). After a framebuffer capture
(`adb exec-out screencap -p > <file>.png`), probe individual pixels from the PC:

```powershell
Add-Type -AssemblyName System.Drawing
$b = [System.Drawing.Bitmap]::FromFile("<file>.png")
$b.GetPixel(48, 540)    # expect ~#8BB5DA (sheffield-light, the frame), tolerance +/- 8 per channel
$b.GetPixel(98, 540)    # expect ~#1E293B (slate-800, the card's border-4)
$b.GetPixel(140, 540)   # expect #FFFFFF (the card)
```

Any of these off by more than the tolerance means the layout is not 1920 CSS px wide —
see `docs/design/PLAN_TV_VIEWPORT.md` §4.2 for the full pixel set and the composition
checklist.

## Troubleshooting

| Symptom | Most likely cause | Fix |
| --- | --- | --- |
| Kiosk shows but goes dark after ~5–14 min | Only one of the three timers was cleared | Redo Branch A steps 3 **and** 5 — `sleep_timeout`, `screen_off_timeout` and the television's own power saver are three independent settings |
| Fire OS home screen appears over the kiosk | `GET_USAGE_STATS` not granted, or Fully Kiosk's watchdog off | Re-run step 2 and check `adb shell appops get de.ozerov.fully` |
| Boot lands on Fire TV home, not the kiosk | Fully Kiosk boot-launch (vendor-disclaimed, RR-2) | Retry step 6; if it fails repeatedly, move to Branch B′ |
| A hub-side screensaver never appears | Fully Kiosk's own **Screensaver** is on and wins | Set it to Never (step 4) |
| `adb connect` says `unauthorized` | The one-time on-screen prompt was never accepted | Accept it on the TV with the remote, then reconnect |
| Page loads but the clock is frozen and a red badge shows | Server unreachable for > 90 s | `docs/RECOVERY.md` — "The hub is unreachable" |
| Kiosk is huge and clipped (one rail card, text off the card's edge) | The page is laying out at `device-width`: the running binary predates TV1, or a WebView setting is ignoring the viewport meta | Check the `/tv?keys=1` readout; reinstall the service; check the two step-4 rows |

---

## Not attempted: Phase C (Rust-native Android shell)

Cut, not deferred (`docs/reviews/PURPLE_TEAM.md` R-29): a WebView app the household
would own, Leanback manifest injection, and a full Android toolchain — impossible on
Vega OS anyway, and unnecessary once the TV is a plain HTTP client (D3′). Not
reconsidered unless Phase 4 is revived by the owner.

---

Related: `docs/OWNER_CHECKLIST.md` (the one-time setup, in order) ·
`docs/RECOVERY.md` (when it breaks) · `docs/PWA.md` (the phones) ·
`docs/DEV_WINDOWS.md` (the development box) · `docs/NON_RUST.md` (why Fully Kiosk and
`adb` are allowed) · `docs/device.toml` (the probed device record).
