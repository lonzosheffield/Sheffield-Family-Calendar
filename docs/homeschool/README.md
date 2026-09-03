# Homeschool reference material

Owner-supplied source material for the Homeschool ("house") tab. Nothing in
here is parsed by the build; it is the reference the curriculum data model and
seed data are derived from.

Put files under `curriculum/`. Any format is fine: PDF, DOCX, XLSX, CSV,
images/photos of a printed plan, plain text, or a pasted outline in a `.md`.

Helpful, if you have it:

- One file (or subfolder) per subject, or per boy if their curricula differ.
- The school-year calendar: start date, end date, days off, term breaks.
- Which days of the week each subject is done, if that is fixed.
- Any weekly/daily pacing already written into the curriculum (e.g. "Lesson 12
  on day 45"), so "what do we do today" can be computed rather than typed in.

## Transcription status

- HS2a: AO Year 1 transcription weeks 1-18 done 2026-09-02, file in the
  gitignored `curriculum/` directory (`ao-year-1.toml`). Weeks 19-36 done by
  HS2b the same day; the file now covers all 36 weeks.

## Using School — for parents

The **School** tab (🏠, tab 2 of 6 on the phone) is where the boys' curriculum
for the year lives. It has one job most mornings: show exactly what to do
today, and let you tick things off as they happen.

- **Today** is the tab itself — there is no separate button for it. At the
  top: a chip reading `Week 3 of 36 · Term 1 · 14 done · 2 skipped / 22` (tap
  it to open **School settings**), then **Together** (the read-alouds and
  other shared work, once for every boy it covers), then one block per
  enrolled boy for his own work, then **This term** (a collapsed card with
  the geography concept, poetry book and free reads for reference). Tap a
  row's checkbox to tick it; tap the assignment text itself to edit it for
  that week; long-press for **Skip** or a **Note**.
- **Finish week** appears on the header chip's nudge banner once every item
  for the week is done or skipped (or you have been on the same week two
  weeks running) — tapping it moves everyone in that Together group on to
  the next week. Nothing advances on its own; forgetting to tap it just
  means tomorrow's Today still shows this week's leftovers as catch-up.
- **Year** (the toggle beside the header chip) shows all 36 weeks,
  Monday–Friday, as one grid — pick a week to preview it. Only the current
  week is dated and tickable; other weeks show what is coming and can be
  edited (the assignment text, which days a subject runs) ahead of time.
- **Month** (the same toggle) shows a calendar for one boy at a time, with a
  done/total count on each school day and a 📌 wherever you have added a
  task. Tap a day to open it.
- **Add task** lives in the Month view's day sheet: pick the boy, a title
  (up to 80 characters), a category (✏️ daily, 📖 reading, 🎨 weekly) and,
  optionally, more detail. Use it for anything outside the curriculum —
  copywork, extra reading, whatever that day needs. Added tasks show up on
  that boy's Today and TV lists on their date, with a 📌, and can be ticked,
  edited, skipped or deleted from the same day sheet.
- **Pause** ("School's out ⚽") lives in **School settings**, reached by
  tapping the header chip. Use it for a summer or a break — every surface
  shows "no school today" and nothing in the log is touched, so resuming
  later picks up exactly where you left off.
- **Enroll a boy** is also in School settings: pick him, a curriculum, a
  starting week and which days count as school days. The same screen holds
  **Unenroll** (keeps everything he has already logged) and each subject's
  days/shared toggles.

On the television, the boy whose profile is active sees his own part of
School — his curriculum items and any tasks a parent added for him,
tickable with the remote alone. Shared read-alouds are ticked on a phone by
whoever is holding the book, not on the TV (so the tick is not attributed to
the wrong boy).

## After replacing a curriculum file

After `family-hub.exe import-curriculum <file> --replace`, restart the service (`family-hub.exe stop` then `start`, elevated) or wait for the next School change, so open phones and the TV refetch the rewritten plan. The rows are already on disk.
