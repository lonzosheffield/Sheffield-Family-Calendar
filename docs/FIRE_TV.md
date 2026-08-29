STATUS: FIRE_OS

# Fire TV kiosk runbook (T0.0 device-ID gate)

This file is produced by **T0.0**, the non-blocking device probe. It never halts the
run: whatever the TV turns out to be, all three kiosk branches below are documented,
and the branch that matches the detected device is promoted to the front. **T3.2**
finalises this file at the end of the run (adds the completed step-by-step runbook,
cross-links `OWNER_CHECKLIST.md`/`RECOVERY.md`) but does not change the `STATUS` line
or remove any branch.

## Probe result (2026-08-29, live `adb`)

Source of the IP: `docs/device.toml` (`10.0.0.178`, DHCP lease at time of writing —
Appendix A2 asks the owner to reserve it). `adb connect 10.0.0.178:5555` succeeded
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
resolves it via `OWNER_CHECKLIST.md` step A1.

This device matches `docs/device.toml` exactly (same model, OS build, IP, MAC) — the
values recorded there by the owner on 2026-08-29 are confirmed live, not stale.

**Result: FIRE_OS. Branch A is promoted below.**

---

## Branch A — Fire OS (Fully Kiosk Browser) — **detected, promoted**

Applies to Fire OS 5/6/7/8/14/16 devices with ADB debugging available (this TV:
Insignia NS-50F301NA22, Fire OS 7.7.1.5, Android 9 / API 28, IP `10.0.0.178`).

1. **Sideload Fully Kiosk Browser** ≥ 1.61.2 (PLUS licence, **one-off ~$11 / €8.90**)
   over `adb install fully-kiosk-plus.apk` — no Play Store needed on Fire OS.
2. **Grant permissions over adb** (Fully Kiosk cannot self-request these on Fire TV):
   - `adb shell appops set de.ozerov.fully SYSTEM_ALERT_WINDOW allow`
   - `adb shell appops set de.ozerov.fully GET_USAGE_STATS allow` (needed for
     Fully Kiosk's "return to kiosk after other app opens" watchdog)
3. **Disable device sleep** so the kiosk never goes dark waiting for a remote press:
   - `adb shell settings put secure sleep_timeout 0` (currently `840000`)
   - `adb shell settings put system screen_off_timeout 2147483647` (currently
     `300000`)
4. In Fully Kiosk's own settings: **Screensaver → Never**, set the Start URL to
   `http://<dhcp-reserved-ip>:8080/tv`, enable "Launch on boot" and "Keep screen on".
5. This TV is a **television**, not a stick, so there is no separate HDMI-CEC source
   device to worry about — the equivalent step is disabling the **television's own**
   sleep/power-saver timers (Settings → Display & Sounds → power/standby timers →
   Never) so the panel itself doesn't blank behind the kiosk.
6. Reboot the TV **three times** in a row and confirm it lands back on the kiosk URL
   each time without a manual relaunch (Appendix A4 / `OWNER_CHECKLIST.md`).

Fully Kiosk / Fire OS is vendor-disclaimed by Amazon (not an officially supported
combination) — declared as a non-Rust dependency with this exit criterion in
`docs/NON_RUST.md`: if Amazon ever blocks sideloading on a future Fire OS update,
fall back to Branch B (Silk bookmark) with reduced boot-resilience, or move to
Branch B′ (a dedicated Fully-Kiosk-capable box).

## Branch B — Vega OS (no sideloading)

Applies to newer Fire TV Stick 4K Select (2025) / Stick HD (2026) hardware running
Vega OS, which has no ADB access and no app sideloading at all.

1. Open **Amazon Silk** (pre-installed) and navigate to
   `http://<dhcp-reserved-ip>:8080/tv`.
2. Bookmark the page / pin it to the Vega OS home row for one-tap access.
3. **No boot auto-launch is possible on Vega OS.** After a power cut the TV returns
   to the Vega OS home screen; the recovery step is "Alexa, open Silk" (or a remote
   press) followed by selecting the bookmark.
4. Vega OS still ships a device-level **Screensaver** setting
   (Settings → Display → Screensaver) — set it to the longest available value so it
   does not obscure the kiosk during a routine.

## Branch B′ — Vega + boot resilience wanted

For a household that lands on Branch B but wants Branch-A-style unattended
boot-launch without Fire OS sideloading:

1. Buy a small **Google TV box** (~$35, e.g. Chromecast with Google TV) or repurpose
   a retired **Android tablet**, either of which supports sideloading.
2. Install **Fully Kiosk Browser** (same PLUS licence, ~$11 one-off) on that device
   and follow the Branch A steps (2–4) on it instead of the television.
3. Connect the box to the television over its own HDMI input and set that input as
   the TV's default-on source (most TVs expose a "Power On behavior" / default input
   setting).
4. This is a **priced upgrade, not a dependency** — the household is fully
   functional on plain Branch B (Silk bookmark) without buying anything.

---

## Not attempted: Phase C (Rust-native Android shell)

Cut, not deferred (`docs/reviews/PURPLE_TEAM.md` R-29): a WebView app the household
would own, Leanback manifest injection, and a full Android toolchain — impossible on
Vega OS anyway, and unnecessary once the TV is a plain HTTP client (D3′). Not
reconsidered unless Phase 4 is revived by the owner.
