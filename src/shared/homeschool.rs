//! **HS3** — the homeschool scheduling core.
//!
//! Normative spec: `docs/homeschool/PLAN_HOMESCHOOL.md` §2 H2 (the anchored
//! week pointer) and H3 (the occurrence rule, rules 1–10). Everything here is
//! **pure**: no clock, no database, no date crate (§0 / accept (e) — this module
//! compiles to `wasm32-unknown-unknown` for the phone and the kiosk, and the
//! only date type that crosses the wire is a `YYYY-MM-DD` string, exactly as
//! `docs/PROTOCOL.md` already says of every other date in `shared::types`).
//!
//! The three layers:
//!
//! 1. **Calendar arithmetic** — [`weekday`] (Sakamoto), [`add_days`],
//!    [`date_for`], [`last_school_day`]. `YYYY-MM-DD` sorts lexicographically
//!    in date order, so every comparison below is a plain string comparison.
//! 2. **The occurrence rule** — [`occurrences`] turns a [`WeekPlan`] plus an
//!    [`Enrollment`] into the dated [`LessonOccurrence`]s of one week.
//! 3. **The views** — [`today_view`], [`merge_extras`], [`together_view`],
//!    [`week_grid`], [`month_view`], which are what the server functions of
//!    HS4 return and the surfaces of HS5/HS6 render.

use serde::{Deserialize, Serialize};

use crate::shared::types::{
    BoyToday, DayItem, ExtraTask, LessonOccurrence, MonthDay, MonthView, TogetherOccurrence,
    WeekGrid, WeekGridRow,
};

// ---------------------------------------------------------------------------
// Weekday
// ---------------------------------------------------------------------------

/// A day of the school week.
///
/// The letters are the plan's (§4 default 3): `M T W R F S U`, with `R` for
/// Thursday and `U` for Sunday, **always in that order**. Homeschool weeks are
/// anchored on `week_started_on`, not on Sunday like the calendar tab, so
/// `Mon` being first here is a rendering order, never a week boundary.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    #[default]
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// The fixed order `M T W R F S U` that `parse_days` sorts into and every
    /// grid renders in.
    pub const ORDER: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    /// The single letter used in `subjects.days`, `assignments.days` and
    /// `enrollments.school_days`.
    pub fn letter(self) -> char {
        match self {
            Weekday::Mon => 'M',
            Weekday::Tue => 'T',
            Weekday::Wed => 'W',
            Weekday::Thu => 'R',
            Weekday::Fri => 'F',
            Weekday::Sat => 'S',
            Weekday::Sun => 'U',
        }
    }

    pub fn from_letter(c: char) -> Option<Self> {
        match c {
            'M' => Some(Weekday::Mon),
            'T' => Some(Weekday::Tue),
            'W' => Some(Weekday::Wed),
            'R' => Some(Weekday::Thu),
            'F' => Some(Weekday::Fri),
            'S' => Some(Weekday::Sat),
            'U' => Some(Weekday::Sun),
            _ => None,
        }
    }

    /// Position in [`Weekday::ORDER`], i.e. Monday is 0 and Sunday is 6.
    pub fn index(self) -> usize {
        match self {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        }
    }
}

/// Why a `days` / `school_days` string was rejected (H3 rule 1: "`parse_days`
/// rejects any letter outside `MTWRFSU` or a repeat").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DayError {
    /// A character that is not one of `M T W R F S U`.
    UnknownLetter(char),
    /// A day named twice, e.g. `"MM"`.
    RepeatedLetter(char),
}

impl std::fmt::Display for DayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DayError::UnknownLetter(c) => {
                write!(f, "{c:?} is not one of the day letters MTWRFSU")
            }
            DayError::RepeatedLetter(c) => write!(f, "day letter {c:?} appears twice"),
        }
    }
}

impl std::error::Error for DayError {}

/// Parse a `days` string into the days it names, **in `M T W R F S U` order**
/// however they were written, rejecting an unknown letter or a repeat
/// (H3 rule 1).
///
/// An empty string is `Ok(vec![])` — it names no day, which every caller
/// already treats as "no occurrence" (rule 4 and rule 5 both check for it).
/// The SQL `CHECK (days GLOB '[MTWRFSU]*')` keeps empty strings out of the
/// database anyway.
pub fn parse_days(letters: &str) -> Result<Vec<Weekday>, DayError> {
    let mut seen = [false; 7];
    for c in letters.chars() {
        let day = Weekday::from_letter(c).ok_or(DayError::UnknownLetter(c))?;
        if seen[day.index()] {
            return Err(DayError::RepeatedLetter(c));
        }
        seen[day.index()] = true;
    }
    Ok(Weekday::ORDER
        .into_iter()
        .filter(|day| seen[day.index()])
        .collect())
}

/// Render days back into a `days` string in `M T W R F S U` order.
pub fn days_to_string(days: &[Weekday]) -> String {
    Weekday::ORDER
        .into_iter()
        .filter(|day| days.contains(day))
        .map(Weekday::letter)
        .collect()
}

/// `a ∩ b`, in [`Weekday::ORDER`] order (H3 rule 1's intersection with
/// `enrollment.school_days`).
fn intersect_days(a: &[Weekday], b: &[Weekday]) -> Vec<Weekday> {
    Weekday::ORDER
        .into_iter()
        .filter(|day| a.contains(day) && b.contains(day))
        .collect()
}

// ---------------------------------------------------------------------------
// Categories and log status
// ---------------------------------------------------------------------------

/// `subjects.category`, the four kinds of content in §1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Daily,
    Reading,
    Weekly,
    FreeRead,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Daily => "daily",
            Category::Reading => "reading",
            Category::Weekly => "weekly",
            Category::FreeRead => "free_read",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "daily" => Some(Category::Daily),
            "reading" => Some(Category::Reading),
            "weekly" => Some(Category::Weekly),
            "free_read" => Some(Category::FreeRead),
            _ => None,
        }
    }

    /// §4 default 5: `reading` / `weekly` subjects are read aloud to everyone,
    /// `daily` work is each boy's own, unless the TOML or settings say
    /// otherwise.
    pub fn shared_by_default(self) -> bool {
        matches!(self, Category::Reading | Category::Weekly)
    }
}

/// `lesson_log.status` / `lesson_extras.status`. A missing row (or a `NULL`
/// status on an extra) means "still to do"; there is no third state.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStatus {
    Done,
    Skipped,
}

impl LogStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LogStatus::Done => "done",
            LogStatus::Skipped => "skipped",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "done" => Some(LogStatus::Done),
            "skipped" => Some(LogStatus::Skipped),
            _ => None,
        }
    }
}

/// `term_notes.kind` — the three per-term reference texts (H1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermNoteKind {
    Geography,
    FreeRead,
    Poetry,
}

impl TermNoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TermNoteKind::Geography => "geography",
            TermNoteKind::FreeRead => "free_read",
            TermNoteKind::Poetry => "poetry",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "geography" => Some(TermNoteKind::Geography),
            "free_read" => Some(TermNoteKind::FreeRead),
            "poetry" => Some(TermNoteKind::Poetry),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Calendar arithmetic, hand-rolled (§0 / H3: no date crate here)
// ---------------------------------------------------------------------------

/// The supported year range. Wide enough for any curriculum and narrow enough
/// that a corrupt string can never produce a date the SQLite `DATE` columns
/// would not accept.
const MIN_YEAR: i32 = 1;
const MAX_YEAR: i32 = 9999;

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Strictly parse `YYYY-MM-DD`. Rejects anything else, including a real date
/// written some other way — the wire format is the only accepted spelling.
fn parse_date(date: &str) -> Option<(i32, u32, u32)> {
    let bytes = date.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let year: i32 = date[0..4].parse().ok()?;
    let month: u32 = date[5..7].parse().ok()?;
    let day: u32 = date[8..10].parse().ok()?;
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    if day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn format_date(year: i32, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// The weekday of a `YYYY-MM-DD` date, by **Sakamoto's method**. `None` when
/// the string is not a valid date (`"2100-02-29"` is not: 2100 is not a leap
/// year).
pub fn weekday(date: &str) -> Option<Weekday> {
    let (year, month, day) = parse_date(date)?;
    // Sakamoto's table, offset per month, with January and February counted
    // in the previous year so the leap day falls at the end.
    const TABLE: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = year;
    if month < 3 {
        y -= 1;
    }
    let sunday_based =
        (y + y / 4 - y / 100 + y / 400 + TABLE[(month - 1) as usize] + day as i32).rem_euclid(7);
    Some(match sunday_based {
        0 => Weekday::Sun,
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        _ => Weekday::Sat,
    })
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = ((month as i64) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + (day as i64) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

/// `date` shifted by `delta` days, still as `YYYY-MM-DD`. `None` when `date`
/// does not parse or the result leaves the supported year range.
pub fn add_days(date: &str, delta: i32) -> Option<String> {
    let (year, month, day) = parse_date(date)?;
    let (year, month, day) = civil_from_days(days_from_civil(year, month, day) + delta as i64);
    if !(MIN_YEAR..=MAX_YEAR).contains(&year) {
        return None;
    }
    Some(format_date(year, month, day))
}

/// H2: the week's calendar span is the **7 days from `week_started_on`**, so
/// the occurrence date for weekday `day` is the first (and only) date in that
/// span whose weekday is `day`.
///
/// A week that starts on a Wednesday therefore puts Monday and Tuesday in the
/// *following* calendar week — which is exactly what §4 default 2's "no
/// Monday arithmetic" means: the parent decides when a week starts.
pub fn date_for(week_started_on: &str, day: Weekday) -> Option<String> {
    let start = weekday(week_started_on)?;
    let offset = (day.index() + 7 - start.index()) % 7;
    add_days(week_started_on, offset as i32)
}

/// H2: the **last school day** of the week is the last date in the 7-day span
/// whose weekday is in `school_days`. `None` when `school_days` is empty or
/// the anchor does not parse.
pub fn last_school_day(week_started_on: &str, school_days: &[Weekday]) -> Option<String> {
    let start = weekday(week_started_on)?;
    let offset = school_days
        .iter()
        .map(|day| (day.index() + 7 - start.index()) % 7)
        .max()?;
    add_days(week_started_on, offset as i32)
}

/// The 7-day span `[week_started_on, week_started_on + 6]` (H2), inclusive.
pub fn week_span(week_started_on: &str) -> Option<(String, String)> {
    let last = add_days(week_started_on, 6)?;
    Some((week_started_on.to_string(), last))
}

/// §4 default 4: `term = (current_week − 1) / term_weeks + 1`.
pub fn term_of(week: i64, term_weeks: i64) -> i64 {
    if term_weeks <= 0 || week <= 0 {
        return 1;
    }
    (week - 1) / term_weeks + 1
}

// ---------------------------------------------------------------------------
// The plan, as read out of the database
// ---------------------------------------------------------------------------

/// One row of `assignments` for a subject in a given week.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AssignmentRow {
    pub assignment_id: i64,
    pub ordinal: i64,
    pub text: String,
    pub detail: Option<String>,
    /// `assignments.days`, already parsed. `Some` **pins** this row to those
    /// days (H3 rule 1's `assignment.days ∨ subject.days`); `None` lets the
    /// row take part in the subject-wide spread of rule 5.
    pub days: Option<Vec<Weekday>>,
}

/// One `subjects` row with this week's assignment rows attached.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SubjectPlan {
    pub subject_id: i64,
    pub name: String,
    pub category: Category,
    pub source: Option<String>,
    pub icon_name: Option<String>,
    pub sort_order: i64,
    pub days: Vec<Weekday>,
    pub shared: bool,
    /// This week's rows, in `ordinal` order (H3 rule 2).
    pub rows: Vec<AssignmentRow>,
}

/// One `term_notes` row — the per-term reference texts (geography concept,
/// poetry book, free reads). §4 default 12: read-only, never an occurrence.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct TermNote {
    pub id: i64,
    pub term: i64,
    pub kind: TermNoteKind,
    pub text: String,
    pub sort_order: i64,
}

/// Everything the occurrence rule needs about one week of one curriculum.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WeekPlan {
    pub curriculum_id: i64,
    pub week: i64,
    pub weeks: i64,
    pub term: i64,
    pub subjects: Vec<SubjectPlan>,
    pub term_notes: Vec<TermNote>,
}

/// One `enrollments` row: which boy, which curriculum, where his pointer is
/// and what his week is anchored on.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Enrollment {
    pub profile_id: i64,
    pub curriculum_id: i64,
    pub current_week: i64,
    pub weeks: i64,
    pub term_weeks: i64,
    pub week_started_on: String,
    pub school_days: Vec<Weekday>,
    pub paused: bool,
}

impl Enrollment {
    /// H2: `current_week > weeks` is the terminal **Year complete 🎉** state,
    /// not an error.
    pub fn year_complete(&self) -> bool {
        self.current_week > self.weeks
    }
}

/// One `lesson_log` row, already narrowed to a single boy and week.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct LogRow {
    pub subject_id: i64,
    pub assignment_id: Option<i64>,
    pub scheduled_date: String,
    pub status: LogStatus,
    pub note: Option<String>,
}

/// H3 rule 9: the log key is
/// `(profile, week, subject, IFNULL(assignment, 0), scheduled_date)`. The
/// profile and week are fixed by the caller, so matching an occurrence to its
/// log row is this triple.
fn log_key(subject_id: i64, assignment_id: Option<i64>, scheduled_date: &str) -> (i64, i64, &str) {
    (subject_id, assignment_id.unwrap_or(0), scheduled_date)
}

fn find_log<'a>(logs: &'a [LogRow], occurrence: &LessonOccurrence) -> Option<&'a LogRow> {
    let wanted = log_key(
        occurrence.subject_id,
        occurrence.assignment_id,
        &occurrence.scheduled_date,
    );
    logs.iter()
        .find(|row| log_key(row.subject_id, row.assignment_id, &row.scheduled_date) == wanted)
}

// ---------------------------------------------------------------------------
// H3 — the occurrence rule
// ---------------------------------------------------------------------------

/// `true` when `date` is a school day for this boy: its weekday is in
/// `school_days` **and** school is not paused (H3 rule 8).
pub fn is_school_day(enrollment: &Enrollment, date: &str) -> bool {
    if enrollment.paused {
        return false;
    }
    weekday(date).is_some_and(|day| enrollment.school_days.contains(&day))
}

/// How many days the parent has been on this week (H2's 14-day nudge).
pub fn days_on_week(week_started_on: &str, today: &str) -> u32 {
    let (Some((y0, m0, d0)), Some((y1, m1, d1))) = (parse_date(week_started_on), parse_date(today))
    else {
        return 0;
    };
    let delta = days_from_civil(y1, m1, d1) - days_from_civil(y0, m0, d0);
    u32::try_from(delta.max(0)).unwrap_or(u32::MAX)
}

#[allow(clippy::too_many_arguments)]
fn occurrence(
    subject: &SubjectPlan,
    week: i64,
    row: Option<&AssignmentRow>,
    day: Weekday,
    date: String,
    part: Option<(u32, u32)>,
) -> LessonOccurrence {
    LessonOccurrence {
        subject_id: subject.subject_id,
        assignment_id: row.map(|row| row.assignment_id),
        week,
        scheduled_date: date,
        weekday: day,
        category: subject.category,
        title: subject.name.clone(),
        text: row.map(|row| row.text.clone()),
        detail: row.and_then(|row| row.detail.clone()),
        source: subject.source.clone(),
        icon_name: subject.icon_name.clone(),
        part,
        shared: subject.shared,
        sort_order: subject.sort_order,
        status: None,
        note: None,
        // Rule 1's per-week override, carried through so Today can hand it
        // back to `upsert_assignment` unchanged (QH4-03 / R-11).
        days: row.and_then(|row| row.days.clone()),
    }
}

/// H3 rule 5's chunking, shared by `reading` and by `daily` subjects that do
/// carry per-week rows (rule 3).
///
/// * `rows.len() ≤ days.len()` — chunk `days` into `rows.len()` **contiguous**
///   groups, earlier groups taking the extra day; one occurrence per
///   (row, day-in-group), with `part = Some((k, n))` when the group has
///   `n > 1` days.
/// * otherwise — one occurrence per row at `days[i % days.len()]`, no part.
fn spread_rows(
    subject: &SubjectPlan,
    week: i64,
    week_started_on: &str,
    rows: &[&AssignmentRow],
    days: &[Weekday],
    out: &mut Vec<LessonOccurrence>,
) {
    // Checked first, so neither branch below can divide by zero (rule 5).
    if rows.is_empty() || days.is_empty() {
        return;
    }
    if rows.len() <= days.len() {
        let base = days.len() / rows.len();
        let extra = days.len() % rows.len();
        let mut cursor = 0usize;
        for (index, row) in rows.iter().enumerate() {
            let size = base + usize::from(index < extra);
            let group = &days[cursor..cursor + size];
            cursor += size;
            for (offset, day) in group.iter().enumerate() {
                let part = if size > 1 {
                    Some((offset as u32 + 1, size as u32))
                } else {
                    None
                };
                if let Some(date) = date_for(week_started_on, *day) {
                    out.push(occurrence(subject, week, Some(row), *day, date, part));
                }
            }
        }
    } else {
        for (index, row) in rows.iter().enumerate() {
            let day = days[index % days.len()];
            if let Some(date) = date_for(week_started_on, day) {
                out.push(occurrence(subject, week, Some(row), day, date, None));
            }
        }
    }
}

fn subject_occurrences(
    subject: &SubjectPlan,
    week: i64,
    week_started_on: &str,
    school_days: &[Weekday],
    out: &mut Vec<LessonOccurrence>,
) {
    // Rule 6: a free read is never an occurrence.
    if subject.category == Category::FreeRead {
        return;
    }
    let days = intersect_days(&subject.days, school_days);

    // Rule 1: a row may carry its own `days`, which override the subject's.
    // Such a row is placed on exactly those days and takes no part in the
    // spread of rule 5.
    let (pinned, floating): (Vec<&AssignmentRow>, Vec<&AssignmentRow>) =
        subject.rows.iter().partition(|row| row.days.is_some());

    if subject.category == Category::Weekly {
        // Rule 4: one occurrence on the **first** day of `days`, carrying
        // `rows[0]` if any. Empty `days` → none.
        let row = subject.rows.first();
        let weekly_days = match row.and_then(|row| row.days.as_deref()) {
            Some(row_days) => intersect_days(row_days, school_days),
            None => days,
        };
        if let Some(day) = weekly_days.first() {
            if let Some(date) = date_for(week_started_on, *day) {
                out.push(occurrence(subject, week, row, *day, date, None));
            }
        }
        return;
    }

    for row in &pinned {
        let row_days = intersect_days(row.days.as_deref().unwrap_or(&[]), school_days);
        for day in row_days {
            if let Some(date) = date_for(week_started_on, day) {
                out.push(occurrence(subject, week, Some(row), day, date, None));
            }
        }
    }

    if subject.category == Category::Daily && subject.rows.is_empty() {
        // Rule 3: no rows at all → one untitled occurrence per day in `days`.
        for day in days {
            if let Some(date) = date_for(week_started_on, day) {
                out.push(occurrence(subject, week, None, day, date, None));
            }
        }
        return;
    }

    // Rule 3 (daily with rows) and rule 5 (reading).
    spread_rows(subject, week, week_started_on, &floating, &days, out);
}

/// H3 rules 1–7: every dated occurrence of one week for one boy.
///
/// Sorted by `(scheduled_date, sort_order, subject_id)`, stably, so rows of
/// the same subject keep their `ordinal` order inside a day.
pub fn occurrences(plan: &WeekPlan, enrollment: &Enrollment) -> Vec<LessonOccurrence> {
    let mut out = Vec::new();
    if enrollment.year_complete() {
        return out;
    }
    for subject in &plan.subjects {
        subject_occurrences(
            subject,
            plan.week,
            &enrollment.week_started_on,
            &enrollment.school_days,
            &mut out,
        );
    }
    out.sort_by(|a, b| {
        a.scheduled_date
            .cmp(&b.scheduled_date)
            .then(a.sort_order.cmp(&b.sort_order))
            .then(a.subject_id.cmp(&b.subject_id))
    });
    out
}

/// H2: the week is complete when **no** occurrence lacks a log row
/// (rule 8's "Week complete ⇔ …").
pub fn week_complete(plan: &WeekPlan, enrollment: &Enrollment, logs: &[LogRow]) -> bool {
    occurrences(plan, enrollment)
        .iter()
        .all(|occurrence| find_log(logs, occurrence).is_some())
}

/// H2: **Finish week** is offered when every occurrence of the week is done or
/// skipped, **or** `today` has reached the last school day of the span.
pub fn can_finish_week(
    plan: &WeekPlan,
    enrollment: &Enrollment,
    logs: &[LogRow],
    today: &str,
) -> bool {
    if enrollment.year_complete() {
        return false;
    }
    if week_complete(plan, enrollment, logs) {
        return true;
    }
    last_school_day(&enrollment.week_started_on, &enrollment.school_days)
        .is_some_and(|last| today >= last.as_str())
}

/// H3 rule 10: a parent-added task counts in the Finish-week completeness
/// check **only** while it is dated inside the current week's span.
pub fn extras_complete(extras: &[ExtraTask], user_id: i64, week_span: (&str, &str)) -> bool {
    extras
        .iter()
        .filter(|extra| {
            extra.user_id == user_id
                && extra.scheduled_date.as_str() >= week_span.0
                && extra.scheduled_date.as_str() <= week_span.1
        })
        .all(|extra| extra.status.is_some())
}

/// [`can_finish_week`] with H3 rule 10 applied: complete only when every
/// occurrence **and** every extra dated inside the span is logged; the
/// last-school-day clause is unchanged.
pub fn can_finish_week_with_extras(
    plan: &WeekPlan,
    enrollment: &Enrollment,
    logs: &[LogRow],
    extras: &[ExtraTask],
    today: &str,
) -> bool {
    if enrollment.year_complete() {
        return false;
    }
    let extras_done = week_span(&enrollment.week_started_on)
        .is_some_and(|(from, to)| extras_complete(extras, enrollment.profile_id, (&from, &to)));
    if week_complete(plan, enrollment, logs) && extras_done {
        return true;
    }
    last_school_day(&enrollment.week_started_on, &enrollment.school_days)
        .is_some_and(|last| today >= last.as_str())
}

// ---------------------------------------------------------------------------
// H3 rule 8 — the Today view for one boy
// ---------------------------------------------------------------------------

/// H3 rule 8. `paused` empties every list (H2's "School's out ⚽").
///
/// `due_today` is every occurrence dated `date` — done or not, so the surface
/// renders it with its box ticked rather than making the row vanish under the
/// child's finger. `catch_up` is everything **earlier in the same week** with
/// no log row (daily work included — R-13/P-11), and `done` is every
/// occurrence of the week that does have one.
///
/// `name` is left empty: the roster lives in `profiles`, which this pure
/// module cannot see, so HS4 fills it in from the row it already has.
pub fn today_view(
    plan: &WeekPlan,
    enrollment: &Enrollment,
    logs: &[LogRow],
    date: &str,
) -> BoyToday {
    let mut view = BoyToday {
        user_id: enrollment.profile_id,
        name: String::new(),
        due_today: Vec::new(),
        catch_up: Vec::new(),
        done: Vec::new(),
        done_count: 0,
        skipped_count: 0,
        total_count: 0,
    };
    if enrollment.paused {
        return view;
    }

    for mut occurrence in occurrences(plan, enrollment) {
        view.total_count = view.total_count.saturating_add(1);
        let logged = find_log(logs, &occurrence).cloned();
        if let Some(row) = &logged {
            occurrence.status = Some(row.status);
            occurrence.note.clone_from(&row.note);
            match row.status {
                LogStatus::Done => view.done_count = view.done_count.saturating_add(1),
                LogStatus::Skipped => view.skipped_count = view.skipped_count.saturating_add(1),
            }
            view.done.push(DayItem::Lesson(occurrence.clone()));
        }
        if occurrence.scheduled_date == date {
            view.due_today.push(DayItem::Lesson(occurrence));
        } else if occurrence.scheduled_date.as_str() < date && logged.is_none() {
            view.catch_up.push(DayItem::Lesson(occurrence));
        }
    }
    view
}

/// How far back an unfinished extra keeps rolling forward (H3 rule 10 /
/// §4 default 16).
pub const EXTRA_CATCH_UP_DAYS: i32 = 14;

/// H3 rule 10 — fold the boy's parent-added tasks into his Today lists.
///
/// The checks run in H3 rule 10's own order — `due_today` first — so that a
/// **ticked** extra dated `date` stays in `due_today` carrying its status,
/// exactly as [`today_view`] keeps a ticked lesson there. It is listed in
/// `done` as well; the two lists overlap by design, and a boy on the TV can
/// therefore untick a mis-tick instead of watching the row vanish (D-2).
///
/// * dated `date` → `due_today` (ticked or not); dated in `[date − 14, date)`
///   and **unfinished** → `catch_up`; a finished extra → `done` as well;
///   anything else is in no list.
/// * an extra counts in `done_count` / `skipped_count` / `total_count`
///   **only** while it is dated inside `week_span`, and drops out again once
///   the span has passed. An extra dated `date` and ticked is counted once,
///   despite appearing in two lists.
///
/// Extras are per boy and per date, never Together (§4 default 16).
pub fn merge_extras(
    today: &mut BoyToday,
    extras: &[ExtraTask],
    date: &str,
    week_span: (&str, &str),
) {
    let earliest = add_days(date, -EXTRA_CATCH_UP_DAYS);
    for extra in extras {
        if extra.user_id != today.user_id {
            continue;
        }
        let in_span = extra.scheduled_date.as_str() >= week_span.0
            && extra.scheduled_date.as_str() <= week_span.1;
        if in_span {
            today.total_count = today.total_count.saturating_add(1);
            match extra.status {
                Some(LogStatus::Done) => today.done_count = today.done_count.saturating_add(1),
                Some(LogStatus::Skipped) => {
                    today.skipped_count = today.skipped_count.saturating_add(1)
                }
                None => {}
            }
        }
        if extra.status.is_some() {
            today.done.push(DayItem::Extra(extra.clone()));
        }
        if extra.scheduled_date == date {
            today.due_today.push(DayItem::Extra(extra.clone()));
        } else if extra.status.is_none()
            && extra.scheduled_date.as_str() < date
            && earliest
                .as_deref()
                .is_some_and(|floor| extra.scheduled_date.as_str() >= floor)
        {
            today.catch_up.push(DayItem::Extra(extra.clone()));
        }
    }
}

// ---------------------------------------------------------------------------
// H4 — Together
// ---------------------------------------------------------------------------

/// H4: the `shared` occurrences of a Together group, rendered **once** with
/// the boys they cover.
///
/// `groups` is every `(enrollment, its log rows)` sharing
/// `(curriculum_id, current_week)`; `plan` is that shared week. Occurrences
/// due on `date`, plus earlier ones the group has not finished (the Together
/// catch-up of H6), are returned in occurrence order. A paused or
/// year-complete enrollment contributes nothing.
pub fn together_view(
    groups: &[(Enrollment, Vec<LogRow>)],
    plan: &WeekPlan,
    date: &str,
) -> Vec<TogetherOccurrence> {
    let mut out: Vec<TogetherOccurrence> = Vec::new();
    for (enrollment, logs) in groups {
        if enrollment.paused || enrollment.year_complete() {
            continue;
        }
        for occurrence in occurrences(plan, enrollment) {
            if !occurrence.shared {
                continue;
            }
            let logged = find_log(logs, &occurrence).cloned();
            let slot = out.iter_mut().find(|candidate| {
                candidate.occurrence.subject_id == occurrence.subject_id
                    && candidate.occurrence.assignment_id == occurrence.assignment_id
                    && candidate.occurrence.scheduled_date == occurrence.scheduled_date
            });
            let slot = match slot {
                Some(slot) => slot,
                None => {
                    out.push(TogetherOccurrence {
                        occurrence,
                        user_ids: Vec::new(),
                        done_user_ids: Vec::new(),
                    });
                    out.last_mut().expect("just pushed")
                }
            };
            if !slot.user_ids.contains(&enrollment.profile_id) {
                slot.user_ids.push(enrollment.profile_id);
            }
            if let Some(row) = logged {
                if !slot.done_user_ids.contains(&enrollment.profile_id) {
                    slot.done_user_ids.push(enrollment.profile_id);
                }
                // The row carries a status only once every covered boy has
                // one and they agree; a partial group shows "2 of 3" instead.
                slot.occurrence.status = match slot.occurrence.status {
                    None if slot.done_user_ids.len() == 1 => Some(row.status),
                    Some(status) if status == row.status => Some(status),
                    _ => None,
                };
                if slot.occurrence.note.is_none() {
                    slot.occurrence.note = row.note;
                }
            }
        }
    }
    for slot in &mut out {
        if slot.done_user_ids.len() != slot.user_ids.len() {
            slot.occurrence.status = None;
        }
    }
    out.retain(|slot| {
        slot.occurrence.scheduled_date == date
            || (slot.occurrence.scheduled_date.as_str() < date
                && slot.done_user_ids.len() < slot.user_ids.len())
    });
    out.sort_by(|a, b| {
        a.occurrence
            .scheduled_date
            .cmp(&b.occurrence.scheduled_date)
            .then(a.occurrence.sort_order.cmp(&b.occurrence.sort_order))
            .then(a.occurrence.subject_id.cmp(&b.occurrence.subject_id))
    });
    out
}

// ---------------------------------------------------------------------------
// H6 — the Year view's week grid
// ---------------------------------------------------------------------------

/// H6 Year view: one week of the plan as a **subject × school-day** grid,
/// built by the same occurrence rule so the parent sees the year exactly as it
/// will be dealt out.
///
/// `anchor` is the `week_started_on` the week is laid out against (HS4 derives
/// it from the pointer: `add_days(week_started_on, (week − current_week) × 7)`).
/// `dated = false` — §4 default 17 — means the week is **not** the current one:
/// the dates are advisory, and the surface renders weekday columns without
/// them and without checkboxes.
///
/// `free_read` subjects have no row (rule 6); every other subject has one,
/// even when this week deals it nothing.
pub fn week_grid(
    plan: &WeekPlan,
    enrollment: &Enrollment,
    logs: &[LogRow],
    anchor: &str,
    dated: bool,
) -> WeekGrid {
    let days = enrollment.school_days.clone();
    let anchored = Enrollment {
        week_started_on: anchor.to_string(),
        // A grid is drawn for every week 1…36 regardless of the pointer
        // (§4 default 17), so the terminal year-complete state of the *live*
        // enrollment must not blank it.
        current_week: plan.week,
        ..enrollment.clone()
    };

    let mut rows: Vec<WeekGridRow> = plan
        .subjects
        .iter()
        .filter(|subject| subject.category != Category::FreeRead)
        .map(|subject| WeekGridRow {
            subject_id: subject.subject_id,
            title: subject.name.clone(),
            category: subject.category,
            shared: subject.shared,
            cells: vec![Vec::new(); days.len()],
        })
        .collect();

    for mut occurrence in occurrences(plan, &anchored) {
        if let Some(row) = find_log(logs, &occurrence) {
            occurrence.status = Some(row.status);
            occurrence.note.clone_from(&row.note);
        }
        let Some(column) = days.iter().position(|day| *day == occurrence.weekday) else {
            continue;
        };
        if let Some(row) = rows
            .iter_mut()
            .find(|row| row.subject_id == occurrence.subject_id)
        {
            row.cells[column].push(occurrence);
        }
    }

    WeekGrid {
        week: plan.week,
        weeks: plan.weeks,
        term: plan.term,
        dated,
        days,
        rows,
    }
}

// ---------------------------------------------------------------------------
// H6 / H8 — the Month view
// ---------------------------------------------------------------------------

/// H6 Month view: one calendar month for **exactly one boy** (§4 default 17).
///
/// * `done` counts what he has finished on that date — log rows plus extras
///   whose status is set. Days strictly after `today` that lie outside the
///   current week's span report nothing: a future week has not been dealt out
///   yet, so there is nothing to have done.
/// * `total` is `Some` **only** for days inside the current week's span, where
///   it is that day's merged occurrences plus extras; every other day shows a
///   bare `done` count (H6: "the plan for a past week is not reconstructed").
/// * `extras` counts his parent-added tasks on that date, anywhere in the
///   year — extras are independent of the curriculum pointer (H8).
///
/// With `enrollment = None` (nobody enrolled) every day is `total = None`,
/// `week = None`, `is_school_day = false`, and the extras are still counted.
pub fn month_view(
    enrollment: Option<&Enrollment>,
    plan: Option<&WeekPlan>,
    logs: &[LogRow],
    extras: &[ExtraTask],
    year: i32,
    month: u32,
    today: &str,
) -> MonthView {
    let user_id = enrollment
        .map(|enrollment| enrollment.profile_id)
        .or_else(|| extras.first().map(|extra| extra.user_id))
        .unwrap_or(0);

    let span = enrollment.and_then(|enrollment| week_span(&enrollment.week_started_on));
    let dealt: Vec<LessonOccurrence> = match (enrollment, plan) {
        (Some(enrollment), Some(plan)) => occurrences(plan, enrollment),
        _ => Vec::new(),
    };

    let mut days = Vec::new();
    let last = days_in_month(year, month);
    for day_of_month in 1..=last {
        let date = format_date(year, month, day_of_month);
        let Some(day) = weekday(&date) else { continue };

        let in_current_week = span.as_ref().is_some_and(|(from, to)| {
            date.as_str() >= from.as_str() && date.as_str() <= to.as_str()
        });
        let is_school_day = enrollment.is_some_and(|enrollment| is_school_day(enrollment, &date));

        let extras_today = extras
            .iter()
            .filter(|extra| extra.user_id == user_id && extra.scheduled_date == date)
            .count() as u32;

        let done = if date.as_str() > today && !in_current_week {
            0
        } else {
            let logged = logs.iter().filter(|row| row.scheduled_date == date).count() as u32;
            let finished_extras = extras
                .iter()
                .filter(|extra| {
                    extra.user_id == user_id
                        && extra.scheduled_date == date
                        && extra.status.is_some()
                })
                .count() as u32;
            logged + finished_extras
        };

        let total = in_current_week.then(|| {
            dealt
                .iter()
                .filter(|occurrence| occurrence.scheduled_date == date)
                .count() as u32
                + extras_today
        });

        days.push(MonthDay {
            date,
            weekday: day,
            is_school_day,
            in_current_week,
            week: match (in_current_week, enrollment) {
                (true, Some(enrollment)) => Some(enrollment.current_week),
                _ => None,
            },
            done,
            total,
            extras: extras_today,
        });
    }

    MonthView {
        year,
        month,
        user_id,
        days,
    }
}

// ---------------------------------------------------------------------------
// HS3 acceptance (a)–(i) — `docs/homeschool/PLAN_HOMESCHOOL.md` §3 HS3
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- fixture -----------------------------------------------------------
    //
    // The **synthetic** curriculum of H5, built in Rust rather than read from
    // `tests/fixtures/curricula/sample-year.toml` (HS1): this module is pure,
    // wasm-safe and has no TOML parser — `toml` is a server-only dependency
    // (H5). The shape is the same one HS1 commits, so the two agree by
    // construction: 3 weeks, `term_weeks = 1`, subjects `Sums` (daily MTWRF,
    // not shared), `Copywork` (daily MTWRF), `Old Tales` (reading MW, one row
    // a week), `Fables` (reading TF, two rows in weeks 1 and 3 and **none** in
    // week 2), `Twice Told` (reading T, two rows in week 2), `Painting`
    // (weekly F) and `Reading Basket` (free_read). Invented names only — N1.

    fn days(letters: &str) -> Vec<Weekday> {
        parse_days(letters).expect("fixture day letters parse")
    }

    fn row(assignment_id: i64, ordinal: i64, text: &str) -> AssignmentRow {
        AssignmentRow {
            assignment_id,
            ordinal,
            text: text.to_string(),
            detail: None,
            days: None,
        }
    }

    fn subject(
        subject_id: i64,
        name: &str,
        category: Category,
        letters: &str,
        rows: Vec<AssignmentRow>,
    ) -> SubjectPlan {
        SubjectPlan {
            subject_id,
            name: name.to_string(),
            category,
            source: None,
            icon_name: None,
            sort_order: subject_id,
            days: days(letters),
            shared: category.shared_by_default(),
            rows,
        }
    }

    /// The fixture's week `week`, anchored on a Monday so the span is a plain
    /// Mon–Sun calendar week (`2026-09-07` is a Monday).
    fn sample_week(week: i64) -> WeekPlan {
        let (fables, twice_told): (Vec<AssignmentRow>, Vec<AssignmentRow>) = match week {
            1 => (
                vec![row(41, 1, "Fable one"), row(42, 2, "Fable two")],
                vec![],
            ),
            2 => (
                vec![],
                vec![row(51, 1, "Twice told one"), row(52, 2, "Twice told two")],
            ),
            _ => (
                vec![row(43, 1, "Fable three"), row(44, 2, "Fable four")],
                vec![],
            ),
        };
        WeekPlan {
            curriculum_id: 1,
            week,
            weeks: 3,
            term: term_of(week, 1),
            subjects: vec![
                subject(1, "Sums", Category::Daily, "MTWRF", vec![]),
                subject(2, "Copywork", Category::Daily, "MTWRF", vec![]),
                subject(
                    3,
                    "Old Tales",
                    Category::Reading,
                    "MW",
                    vec![row(30 + week, 1, "Old tales chapter")],
                ),
                subject(4, "Fables", Category::Reading, "TF", fables),
                subject(5, "Twice Told", Category::Reading, "T", twice_told),
                subject(
                    6,
                    "Painting",
                    Category::Weekly,
                    "F",
                    vec![row(61, 1, "Study the picture")],
                ),
                subject(7, "Reading Basket", Category::FreeRead, "MTWRF", vec![]),
            ],
            term_notes: vec![TermNote {
                id: 1,
                term: term_of(week, 1),
                kind: TermNoteKind::Poetry,
                text: "A book of verses".into(),
                sort_order: 0,
            }],
        }
    }

    fn sample_enrollment(week: i64, week_started_on: &str) -> Enrollment {
        Enrollment {
            profile_id: 1,
            curriculum_id: 1,
            current_week: week,
            weeks: 3,
            term_weeks: 1,
            week_started_on: week_started_on.to_string(),
            school_days: days("MTWRF"),
            paused: false,
        }
    }

    /// A one-subject plan, for the rule-5 worked cases.
    fn reading_plan(subject_days: &str, row_count: usize) -> WeekPlan {
        let rows = (0..row_count)
            .map(|index| row(100 + index as i64, index as i64 + 1, "passage"))
            .collect();
        WeekPlan {
            curriculum_id: 9,
            week: 1,
            weeks: 36,
            term: 1,
            subjects: vec![subject(
                9,
                "Readings",
                Category::Reading,
                subject_days,
                rows,
            )],
            term_notes: Vec::new(),
        }
    }

    /// `(weekday, part)` for every occurrence, in order — the shape rule 5's
    /// worked cases are stated in.
    fn spread_of(plan: &WeekPlan, enrollment: &Enrollment) -> Vec<(Weekday, Option<(u32, u32)>)> {
        occurrences(plan, enrollment)
            .into_iter()
            .map(|occurrence| (occurrence.weekday, occurrence.part))
            .collect()
    }

    fn lessons(items: &[DayItem]) -> Vec<LessonOccurrence> {
        items
            .iter()
            .filter_map(|item| match item {
                DayItem::Lesson(lesson) => Some(lesson.clone()),
                DayItem::Extra(_) => None,
            })
            .collect()
    }

    fn titles(items: &[DayItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                DayItem::Lesson(lesson) => lesson.title.clone(),
                DayItem::Extra(extra) => extra.title.clone(),
            })
            .collect()
    }

    fn extra(id: i64, scheduled_date: &str, status: Option<LogStatus>) -> ExtraTask {
        ExtraTask {
            id,
            user_id: 1,
            scheduled_date: scheduled_date.to_string(),
            title: format!("Task {id}"),
            category: Category::Daily,
            text: None,
            sort_order: id,
            status,
            note: None,
        }
    }

    // -- accept (a): weekday -----------------------------------------------

    #[test]
    fn hs3_a_weekday_reads_the_four_stated_dates_by_sakamotos_method() {
        assert_eq!(weekday("2026-09-02"), Some(Weekday::Wed));
        assert_eq!(weekday("2000-02-29"), Some(Weekday::Tue));
        assert_eq!(weekday("2100-03-01"), Some(Weekday::Mon));
        assert_eq!(weekday("1970-01-01"), Some(Weekday::Thu));
    }

    #[test]
    fn hs3_a_weekday_rejects_a_day_that_does_not_exist_because_2100_is_not_a_leap_year() {
        assert_eq!(weekday("2100-02-29"), None);
        assert!(
            weekday("2000-02-29").is_some(),
            "2000 is a leap year, so the same day-of-month does exist"
        );
        assert_eq!(weekday("2026-13-01"), None);
        assert_eq!(weekday("2026-09-31"), None);
        assert_eq!(weekday("2026-9-2"), None);
        assert_eq!(weekday(""), None);
        assert_eq!(weekday("not-a-date"), None);
    }

    #[test]
    fn hs3_a_add_days_crosses_month_year_and_leap_boundaries() {
        assert_eq!(add_days("2026-09-02", 1).as_deref(), Some("2026-09-03"));
        assert_eq!(add_days("2026-09-30", 1).as_deref(), Some("2026-10-01"));
        assert_eq!(add_days("2026-01-01", -1).as_deref(), Some("2025-12-31"));
        assert_eq!(add_days("2024-02-28", 1).as_deref(), Some("2024-02-29"));
        assert_eq!(add_days("2100-02-28", 1).as_deref(), Some("2100-03-01"));
        assert_eq!(add_days("2026-09-02", 0).as_deref(), Some("2026-09-02"));
        assert_eq!(add_days("2026-09-02", 365).as_deref(), Some("2027-09-02"));
        assert_eq!(add_days("nonsense", 1), None);
    }

    // -- accept (f): parse_days --------------------------------------------

    #[test]
    fn hs3_f_parse_days_rejects_an_unknown_letter_or_a_repeat() {
        assert_eq!(parse_days("Th"), Err(DayError::UnknownLetter('h')));
        assert_eq!(parse_days("MM"), Err(DayError::RepeatedLetter('M')));
        assert_eq!(parse_days("X"), Err(DayError::UnknownLetter('X')));
        assert_eq!(parse_days("mtwrf"), Err(DayError::UnknownLetter('m')));
    }

    #[test]
    fn hs3_f_parse_days_returns_the_days_in_mtwrfsu_order_however_they_were_written() {
        assert_eq!(
            parse_days("MTWRF"),
            Ok(vec![
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri
            ])
        );
        assert_eq!(parse_days("FM"), Ok(vec![Weekday::Mon, Weekday::Fri]));
        assert_eq!(parse_days("US"), Ok(vec![Weekday::Sat, Weekday::Sun]));
        assert_eq!(parse_days(""), Ok(Vec::new()));
        assert_eq!(days_to_string(&days("FM")), "MF");
    }

    #[test]
    fn hs3_f_a_day_error_says_which_letter_was_wrong() {
        assert_eq!(
            DayError::UnknownLetter('h').to_string(),
            "'h' is not one of the day letters MTWRFSU"
        );
        assert_eq!(
            DayError::RepeatedLetter('M').to_string(),
            "day letter 'M' appears twice"
        );
    }

    // -- accept (c): date_for and last_school_day --------------------------

    #[test]
    fn hs3_c_a_week_started_on_a_wednesday_puts_monday_and_tuesday_in_the_following_week() {
        // 2026-09-02 is a Wednesday (accept (a)).
        let anchor = "2026-09-02";
        assert_eq!(weekday(anchor), Some(Weekday::Wed));
        assert_eq!(date_for(anchor, Weekday::Wed).as_deref(), Some(anchor));
        assert_eq!(
            date_for(anchor, Weekday::Fri).as_deref(),
            Some("2026-09-04")
        );
        assert_eq!(
            date_for(anchor, Weekday::Mon).as_deref(),
            Some("2026-09-07"),
            "Monday falls in the following calendar week"
        );
        assert_eq!(
            date_for(anchor, Weekday::Tue).as_deref(),
            Some("2026-09-08"),
            "Tuesday too — the span is the seven days from the anchor"
        );
        // Every day of the span is inside `[anchor, anchor + 6]`.
        let last = add_days(anchor, 6).expect("the span ends six days later");
        for day in Weekday::ORDER {
            let date = date_for(anchor, day).expect("every weekday is in the span");
            assert!(date.as_str() >= anchor && date.as_str() <= last.as_str());
        }
    }

    #[test]
    fn hs3_c_last_school_day_is_the_last_date_of_the_span_in_school_days() {
        // A week anchored on Monday 2026-09-07.
        assert_eq!(
            last_school_day("2026-09-07", &days("MTWRF")).as_deref(),
            Some("2026-09-11"),
            "Friday"
        );
        assert_eq!(
            last_school_day("2026-09-07", &days("MTWR")).as_deref(),
            Some("2026-09-10"),
            "Thursday"
        );
        // A week anchored on a Wednesday: Tuesday is the last school day of
        // the span, six days later.
        assert_eq!(
            last_school_day("2026-09-02", &days("MTWRF")).as_deref(),
            Some("2026-09-08")
        );
        assert_eq!(last_school_day("2026-09-07", &[]), None);
        assert_eq!(last_school_day("nonsense", &days("MTWRF")), None);
    }

    #[test]
    fn hs3_c_week_span_is_the_seven_days_from_the_anchor_and_term_follows_default_four() {
        assert_eq!(
            week_span("2026-09-07"),
            Some(("2026-09-07".into(), "2026-09-13".into()))
        );
        assert_eq!(term_of(1, 12), 1);
        assert_eq!(term_of(12, 12), 1);
        assert_eq!(term_of(13, 12), 2);
        assert_eq!(term_of(36, 12), 3);
        assert_eq!(days_on_week("2026-09-07", "2026-09-21"), 14);
        assert_eq!(days_on_week("2026-09-07", "2026-09-07"), 0);
    }

    // -- accept (b): the occurrence rule -----------------------------------

    #[test]
    fn hs3_b_rule_five_splits_one_row_over_two_days_into_part_one_and_part_two() {
        let plan = reading_plan("MW", 1);
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert_eq!(
            spread_of(&plan, &enrollment),
            vec![(Weekday::Mon, Some((1, 2))), (Weekday::Wed, Some((2, 2)))]
        );
    }

    #[test]
    fn hs3_b_rule_five_gives_two_rows_over_two_days_one_each_with_no_part_label() {
        let plan = reading_plan("MW", 2);
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert_eq!(
            spread_of(&plan, &enrollment),
            vec![(Weekday::Mon, None), (Weekday::Wed, None)]
        );
    }

    #[test]
    fn hs3_b_rule_five_puts_two_rows_over_one_day_both_on_that_day() {
        let plan = reading_plan("T", 2);
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert_eq!(
            spread_of(&plan, &enrollment),
            vec![(Weekday::Tue, None), (Weekday::Tue, None)]
        );
    }

    #[test]
    fn hs3_b_rule_five_gives_the_earlier_group_the_extra_day_for_two_rows_over_three() {
        let plan = reading_plan("MWF", 2);
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert_eq!(
            spread_of(&plan, &enrollment),
            vec![
                (Weekday::Mon, Some((1, 2))),
                (Weekday::Wed, Some((2, 2))),
                (Weekday::Fri, None)
            ]
        );
    }

    #[test]
    fn hs3_b_rule_five_deals_nothing_when_the_rows_or_the_days_are_empty() {
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert!(
            occurrences(&reading_plan("MW", 0), &enrollment).is_empty(),
            "no rows → no occurrence"
        );
        let mut no_school_days = enrollment.clone();
        no_school_days.school_days = days("SU");
        assert!(
            occurrences(&reading_plan("MW", 2), &no_school_days).is_empty(),
            "days ∩ school_days empty → no occurrence"
        );
    }

    #[test]
    fn hs3_b_a_daily_subject_with_no_rows_deals_one_untitled_occurrence_a_school_day() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let sums: Vec<LessonOccurrence> = occurrences(&plan, &enrollment)
            .into_iter()
            .filter(|occurrence| occurrence.subject_id == 1)
            .collect();
        assert_eq!(sums.len(), 5, "one a day, Monday to Friday");
        assert!(sums
            .iter()
            .all(|occurrence| occurrence.assignment_id.is_none()
                && occurrence.text.is_none()
                && occurrence.part.is_none()));
        assert_eq!(
            sums.iter()
                .map(|occurrence| occurrence.scheduled_date.clone())
                .collect::<Vec<_>>(),
            vec![
                "2026-09-07",
                "2026-09-08",
                "2026-09-09",
                "2026-09-10",
                "2026-09-11"
            ]
        );
    }

    #[test]
    fn hs3_b_a_weekly_subject_deals_one_occurrence_on_the_first_of_its_days() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let painting: Vec<LessonOccurrence> = occurrences(&plan, &enrollment)
            .into_iter()
            .filter(|occurrence| occurrence.subject_id == 6)
            .collect();
        assert_eq!(painting.len(), 1);
        assert_eq!(painting[0].weekday, Weekday::Fri);
        assert_eq!(painting[0].text.as_deref(), Some("Study the picture"));
        assert!(painting[0].shared, "weekly work is read to everyone");
    }

    #[test]
    fn hs3_b_a_free_read_subject_is_never_an_occurrence() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        assert!(occurrences(&plan, &enrollment)
            .iter()
            .all(|occurrence| occurrence.category != Category::FreeRead));
    }

    #[test]
    fn hs3_b_a_monday_reading_unticked_on_wednesday_is_in_catch_up_and_not_due_today() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let view = today_view(&plan, &enrollment, &[], "2026-09-09");

        let old_tales_catch_up: Vec<LessonOccurrence> = lessons(&view.catch_up)
            .into_iter()
            .filter(|occurrence| occurrence.subject_id == 3)
            .collect();
        assert_eq!(old_tales_catch_up.len(), 1, "Old Tales part 1 of 2, Monday");
        assert_eq!(old_tales_catch_up[0].scheduled_date, "2026-09-07");
        assert_eq!(old_tales_catch_up[0].part, Some((1, 2)));

        let due_subjects: Vec<i64> = lessons(&view.due_today)
            .into_iter()
            .map(|occurrence| occurrence.subject_id)
            .collect();
        assert!(
            due_subjects.contains(&3),
            "Wednesday carries Old Tales part 2 of 2"
        );
        assert!(lessons(&view.due_today)
            .iter()
            .all(|occurrence| occurrence.scheduled_date == "2026-09-09"));
    }

    #[test]
    fn hs3_b_a_daily_occurrence_reaches_catch_up_on_a_later_day_and_never_twice_in_due_today() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let view = today_view(&plan, &enrollment, &[], "2026-09-09");

        // R-13/P-11: daily work is in catch-up, not silently dropped.
        let daily_catch_up = lessons(&view.catch_up)
            .into_iter()
            .filter(|occurrence| occurrence.category == Category::Daily)
            .count();
        assert_eq!(
            daily_catch_up, 4,
            "Sums and Copywork for Monday and Tuesday"
        );

        let mut keys: Vec<(i64, i64, String)> = lessons(&view.due_today)
            .into_iter()
            .map(|occurrence| {
                (
                    occurrence.subject_id,
                    occurrence.assignment_id.unwrap_or(0),
                    occurrence.scheduled_date,
                )
            })
            .collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(
            before,
            keys.len(),
            "no occurrence appears twice in due_today"
        );
    }

    #[test]
    fn hs3_b_a_saturday_is_not_a_school_day_and_leaves_everything_unfinished_in_catch_up() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let saturday = "2026-09-12";
        assert_eq!(weekday(saturday), Some(Weekday::Sat));
        assert!(!is_school_day(&enrollment, saturday));

        let view = today_view(&plan, &enrollment, &[], saturday);
        assert!(view.due_today.is_empty(), "nothing is dealt to a Saturday");
        assert_eq!(
            view.catch_up.len(),
            view.total_count as usize,
            "everything unfinished has rolled into catch-up"
        );
        assert_eq!(view.done_count, 0);
    }

    #[test]
    fn hs3_b_a_skipped_row_leaves_catch_up_and_counts_in_skipped_count() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let monday_sums = LogRow {
            subject_id: 1,
            assignment_id: None,
            scheduled_date: "2026-09-07".into(),
            status: LogStatus::Skipped,
            note: Some("out all morning".into()),
        };
        let view = today_view(&plan, &enrollment, &[monday_sums], "2026-09-09");

        assert!(
            !lessons(&view.catch_up)
                .iter()
                .any(|occurrence| occurrence.subject_id == 1
                    && occurrence.scheduled_date == "2026-09-07"),
            "a skipped occurrence has a log row, so it leaves catch-up"
        );
        assert_eq!(view.skipped_count, 1);
        assert_eq!(view.done_count, 0);
        let skipped = lessons(&view.done);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].status, Some(LogStatus::Skipped));
        assert_eq!(skipped[0].note.as_deref(), Some("out all morning"));
    }

    #[test]
    fn hs3_b_a_paused_enrollment_empties_every_list_without_touching_the_log() {
        let plan = sample_week(2);
        let mut enrollment = sample_enrollment(2, "2026-09-07");
        enrollment.paused = true;
        let logs = vec![LogRow {
            subject_id: 1,
            assignment_id: None,
            scheduled_date: "2026-09-07".into(),
            status: LogStatus::Done,
            note: None,
        }];
        let view = today_view(&plan, &enrollment, &logs, "2026-09-09");
        assert!(view.due_today.is_empty());
        assert!(view.catch_up.is_empty());
        assert!(view.done.is_empty());
        assert_eq!(
            (view.done_count, view.skipped_count, view.total_count),
            (0, 0, 0)
        );
        assert!(!is_school_day(&enrollment, "2026-09-09"));
        assert_eq!(logs.len(), 1, "the log itself is untouched");
    }

    #[test]
    fn hs3_b_a_year_complete_pointer_deals_nothing_and_is_not_an_error() {
        let plan = sample_week(3);
        let mut enrollment = sample_enrollment(4, "2026-09-07");
        assert!(enrollment.year_complete());
        assert!(occurrences(&plan, &enrollment).is_empty());
        enrollment.current_week = 3;
        assert!(!enrollment.year_complete());
        assert!(!occurrences(&plan, &enrollment).is_empty());
    }

    #[test]
    fn hs3_b_finish_week_is_offered_once_the_week_is_complete_or_the_last_school_day_arrives() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        assert!(!week_complete(&plan, &enrollment, &[]));
        assert!(
            !can_finish_week(&plan, &enrollment, &[], "2026-09-09"),
            "midweek with work outstanding"
        );
        assert!(
            can_finish_week(&plan, &enrollment, &[], "2026-09-11"),
            "today has reached the last school day"
        );

        let every_log: Vec<LogRow> = occurrences(&plan, &enrollment)
            .into_iter()
            .map(|occurrence| LogRow {
                subject_id: occurrence.subject_id,
                assignment_id: occurrence.assignment_id,
                scheduled_date: occurrence.scheduled_date,
                status: LogStatus::Done,
                note: None,
            })
            .collect();
        assert!(week_complete(&plan, &enrollment, &every_log));
        assert!(can_finish_week(
            &plan,
            &enrollment,
            &every_log,
            "2026-09-08"
        ));
    }

    #[test]
    fn hs3_b_an_assignment_row_with_its_own_days_overrides_the_subjects() {
        let mut plan = reading_plan("MW", 1);
        plan.subjects[0].rows[0].days = Some(days("F"));
        let enrollment = sample_enrollment(1, "2026-09-07");
        assert_eq!(
            spread_of(&plan, &enrollment),
            vec![(Weekday::Fri, None)],
            "rule 1: assignment.days ∨ subject.days"
        );
    }

    #[test]
    fn hs3_b_together_renders_a_shared_occurrence_once_and_names_the_boys_who_finished_it() {
        let plan = sample_week(2);
        let isaiah = sample_enrollment(2, "2026-09-07");
        let mut nathaniel = isaiah.clone();
        nathaniel.profile_id = 2;

        let isaiah_logs = vec![LogRow {
            subject_id: 3,
            assignment_id: Some(32),
            scheduled_date: "2026-09-09".into(),
            status: LogStatus::Done,
            note: None,
        }];
        let together = together_view(
            &[(isaiah, isaiah_logs), (nathaniel, Vec::new())],
            &plan,
            "2026-09-09",
        );

        let old_tales: Vec<&TogetherOccurrence> = together
            .iter()
            .filter(|slot| {
                slot.occurrence.subject_id == 3 && slot.occurrence.scheduled_date == "2026-09-09"
            })
            .collect();
        assert_eq!(old_tales.len(), 1, "one row, not one a boy");
        assert_eq!(old_tales[0].user_ids, vec![1, 2]);
        assert_eq!(old_tales[0].done_user_ids, vec![1], "1 of 2");
        assert_eq!(
            old_tales[0].occurrence.status, None,
            "a partial group is not done"
        );
        assert!(
            together.iter().all(|slot| slot.occurrence.shared),
            "Together carries only shared subjects — Sums and Copywork stay per boy"
        );
    }

    // -- accept (g): week_grid ---------------------------------------------

    #[test]
    fn hs3_g_the_week_grid_for_fixture_week_two_has_six_rows_of_five_cells() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let grid = week_grid(&plan, &enrollment, &[], "2026-09-07", true);

        assert_eq!(grid.rows.len(), 6, "the free_read subject has no row");
        assert!(grid
            .rows
            .iter()
            .all(|row| row.category != Category::FreeRead));
        assert_eq!(grid.days.len(), 5);
        assert!(grid
            .rows
            .iter()
            .all(|row| row.cells.len() == grid.days.len()));
        assert!(grid.dated);
        assert_eq!((grid.week, grid.weeks, grid.term), (2, 3, 2));

        let fables = grid
            .rows
            .iter()
            .find(|row| row.title == "Fables")
            .expect("Fables has a row even in the week it is dealt nothing");
        assert!(
            fables.cells.iter().all(|cell| cell.is_empty()),
            "Fables has no rows in week 2"
        );

        let twice_told = grid
            .rows
            .iter()
            .find(|row| row.title == "Twice Told")
            .expect("Twice Told row");
        let tuesday = grid
            .days
            .iter()
            .position(|day| *day == Weekday::Tue)
            .expect("Tuesday is a school day");
        assert_eq!(
            twice_told.cells[tuesday].len(),
            2,
            "two rows over one day land on the same Tuesday"
        );
        assert_eq!(
            twice_told
                .cells
                .iter()
                .map(|cell| cell.len())
                .sum::<usize>(),
            2
        );
    }

    #[test]
    fn hs3_g_an_undated_grid_reports_dated_false_and_its_dates_are_only_advisory() {
        let plan = sample_week(3);
        let enrollment = sample_enrollment(2, "2026-09-07");
        // HS4's anchor for week 3 while the pointer is on week 2.
        let anchor = add_days(&enrollment.week_started_on, 7).expect("one week on");
        let grid = week_grid(&plan, &enrollment, &[], &anchor, false);

        assert!(!grid.dated);
        assert_eq!(grid.week, 3);
        assert_eq!(grid.rows.len(), 6);
        let dates: Vec<String> = grid
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().flatten())
            .map(|occurrence| occurrence.scheduled_date.clone())
            .collect();
        assert!(!dates.is_empty());
        assert!(
            dates.iter().all(|date| date.as_str() >= "2026-09-14"),
            "every advisory date sits in the week the anchor names"
        );
    }

    // -- accept (h): month_view --------------------------------------------

    #[test]
    fn hs3_h_september_2026_has_thirty_days_and_only_the_span_is_dealt_out() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-28");
        let logs = vec![LogRow {
            subject_id: 1,
            assignment_id: None,
            scheduled_date: "2026-09-01".into(),
            status: LogStatus::Done,
            note: None,
        }];
        let extras = vec![extra(7, "2026-09-10", None)];
        let view = month_view(
            Some(&enrollment),
            Some(&plan),
            &logs,
            &extras,
            2026,
            9,
            "2026-09-28",
        );

        assert_eq!(view.days.len(), 30);
        assert_eq!(view.user_id, 1);
        let in_week: Vec<&str> = view
            .days
            .iter()
            .filter(|day| day.in_current_week)
            .map(|day| day.date.as_str())
            .collect();
        assert_eq!(
            in_week,
            vec!["2026-09-28", "2026-09-29", "2026-09-30"],
            "the span's other four days fall in October"
        );

        let first = &view.days[0];
        assert_eq!(first.date, "2026-09-01");
        assert_eq!(first.total, None, "a past week's plan is not reconstructed");
        assert_eq!(first.done, 1, "its log rows");
        assert_eq!(first.week, None);

        let tenth = view
            .days
            .iter()
            .find(|day| day.date == "2026-09-10")
            .expect("the tenth");
        assert_eq!((tenth.extras, tenth.total), (1, None));

        let monday = view
            .days
            .iter()
            .find(|day| day.date == "2026-09-28")
            .expect("the twenty-eighth");
        assert_eq!(monday.weekday, Weekday::Mon);
        assert!(monday.is_school_day);
        assert_eq!(monday.week, Some(2));
        let dealt_that_day = occurrences(&plan, &enrollment)
            .iter()
            .filter(|occurrence| occurrence.scheduled_date == "2026-09-28")
            .count() as u32;
        assert_eq!(
            monday.total,
            Some(dealt_that_day + monday.extras),
            "merged occurrences plus that day's extras"
        );
        assert!(dealt_that_day > 0);
    }

    #[test]
    fn hs3_h_with_nobody_enrolled_a_month_is_extras_only() {
        let extras = vec![extra(7, "2026-09-10", None), extra(8, "2026-09-10", None)];
        let view = month_view(None, None, &[], &extras, 2026, 9, "2026-09-28");

        assert_eq!(view.days.len(), 30);
        assert!(view.days.iter().all(|day| day.total.is_none()
            && day.week.is_none()
            && !day.is_school_day
            && !day.in_current_week));
        let tenth = view
            .days
            .iter()
            .find(|day| day.date == "2026-09-10")
            .expect("the tenth");
        assert_eq!(tenth.extras, 2, "extras are still counted");
        assert_eq!(
            view.user_id, 1,
            "taken from the extras when nobody is enrolled"
        );
    }

    #[test]
    fn hs3_h_february_is_twenty_eight_days_in_2026_and_twenty_nine_in_2024() {
        assert_eq!(
            month_view(None, None, &[], &[], 2026, 2, "2026-02-01")
                .days
                .len(),
            28
        );
        assert_eq!(
            month_view(None, None, &[], &[], 2024, 2, "2024-02-01")
                .days
                .len(),
            29
        );
    }

    // -- accept (i): merge_extras ------------------------------------------

    /// The whole of accept (i) in one table: `week_started_on = date − 2`, so
    /// the span is `[date − 2, date + 4]`.
    #[test]
    fn hs3_i_merge_extras_files_each_task_by_its_date_and_counts_only_the_current_span() {
        let date = "2026-09-09";
        let span_from = add_days(date, -2).expect("two days back");
        let span_to = add_days(&span_from, 6).expect("the span ends six days later");
        assert_eq!(
            (span_from.as_str(), span_to.as_str()),
            ("2026-09-07", "2026-09-13")
        );

        let at = |delta: i32| add_days(date, delta).expect("a date in range");
        let extras = vec![
            extra(1, date, None),                     // due today
            extra(2, &at(-3), None),                  // catch-up
            extra(3, &at(-14), None),                 // catch-up, the floor
            extra(4, &at(-15), None),                 // nowhere
            extra(5, &at(-1), Some(LogStatus::Done)), // done
            extra(6, &at(9), None),                   // no list, no count
            extra(7, &at(1), None),                   // no list, counted
            // QH1-03: ticked today — `due_today` **and** `done`, counted once.
            extra(8, date, Some(LogStatus::Done)),
        ];

        let mut today = BoyToday {
            user_id: 1,
            name: String::new(),
            due_today: Vec::new(),
            catch_up: Vec::new(),
            done: Vec::new(),
            done_count: 0,
            skipped_count: 0,
            total_count: 0,
        };
        merge_extras(&mut today, &extras, date, (&span_from, &span_to));

        assert_eq!(
            titles(&today.due_today),
            vec!["Task 1", "Task 8"],
            "QH1-03: a ticked extra dated today keeps its place in due_today, \
             so the boy can untick it on the TV"
        );
        assert_eq!(titles(&today.catch_up), vec!["Task 2", "Task 3"]);
        assert_eq!(
            titles(&today.done),
            vec!["Task 5", "Task 8"],
            "and is listed in done as well — the two lists overlap by design"
        );

        // Counted: 1 (today), 5 (yesterday, done), 7 (tomorrow) and 8 (today,
        // done) are inside `[date − 2, date + 4]`. 2, 3, 4 are before it and 6
        // is after it. Task 8 appears in two lists and is still counted once.
        assert_eq!(today.total_count, 4);
        assert_eq!(today.done_count, 2);
        assert_eq!(today.skipped_count, 0);

        for item in today
            .due_today
            .iter()
            .chain(&today.catch_up)
            .chain(&today.done)
        {
            assert!(
                matches!(item, DayItem::Extra(_)),
                "D-2: an extra is a DayItem::Extra, never a LessonOccurrence"
            );
        }
    }

    #[test]
    fn hs3_i_merge_extras_ignores_another_boys_task_and_counts_a_skipped_one() {
        let date = "2026-09-09";
        let mut someone_else = extra(1, date, None);
        someone_else.user_id = 2;
        let extras = vec![someone_else, extra(2, date, Some(LogStatus::Skipped))];

        let mut today = BoyToday {
            user_id: 1,
            name: String::new(),
            due_today: Vec::new(),
            catch_up: Vec::new(),
            done: Vec::new(),
            done_count: 0,
            skipped_count: 0,
            total_count: 0,
        };
        merge_extras(&mut today, &extras, date, ("2026-09-07", "2026-09-13"));

        assert_eq!(
            titles(&today.due_today),
            vec!["Task 2"],
            "another boy's task is ignored; the skipped one is dated today and \
             so keeps its place in due_today (QH1-03)"
        );
        assert_eq!(titles(&today.done), vec!["Task 2"]);
        assert_eq!((today.total_count, today.skipped_count), (1, 1));
    }

    #[test]
    fn hs3_i_extras_join_a_boys_real_today_lists_alongside_his_lessons() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let mut view = today_view(&plan, &enrollment, &[], "2026-09-09");
        let lessons_due = view.due_today.len();
        let lesson_total = view.total_count;

        merge_extras(
            &mut view,
            &[extra(11, "2026-09-09", None)],
            "2026-09-09",
            ("2026-09-07", "2026-09-13"),
        );

        assert_eq!(view.due_today.len(), lessons_due + 1);
        assert_eq!(view.total_count, lesson_total + 1);
        assert!(matches!(
            view.due_today.last(),
            Some(DayItem::Extra(task)) if task.id == 11
        ));
    }

    /// QA round 4, QH4-01 — H3 rule 10's completeness half: an extra dated
    /// inside the current week's span is part of the Finish-week check.
    #[test]
    fn hs3_i_an_unfinished_extra_inside_the_span_holds_finish_week_back() {
        let plan = sample_week(2);
        let enrollment = sample_enrollment(2, "2026-09-07");
        let every_log: Vec<LogRow> = occurrences(&plan, &enrollment)
            .into_iter()
            .map(|o| LogRow {
                subject_id: o.subject_id,
                assignment_id: o.assignment_id,
                scheduled_date: o.scheduled_date,
                status: LogStatus::Done,
                note: None,
            })
            .collect();
        let midweek = "2026-09-09";

        assert!(can_finish_week_with_extras(
            &plan,
            &enrollment,
            &every_log,
            &[],
            midweek
        ));
        assert!(
            !can_finish_week_with_extras(
                &plan,
                &enrollment,
                &every_log,
                &[extra(1, "2026-09-10", None)],
                midweek
            ),
            "rule 10: an unfinished extra dated inside the span is part of the week"
        );
        assert!(can_finish_week_with_extras(
            &plan,
            &enrollment,
            &every_log,
            &[extra(1, "2026-09-10", Some(LogStatus::Done))],
            midweek
        ));
        assert!(
            can_finish_week_with_extras(
                &plan,
                &enrollment,
                &every_log,
                &[extra(2, "2026-09-18", None)],
                midweek
            ),
            "an extra outside the span is not this week's"
        );
        assert!(
            can_finish_week_with_extras(
                &plan,
                &enrollment,
                &[],
                &[extra(1, "2026-09-10", None)],
                "2026-09-11"
            ),
            "the last-school-day clause is untouched"
        );
    }
}
