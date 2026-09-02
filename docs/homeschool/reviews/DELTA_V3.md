# DELTA_V3 — red+purple review of the v3 delta to `PLAN_HOMESCHOOL.md`

Scope: only what v3 added. v2 is out of scope. **High** = ships broken · **Med** = will be reworked ·
**Low** = wording.

---

## D-1 · `lesson_extras` is valid SQL; do **not** reuse `custom_tasks` — Low (confirm) · H1

The DDL runs as written; `IS NULL OR` in the status CHECK is redundant but harmless.

**Reuse `custom_tasks`? No, decisively.** It is `is_completed BOOLEAN` — no `skipped`, `note`,
`category`, `completed_on`, all four of which rule 10 needs. And its rows are already focusable via
`tv/model.rs::body_order` (`routine.iter().chain(tasks.iter())`) and rendered by `routine.rs`, so
every extra would appear a second time on the Routine tab and panel; suppressing that means editing
`routine.rs` and `body_order` — which no HS task owns — and changing
`tests/golden/tv_focus_order.txt`, breaking HS6 (a). **Keep the new table.**

**Two fixes.** Order `extras_between` by `(scheduled_date, sort_order, id)`; without the tiebreak
HS1 (i) is non-deterministic, since nothing sets `sort_order` and the default is `0`. Add
`updated_at TIMESTAMP`, written by `update_extra` / `set_extra_status`.

---

## D-2 · `subject_id = 0` for an extra inside `LessonOccurrence` — High · HS3 DTOs, rule 10

It does *not* collide with `lesson_log_occurrence` (extras never write to `lesson_log`), but it
breaks four other things:

1. **FK** — `lesson_log.subject_id REFERENCES subjects(id)`; any path reaching `set_occurrence` with
   `0` fails at runtime, not compile time.
2. **H7 validation** — `occurrences()` never emits extras (they arrive via `merge_extras`), so an
   extra reaching `toggle_lesson` is rejected with a misleading message, and nothing requires
   `toggle_lesson` to reject `subject_id <= 0` outright.
3. **Offline queue** — `ToggleLesson { subject_id, … }` can serialise `0` into `localStorage` and
   replay it into a certain rejection.
4. **`FocusId`** — with one merged `Vec`, `i` indexes the merged list, so `Lesson(3)` and `Extra(3)`
   coexist meaning different rows.

**Replacement (HS3, normative):** drop `extra_id` from `LessonOccurrence`; delete the
`subject_id = 0` sentence. Add `pub enum DayItem { Lesson(LessonOccurrence), Extra(ExtraTask) }`
(`Clone, PartialEq, Debug, Serde`); `BoyToday`'s three lists become `Vec<DayItem>`. `occurrences()`
and `TogetherOccurrence` are untouched — extras are never Together. `merge_extras` gains a
`week_span: (&str, &str)` argument and pushes `DayItem::Extra`.

**HS6:** `FocusId::Lesson(String)` keyed `s{subject}-a{assignment|0}-{date}`, `FocusId::Extra(i64)`
keyed on `extra_id`; `dom_id()` → `tv-lesson-{key}` / `tv-extra-{id}`. Index-keyed ids are wrong for
the same reason `FocusId::Event` is a string: the list refetches on `homeschool_version` and a tick
reorders `due_today`/`done`, moving the cursor under the child's thumb.

**HS4 (e), append:** "`toggle_lesson` with `subject_id <= 0` is rejected before any write."

---

## D-3 · The press budget holds; HS6 (b) and (h) contradict each other — Med · HS6

The budget is fine; `step()` needs no change. With `ALL` at 4 and School last, `previous()` from
`Routine` wraps straight to School: 1 press to the panel, 1 `Enter` into the body, ⌊n/2⌋ to the
furthest row of a wrapping list — 13 rows → **8 presses**. The existing
`every_routine_item_is_within_twelve_presses_from_any_panel` also survives (worst case 7).

The defect is arithmetic in the prose: **Do** says "exactly 12 focusable lesson rows", (h) adds an
extra on top (13), (b) says "every `FocusId::Lesson(i)` of the 12-row fixture". Pin it:

> **Do:** "…fixture day with exactly **11 lesson rows and 1 extra** — 12 focusable body rows."
> **(b):** "BFS over the pure key handler reaches every `FocusId::Lesson(_)` **and the
> `FocusId::Extra(_)`** of the 12-row fixture from `TvState::initial()` in ≤ 12 presses."
> **(h):** "…the extra renders `📌`, is focusable as `FocusId::Extra(extra_id)`, and `Enter` yields
> `TvAction::Activate(FocusId::Extra(extra_id))`, dispatched to `toggle_extra`."

---

## D-4 · `MonthView.total` is not computable for past weeks — High · H6, HS3, HS4

`enrollments.week_started_on` is **overwritten** by every Finish week; nothing records which dates
belonged to which `week`. For a past date the server can pick neither the right `WeekPlan` nor the
occurrence dates, so `total` is unknowable — `log_counts_between` yields only `done`. HS3 (h) never
tests `total` and HS5 (i) tests it only inside the current week, so the hole ships as `n/n` on every
past day.

**Replacement.** `MonthDay` gains `total: Option<u32>` — `Some(n)` only when `in_current_week`,
`None` elsewhere, rendered as a bare `done` count plus 📌, never `n/m`. `month_view` gains a final
`today: &str` argument. `get_month(user_id, year, month)` fetches exactly: `enrollment(user_id)`;
`week_plan(curriculum_id, current_week)` **only if** the current week's span intersects the month;
`log_counts_between` and `extras_between` over the month. Nothing else. Add HS3 (h′): "a day before
`week_started_on` has `total = None` and `done` equal to its log rows; a day inside the span has
`total = Some(n)`, `n` = merged occurrences + extras dated that day."

**Also:** `MonthView` is per boy while H6 says the chip defaults to everyone. Add to default 17:
"Month view always shows exactly one boy; with more than one enrolled, the first enrolled boy is
preselected and the chip is a required selector, not a filter."

---

## D-5 · Year view for a non-current week has no anchor date — High · H6, HS3 (g), HS4

`occurrences()` derives `scheduled_date` from `week_started_on`, which describes only the current
week. For any other week there is no true date, and inventing one invites a Year-view tick that H7
then rejects confusingly.

**Specify:** `get_week_grid` uses the synthetic anchor `add_days(week_started_on,
(week − current_week) * 7)`, and `WeekGrid` gains `dated: bool` (true iff `week == current_week`).
When `dated == false` the UI labels columns by weekday name only, `scheduled_date` is advisory, and
**no cell is tickable** — the sheet offers only `assignment.text` / `days` editing, both week-scoped
and date-free. HS4 (l′): "`week != current_week` → `dated = false`; `week = 0` and `week > weeks`
both error; an unenrolled `user_id` errors."

**HS3 (g) bakes in a bug:** the fixture has 7 subjects, but `Reading Basket` is `free_read`, never an
occurrence per rule 6 — a permanently empty row. Rewrite to "**6** rows (free_read excluded) × 5
cells; `Fables` row all empty; `Twice Told` both rows in the Tuesday cell."

---

## D-6 · The Isaiah seed re-runs every boot and resets week 1 — High · micro-commits, default 1

1. **`upsert_enrollment` at boot is destructive.** HS1 (e) makes it "replace, never duplicate"; run
   from `pools()` on every boot it re-stamps `current_week = 1` and `week_started_on = today`,
   undoing every Finish week the parents have done. Use `INSERT … ON CONFLICT (profile_id) DO
   NOTHING`, logging at `info` either way.
2. **Profile id 1 is not guaranteed to be Isaiah.** `0004_name_the_boys.sql` matches on
   `name = 'Boy 1'` and deliberately skips an already-renamed row — so if profile 1 was renamed
   before 0004 ran, id 1 is someone else and no row is named Isaiah. Seed by `SELECT id FROM
   profiles WHERE name = 'Isaiah'`; if that is not exactly one row, log at `warn` and skip.
3. **The Monday rule is unnecessary and trappy.** On `NaiveDate` there is no DST hazard; on
   `DateTime<Local>` with `Duration::days(n)` a spring-forward boundary can land a day early, and
   `Local.from_local_datetime(midnight)` is `None` in zones with a midnight transition. The weekend
   case is worse: run on a Saturday the anchor is 5 days back, all of `MTWRF` is already past, and
   week 1 opens with every occurrence in `catch_up` and "Finish week" already offered. **Delete the
   Monday rule:** `week_started_on = started_on = the run date` — what H2 stamps on every later move
   and what default 2 already says; `date_for` then pushes Mon/Tue into the following week of the
   span. Removes both traps and the contradiction with default 2.

---

## D-7 · The queue needs `ToggleExtra`; no `ClientMessage` change is needed — Med · HS5, HS3

`ClientMessage` carries only phone→TV steering (`SetView`, `SetActiveProfile`, `Draw`); extras steer
nothing, so **HS3 needs no `ClientMessage` variant** — correct as written.

The queue differs: without it a parent's offline tick on an extra is silently lost while the
identical tick one row above (a lesson) survives — an inconsistency `docs/PWA.md` promises does not
exist. Six lines in `mobile/queue.rs`, which HS5 already owns. Add
`QueuedMutation::ToggleExtra { user_id: u32, extra_id: u32, completed: bool }`, its `user_id()` arm,
`label()` → `"School task"`; HS5 (d) gains "…and a failed `toggle_extra` likewise enqueues, replays
once on reconnect, and is idempotent under the same key." `toggle_extra` is cookie-free per H7, so
replay works from a signed-out phone.

---

## D-8 · Vague or unexecutable acceptance clauses — Med

**HS3 (h)** — "exactly the 7 days from `week_started_on`" is false when the span straddles a month
boundary; "works with `enr = None`" asserts nothing. Replace:

> (h) `month_view` with `week_started_on = "2026-09-28"`, `year = 2026, month = 9` returns 30 days;
> exactly days 28–30 have `in_current_week = true` (the span's other 4 fall in October and are
> absent); an extra dated `2026-09-10` gives that day `extras = 1, total = None`; with
> `enr = None, plan = None` every day has `total = None`, `week = None`, `is_school_day = false`,
> and `extras` still counted.

**HS3 (i)** — no span stated. Replace the last clause: "with `week_started_on = date − 2`, an extra
dated `date + 9` is in no list and absent from `total_count`; one dated `date + 1` is in no list but
**does** count in `total_count`."

**HS4 (k)** — "`scheduled_date` may be any date (past or future, no window)" is an unbounded write
primitive. Replace: "`scheduled_date` must parse as `YYYY-MM-DD` and lie within
`[today − 365, today + 365]`; outside that, rejected, nothing written. The ±1 day `date` window
still applies to the mutation's own `date` argument."

**HS5 (i)** — "the fixture month", "a day inside the current week" name no date. Replace: "SSR of
Month view for September 2026 with `week_started_on = 2026-09-07` and one fixture extra dated
`2026-09-10`: 30 day cells, `📌` on `2026-09-10`, an `n/m` badge on `2026-09-08`, a bare count on
`2026-09-01`, and a weekend strip carrying `data-month-weekend`."

**HS5 (j)** — replace "shows the 'not dealt out yet' line" with the exact string
`Not dealt out yet — finish week 2 first.`, plus "no curriculum rows, extras only".

**HS1 (i)** — add D-1's `id` tiebreak to the ordering assertion.

---

## D-9 · Smaller items — Low

- **Default 6** should state the consequence of the H7 table: *a child on the TV can tick an extra a
  parent created, and cannot create or delete one.*
- **Rule 10** needs an inclusivity rule — `date − 14 <= scheduled_date < date` — giving HS3 (i)
  boundary cases at 14 and 15.
- **§0 tail** reads as though shared items appear on the TV; H6 shows non-shared occurrences +
  extras. Append "(shared read-alouds stay on the phone, W-16)".
- **Default 16** — append "and are excluded from `total_count` once outside the current week's span".
- **HS1 `add_extra`** — assign `MAX(sort_order) + 1` within `(profile_id, scheduled_date)`;
  otherwise every extra is `0`.
