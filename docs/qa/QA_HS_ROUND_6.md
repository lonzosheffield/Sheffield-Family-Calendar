VERDICT: FAIL

# QA — Homeschool ("School") wave, round 6

**Auditor:** Fable 5 (HS8, fresh context, no prior knowledge of this run) · **Date:** 2026-09-03 ·
**Tree:** `main` @ `2bf9555`, audited as the round-5 fix wave `a49a4ca..HEAD` (7 commits: `b09580d`
plan amendment, `c9691eb`/`eee7ae3` HS4-qa5, `7a1245b`/`6f2fcb8` HS5-qa5, `2bf9555` Boss fix close;
10 files, +412 / −187) with every changed Rust, doc and test file read in full, plus the whole of
`src/client/components/homeschool/{today,year,mod,day_sheet,month}.rs`,
`src/client/components/tv/{fixture,model}.rs`, `src/shared/{types,homeschool}.rs`,
`src/server/api/homeschool.rs`, `src/server/homeschool/db.rs`, `migrations/0005_homeschool.sql`,
`tests/{homeschool_tests,glyph_tests}.rs` and `tests/fixtures/curricula/sample-year.toml` ·
**Contract:** `docs/homeschool/PLAN_HOMESCHOOL.md` v3.1 §0 (N1), §2 H1–H8 (H2's nudge sentences and
Finish-week clause, H3 rules 1–10, H6 items 1–6 and D-5, H7, H8), §3 HS1–HS7 Owns/Do/Accept including
the QH3-04 amendment of HS4 "Do", the QH4-03 `days` amendment and the **new QH5-01 `ordinal`
amendment** of the `LessonOccurrence` line, §4 defaults; `docs/qa/QA_HS_ROUND_5.md` (every QH5 item
re-verified on `main` by reading the code and running its test, never on trust), `QA_HS_ROUND_4.md`,
`QA_HS_ROUND_3.md`, `QA_HS_ROUND_2.md`, `QA_HS_ROUND_1.md`; `docs/RESIDUAL.md` R-1…R-16 (every CLOSED
claim added by `2bf9555` checked against the code); `docs/HANDOFF.md`; `docs/PLAN.md` §5;
`docs/design/DESIGN_DIRECTION.md` §2–§3; `docs/BACKLOG.md`; `docs/BLOCKED.md`; the reviews
`docs/homeschool/reviews/{RED_HS,PURPLE_HS,WHITE_HS,DELTA_V3}.md`.

Verdict is **FAIL** on one High and two Med. The High is the one the provenance warned about: the two
fixing agents were interrupted, nothing they produced was verified by them, and one of the two storage
proofs the Boss added on their behalf is **order-dependent on shared test state and fails on `main`**.
`cargo test --features server` exits 101 here — `homeschool_tests::hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating`
fails 2 runs in 6 at the default job count and **2 in 2** under `--test-threads=1`, because
`hs4_f_toggle_lesson_together_writes_exactly_the_two_boys_sharing_the_week` widens `Old Tales` to
`MTWRFSU` through `set_subject_schedule` and `reset_homeschool_state` never puts `subjects` back
(`load_fixture` is insert-missing-only). It is **not** an R-4 or R-12 load flake — every one of those
lives in `realtime_tests`, `loop_tests`, `backup_tests`, `homeschool_db_tests` or `service_tests`, all
of which were green in every run here, and this one reproduces deterministically in libtest's serial
mode on an idle box. The consequence beyond the red build is that QH5-02's **only** storage proof is a
coin flip. The two Meds are both consequences of the QH5-03 and QH5-02 fixes as prescribed: the nudge
can now print "Week N done — start week N+1?" while the **Finish week** button that answers it is
withheld (a paused brother's zeroed counts make the group's sums look complete while the server's
`all_can_finish`, which does not check `paused`, says no), and the Year cell sheet's text **Save** now
silently discards a days-control edit the parent has typed but not yet saved — the mirror image of
QH5-02, and a behaviour regression against the pre-wave code, which sent the control's value.
Everything else in the fix wave is genuinely right: `LessonOccurrence.ordinal` is filled at all four
literal sites and derived from the row itself in `sched::occurrence()`, no surface infers an ordinal
from position any more, the TV deserialises the new field with no behaviour change, `check_extra_date`
bounds both callers, and no round-1…5 finding has regressed.

## What was run on this machine

Shell preamble for every command: `PATH` (cargo/scoop/npm), `RUST_BACKTRACE=1`,
`FAMILY_HUB_DATA_DIR=%TEMP%\familyhub-qa6`, `FAMILY_HUB_REFUSE_SYSTEM_DIR=1`. Nothing else built or
tested on the box while a suite ran; the two full runs were consecutive, never concurrent.
`C:\ProgramData\FamilyHub` was never opened, listed or resolved by anything run here; the `FamilyHub`
service was not stopped, restarted or reinstalled; the Fire TV was not touched.

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | **exit 0** |
| `cargo clippy --features server --all-targets -- -D warnings` | **exit 0** |
| `cargo clippy --features web --target wasm32-unknown-unknown -- -D warnings` | **exit 0** |
| `cargo test --features server` (run 1) | **exit 101** — `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating` FAILED |
| `cargo test --features server --no-fail-fast` (run 2) | **exit 101** — 33 `test result:` lines, **632 passed, 1 failed**, 2 ignored; the same single failure, every other binary green |
| `cargo test --features server --test homeschool_tests` ×6 (default parallel) | 4 × `22 passed`, **2 × `21 passed; 1 failed`** |
| `cargo test --features server --test homeschool_tests -- --test-threads=1` ×2 | **2 × `21 passed; 1 failed`** — deterministic |
| `cargo test --features server --test homeschool_tests hs4_i_a_text_edit_from_the_year_sheet` (alone) | `1 passed` — passes in isolation, which is the whole tell |

The Boss's reported "33 `test result:` lines and 633 passed, 0 failed" is **confirmed as a case count
and refuted as a result**: 33 lines and 633 cases (631 round-4 baseline + 5 new − 3 deleted `year.rs`
unit tests) is exactly right, but the tree does not pass them. The Boss's run evidently drew one of
the 4-in-6 orderings in which the new test wins the race for `hs4_lock()`.

## Findings

Tier = the tier of the agent originally assigned (§3 roster: HS1 HS3 HS5 HS6 HS9 = O; HS2a HS2b HS4
HS7 = S). Severity: Critical / High / Med / Low. PASS requires zero Critical/High/Med.

| id | task | tier | branch | file:line | severity |
| --- | --- | --- | --- | --- | --- |
| QH6-01 | HS4 (test harness) | S | `hs/HS4-qa6` | `tests/homeschool_tests.rs:1348`, `:217-224`, `:257-262`, `:706-707`, `:196-202` | **High** |
| QH6-02 | HS5 | O | `hs/HS5-qa6` | `src/client/components/homeschool/today.rs:94-109`; `src/server/api/homeschool.rs:423,437-444`; `src/shared/homeschool.rs:801-804,761-774` | **Med** |
| QH6-03 | HS5 | O | `hs/HS5-qa6` | `src/client/components/homeschool/year.rs:339`, `:368`, `:377`, `:334`, `:409` | **Med** |
| QH6-04 | HS5 | O | `hs/HS5-qa6` | `src/client/components/homeschool/year.rs:313-320` | Low |
| QH6-05 | Boss (plan) + HS5 | — | `hs/HS5-qa6` | `docs/homeschool/PLAN_HOMESCHOOL.md:157-159` vs `src/client/components/homeschool/today.rs:126-131` | Low |
| QH6-06 | Boss (docs) | — | fix close | `docs/HANDOFF.md` (tail) | Low |

---

### QH6-01 (High) — the round-5 fix wave's own storage proof is order-dependent, and `cargo test --features server` fails on `main`

**What ships.** `tests/homeschool_tests.rs:1326` `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating`
— the Boss-added storage proof for QH5-02 — asserts at `:1348` that the fixture's `Old Tales` week 2
row is dealt out as `part 1 of 2` on Monday:

```rust
let monday = row.cells[0].first().cloned().expect("part 1 of 2 on Monday");
assert_eq!(monday.part, Some((1, 2)));                 // tests/homeschool_tests.rs:1348
assert_eq!(monday.days, None, "the fixture row floats");
```

That is true only while `subjects.days` for `Old Tales` is still the fixture's `"MW"`. It is not, in
general: `hs4_f_toggle_lesson_together_writes_exactly_the_two_boys_sharing_the_week` calls
`widen_to_every_day(pool, old_tales, true)` at `:707`, which is
`hs::set_subject_schedule(pool, subject_id, "MTWRFSU", shared)` (`:257-262`) — a **permanent** write to
the shared `subjects` table. `reset_homeschool_state` (`:217-224`) clears only `lesson_log`,
`lesson_extras` and `enrollments`; `load_fixture` (`:196-202`) is `loader::insert_missing`, which by
design never updates a row that already exists. So once `hs4_f` has run in a given test process,
`Old Tales` runs `MTWRFSU` for the rest of it: intersected with `MTWRF` that is five days, one
floating row over five days, and H3 rule 5 deals `part 1 of 5`.

**Why it is wrong against the contract.** §3 HS4 Accept and the QA loop's DONE gate require
`cargo test --features server` green; PLAN §3's test rules forbid shipping a red gate. It also
defeats the guard's own purpose: QH5-02's product fix is correct (verified by reading `year.rs:339,377`
and by the source-shape guard in `glyph_tests`), but its **only** storage proof — the one R-14's CLOSED
line cites as evidence — only executes its assertion when it happens to acquire `hs4_lock()` before
`hs4_f`. R-14 is therefore CLOSED on evidence that does not reliably exist.

**Concrete reproduction** (real data, real order, observed on this box):

```
$ cargo test --features server --test homeschool_tests hs4_i_a_text_edit_from_the_year_sheet
test hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating ... ok
test result: ok. 1 passed; 0 failed; ... 21 filtered out

$ cargo test --features server --test homeschool_tests -- --test-threads=1
test hs4_f_toggle_lesson_together_writes_exactly_the_two_boys_sharing_the_week ... ok
...
test hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating ... FAILED
thread '...' panicked at tests\homeschool_tests.rs:1348:5:
assertion `left == right` failed
  left: Some((1, 5))
 right: Some((1, 2))
test result: FAILED. 21 passed; 1 failed; ... finished in 0.93s
```

`Some((1, 5))` is the signature: five day columns, not two. Six default-parallel runs gave
`ok / ok / ok / ok / FAILED / FAILED`; two `--test-threads=1` runs gave `FAILED / FAILED`. The full
suite (`--no-fail-fast`) gave 33 `test result:` lines, 632 passed, **1 failed**, 2 ignored, with every
other binary green — no R-4 item 1–6 and no R-12 sighting.

**Not a catalogued flake.** R-4 items 1–6 name `realtime_tests::t1_2_7_*`, `loop_tests::t2_6_*`,
`backup_tests::restore_drill_*`, `homeschool_db_tests::a_bad_file_beside_a_good_one_*`,
`realtime_tests::t1_2_3_*` and `service_tests::a_startup_bind_failure_*`; R-12 names the same
`homeschool_db_tests` case. None is in `homeschool_tests`, and every one of those binaries passed in
both full runs here. R-4/R-12 are wall-clock or capture flakes under concurrent load; this is a
deterministic intra-binary state dependence on an idle machine.

**Which existing tests fail to catch it and why.** None can: the defect *is* a test, and the only
mechanism that would surface it — running the binary in a fixed order — is what nobody ran. Every
other `hs4_*` case either widens the subject it depends on itself or depends on a subject nobody
widens (`Twice Told`, `Fables`), which is why 21 of 22 pass in every order.

**Secondary defect in the same class**, worth fixing in the same change:
`hs4_i_editing_a_pinned_second_reading_never_overwrites_the_first` (`:1226`) pins `Fables` week 1
ordinal 2 to `"M"` **before** its first assertions (`:1268-1274`) and only restores the fixture row at
`:1288`. If any of those assertions or the two `.expect()`s above them fires, `assignments` keeps
`days = 'M'` for the rest of the process and nothing restores it. Both new proofs are written to
restore, but only on the happy path.

**Complete solution — `hs/HS4-qa6`, tier S.**

1. Make the reset actually reset. In `tests/homeschool_tests.rs`, replace `reset_homeschool_state`
   (`:215-224`) with:

```rust
/// Wipe every homeschool table — the curriculum rows included — so a test
/// starts from a known-empty state and `load_fixture` re-inserts the fixture
/// exactly as it is committed.
///
/// The curriculum tables have to go too (QA round 6, QH6-01): `load_fixture`
/// is `loader::insert_missing`, which never updates a row it already sees, and
/// `widen_to_every_day` writes `subjects.days = 'MTWRFSU'` permanently. Leaving
/// `subjects` behind made every assertion about a subject's *dealt-out shape*
/// depend on which earlier test in this binary had run — which is exactly how
/// `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating` came
/// to read `part 1 of 5` after `hs4_f_*` widened `Old Tales`. Deleted
/// child-first rather than leaning on `ON DELETE CASCADE`, so the order is
/// explicit and does not depend on `PRAGMA foreign_keys`.
async fn reset_homeschool_state(pool: &SqlitePool) {
    for table in [
        "lesson_log",
        "lesson_extras",
        "enrollments",
        "assignments",
        "term_notes",
        "subjects",
        "curricula",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(pool)
            .await
            .unwrap_or_else(|err| panic!("clear {table}: {err}"));
    }
}
```

   Every test in the file already calls `reset_homeschool_state` then `load_fixture` then looks its
   subject ids up by name (`subject_id`, `:204-213`), so nothing caches an id across the reset and the
   change is behaviour-preserving for the other 21 cases.

2. Belt and braces in the proof itself, so it states its own precondition. In
   `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating`, directly after
   `let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;` (`:1331`):

```rust
    // The shape this proof reads is the fixture's own: `Old Tales` runs on M
    // and W, `shared` (a reading subject, §4 default). Stated here rather than
    // assumed, because `subjects` is process-wide (QH6-01).
    hs::set_subject_schedule(pool, old_tales, "MW", true)
        .await
        .expect("the fixture's own days for Old Tales");
```

3. Move the first proof's restore off the happy path. In
   `hs4_i_editing_a_pinned_second_reading_never_overwrites_the_first`, read the two grid facts into
   locals and assert them **after** the restore, exactly as the same test already does for its final
   `assert_eq!(rows, …)`: replace `:1268-1274`

```rust
    assert_eq!(monday.text.as_deref(), Some("The Patient Heron"));
    assert_eq!(
        monday.ordinal, 2,
        "QH5-01: the Monday entry is ordinal 2 however early it falls"
    );
```

   with

```rust
    let dealt = (monday.text.clone(), monday.ordinal);
```

   and add, beside the existing final assertion (after the restore call at `:1288`):

```rust
    assert_eq!(
        dealt,
        (Some("The Patient Heron".to_string()), 2),
        "QH5-01: the Monday entry is ordinal 2 however early it falls"
    );
```

4. Regression guard, in `tests/homeschool_tests.rs` beside the other `hs4_i_*` cases:

```rust
/// QA round 6, QH6-01. The reset has to undo a subject-schedule write, or every
/// assertion about a dealt-out shape in this binary is order-dependent.
#[tokio::test]
async fn hs4_i_a_widened_subject_does_not_survive_the_reset() {
    let _guard = hs4_lock().await;
    let pool = db::pool().await.expect("pool");
    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;
    widen_to_every_day(pool, old_tales, true).await;

    reset_homeschool_state(pool).await;
    let curriculum_id = load_fixture(pool).await;
    let old_tales = subject_id(pool, curriculum_id, "Old Tales").await;
    let days: (String,) = sqlx::query_as("SELECT days FROM subjects WHERE id = ?1")
        .bind(old_tales)
        .fetch_one(pool)
        .await
        .expect("the subject after a reload");
    assert_eq!(
        days.0, "MW",
        "QH6-01: reset_homeschool_state must restore the fixture's own subject days"
    );
}
```

5. Gate the branch with `cargo test --features server --test homeschool_tests -- --test-threads=1`
   **and** two full `cargo test --features server` runs, and record the serial run in
   `docs/HANDOFF.md` as the gate that would have caught this.

---

### QH6-02 (Med) — the nudge calls the week done while the **Finish week** button that answers it is withheld

**What ships.** `today.rs:97-109`:

```rust
pub fn week_is_complete(group: &TogetherGroup) -> bool {
    let logged: u32 = group.boys.iter().map(|boy| boy.done_count + boy.skipped_count).sum();
    let total: u32 = group.boys.iter().map(|boy| boy.total_count).sum();
    total > 0 && logged >= total
}
```

and `nudge_line` prints "Week N done — start week N+1?" on that alone, while the **Finish week →**
button inside the same banner is rendered `if group.can_finish_week` (`today.rs:277`).

**Why it is wrong against the contract.** H2: "**Finish week** is offered (parent only) when every
occurrence of the week is done or skipped, **or** today ≥ the last school day. The Today footer nudges
… 'Week 3 done — start week 4?' when complete". The two are meant to be one state. They are computed
from two different things that disagree for a **paused boy in an unpaused group** — a state the server
explicitly supports and comments on (`api/homeschool.rs:437-441`: "The group as a whole reads
'School's out' only once every member is paused; a single paused boy still has an empty personal
block, but his brothers' Together items stay live"):

* `sched::today_view` returns early on `enrollment.paused` (`shared/homeschool.rs:801-804`), so the
  paused boy's `done_count`, `skipped_count` **and** `total_count` are all `0`. He contributes nothing
  to either side of `week_is_complete`.
* `sched::can_finish_week_with_extras` (`shared/homeschool.rs:761-774`) does **not** check `paused`;
  it evaluates his real occurrences. `get_homeschool_today` folds it into `all_can_finish` for every
  member (`api/homeschool.rs:423`), so one paused boy with an unfinished week sets
  `can_finish_week = false` for the whole group.

Before this wave the nudge was driven by `can_finish_week` too, so the pair could not disagree. The
QH5-03 fix decoupled them without gating on the server's own answer.

**Concrete reproduction.** Isaiah (`profile_id 1`) and Nathaniel (`profile_id 2`) both enrolled on
`sample-year` week 2, `school_days = MTWRF`, `week_started_on = 2026-09-07` (a Monday). Today is
Wednesday `2026-09-09` (not the last school day, so H2's second clause is not in play).

1. A parent taps **Pause school** for Nathaniel → `set_paused(2, true, auth)`; `enrollments.paused = 1`
   for him only. The group's `paused` stays `false` (it is `all(...)`).
2. Isaiah works through the week and every one of his occurrences and in-span extras is ticked or
   skipped — 11 of 11.
3. `get_homeschool_today("2026-09-09")` returns one group with
   `boys = [Isaiah { done_count: 10, skipped_count: 1, total_count: 11 }, Nathaniel { 0, 0, 0 }]`,
   `can_finish_week = false` (Nathaniel's week is not complete and `2026-09-09 < 2026-09-11`),
   `paused = false`, `days_on_week = 2`.
4. The phone renders the header chip `Week 2 of 3 · Term 2 · 10 done · 1 skipped / 11` and, directly
   under it, the banner **"Week 2 done — start week 3?"** — with **no Finish week button**, because
   `can_finish_week` is false. The parent is told the week is finished and given nothing to finish it
   with. `nudge_line` also swallows the fortnight sentence for that group until day 14, which is
   correct, and never reaches the last-school-day sentence, which is also correct — the only wrong
   thing on screen is the first sentence.

This is the same "surface contradicts itself" shape round 4 filed as QH4-01 and round 5 filed as
QH5-03, one step further along: the chip and the nudge now agree, and the nudge and the button do not.

**Which existing tests fail to catch it and why.** `today.rs`'s three `nudge_line` unit tests all use
`group(week, can_finish_week, days_on_week)` (`:780-805`), which builds a **single** boy with
`total_count: 11`; there is no case with a second boy at `0/0/0`, and no case where
`week_is_complete` is true while `can_finish_week` is false. `glyph_tests::hs5_b_*` renders
`fixture_today_view()`, whose two groups have nothing logged, so the "done" branch is never taken
there at all — the new `!html.contains("done — start week")` assertion proves only that the branch is
*not* entered. `hs4_j_get_homeschool_today_reports_nobody_enrolled_and_a_paused_boys_empty_lists`
covers a paused boy alone in his group, which the `group.paused` guard short-circuits.

**Complete solution — `hs/HS5-qa6`, tier O.** Gate the client's answer on the server's. In
`today.rs`, replace `week_is_complete` (`:94-109`) with:

```rust
/// H2: "complete" is every occurrence and every in-span extra logged — the rows
/// `header_chip_text` sums (H3 rule 8 + rule 10), so the chip and the nudge
/// cannot disagree — **and** a week the server will actually let the parent
/// finish, so the nudge and the button cannot disagree either.
///
/// The second half is QA round 6, QH6-02: `sched::today_view` returns early on
/// a paused enrollment, so a paused brother contributes `0` to both sums while
/// `can_finish_week_with_extras` — which does not check `paused` — still reads
/// his unfinished week and holds `all_can_finish` back. Without the gate the
/// banner asked "Week N done — start week N+1?" with no **Finish week** button
/// beside it. Dropping the old `total > 0` floor is part of the same gate: a
/// week with nothing in it at all is "done" only when the server agrees it can
/// be finished.
pub fn week_is_complete(group: &TogetherGroup) -> bool {
    if !group.can_finish_week {
        return false;
    }
    let logged: u32 = group
        .boys
        .iter()
        .map(|boy| boy.done_count + boy.skipped_count)
        .sum();
    let total: u32 = group.boys.iter().map(|boy| boy.total_count).sum();
    logged >= total
}
```

`nudge_line` is unchanged, and so is every existing assertion: `group(2, true, 3)` with
`done_count = 10, skipped_count = 1, total_count = 11` still takes the first branch;
`group(2, true, 4)` (4 of 11 logged) still takes the third; `group(3, false, 15)` and
`group(3, true, 15)` still take the fortnight branch; `group(3, false, 13)` is still `None`; and the
`glyph_tests` fixture group (`can_finish_week = true`, nothing logged) still renders the third
sentence and exactly one **Finish week**.

Unit test, in `today.rs`'s `mod tests` beside `the_last_school_day_offers_finish_week_without_calling_the_week_done`:

```rust
    #[test]
    fn a_paused_brothers_unfinished_week_never_calls_the_group_done() {
        // QH6-02: `today_view` zeroes a paused boy's three counts, so the
        // group's sums look complete while the server's `all_can_finish` —
        // which does not check `paused` — withholds the Finish week button.
        // The nudge must not ask a question the banner cannot answer.
        let mut group = group(2, false, 3);
        group.boys[0].done_count = 10;
        group.boys[0].skipped_count = 1;
        group.boys.push(BoyToday {
            user_id: 2,
            name: "Nathaniel".into(),
            due_today: Vec::new(),
            catch_up: Vec::new(),
            done: Vec::new(),
            done_count: 0,
            skipped_count: 0,
            total_count: 0,
        });
        assert!(
            !week_is_complete(&group),
            "QH6-02: a paused brother's zeroed counts must not read as a finished week"
        );
        assert_eq!(nudge_line(&group), None);
    }
```

(fails before the change with `Some("Week 2 done — start week 3?")`, passes after). `BoyToday` is
already imported in that module.

---

### QH6-03 (Med) — the Year cell sheet's text **Save** now silently discards a days edit the parent has typed

**What ships.** `year.rs:339` computes `let stored_days = occurrence.days.as_deref().map(days_to_string);`
and the text **Save** button sends `days: stored_days.clone()` (`:377`), unconditionally. The days
control's own signal (`let mut days = use_signal(|| entry_days.clone());`, `:334`) is read only by
**Save days** (`days: Some(days())`, `:409`).

**Why it is wrong against the contract.** H6 item 6 / D-5 make the cell sheet's controls "inline edit
of `assignment.text` / `days` **for that week**", and `upsert_assignment` replaces the whole row, which
is why the module's own doc comment (`year.rs:313-320`) states the invariant "each of them sends all
three: the text control carries the days on screen, the days control carries the text on screen, and
neither can quietly erase the other's value". After this wave the text control no longer carries the
days on screen and **can** quietly erase the other's value. This is a behaviour regression: before
`7a1245b` the text Save sent `pinned_days(&days())`, i.e. what the parent had typed. QH5-02 was right
that an *untouched* control must not be written back as a pin; the fix as prescribed threw away the
*touched* case with it.

**Concrete reproduction.** Committed fixture, `Fables` week 1 ordinal 2 ("The Patient Heron"), already
pinned to Monday (`assignments.days = 'M'`) — the exact row QH5-01's own storage proof creates.

1. Parent opens Year → week 1 → the Monday `Fables` cell. `CellEntry` renders with
   `entry_days = "M"` (from `entry_days(&row, &days, Some(id))`), so the days box shows `M` and
   `stored_days = Some("M")`.
2. The parent decides to move it to Wednesday **and** retype it. They type
   `The Patient Heron, retold` in the text box and `W` in the days box.
3. They tap **Save** — the first button, sitting on the text row.
4. `SchoolAction::EditAssignment { subject_id, week: 1, ordinal: 2, text: "The Patient Heron, retold",
   detail: None, days: Some("M") }` → `upsert_assignment` writes `text` and `days = 'M'`. The row is
   still on Monday. The `W` the parent typed is gone, with no toast, no error and no visual change to
   the box — which still reads `W` until the resource refetches and the component re-renders.

The same sequence on the pre-wave code wrote `days = 'W'`. A parent who taps **Save days** instead is
fine; a parent who taps the sheet's primary button is not.

**Which existing tests fail to catch it and why.**
`glyph_tests::hs5_qa3_the_year_cell_sheet_edits_the_days_of_one_week_not_of_every_week` is a
source-shape guard: it asserts the *presence* of `let stored_days = …` and `days: stored_days.clone(),`
and the *absence* of `pinned_days(` — all of which stay true under the defect, because the defect is
that `stored_days` is used unconditionally. `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating`
calls `upsert_assignment` directly with `monday.days` and never exercises the component's two-control
interaction. No test in the tree drives `CellEntry`'s signals.

**Complete solution — `hs/HS5-qa6`, tier O** (same branch as QH6-02; also closes QH6-04). Make the
decision a named pure function so it can be unit-tested, and restore the one thing the deleted
`pinned_days` was actually good for (a blank control never reaching the schema's `GLOB`).

In `year.rs`, beside `entry_days`:

```rust
/// Which `days` the cell sheet's **text** Save writes back.
///
/// The days control is prefilled with the row's *resolved* days, so writing
/// that back on a text-only edit pinned a floating row and dropped its rule-5
/// part labels (QA round 5, QH5-02) — while the row's stored value is `None`,
/// meaning "inherit the subject's". But writing the stored value back
/// *unconditionally* threw away a days edit the parent had typed and not yet
/// saved (QA round 6, QH6-03), which the pre-QH5-02 code did land. So: the
/// stored value while the control still shows its prefill, the parent's own
/// value once they have changed it — and never a blank string, which the
/// schema's `GLOB` would take (the one thing the deleted `pinned_days` guarded).
pub fn text_save_days(stored: Option<&str>, prefill: &str, typed: &str) -> Option<String> {
    if typed.trim() == prefill.trim() {
        return stored.map(str::to_string);
    }
    let typed = typed.trim();
    (!typed.is_empty()).then(|| typed.to_string())
}
```

In `CellEntry`, replace `:339` and the text Save's `onclick` capture and payload:

```rust
    let stored_days = occurrence.days.as_deref().map(days_to_string);
    // The value the days control was prefilled with, so a Save can tell an
    // untouched control from one the parent has changed (QH5-02 / QH6-03).
    let prefill = entry_days.clone();
```

```rust
                        onclick: {
                            let detail = detail.clone();
                            let stored_days = stored_days.clone();
                            let prefill = prefill.clone();
                            move |_| {
                                on_action
                                    .call(SchoolAction::EditAssignment {
                                        subject_id,
                                        week,
                                        ordinal,
                                        text: draft(),
                                        detail: detail.clone(),
                                        days: text_save_days(
                                            stored_days.as_deref(),
                                            &prefill,
                                            &days(),
                                        ),
                                    })
                            }
                        },
```

Leave **Save days** at `days: Some(days())`.

Unit tests, in `year.rs`'s `mod tests` (they replace the value the deleted
`an_empty_days_control_inherits_the_subject_rather_than_writing_nonsense` carried):

```rust
    #[test]
    fn a_text_save_leaves_an_untouched_days_control_alone_and_honours_a_changed_one() {
        // QH5-02: untouched → the row's stored value; `None` keeps it floating.
        assert_eq!(text_save_days(None, "MW", "MW"), None);
        assert_eq!(text_save_days(None, "MW", " MW "), None);
        assert_eq!(text_save_days(Some("M"), "M", "M"), Some("M".to_string()));
        // QH6-03: changed → what the parent typed, never silently discarded.
        assert_eq!(text_save_days(None, "MW", "MF"), Some("MF".to_string()));
        assert_eq!(text_save_days(Some("M"), "M", "W"), Some("W".to_string()));
        // The deleted `pinned_days`'s one piece of value: blank is not a pin.
        assert_eq!(text_save_days(Some("M"), "M", "   "), None);
        assert_eq!(text_save_days(None, "MW", ""), None);
    }
```

Update the source-shape guard in `tests/glyph_tests.rs::hs5_qa3_the_year_cell_sheet_edits_the_days_of_one_week_not_of_every_week`:
replace `assert!(one_line(&year).contains("days: stored_days.clone(),"));` with

```rust
    assert!(
        one_line(&year).contains("days: text_save_days(stored_days.as_deref(), &prefill, &days(),)")
            || one_line(&year).contains("days: text_save_days("),
        "QH6-03: the text Save must decide between the stored value and a changed control: {year}"
    );
    assert!(
        one_line(&year).contains("let prefill = entry_days.clone();"),
        "QH6-03: the text Save needs the prefill to tell touched from untouched: {year}"
    );
```

and keep the existing `assert!(!year.contains("pinned_days("), …)` and the `let stored_days = …`
assertion as they are.

---

### QH6-04 (Low) — `CellEntry`'s doc comment states the invariant the wave broke

`year.rs:313-320` still reads: "Both parent controls write the **same row** … so each of them sends
all three: the text control carries the days on screen, the days control carries the text on screen,
and neither can quietly erase the other's value or the source's `detail` (QH3-02, QH3-04)." The first
clause has been false since `7a1245b` and the third is precisely QH6-03. A reader (or the next fixing
agent) taking the comment at its word will not look for the defect.

**Solution — `hs/HS5-qa6`, tier O**, applied together with QH6-03, which makes the sentence true again
except for the untouched-control case. Replace the second half of that doc comment with:

```rust
/// Both parent controls write the **same row** — `upsert_assignment` replaces
/// `text`, `detail` and `days` together — so each of them sends all three: the
/// days control carries the text on screen, and the text control carries the
/// row's stored `days` while the days control is untouched (QH5-02: its prefill
/// is the *resolved* days, which would pin a floating row) and the parent's own
/// value once they have changed it (QH6-03). Neither can quietly erase the
/// other's value or the source's `detail` (QH3-02, QH3-04).
```

---

### QH6-05 (Low) — H2's normative nudge list was never amended for the third sentence

`docs/homeschool/PLAN_HOMESCHOOL.md:157-159` still says, verbatim: "The Today footer nudges (never
auto-advances): 'Week 3 done — start week 4?' when complete; 'You've been on week 3 for 15 days' once
`today − week_started_on ≥ 14`." Two sentences. `today.rs:126-131` now ships a third — "Last school
day of week {N} — finish it now, or carry the rest into next week" — on the family's most-used
surface. The Boss's amendment commit `b09580d` amended the `LessonOccurrence` DTO line for `ordinal`
and nothing else, so the plan and the code disagree about what the footer says. The wave's own
precedent (QH3-04's HS4 "Do" amendment, QH4-03's and QH5-01's DTO amendments) is that a normative
change gets a plan line with its provenance before it ships.

**Solution — Boss amendment, then recorded on `hs/HS5-qa6`.** In §2 H2, replace the nudge sentence
with:

> The Today footer nudges (never auto-advances): "Week 3 done — start week 4?" when complete;
> "You've been on week 3 for 15 days" once `today − week_started_on ≥ 14`; and — the QA round 5
> amendment (QH5-03 / `docs/RESIDUAL.md` R-15, 2026-09-03) — "Last school day of week 3 — finish it
> now, or carry the rest into next week" when today has reached the last school day with work still
> open. The three are exclusive and are tested in that order, so the "done" sentence is never printed
> for a week that merely *may* be finished; **Finish week** is still offered under all three.

---

### QH6-06 (Low) — `docs/HANDOFF.md` carries nothing at all for the round-5 fix wave

The diff `a49a4ca..HEAD` touches `docs/RESIDUAL.md` and `docs/homeschool/PLAN_HOMESCHOOL.md` and no
other doc. `docs/HANDOFF.md` still ends at "## Boss, round-4 fix close (2026-09-03)": there is no
`H-HS4-qa5-*`, no `H-HS5-qa5-*`, and no round-5 fix-close section. This is the visible trace of the
two agents being interrupted — neither wrote their handoff — and it leaves three things unrecorded
that the wave's own conventions require:

* QH5-01's solution **item 10** says in as many words: "record the two `year.rs` unit-test deletions in
  `docs/HANDOFF.md`". Three unit tests were in fact deleted — `ordinals_come_from_first_appearance_across_the_week`,
  `a_daily_row_with_no_assignment_rows_has_no_ordinals_to_recover` and
  `an_empty_days_control_inherits_the_subject_rather_than_writing_nonsense`. R-13 and R-14 mention the
  deleted *helpers* in passing; `HANDOFF.md`, which is where §3's "mechanical class" of test changes is
  recorded and where round 5's "No test weakened" finding was evidenced, says nothing.
* Nothing records that the two storage proofs were written by the Boss rather than by the owning
  agents, which is exactly the provenance a later auditor needs (and is how QH6-01 got through).
* Nothing records the gate runs of the merge.

Neither deletion is itself a weakening — each removed test covered a helper that the round-5 solutions
deleted by name, and no Accept clause lost its guard (`text_save_days` in QH6-03 restores the one
behavioural assertion the third deletion carried). The finding is the missing record.

**Solution — Boss, at the round-6 fix close.** Append to `docs/HANDOFF.md`:

```markdown
## Boss, round-5 fix close (2026-09-03) — HS4-qa5 / HS5-qa5 merged, recovered from interrupted worktrees

Both agents were interrupted by a session ending before they committed or ran a gate. Their working
trees were recovered, committed as `c9691eb` (HS4-qa5) and `7a1245b` (HS5-qa5) and merged as `eee7ae3`
and `6f2fcb8`; **every gate on those branches was run by the Boss after the fact, not by the agents.**
The two `tests/homeschool_tests.rs` storage proofs that the R-13 and R-14 solutions name fell between
the two agents' file ownership and were written by the Boss in `2bf9555`.

**Tests deleted (§3 mechanical class, each named for deletion by its round-5 solution):**
- `year.rs::ordinals_come_from_first_appearance_across_the_week` and
  `year.rs::a_daily_row_with_no_assignment_rows_has_no_ordinals_to_recover` — both tested `row_ordinals`,
  which QH5-01 deleted; the behaviour they stood in for is now carried by
  `shared/homeschool.rs::hs3_b_a_pinned_later_ordinal_keeps_its_own_ordinal_ahead_of_an_earlier_row`
  and `glyph_tests::hs5_qa5_the_ordinal_an_edit_writes_to_is_the_rows_own`.
- `year.rs::an_empty_days_control_inherits_the_subject_rather_than_writing_nonsense` — tested
  `pinned_days`, which QH5-02 deleted. Its assertion (a blank control is `None`, never `Some("")`) was
  left unguarded until QA round 6 QH6-03 restored it in `text_save_days`.

**Gates at the merge:** `cargo fmt --check`, both clippy gates, `cargo test --features server` —
33 `test result:` lines, 633 cases. See QA round 6 QH6-01 for why the result was not reproducible.
```

---

## Gate output observed on this box

```
=== FMT ===
FMT_EXIT=0
=== CLIPPY SERVER ===
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.50s
CLIPPY_SERVER_EXIT=0
=== CLIPPY WASM ===
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.86s
CLIPPY_WASM_EXIT=0
```

`cargo test --features server` (run 1, default job count) — **exit 101**:

```
     Running tests\homeschool_tests.rs (target\debug\deps\homeschool_tests-b3cbdb54354866ad.exe)
running 22 tests
...
test hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating ... FAILED

failures:

---- hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating stdout ----
thread 'hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating' (14784)
panicked at tests\homeschool_tests.rs:1348:5:
assertion `left == right` failed
  left: Some((1, 5))
 right: Some((1, 2))

failures:
    hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating

test result: FAILED. 21 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.19s
error: test failed, to rerun pass `--test homeschool_tests`
```

`cargo test --features server --no-fail-fast` (run 2) — **exit 101**; 33 `test result:` lines,
**632 passed, 1 failed, 2 ignored**, the same single failure:

| binary | result |
| --- | --- |
| `unittests src\lib.rs` | ok. 305 passed |
| `unittests src\main.rs` / `src\bin\family_hub.rs` | ok. 0 passed (no cases) |
| `backup_tests` | ok. 11 passed |
| `calendar_tests` | ok. 15 passed |
| `ci_tests` | ok. 7 passed |
| `config_tests` | ok. 4 passed |
| `curriculum_tests` | ok. 9 passed |
| `db_tests` | ok. 11 passed |
| `docs_tests` | ok. 17 passed |
| `font_tests` | ok. 9 passed |
| `glyph_tests` | ok. 33 passed |
| `health_pool_closed_tests` / `health_tests` | ok. 1 / 2 passed |
| `homeschool_db_tests` | ok. 26 passed (no R-12 sighting) |
| `homeschool_loop_tests` | ok. 1 passed |
| **`homeschool_tests`** | **FAILED. 21 passed; 1 failed** |
| `http_tests` | ok. 14 passed |
| `loop_tests` | ok. 1 passed |
| `palette_tests` | ok. 6 passed |
| `photo_tests` | ok. 7 passed |
| `profiles_tests` | ok. 6 passed |
| `pwa_tests` | ok. 16 passed |
| `realtime_tests` | ok. 18 passed (no R-4 item 1 or 5 sighting) |
| `router_tests` | ok. 12 passed |
| `routine_tests` | ok. 10 passed |
| `screensaver_tests` | ok. 5 passed |
| `service_tests` | ok. 5 passed (no R-4 item 6 sighting) |
| `storage_tests` | ok. 9 passed, 1 ignored |
| `tls_tests` | ok. 8 passed |
| `tv_tests` | ok. 40 passed |
| `whiteboard_tests` | ok. 3 passed |
| Doc-tests | ok. 0 passed, 1 ignored |

Repeat study of the failing binary (idle box, one suite at a time):

```
run 1 (default parallel): test result: ok. 22 passed; 0 failed
run 2 (default parallel): test result: ok. 22 passed; 0 failed
run 3 (default parallel): test result: ok. 22 passed; 0 failed
run 4 (default parallel): test result: ok. 22 passed; 0 failed
run 5 (default parallel): test result: FAILED. 21 passed; 1 failed
run 6 (default parallel): test result: FAILED. 21 passed; 1 failed
run 1 (--test-threads=1):  test result: FAILED. 21 passed; 1 failed
run 2 (--test-threads=1):  test result: FAILED. 21 passed; 1 failed
alone (filtered):          test result: ok. 1 passed; 0 failed; 21 filtered out
```

## Round-1…5 findings: still fixed, or regressed

| Item | Status on `main` @ `2bf9555` | How verified here |
| --- | --- | --- |
| **QH5-01** (High) | **FIXED** | `src/shared/types.rs:391-403`: `ordinal: i64` with `#[serde(default = "first_ordinal")]` appended last after `days`, `fn first_ordinal() -> i64 { 1 }` beside it. `src/shared/homeschool.rs:581`: `ordinal: row.map_or(1, |row| row.ordinal)` in `occurrence()` — the row's own value at every call site (`spread_rows`'s both branches, the pinned loop, rule 3's untitled daily, rule 4's weekly), so a pinned later-ordinal row keeps its ordinal however early it falls. `grep -rn "LessonOccurrence {"` finds exactly four literals and all four fill the field (`shared/homeschool.rs:558`, `tv/fixture.rs:90`, and the `today.rs:865` / `year.rs:427` test fixtures). `today.rs:401,610` are both `let edit_ordinal = Some(occurrence.ordinal);`; `year.rs:301` is `ordinal: occurrence.ordinal,`; `row_ordinals`, `assignment_ordinals`, `edit_ordinal_for`, `group_items`, `item_date` and the `ordinals` prop on four components are gone (`grep` clean), as is the `BTreeMap` import in both files. TV: `tv/fixture.rs:110` sets `ordinal: 1` and nothing under `src/client/components/tv/` reads or sends the field — the kiosk deserialises it and never edits, and `#[serde(default = "first_ordinal")]` keeps an older payload deserialising. `hs3_b_a_pinned_later_ordinal_keeps_its_own_ordinal_ahead_of_an_earlier_row`, `glyph_tests::hs5_qa5_the_ordinal_an_edit_writes_to_is_the_rows_own` and `hs4_i_editing_a_pinned_second_reading_never_overwrites_the_first` all green here. The plan amendment is at `PLAN_HOMESCHOOL.md:413-425` with the QH5-01 provenance beside `days`'s QH4-03 one. |
| **QH5-02** (Med) | **FIXED in the product, its storage proof unreliable** | `year.rs:339,377`: the text Save sends `stored_days` (the row's own `occurrence.days`), `Save days` still sends `Some(days())` at `:409`, `pinned_days` is deleted. A *days* edit still works end to end (the control's value is what `Save days` sends; `upsert_assignment` runs it through `parse_days`). But see **QH6-01** — `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating` fails in most orderings, so the storage half of R-14's CLOSED claim is not reliably evidenced — and **QH6-03**, the new inverse defect the same change introduced. |
| **QH5-03** (Med) | **FIXED, with a new consequence** | `today.rs:97-131`: `week_is_complete` sums `done_count + skipped_count` against `total_count`, which are exactly the rows `header_chip_text:138-141` sums, and `merge_extras` (`shared/homeschool.rs:866-878`) counts an in-span extra into all three — so the chip and the nudge agree, and H3 rule 8 + rule 10 are both covered. The fortnight sentence now outranks the last school day (`:120`) and is reachable; the last-school-day sentence is third; **Finish week** stays behind `can_finish_week` at `:277`. `a_complete_week_nudges_towards_the_next_one`, `the_last_school_day_offers_finish_week_without_calling_the_week_done`, the precedence case in `a_fortnight_on_one_week_nudges_by_elapsed_days_instead` and `!html.contains("done — start week")` in `hs5_b_today_renders_the_fixture_the_way_h6_lays_it_out` are all green. The nudge/button disagreement this decoupling opened is filed as **QH6-02**, and the un-amended H2 as **QH6-05**. |
| **QH5-04** (Low) | **FIXED** | `src/server/api/homeschool.rs:1399-1418`: `check_extra_date` sits directly above `add_extra`, `#[cfg(feature = "server")]`, and reproduces HS4 (k)'s two error strings verbatim (`"scheduled_date must be a valid YYYY-MM-DD date"`, `"scheduled_date must be within a year of today"`, plus the `add_days` fallback). Both callers use it: `add_extra:1438` (after `check_date_window(&date)`) and `update_extra:1528` (before the pool is opened, after `require_session_or_cookie`). `hs4_k_add_extra_requires_a_session_and_bounds_scheduled_date` now asserts `update_extra(… "2099-01-01" …)` is an error, and is green. |
| QH4-01 | **still fixed** | `extras_complete` / `can_finish_week_with_extras` (`shared/homeschool.rs:743-774`) untouched by this wave; `get_homeschool_today` still hoists `extras` out of the span block (`api/homeschool.rs:397-423`); `hs3_i_*` and `hs4_h_an_unfinished_extra_inside_the_span_holds_finish_week_back` green. |
| QH4-02 | **still fixed** | `mark_all_done`'s `status.is_none() && !occurrence.shared` filter untouched; `hs4_h_mark_all_done_ticks_only_unticked_items_and_is_idempotent` green. |
| QH4-03 / R-11 | **still fixed** | `types.rs:387-390` keeps `days` with `#[serde(default)]`; `shared/homeschool.rs:577` still sets it from the stored value; `today.rs:609` computes `let days = occurrence.days.as_deref().map(days_to_string);` and both `EditAssignment` arms still send `days: days.clone()`; `glyph_tests::hs5_qa3_an_inline_text_edit_carries_the_rows_detail_and_days_through` and `hs4_i_a_pinned_rows_inline_text_edit_from_today_leaves_its_days_untouched` green. The `ordinal` field was appended **after** `days`, so the round-4 amendment's shape is intact. |
| QH3-01 | **still fixed** | `assets/tailwind.css` untouched by this wave; `ci_tests::every_tailwind_utility_named_under_components_has_a_rule_in_the_committed_css` green (7 passed). No new utility class appears in the diff. |
| QH3-02 | **still fixed** | Both `year.rs` Save buttons still capture and send `detail: detail.clone()` (`:367,377` and `:399,406`); both `today.rs` arms likewise; the `glyph_tests` `detail` guard green. |
| QH3-03 | **still fixed** | `today.rs`'s `year_complete` arm still renders the card **and** the parent-only **Back a week** button (`:241-269`); untouched by the diff. |
| QH3-04 | **still fixed** | `upsert_assignment` still takes and `parse_days`-validates `days`; `hs4_i_upsert_assignment_rejects_a_bad_days_string_and_writes_nothing` and `hs4_i_set_subject_schedule_rejects_th_and_writes_nothing` green. |
| QH3-05, QH3-06 | **still fixed** | `NoSchoolPlan` on Year/Month and the restart sentence are outside the diff; `glyph_tests` (33) and `tv_tests` (40) green. |
| QH2-01…QH2-07 | **still fixed** | Together Skip/Note fan-out, `UpdateExtra`, the notice banner, clear-then-set, the truthful import count, the `user_id` stamp and "School's out for Nathaniel" are all outside the diff; `homeschool_tests` (21 of 22), `homeschool_db_tests` (26), `curriculum_tests` (9), `glyph_tests` (33) green. One observation on QH2-02, below. |
| QH1-01…QH1-10 | **still fixed** | None of the ten touches a file in this diff. `hs4_n_a_skip_or_a_note_on_an_already_ticked_row_replaces_its_state`, `hs4_d_*`'s 500-char cap, `hs6_i_*`'s celebrate, `BoyChips` on Year/Month (`year.rs:105-112`, untouched), `before_span`, the pool-before-copy and the amended (e) wasm gate are all green/verified. |
| R-1…R-16 CLOSED claims | **R-13, R-15, R-16 and R-2 true; R-14 true in the product, over-claimed in its evidence** | Each CLOSED line was checked against the code above. R-2's closure is real: `LessonOccurrence` now carries the ordinal, so Today no longer has a row it cannot offer an edit for, and `edit_ordinal_for` (which returned `None` for exactly those rows) is gone — `today.rs:401,610` hand `Some(occurrence.ordinal)` to every row on screen. R-14's CLOSED line cites `hs4_i_a_text_edit_from_the_year_sheet_leaves_a_floating_row_floating` as its storage proof; that proof does not run its assertion reliably (**QH6-01**). R-4 and R-12 stand as recorded and neither fired in either full run here. |

## Observed, not filed (no contract clause is failed)

- `update_extra` now inherits `add_extra`'s ±365-day window, so an extra dated more than a year in the
  past can no longer be re-titled — only deleted. Unreachable inside a 36-week year (`get_month`'s
  window is a month, and `add_extra` bounded the date at creation), and `delete_extra` is still
  unbounded, so there is always a way out. Correct per HS4 (k) as written.
- `week_is_complete`'s summing over boys is safe in the direction that matters: `done_count +
  skipped_count ≤ total_count` holds per boy by construction in `today_view`/`merge_extras`, so a
  group sum can only be equal when every boy's is — the client can never call a week complete that
  the per-boy rule would not, except through the paused-boy hole filed as QH6-02.
- `CellEntry` does not close or re-read when a Save lands; the sheet stays open on a stale
  `occurrence` until the `homeschool_version`-keyed resource refetches. Two Saves in quick succession
  can therefore send a stale `days`. QH6-03's fix narrows this to the sub-second window; a full fix
  would close the sheet on Save, which H6 does not require.
- Round 5's own three observations stand unchanged: `School()`'s `SetWeek` arm calls
  `set_school_week` once per boy, `mark_all_done` carries no `auth`, and the header chip is inert
  until the first `today` round trip.
- `%TEMP%\familyhub-*` scratch directories are still left behind by each run (R-4 item 4); the
  `familyhub-hs4-tests-<pid>` directory from every `homeschool_tests` run here is still on disk.

## Notes for the fixing wave

1. **QH6-01 first, on its own.** It is the red gate, and the two Meds cannot be evidenced until the
   suite is deterministic. Land the `reset_homeschool_state` change, then run
   `cargo test --features server --test homeschool_tests -- --test-threads=1` (the run that catches
   this class) and two consecutive full `cargo test --features server` runs. `hs/HS4-qa6`, tier S.
2. QH6-02, QH6-03 and QH6-04 are all `hs/HS5-qa6`, tier O, and touch only `today.rs`, `year.rs` and
   two guards in `tests/glyph_tests.rs`. Nothing on the server, nothing on the TV, no DTO change — the
   `ordinal` and `days` amendments both stand as they are.
3. QH6-05 needs a Boss amendment of §2 H2 before the branch closes (same shape as QH5-01's DTO
   amendment); QH6-06 is the Boss's own `docs/HANDOFF.md` entry at the fix close, and should say
   plainly that the round-5 branches were recovered rather than agent-verified.
4. `docs/RESIDUAL.md` R-14's CLOSED line should be amended when QH6-01 lands, to name the proof as
   reliable rather than merely present.
