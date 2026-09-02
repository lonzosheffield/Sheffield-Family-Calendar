# WHITE TEAM — Homeschool tab, Plan v1

**Reviewer:** White (owner's side) · **Date:** 2026-09-02 · **Verdict: REWORK** — the bones are right, but as
drafted the morning costs too many taps, one boy at a time, and one acceptance test would put AmblesideOnline's
schedule into a public repo.

**Sources:** the plan; `docs/PLAN.md` §0/§2; `DESIGN_DIRECTION.md` §1–§3; `routine.rs`, `mobile/mod.rs`,
`tv/surface.rs`; `docs/homeschool/curriculum/` confirmed to hold three owner files (not read).
AO `amblesideonline.org/ao-y1-sch`: *"The following weekly readings should be broken up into daily readings in
whatever way works best for your family."* AO FAQ: *"we leave it up to you to decide how to tackle a week's list
day by day"*; *"Exams are optional--nobody at this website will be checking or grading your exams"*; on combining
children, *"Some families find combining for many books to be helpful."*

---

## 1. The morning walk

Open PWA → lands on Routine → tap **School** (1) → Today for *the first enrolled boy*. To see the second boy,
tap his chip (2); third (3); fourth (4). Four screens, four scrolls, and the same Genesis reading ticked four
times. Then roughly seven identical daily boxes per boy — ~28 taps of ceremony before a book is opened.

That is not "know exactly what to do today." It is "know exactly what Isaiah does today, then repeat."

**W-1 — CHANGE: Today opens on *Everyone*, not on a boy.** The default view is one list for the whole
family: **Together** (the shared readings + weekly work), then a short block per boy for his own daily work.
Boy chips stay, as a *filter*, not as the primary axis. If only one boy ends up enrolled, "Everyone" is his
list and costs nothing — so this is safe whatever the answer to Q1 is. Reasoning: at 7am the parent needs one
screen, in reading order, that they work down.

**W-2 — ADD: shared readings tick once for everyone.** AO Y1 is read *aloud*: Bible OT/NT, Shakespeare, poetry,
hymn, folksong, composer, artist study, nature study and most story books are read once to whoever is in the room.
The plan's model (enrollment per boy, one log row per boy per occurrence) makes the parent tick the same reading
four times. Fix: mark a subject `shared` (a column on `subjects`), render it once under **Together**, and have
`toggle_lesson` fan out one log row per enrolled boy in the same transaction. Server-side fan-out, no new UI.
This is the largest change here and the one the owner's sentence actually asks for.

**W-3 — ADD: "done" at the section level.** A "Mark all daily done" control on the *Every day* heading (and per
boy). Seven identical ticks × four boys, every day, will kill this tab inside a fortnight. One tap, undoable.

**W-4 — CHANGE: drop the four-way segmented control from the top of Today.** Today is the tab. Reach Week/Plan
from the footer week chip; reach Enroll only from the empty state and Settings. The morning screen should open
with the first thing to do above the fold, not with navigation the parent uses twice a year.

**W-5 — ADD: the narration prompt, free.** AO narrates after every reading; the plan calls it "a parenting
practice, not an app state" and then omits it. It *is* what to do today. Render it in the row's existing
parenthetical-why slot — `The Coming of the Romans **(then tell it back)**` — exactly the poster's voice
(§1.3, "(Quench your thirst.)"). Zero schema, zero state, one string. Highest value per line in this plan.

**W-6 — ADD: a note per occurrence.** One free-text line on the log row — *"stopped at p.40"*, *"Math: lesson 14"*.
Without it, tomorrow's Today view is wrong for every reading the family didn't finish, and "exactly what to do"
quietly becomes "roughly where you were." This also answers §5 Q3 outright (see W-16).

---

## 2. Over-built for a family of one

**W-7 — CUT: exam weeks (v1).** AO says exams are optional and ungraded; a Year-1 exam is a parent asking a
six-year-old to tell about Genesis. The plan pays for two schema columns, an alternate Today view that is a
*free-text checklist the parent types* (a whole new editable-list feature), three HS4 test branches and a state
machine at weeks 12/24/36. Delete it. Week 12 arrives in December; a later migration can add it if he misses it.

**W-8 — DEFER: the Plan editor (view 3).** A 36-week picker with every assignment in an editable field is a
data-entry app bolted to a morning app. Note the trap that makes it look load-bearing: the loader inserts
*missing rows only*, so a typo fixed in the TOML never reaches the database — the editor is currently the only
repair path. Replace it with two cheap things: (a) inline edit of the assignment text from the Today row itself
(parent, tap the text) — the only edit anyone makes in the moment; (b) `import-curriculum --replace` for bulk
fixes. Per-subject day toggles move to the Enroll/setup screen, a once-a-year decision.

**W-9 — CUT: the Week grid (view 2).** Twenty-odd subjects × five weekday columns on a 360 px phone gives tap
targets far under the 44 px the rest of this app holds itself to, and it answers a question Catch-up already
answers ("did we miss Tuesday?"). If something is wanted, a one-line "Week 3 · 14/22" progress strip on Today.

**W-10 — CHANGE: Enroll is an empty-state flow, not a permanent view.** Used once a year. The poster card
"No school plan yet → Enroll a boy" is already specified; that plus a Settings link is the whole feature.

**W-11 — KEEP: Catch-up, term notes (read-only), `import-curriculum`, free reads.** Catch-up is what makes the
manual pointer honest, and with four boys it is every week. Term notes carry the term's poetry book, which *is* a
today-item — keep the table, cut the editing, render one collapsed "This term" card. `import-curriculum` earns its
place only if it validates *and* can replace (W-8b); otherwise it is a file copy the owner can do in Explorer.

**W-12 — CHANGE: simplify the occurrence rule; stop saying "part 1 of 2".** The `j % rows.len()` / `j % days.len()`
pairing surprises (2 rows over 3 days → row0, row1, row0 again). Predictable instead: rows fill days in order,
leftover days empty, extra rows pile onto the last day. And a reading spanning two days should read *"(continue)"* —
the app does not know where part 1 ended; the family decides that at the table. Claiming a split that didn't
happen is the small kind of lie that makes someone stop trusting the screen.

---

## 3. Under-built

**W-13 — ADD: "we skipped this."** Default 7 silently drops unfinished items at week end. The parent needs to
say *skip it* — for the morning that went sideways, and for the book this family isn't doing this year. Add
`status TEXT NOT NULL DEFAULT 'done'` to `lesson_log` **now** (append-only migrations make it expensive later),
write `'skipped'` from a secondary action, and let skipped items leave Catch-up without pretending they were done.

**W-14 — ADD: an end, and a summer.** `current_week = 37` is specified as a dead end. What does the School tab
say for the twelve weeks of summer, and what does the TV render for three months? Give it: a "Year complete 🎉"
card, and a **Pause school** switch (parent) that shows "No school today" without unenrolling and losing the log.
Related shape question the owner should settle before the migration lands: `UNIQUE(profile_id)` on `enrollments`
means Year 2 next fall either overwrites Year 1 or needs a painful migration. Add `ended_on` and make the
uniqueness "one *active* enrollment per boy."

---

## 4. The week pointer (H2)

**W-15 — KEEP the manual pointer, with one nudge.** Three reasons: AO ships a weekly, undated schedule and tells
you to break it up your way; with four boys, sickness/travel/a bad morning is normal, so a date-driven pointer
would be wrong most weeks and the parent would spend the morning correcting the app instead of teaching; and a
calendar pointer needs a school-year calendar with holidays and breaks — configuration the owner maintains
forever. That is the failure mode of every homeschool app.

Its own failure is the parent forgetting to tap **Finish week** and living in week 3 for a fortnight. So: record
`week_started_on` (one column), and let the Today footer say **"Week 3 done — start week 4?"** once the week's
items are finished, and **"You've been on week 3 for two weeks"** after 14 days. Nudge, never auto-advance. And
surface **Finish week** at the *top* of Today once the week is complete, not only at the bottom of a long scroll.

---

## 5. The TV

**W-16 — CHANGE: ship the fourth panel, scoped to the boy's *own* work.** Routine item 8 ("Start your school work",
📚) stays as the doorway. Behind it the boy sees only what he can honestly tick himself — math, copywork, phonics,
recitation, foreign language, physical activity, poetry, plus his catch-up. Read-alouds are ticked by whoever is
holding the book, on the phone. This halves HS6, keeps D1's promise ("a child completes his whole list with the
remote alone"), and stops four boys racing to tick Genesis. The panel must also read well when school is done,
paused or on summer break — "No school today ⚽", not an empty card.
§5 **Q2 has an obvious answer** (yes, boys tick — it is the D1 premise, and routine ticking is already open on the
LAN): make it a default, delete the question. Same for **Q3** — they need the note (W-6); ship it, don't ask.

---

## 6. Design-language fit

**W-17 — CHANGE: one glyph per *category*, not per subject.** ~22 subject glyphs is noise at 7am and multiplies
the Fire-OS-tofu checklist row by 22. Use 📖 reading · ✏️ daily · 🎨 weekly · 📚 free reads, plus a handful of
specials (Math 🔢, Nature 🌿, Music 🎵). The eye then scans *kind*, which is what a hurried parent needs.
Also: the tab glyph should be **📚**, not 🏠 — 🏠 reads "home", and 📚 is already the seeded glyph on routine
item 8, which makes the link the owner wants between "start your school work" and the tab.

**W-18 — ADD: a section-heading treatment (the one thing the language lacks).** The routine is a flat list of 8;
School has four sections. The phone has the precedent — `h3 text-lg font-bold text-sheffield-dark` ("Extra tasks")
— reuse it verbatim. The **TV has none**, and the four-size scale has no small label: use the poster's own device
from §2.6, tracked caps (`TV_BODY_TEXT`, `font-bold`, `tracking-[0.35em]`, `text-slate-800`, uppercase). On-poster,
no new size, no golden-file change.
Two more decisions to write down. Row anatomy: glyph → checkbox → **subject** (title register) + **assignment text**
(body register, own line), why-slot carrying "(then tell it back)". And Catch-up signals lateness with
`sheffield-accent` as a **chip ground under slate-800 ink** ("from Mon") — an already-declared pair — never as row
ink, or HS5 will invent a red row and fail the palette suite.

---

## 7. Questions §5 misses

**W-19 — ADD to §5, in priority order.** (1) *Do you read the shared books once to all the boys, or separately?*
— shapes W-1/W-2, must be answered before HS1's schema. (2) *Is your school week Mon–Fri, or four days with a
co-op/errand day?* — a four-day week is common in CM homes and rewrites the H4 table. (3) *When does week 1 start?*
— needed for the W-15 nudge. (4) *What should the tab say all summer?* (W-14). (5) *Are both phones signed in as
parent?* — if only Dad's phone holds the cookie, Mom hits a wall on **Finish week** at 7am.

**W-20 — CHANGE: fix an N1 violation inside the plan itself.** HS2 acceptance (d) requires six exact AO week→title
strings (six week→title pairs) inside a **committed**
`tests/curriculum_tests.rs`. That publishes a slice of AO's schedule to a public repo — and HS8 is specified to
grep the diff for exactly those strings, so the plan's own QA gate fails on the plan's own test. The plan document
names them too (currently untracked: `git grep -i ambleside` hits only `.gitignore`). Fix: put the expectations in
a gitignored `expectations.toml` beside the curriculum, or assert a checksum instead of the text, and scrub the
strings from the plan before it is committed. The rest of N1 is right and verified — sources ignored, nothing AO
ever committed.
