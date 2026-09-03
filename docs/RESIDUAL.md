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

## R-2. School's Today inline edit has no affordance for a row not yet on screen

- **Origin:** `docs/HANDOFF.md` H-HS5-6 (HS5 → HS7/HS8).
- **What ships:** `LessonOccurrence` carries the assignment row's **id**, not
  its `ordinal`, and `upsert_assignment` is keyed on `(subject, week,
  ordinal)`. The **Year** view derives the ordinal exactly (first appearance
  in date order, across the whole week), so its inline edit always works.
  **Today** only has the part of the week already dealt out on screen, so
  `homeschool::today::assignment_ordinals` recovers the ordinal for every row
  a parent can see there and simply offers no edit affordance for the rest
  (`edit_ordinal_for` returns `None`) — a parent who wants to pre-write
  Thursday's "Math: lesson 14" on Monday has to do it from **Year**, not
  **Today**.
- **Why it is residual:** H8 (owner, 2026-09-02) asks for Year/Month/extras,
  which is what shipped; this is a gap in Today's own inline edit, not in the
  feature H8 requested, and every occurrence the fixture and every acceptance
  test render is edit-affordanced correctly (§3 HS5 (b), (h)).
- **Solution, if wanted later:** one field — `ordinal: i64` on
  `LessonOccurrence`, which `occurrences()` already computes and has in hand
  — rather than more client-side inference. A schema-additive, backward
  compatible change to `src/shared/types.rs`.

## R-3. HS7's `/health` curricula check depends on the AO transcription being present on the box

- **Origin:** `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS7 Accept (e); N1.
- **What ships:** `/health`'s `curricula` count is exactly the number of
  curricula rows in the database, populated at boot by the directory loader
  (`FAMILY_HUB_CURRICULA_DIR`, default `<data>\curricula`). The transcript in
  `docs/VERIFICATION.md`'s HS7 row demonstrates `curricula: 0` for an empty
  directory and `curricula: 1` for a directory holding the real, gitignored
  `ao-year-1.toml` — but that file is **never** committed (N1), so a
  from-scratch checkout has no curricula directory contents at all until
  either HS2's transcription is regenerated from
  `docs/homeschool/curriculum/ao-year-1.notes.md` or the owner's own
  `import-curriculum` run (`docs/OWNER_CHECKLIST.md` step 14) places a real
  file there.
- **Why it is residual:** this is exactly what N1 requires — the licensed
  content cannot live in the repo — not a defect; recorded here so a future
  agent re-running HS7's Accept (e) on a fresh checkout knows to expect a
  "skip: file absent" result rather than treat it as a regression.
- **Solution:** none needed; this is the intended shape. If a future task
  wants the check to be exercisable from a clean checkout, it would need its
  own committed synthetic curriculum (mirroring `tests/fixtures/curricula/sample-year.toml`)
  loaded into a scratch directory rather than the real AO file.
