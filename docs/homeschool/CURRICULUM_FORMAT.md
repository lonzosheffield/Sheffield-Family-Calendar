# Curriculum file format (`*.toml`)

Normative for the loader in `src/server/homeschool/loader.rs` (HS1) and for anyone writing a
curriculum file by hand. Files live in `FAMILY_HUB_CURRICULA_DIR` (default `<data>\curricula`) and
are loaded at boot, **insert-missing-only**: rows already in the database are never overwritten, so
edits made from the phone survive a reboot. Use `family-hub.exe import-curriculum <file> --replace`
to replace a curriculum's rows wholesale.

The example below uses invented books. See `docs/homeschool/PLAN_HOMESCHOOL.md` H5 for the rules and
`tests/fixtures/curricula/sample-year.toml` for the fixture every test loads.

```toml
[curriculum]
slug        = "sample-year"      # ^[a-z0-9-]{1,64}$ — the stable identity of this curriculum
name        = "Sample Year"
weeks       = 3                  # 1..=104
term_weeks  = 1                  # optional, default 12
source_note = "Made up for tests" # optional

# One [[subject]] per row of the chart. `name` is unique within the file and is the join key
# that [[assignment]] rows use.
[[subject]]
name       = "Old Tales"
category   = "reading"           # daily | reading | weekly | free_read
source     = "Old Tales for Small People"   # optional book / resource label
days       = "MW"                # optional, default "MTWRF"; letters M T W R F S U, R = Thu, U = Sun, no repeats
shared     = true                # optional; default true for reading/weekly, false for daily
icon       = "bible"             # optional; one of the specials in glyphs.rs, else the category glyph
sort_order = 10                  # optional; default = file order

[[subject]]
name     = "Sums"
category = "daily"

# One [[assignment]] per subject per week per ordinal. Omit the row entirely for a week with nothing
# ("-----" in a chart). Two rows with ordinal 1 and 2 in the same week are two readings that week.
[[assignment]]
subject = "Old Tales"
week    = 1
ordinal = 1                      # optional, default 1
text    = "ch. 1 'The Lantern'"
detail  = "skip the last page"   # optional; parentheticals from the source
days    = "M"                    # optional per-week override of the subject's days

# Term-level notes.
[[term_note]]
term = 1
kind = "geography"               # geography | free_read | poetry
text = "Left and right depend on where you stand."

[[term_note]]
term = 1
kind = "free_read"
text = "The Otter's Almanac"
```

## Validation (all-or-nothing; the whole file is rejected on the first failure)

In order: `slug` pattern · `1 ≤ weeks ≤ 104` · `term_weeks ≥ 1` · every `category` and `kind` in its
set · every `days` string passes `parse_days` · subject names unique · every `assignment.subject`
names a subject in this file · `1 ≤ week ≤ weeks` · `(subject, week, ordinal)` unique ·
`1 ≤ term ≤ ceil(weeks / term_weeks)`. Unknown keys are errors (`deny_unknown_fields`). Errors name the
file and the line.

A `days` string may not be empty, may not repeat a letter and may not carry a letter outside
`MTWRFSU` — `"Th"` is a mistake for Thursday (which is `R`) and is rejected rather than guessed at.
Letters may be given in any order; they are stored in the canonical `M T W R F S U` order.

## Loading

* **At boot** the loader scans the directory and inserts only rows that are **missing**, keyed on
  `(slug, subject name, week, ordinal)`. It never updates and never deletes, so an assignment a
  parent retyped on the phone survives every reboot. A file that fails validation is logged at
  `warn` and skipped; a good sibling in the same directory still loads.
* **`import-curriculum <path>`** validates first and copies second: a bad path or a rejected file
  exits non-zero and writes nothing — not the copy, not a row. It then inserts the missing rows, as
  a boot would.
* **`import-curriculum <path> --replace`** additionally rewrites that slug's `subjects`,
  `assignments` and `term_notes` in one transaction. Subjects and assignments the file still names
  are **updated in place**, keeping their ids and therefore the boys' `lesson_log` history; only
  rows the file no longer contains are deleted, and the log rows deleted with them are counted in
  the command's output. The command runs in its own process, so after a `--replace` run
  `family-hub.exe stop` / `start` the service (or wait for the next School change) so open phones
  and the TV refetch the rewritten plan; the rows are already on disk.
