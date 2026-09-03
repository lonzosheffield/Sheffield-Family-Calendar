//! Queries over the School tab's tables (HS1 "Do", `migrations/0005_homeschool.sql`).
//!
//! Conventions copied verbatim from [`crate::server::db`]:
//!
//! * **Mutations are generic over `impl sqlx::SqliteExecutor<'_>`** so a server
//!   fn can share one `pool.begin()` transaction with
//!   [`crate::server::db::claim_mutation`] — a failed write then rolls the
//!   idempotency claim back with it instead of burying the key (QA round 1,
//!   Q1-08). `&SqlitePool`, `&mut *tx` and `&mut SqliteConnection` all satisfy
//!   the bound.
//! * **Reads take `&SqlitePool`** and their callers pass the *read* pool
//!   (`docs/HANDOFF.md` H-9) so a `SELECT` never queues behind the single
//!   write connection.
//! * Every date is a `YYYY-MM-DD` string, server-local, supplied by the caller
//!   — never read from the clock at write time (R-24).
//!
//! The row structs below are storage shapes, not wire types: HS3 owns the
//! shared DTOs in `src/shared/homeschool.rs` and HS4's server functions do the
//! mapping. Keeping them here is what lets HS1 and HS3 run in the same wave on
//! disjoint files.

use sqlx::{Row, SqlitePool};

// ---------------------------------------------------------------------------
// Row shapes
// ---------------------------------------------------------------------------

/// One row of `curricula`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumRow {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub weeks: i64,
    pub term_weeks: i64,
    pub source_note: Option<String>,
}

/// One row of `subjects`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRow {
    pub id: i64,
    pub curriculum_id: i64,
    pub name: String,
    pub category: String,
    pub source: Option<String>,
    pub days: String,
    pub shared: bool,
    pub icon_name: Option<String>,
    pub sort_order: i64,
}

/// One row of `assignments` — a subject's work for one week, at one ordinal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentRow {
    pub id: i64,
    pub subject_id: i64,
    pub week: i64,
    pub ordinal: i64,
    pub text: String,
    pub detail: Option<String>,
    pub days: Option<String>,
}

/// One row of `term_notes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermNoteRow {
    pub id: i64,
    pub curriculum_id: i64,
    pub term: i64,
    pub kind: String,
    pub text: String,
    pub sort_order: i64,
}

/// One row of `enrollments`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentRow {
    pub id: i64,
    pub profile_id: i64,
    pub curriculum_id: i64,
    pub curriculum_slug: String,
    pub curriculum_name: String,
    pub weeks: i64,
    pub term_weeks: i64,
    pub current_week: i64,
    pub week_started_on: String,
    pub school_days: String,
    pub paused: bool,
    pub started_on: String,
}

/// One row of `lesson_log` — the state of one occurrence for one boy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonLogRow {
    pub id: i64,
    pub profile_id: i64,
    pub subject_id: i64,
    pub assignment_id: Option<i64>,
    pub week: i64,
    pub scheduled_date: String,
    pub status: String,
    pub note: Option<String>,
    pub completed_on: String,
}

/// One row of `lesson_extras` — a parent-authored task on a boy's date (H8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraRow {
    pub id: i64,
    pub profile_id: i64,
    pub scheduled_date: String,
    pub title: String,
    pub category: String,
    pub text: Option<String>,
    pub sort_order: i64,
    pub status: Option<String>,
    pub note: Option<String>,
    pub completed_on: Option<String>,
}

/// Everything one week of one curriculum is built from. HS3's occurrence rule
/// turns this into `LessonOccurrence`s; HS1 only fetches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeekPlanRows {
    pub curriculum: CurriculumRow,
    /// Every subject of the curriculum, in `(sort_order, id)` order —
    /// `free_read` included, since the phone renders those as a reference list.
    pub subjects: Vec<SubjectRow>,
    /// Only the assignments **for this week**, in `(subject_id, ordinal)` order.
    pub assignments: Vec<AssignmentRow>,
    /// Only the notes for this week's term, in `(kind, sort_order, id)` order.
    pub term_notes: Vec<TermNoteRow>,
    /// The 1-based term this week falls in — `(week - 1) / term_weeks + 1`
    /// (§4 default 4).
    pub term: i64,
}

/// Per-date log counts for the Month view (H6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayLogCount {
    pub scheduled_date: String,
    pub done: i64,
    pub skipped: i64,
}

// ---------------------------------------------------------------------------
// Row decoding
// ---------------------------------------------------------------------------

fn curriculum_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<CurriculumRow, sqlx::Error> {
    Ok(CurriculumRow {
        id: row.try_get("id")?,
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        weeks: row.try_get("weeks")?,
        term_weeks: row.try_get("term_weeks")?,
        source_note: row.try_get("source_note")?,
    })
}

fn subject_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SubjectRow, sqlx::Error> {
    Ok(SubjectRow {
        id: row.try_get("id")?,
        curriculum_id: row.try_get("curriculum_id")?,
        name: row.try_get("name")?,
        category: row.try_get("category")?,
        source: row.try_get("source")?,
        days: row.try_get("days")?,
        shared: row.try_get::<i64, _>("shared")? != 0,
        icon_name: row.try_get("icon_name")?,
        sort_order: row.try_get("sort_order")?,
    })
}

fn assignment_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<AssignmentRow, sqlx::Error> {
    Ok(AssignmentRow {
        id: row.try_get("id")?,
        subject_id: row.try_get("subject_id")?,
        week: row.try_get("week")?,
        ordinal: row.try_get("ordinal")?,
        text: row.try_get("text")?,
        detail: row.try_get("detail")?,
        days: row.try_get("days")?,
    })
}

fn term_note_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TermNoteRow, sqlx::Error> {
    Ok(TermNoteRow {
        id: row.try_get("id")?,
        curriculum_id: row.try_get("curriculum_id")?,
        term: row.try_get("term")?,
        kind: row.try_get("kind")?,
        text: row.try_get("text")?,
        sort_order: row.try_get("sort_order")?,
    })
}

fn enrollment_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<EnrollmentRow, sqlx::Error> {
    Ok(EnrollmentRow {
        id: row.try_get("id")?,
        profile_id: row.try_get("profile_id")?,
        curriculum_id: row.try_get("curriculum_id")?,
        curriculum_slug: row.try_get("curriculum_slug")?,
        curriculum_name: row.try_get("curriculum_name")?,
        weeks: row.try_get("weeks")?,
        term_weeks: row.try_get("term_weeks")?,
        current_week: row.try_get("current_week")?,
        week_started_on: row.try_get("week_started_on")?,
        school_days: row.try_get("school_days")?,
        paused: row.try_get::<i64, _>("paused")? != 0,
        started_on: row.try_get("started_on")?,
    })
}

fn log_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<LessonLogRow, sqlx::Error> {
    Ok(LessonLogRow {
        id: row.try_get("id")?,
        profile_id: row.try_get("profile_id")?,
        subject_id: row.try_get("subject_id")?,
        assignment_id: row.try_get("assignment_id")?,
        week: row.try_get("week")?,
        scheduled_date: row.try_get("scheduled_date")?,
        status: row.try_get("status")?,
        note: row.try_get("note")?,
        completed_on: row.try_get("completed_on")?,
    })
}

fn extra_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ExtraRow, sqlx::Error> {
    Ok(ExtraRow {
        id: row.try_get("id")?,
        profile_id: row.try_get("profile_id")?,
        scheduled_date: row.try_get("scheduled_date")?,
        title: row.try_get("title")?,
        category: row.try_get("category")?,
        text: row.try_get("text")?,
        sort_order: row.try_get("sort_order")?,
        status: row.try_get("status")?,
        note: row.try_get("note")?,
        completed_on: row.try_get("completed_on")?,
    })
}

/// `SELECT` list shared by every query that returns an [`EnrollmentRow`]: the
/// enrollment joined to the curriculum it points at, so a caller never has to
/// make a second round trip for `weeks` / `term_weeks` (both of which the week
/// pointer and the term number are computed from).
const ENROLLMENT_SELECT: &str = "
SELECT e.id, e.profile_id, e.curriculum_id,
       c.slug AS curriculum_slug, c.name AS curriculum_name,
       c.weeks, c.term_weeks,
       e.current_week, e.week_started_on, e.school_days, e.paused, e.started_on
FROM enrollments e
JOIN curricula c ON c.id = e.curriculum_id
";

/// `(week - 1) / term_weeks + 1`, clamped at 1 (§4 default 4). `term_weeks` is
/// `CHECK (term_weeks >= 1)` in the schema, so the division is always safe.
pub fn term_for(week: i64, term_weeks: i64) -> i64 {
    if term_weeks < 1 {
        return 1;
    }
    (week.max(1) - 1) / term_weeks + 1
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Everything needed to compute week `week` of `curriculum_id`, or `None` when
/// there is no such curriculum.
///
/// Assignments are filtered to this week only; term notes to this week's term.
/// Subjects are **not** filtered — the occurrence rule needs them all, and the
/// phone renders `free_read` subjects as a reference list.
pub async fn week_plan(
    pool: &SqlitePool,
    curriculum_id: i64,
    week: i64,
) -> Result<Option<WeekPlanRows>, sqlx::Error> {
    let Some(curriculum) = sqlx::query(
        "SELECT id, slug, name, weeks, term_weeks, source_note FROM curricula WHERE id = ?1",
    )
    .bind(curriculum_id)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(curriculum_from_row)
    .transpose()?
    else {
        return Ok(None);
    };

    let subjects = sqlx::query(
        "SELECT id, curriculum_id, name, category, source, days, shared, icon_name, sort_order \
         FROM subjects WHERE curriculum_id = ?1 ORDER BY sort_order, id",
    )
    .bind(curriculum_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(subject_from_row)
    .collect::<Result<Vec<_>, _>>()?;

    let assignments = sqlx::query(
        "SELECT a.id, a.subject_id, a.week, a.ordinal, a.text, a.detail, a.days \
         FROM assignments a JOIN subjects s ON s.id = a.subject_id \
         WHERE s.curriculum_id = ?1 AND a.week = ?2 \
         ORDER BY a.subject_id, a.ordinal, a.id",
    )
    .bind(curriculum_id)
    .bind(week)
    .fetch_all(pool)
    .await?
    .iter()
    .map(assignment_from_row)
    .collect::<Result<Vec<_>, _>>()?;

    let term = term_for(week, curriculum.term_weeks);
    let term_notes = sqlx::query(
        "SELECT id, curriculum_id, term, kind, text, sort_order FROM term_notes \
         WHERE curriculum_id = ?1 AND term = ?2 ORDER BY kind, sort_order, id",
    )
    .bind(curriculum_id)
    .bind(term)
    .fetch_all(pool)
    .await?
    .iter()
    .map(term_note_from_row)
    .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(WeekPlanRows {
        curriculum,
        subjects,
        assignments,
        term_notes,
        term,
    }))
}

/// One boy's enrollment, or `None` when he is not enrolled (H6's empty state —
/// never an error, §3 HS4 accept (j)).
pub async fn enrollment(
    pool: &SqlitePool,
    profile_id: i64,
) -> Result<Option<EnrollmentRow>, sqlx::Error> {
    sqlx::query(&format!("{ENROLLMENT_SELECT} WHERE e.profile_id = ?1"))
        .bind(profile_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(enrollment_from_row)
        .transpose()
}

/// Every enrollment, in profile order — the phone's Today view groups these by
/// `(curriculum_id, current_week)` (H4).
pub async fn all_enrollments(pool: &SqlitePool) -> Result<Vec<EnrollmentRow>, sqlx::Error> {
    sqlx::query(&format!("{ENROLLMENT_SELECT} ORDER BY e.profile_id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(enrollment_from_row)
        .collect()
}

/// The **Together group** (H4): every enrollment sharing `(curriculum_id,
/// current_week)`. A shared subject's occurrence is rendered once for this
/// whole group and a tick fans out one `lesson_log` row per member.
pub async fn together_group(
    pool: &SqlitePool,
    curriculum_id: i64,
    week: i64,
) -> Result<Vec<EnrollmentRow>, sqlx::Error> {
    sqlx::query(&format!(
        "{ENROLLMENT_SELECT} WHERE e.curriculum_id = ?1 AND e.current_week = ?2 \
         ORDER BY e.profile_id"
    ))
    .bind(curriculum_id)
    .bind(week)
    .fetch_all(pool)
    .await?
    .iter()
    .map(enrollment_from_row)
    .collect()
}

/// Every log row one boy has for one week.
pub async fn logs(
    pool: &SqlitePool,
    profile_id: i64,
    week: i64,
) -> Result<Vec<LessonLogRow>, sqlx::Error> {
    sqlx::query(
        "SELECT id, profile_id, subject_id, assignment_id, week, scheduled_date, status, note, \
         completed_on FROM lesson_log WHERE profile_id = ?1 AND week = ?2 \
         ORDER BY scheduled_date, subject_id, IFNULL(assignment_id, 0), id",
    )
    .bind(profile_id)
    .bind(week)
    .fetch_all(pool)
    .await?
    .iter()
    .map(log_from_row)
    .collect()
}

/// Per-date `done` / `skipped` counts over `[from, to]`, inclusive on both
/// ends, for the Month view's `done/total` badges (H6).
pub async fn log_counts_between(
    pool: &SqlitePool,
    profile_id: i64,
    from: &str,
    to: &str,
) -> Result<Vec<DayLogCount>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT scheduled_date, \
                SUM(CASE WHEN status = 'done'    THEN 1 ELSE 0 END) AS done, \
                SUM(CASE WHEN status = 'skipped' THEN 1 ELSE 0 END) AS skipped \
         FROM lesson_log \
         WHERE profile_id = ?1 AND scheduled_date BETWEEN ?2 AND ?3 \
         GROUP BY scheduled_date ORDER BY scheduled_date",
    )
    .bind(profile_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(DayLogCount {
                scheduled_date: row.try_get("scheduled_date")?,
                done: row.try_get("done")?,
                skipped: row.try_get("skipped")?,
            })
        })
        .collect()
}

/// How many curricula are loaded — `/health`'s `curricula` key (T1.7's report
/// gains one field in HS1).
pub async fn count_curricula(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM curricula")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

// ---------------------------------------------------------------------------
// Occurrence state (lesson_log)
// ---------------------------------------------------------------------------

/// The identity of one occurrence: exactly the columns of the
/// `lesson_log_occurrence` unique index, in its order (H1).
///
/// A struct rather than five loose parameters so the key can never be
/// assembled in the wrong order at a call site, and so [`set_occurrence`] and
/// [`clear_occurrence`] are provably talking about the same row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrenceKey {
    pub profile_id: i64,
    pub week: i64,
    pub subject_id: i64,
    /// `None` for a daily subject with no per-week assignment row. The index
    /// stores `IFNULL(assignment_id, 0)`, so `None` dedupes like any other key.
    pub assignment_id: Option<i64>,
    pub scheduled_date: String,
}

impl OccurrenceKey {
    pub fn new(
        profile_id: i64,
        week: i64,
        subject_id: i64,
        assignment_id: Option<i64>,
        scheduled_date: impl Into<String>,
    ) -> Self {
        Self {
            profile_id,
            week,
            subject_id,
            assignment_id,
            scheduled_date: scheduled_date.into(),
        }
    }
}

/// Record an occurrence as `done` or `skipped`.
///
/// `INSERT … ON CONFLICT DO NOTHING`, exactly as H1 specifies: calling this
/// twice for one [`OccurrenceKey`] leaves exactly one row, and the `NULL
/// assignment_id` case (a daily subject with no per-week assignment row)
/// dedupes like every other.
///
/// Changing an occurrence from `done` to `skipped` is
/// [`clear_occurrence`] followed by this — untick deletes the row (§4
/// default 7), so there is never a stale status to update in place.
pub async fn set_occurrence(
    executor: impl sqlx::SqliteExecutor<'_>,
    key: &OccurrenceKey,
    status: &str,
    note: Option<&str>,
    completed_on: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO lesson_log
            (profile_id, subject_id, assignment_id, week, scheduled_date, status, note, completed_on)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(key.profile_id)
    .bind(key.subject_id)
    .bind(key.assignment_id)
    .bind(key.week)
    .bind(&key.scheduled_date)
    .bind(status)
    .bind(note)
    .bind(completed_on)
    .execute(executor)
    .await?;
    Ok(())
}

/// Untick an occurrence: delete its row (H1 — untick keeps nothing). Returns
/// how many rows went, so a caller can tell a real untick from a replay.
///
/// The `WHERE` clause is the unique index's key, `IFNULL` included on both
/// sides so a `NULL assignment_id` matches a `NULL assignment_id`.
pub async fn clear_occurrence(
    executor: impl sqlx::SqliteExecutor<'_>,
    key: &OccurrenceKey,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM lesson_log
        WHERE profile_id = ?1 AND week = ?2 AND subject_id = ?3
          AND IFNULL(assignment_id, 0) = IFNULL(?4, 0)
          AND scheduled_date = ?5
        "#,
    )
    .bind(key.profile_id)
    .bind(key.week)
    .bind(key.subject_id)
    .bind(key.assignment_id)
    .bind(&key.scheduled_date)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Enrollment
// ---------------------------------------------------------------------------

/// Enroll a boy, or move an existing enrollment onto a new curriculum / week.
///
/// `profile_id` is `UNIQUE`, so this replaces rather than duplicating (§3 HS1
/// accept (e)). `started_on` is only ever written by the *first* enrollment —
/// re-enrolling keeps the day school actually began.
pub async fn upsert_enrollment(
    executor: impl sqlx::SqliteExecutor<'_>,
    profile_id: i64,
    curriculum_id: i64,
    current_week: i64,
    school_days: &str,
    date: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO enrollments
            (profile_id, curriculum_id, current_week, week_started_on, school_days, started_on)
        VALUES (?1, ?2, ?3, ?4, ?5, ?4)
        ON CONFLICT (profile_id) DO UPDATE SET
            curriculum_id   = excluded.curriculum_id,
            current_week    = excluded.current_week,
            week_started_on = excluded.week_started_on,
            school_days     = excluded.school_days,
            updated_at      = CURRENT_TIMESTAMP
        "#,
    )
    .bind(profile_id)
    .bind(curriculum_id)
    .bind(current_week)
    .bind(date)
    .bind(school_days)
    .execute(executor)
    .await?;
    Ok(())
}

/// Move the week pointer, stamping the new anchor (H2: every move sets
/// `week_started_on = today`, and every occurrence date derives from it).
pub async fn set_week(
    executor: impl sqlx::SqliteExecutor<'_>,
    profile_id: i64,
    week: i64,
    week_started_on: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE enrollments SET current_week = ?2, week_started_on = ?3, \
         updated_at = CURRENT_TIMESTAMP WHERE profile_id = ?1",
    )
    .bind(profile_id)
    .bind(week)
    .bind(week_started_on)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// School's out ⚽ / school's back (H2, W-14). Touches no log row.
pub async fn set_paused(
    executor: impl sqlx::SqliteExecutor<'_>,
    profile_id: i64,
    paused: bool,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE enrollments SET paused = ?2, updated_at = CURRENT_TIMESTAMP WHERE profile_id = ?1",
    )
    .bind(profile_id)
    .bind(i64::from(paused))
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// Unenroll: delete the enrollment and **keep the log** (§4 default 14).
pub async fn unenroll(
    executor: impl sqlx::SqliteExecutor<'_>,
    profile_id: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM enrollments WHERE profile_id = ?1")
        .bind(profile_id)
        .execute(executor)
        .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Plan edits
// ---------------------------------------------------------------------------

/// Create or rewrite one week's text for one subject (H6 row action 6: tapping
/// a daily subject with no row is how "Math: lesson 14" gets typed in).
///
/// `days` is the row's **per-week override** of the subject's own days (H3 rule
/// 1: `assignment.days ∨ subject.days`), the QH3-04 amendment of 2026-09-03.
/// `None` writes `NULL`, which is how a row says "inherit the subject's days" —
/// so a caller that means to leave the override alone must hand back the value
/// it read. Like [`set_subject_schedule`], the string must already have passed
/// the caller's `parse_days` check (H7: "days strings pass `parse_days` in the
/// loader **and** in both server fns that write them"); the schema's `GLOB` is
/// the belt to that braces.
pub async fn upsert_assignment(
    executor: impl sqlx::SqliteExecutor<'_>,
    subject_id: i64,
    week: i64,
    ordinal: i64,
    text: &str,
    detail: Option<&str>,
    days: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO assignments (subject_id, week, ordinal, text, detail, days)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT (subject_id, week, ordinal) DO UPDATE SET
            text   = excluded.text,
            detail = excluded.detail,
            days   = excluded.days
        "#,
    )
    .bind(subject_id)
    .bind(week)
    .bind(ordinal)
    .bind(text)
    .bind(detail)
    .bind(days)
    .execute(executor)
    .await?;
    Ok(())
}

/// School settings' per-subject controls: which days it falls on and whether it
/// is read aloud to everyone (H6).
///
/// `days` must already have passed the caller's `parse_days` check (H7: "days
/// strings pass `parse_days` in the loader **and** in both server fns that
/// write them"); the schema's `GLOB` is the belt to that braces.
pub async fn set_subject_schedule(
    executor: impl sqlx::SqliteExecutor<'_>,
    subject_id: i64,
    days: &str,
    shared: bool,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE subjects SET days = ?2, shared = ?3 WHERE id = ?1")
        .bind(subject_id)
        .bind(days)
        .bind(i64::from(shared))
        .execute(executor)
        .await?;
    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Extras (H8)
// ---------------------------------------------------------------------------

/// `SELECT` list shared by every extras query.
const EXTRA_SELECT: &str = "
SELECT id, profile_id, scheduled_date, title, category, text, sort_order,
       status, note, completed_on
FROM lesson_extras
";

/// Add a parent-authored task to a boy's date.
///
/// `sort_order` is `MAX(sort_order) + 1` **within `(profile_id,
/// scheduled_date)`**, computed inside the same statement so two parents adding
/// a task at once cannot both read the same maximum.
pub async fn add_extra(
    executor: impl sqlx::SqliteExecutor<'_>,
    profile_id: i64,
    scheduled_date: &str,
    title: &str,
    category: &str,
    text: Option<&str>,
) -> Result<ExtraRow, sqlx::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO lesson_extras
            (profile_id, scheduled_date, title, category, text, sort_order)
        VALUES (?1, ?2, ?3, ?4, ?5,
            (SELECT IFNULL(MAX(sort_order), 0) + 1 FROM lesson_extras
             WHERE profile_id = ?1 AND scheduled_date = ?2))
        RETURNING id, profile_id, scheduled_date, title, category, text, sort_order,
                  status, note, completed_on
        "#,
    )
    .bind(profile_id)
    .bind(scheduled_date)
    .bind(title)
    .bind(category)
    .bind(text)
    .fetch_one(executor)
    .await?;
    extra_from_row(&row)
}

/// One extra by id.
pub async fn extra(pool: &SqlitePool, extra_id: i64) -> Result<Option<ExtraRow>, sqlx::Error> {
    sqlx::query(&format!("{EXTRA_SELECT} WHERE id = ?1"))
        .bind(extra_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(extra_from_row)
        .transpose()
}

/// Every extra for one boy over `[from, to]`, **inclusive on both ends**,
/// ordered by `(scheduled_date, sort_order, id)`.
pub async fn extras_between(
    pool: &SqlitePool,
    profile_id: i64,
    from: &str,
    to: &str,
) -> Result<Vec<ExtraRow>, sqlx::Error> {
    sqlx::query(&format!(
        "{EXTRA_SELECT} WHERE profile_id = ?1 AND scheduled_date BETWEEN ?2 AND ?3 \
         ORDER BY scheduled_date, sort_order, id"
    ))
    .bind(profile_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?
    .iter()
    .map(extra_from_row)
    .collect()
}

/// Edit an extra's title, category, body or date (parent only, H7).
/// Deliberately does **not** touch its status — that is [`set_extra_status`].
pub async fn update_extra(
    executor: impl sqlx::SqliteExecutor<'_>,
    extra_id: i64,
    title: &str,
    category: &str,
    text: Option<&str>,
    scheduled_date: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE lesson_extras SET title = ?2, category = ?3, text = ?4, scheduled_date = ?5, \
         updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
    )
    .bind(extra_id)
    .bind(title)
    .bind(category)
    .bind(text)
    .bind(scheduled_date)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// Delete an extra outright (parent only, H7) — ticked or not.
pub async fn delete_extra(
    executor: impl sqlx::SqliteExecutor<'_>,
    extra_id: i64,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM lesson_extras WHERE id = ?1")
        .bind(extra_id)
        .execute(executor)
        .await?;
    Ok(result.rows_affected())
}

/// Tick, skip or untick an extra. `status = None` is "back to do": it clears
/// `completed_on` and `completed_at` with it, so an untick leaves no trace of
/// the tick (§4 default 7). Always bumps `updated_at`.
pub async fn set_extra_status(
    executor: impl sqlx::SqliteExecutor<'_>,
    extra_id: i64,
    status: Option<&str>,
    note: Option<&str>,
    completed_on: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE lesson_extras SET
            status       = ?2,
            note         = ?3,
            completed_on = CASE WHEN ?2 IS NULL THEN NULL ELSE ?4 END,
            completed_at = CASE WHEN ?2 IS NULL THEN NULL ELSE CURRENT_TIMESTAMP END,
            updated_at   = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
    )
    .bind(extra_id)
    .bind(status)
    .bind(note)
    .bind(completed_on)
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

/// One extra's `updated_at`, for the tests that prove an edit bumps it.
pub async fn extra_updated_at(
    pool: &SqlitePool,
    extra_id: i64,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT updated_at FROM lesson_extras WHERE id = ?1")
            .bind(extra_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.0))
}
