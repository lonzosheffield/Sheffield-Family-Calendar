# RESIDUAL

Per PLAN §5 / PURPLE §P5.1 wave gate 6: gaps the Boss has accepted rather than
re-scoped into a task, each with the solution that would close it. Nothing here
is a defect the QA loop (T3.5) is still owed; a residual is a deliberate
"not in this run".

---

## R-1. The join-QR overlay does not carry the first-run setup code

- **Origin:** `docs/HANDOFF.md` T2.1 H-24 (Boss decision, wave 2-a) and
  `docs/qa/QA_ROUND_2.md` Q2-01 solution step 5.
- **What ships:** the first-run parent-PIN setup code is generated at boot by
  `router::run` and written to the hub's log and to `<data>\setup-code.txt`
  only. It is never served over the network and never rendered on the
  television (PLAN §3 T1.4 and PURPLE §P5.5 default 9, as amended at the QA
  round 2 close). Redeeming it is `POST /api/setup` from the phone's Settings
  tab, so setting the family's first PIN needs someone at the hub PC.
- **Why it is residual:** D1/D8 scope calendar editing and administration off
  the TV to phone-only, and the kiosk hydrates from the plain-HTTP listener;
  exposing the code there would gate the very first PIN on nothing more than
  LAN access.
- **Solution, if wanted later:** show the code on T2.1's join-QR overlay
  **only** when `parent_setup_code()` is gated to the kiosk's own HTTP
  listener (loopback / the reserved TV IP), never over `/api/*` or the
  hydration payload. Wave-3 hardening item; not scheduled.
