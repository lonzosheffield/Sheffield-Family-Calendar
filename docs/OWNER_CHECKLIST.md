# Owner verification checklist

This is Appendix A of `docs/PLAN.md`, delivered as its own file. Every step here
needs a human, a phone, the TV, a reboot, or elevation — nothing an autonomous agent
can run — so none of it is required by any task's automated acceptance test. **T3.2**
assembles the full checklist (≥ 8 numbered steps, each with an explicit pass
criterion) at the end of the run; **T0.0** seeds the one row it can already answer.

| # | Step | Pass criterion |
| --- | --- | --- |
| Device | Read the TV's identity (Settings → My Fire TV → About) and compare it to the live probe T0.0 already ran. **Already answered:** `docs/device.toml` + `docs/FIRE_TV.md` STATUS line record `FIRE_OS` — Insignia NS-50F301NA22, Fire OS 7.7.1.5, Android 9 / API 28, IP `10.0.0.178`, `adb` reachable, Fully Kiosk not yet installed — probed live over `adb` on 2026-08-29. Only re-confirm this row if the physical TV is ever replaced. | The device shown in TV Settings matches the model/OS recorded in `docs/device.toml`; if it ever disagrees, re-run the T0.0 probe (or set `FAMILY_HUB_TV_IP` to the new IP) and treat `docs/FIRE_TV.md`'s promoted branch as stale until it does. |

Remaining rows (DHCP reservation, service install, Fully Kiosk sideload + permission
grants, remote-navigation walkthrough, CA install on phones, PWA install, offline
replay, real photo upload, screensaver drop, network-outage recovery, optional
DNS-01) are added by **T3.2** once the tasks that make them meaningful (T3.1 service
install, T2.6 phone→TV loop, T1.3 PKI, T2.2 PWA, T2.5 photos, T2.7 screensaver) have
landed.
