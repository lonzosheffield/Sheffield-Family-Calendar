//! The curriculum file format, its validator, the boot-time loader and the
//! Isaiah enrollment seed (HS1, `docs/homeschool/PLAN_HOMESCHOOL.md` §2 H5 and
//! the post-A micro-commit spec in §"Waves, micro-commits, roster").
//!
//! Three things worth knowing before editing this file:
//!
//! * **The loader never deletes and never overwrites.** It inserts only rows
//!   that are missing, keyed on `(slug, subject name, week, ordinal)`, so a
//!   parent's inline edit on the phone survives every reboot. The bulk-fix path
//!   — the only thing that ever rewrites rows — is
//!   `family-hub.exe import-curriculum <file> --replace`, and even that
//!   preserves `lesson_log` rows by *matching* subjects and assignments rather
//!   than dropping and re-inserting them (both cascade to the log).
//! * **Validation is all-or-nothing and ordered** (H5). The first failure
//!   rejects the whole file, and the message always carries the file name and a
//!   `line N`, resolved from a `toml::Spanned` byte range — a parent editing a
//!   36-week file by hand gets told *where*.
//! * **§0 N1.** Nothing in this module, and nothing in any test that drives it,
//!   may contain curriculum content from `docs/homeschool/curriculum/`. That
//!   directory is gitignored; the only curriculum this repository carries is
//!   the invented `tests/fixtures/curricula/sample-year.toml`.
//!
//! `parse_days` here is a thin wrapper over the canonical
//! `src/shared/homeschool.rs::parse_days` (HS3): same letters, same
//! `M T W R F S U` order, same "no unknown letter, no repeat" rule (H3 rule 1).
//! The one thing the wrapper adds is rejecting the empty string, which the
//! shared rule accepts as "no day" but a curriculum file may never write
//! (the schema's `GLOB '[MTWRFSU]*'` forbids it too). Collapsed by the Boss
//! post-A micro-commit per `docs/HANDOFF.md` HS-3.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sqlx::SqlitePool;
use toml::Spanned;

use crate::server::config::FamilyHubConfig;

/// The four `subjects.category` values the schema's `CHECK` allows.
pub const CATEGORIES: [&str; 4] = ["daily", "reading", "weekly", "free_read"];
/// The three `term_notes.kind` values the schema's `CHECK` allows.
pub const TERM_NOTE_KINDS: [&str; 3] = ["geography", "free_read", "poetry"];
/// The schema default, and the family's school week (§4 default 2).
pub const DEFAULT_DAYS: &str = "MTWRF";
/// H5's default when a file omits `term_weeks`.
pub const DEFAULT_TERM_WEEKS: i64 = 12;
/// The upper bound `curricula.weeks`' `CHECK` enforces.
pub const MAX_WEEKS: i64 = 104;

/// The boy the Boss enrolls during the run (§5 Q1, §4 default 1).
pub const SEED_PROFILE_NAME: &str = "Isaiah";
/// The curriculum slug that seed enrolls him on. Absent from the database
/// (because the family's file has not been copied into the data directory yet)
/// is an ordinary, logged, non-fatal state.
pub const SEED_CURRICULUM_SLUG: &str = "ao-year-1";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A rejected curriculum file. Always carries the file name and a 1-based line,
/// so `Display` reads `sample-year.toml: line 42: ...` (§3 HS1 accept (c)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for CurriculumError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: line {}: {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for CurriculumError {}

/// Anything that can go wrong loading one file: a rejected file or a database
/// failure.
#[derive(Debug)]
pub enum LoadError {
    Curriculum(CurriculumError),
    Sql(sqlx::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Curriculum(err) => write!(f, "{err}"),
            Self::Sql(err) => write!(f, "database error: {err}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<CurriculumError> for LoadError {
    fn from(err: CurriculumError) -> Self {
        Self::Curriculum(err)
    }
}

impl From<sqlx::Error> for LoadError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sql(err)
    }
}

impl From<std::io::Error> for LoadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// The 1-based line containing byte offset `offset` in `source`.
fn line_of(source: &str, offset: usize) -> usize {
    let clamped = offset.min(source.len());
    source[..clamped].bytes().filter(|b| *b == b'\n').count() + 1
}

// ---------------------------------------------------------------------------
// The file format (H5 / docs/homeschool/CURRICULUM_FORMAT.md)
// ---------------------------------------------------------------------------

/// A whole curriculum file. `deny_unknown_fields` everywhere: a typo'd key is a
/// rejected file, not a silently ignored line.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurriculumFile {
    pub curriculum: CurriculumMeta,
    #[serde(default)]
    pub subject: Vec<Spanned<SubjectSpec>>,
    #[serde(default)]
    pub assignment: Vec<Spanned<AssignmentSpec>>,
    #[serde(default)]
    pub term_note: Vec<Spanned<TermNoteSpec>>,
}

/// `[curriculum]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurriculumMeta {
    pub slug: Spanned<String>,
    pub name: String,
    pub weeks: Spanned<i64>,
    pub term_weeks: Option<Spanned<i64>>,
    pub source_note: Option<String>,
}

/// `[[subject]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectSpec {
    pub name: String,
    pub category: String,
    pub source: Option<String>,
    pub days: Option<String>,
    pub shared: Option<bool>,
    pub icon: Option<String>,
    pub sort_order: Option<i64>,
}

/// `[[assignment]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentSpec {
    pub subject: String,
    pub week: i64,
    pub ordinal: Option<i64>,
    pub text: String,
    pub detail: Option<String>,
    pub days: Option<String>,
}

/// `[[term_note]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermNoteSpec {
    pub term: i64,
    pub kind: String,
    pub text: String,
    pub sort_order: Option<i64>,
}

// ---------------------------------------------------------------------------
// The validated, defaulted result
// ---------------------------------------------------------------------------

/// A file that passed every rule in H5, with every default already applied.
/// Nothing downstream of validation ever sees an `Option` that still means
/// "the file did not say".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCurriculum {
    pub slug: String,
    pub name: String,
    pub weeks: i64,
    pub term_weeks: i64,
    pub source_note: Option<String>,
    pub subjects: Vec<ValidatedSubject>,
    pub assignments: Vec<ValidatedAssignment>,
    pub term_notes: Vec<ValidatedTermNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSubject {
    pub name: String,
    pub category: String,
    pub source: Option<String>,
    pub days: String,
    pub shared: bool,
    pub icon_name: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAssignment {
    pub subject: String,
    pub week: i64,
    pub ordinal: i64,
    pub text: String,
    pub detail: Option<String>,
    pub days: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedTermNote {
    pub term: i64,
    pub kind: String,
    pub text: String,
    pub sort_order: i64,
}

// ---------------------------------------------------------------------------
// parse_days
// ---------------------------------------------------------------------------

/// `MTWRF` -> `['M','T','W','R','F']`, in the fixed order `M T W R F S U`.
///
/// Rejects an empty string (the schema's `GLOB '[MTWRFSU]*'` does too), any
/// letter outside the seven, and any repeat — `"Th"` is a mistake for Thursday
/// (which is `R`), `"MM"` is a typo, and both must be told about rather than
/// silently deduplicated (H3 rule 1). The letter rule itself is the shared
/// [`crate::shared::homeschool::parse_days`]; only the empty-string rejection
/// and the `char` shape (what the TOML and the `days` column carry) are local.
pub fn parse_days(letters: &str) -> Result<Vec<char>, String> {
    if letters.is_empty() {
        return Err("a days string may not be empty".to_string());
    }
    crate::shared::homeschool::parse_days(letters)
        .map(|days| {
            days.into_iter()
                .map(crate::shared::homeschool::Weekday::letter)
                .collect()
        })
        .map_err(|err| format!("{err} (R = Thursday, U = Sunday)"))
}

/// The canonical spelling of a validated days string, in `M T W R F S U` order.
fn canonical_days(letters: &str) -> Result<String, String> {
    Ok(parse_days(letters)?.into_iter().collect())
}

/// `ceil(weeks / term_weeks)` — the highest term number a file may name.
/// Written out rather than `i64::div_ceil`, which is still unstable on the
/// pinned toolchain (`int_roundings`); both operands are positive here, so the
/// `+ divisor - 1` form is exact.
fn term_count(weeks: i64, term_weeks: i64) -> i64 {
    let divisor = term_weeks.max(1);
    (weeks.max(0) + divisor - 1) / divisor
}

/// H5's slug rule, `^[a-z0-9-]{1,64}$`, without a regex dependency.
fn slug_is_valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

// ---------------------------------------------------------------------------
// Parse + validate
// ---------------------------------------------------------------------------

/// Parse and fully validate one curriculum file.
///
/// `file_name` is only used for the error message; `source` is the file's text.
/// The rules run in exactly the order H5 lists them, and the first failure
/// rejects the whole file — nothing is ever half-applied.
pub fn parse_curriculum(
    file_name: &str,
    source: &str,
) -> Result<ValidatedCurriculum, CurriculumError> {
    let err = |line: usize, message: String| CurriculumError {
        file: file_name.to_string(),
        line,
        message,
    };

    let parsed: CurriculumFile = toml::from_str(source).map_err(|error| {
        let line = error
            .span()
            .map(|span| line_of(source, span.start))
            .unwrap_or(1);
        err(line, error.message().to_string())
    })?;

    // 1. slug
    let slug_line = line_of(source, parsed.curriculum.slug.span().start);
    let slug = parsed.curriculum.slug.get_ref().clone();
    if !slug_is_valid(&slug) {
        return Err(err(
            slug_line,
            format!("slug {slug:?} must match ^[a-z0-9-]{{1,64}}$"),
        ));
    }

    // 2. weeks
    let weeks_line = line_of(source, parsed.curriculum.weeks.span().start);
    let weeks = *parsed.curriculum.weeks.get_ref();
    if !(1..=MAX_WEEKS).contains(&weeks) {
        return Err(err(
            weeks_line,
            format!("weeks must be between 1 and {MAX_WEEKS}, got {weeks}"),
        ));
    }

    // 3. term_weeks
    let (term_weeks, term_weeks_line) = match &parsed.curriculum.term_weeks {
        Some(spanned) => (*spanned.get_ref(), line_of(source, spanned.span().start)),
        None => (DEFAULT_TERM_WEEKS, weeks_line),
    };
    if term_weeks < 1 {
        return Err(err(
            term_weeks_line,
            format!("term_weeks must be at least 1, got {term_weeks}"),
        ));
    }

    // 4. every category and kind is in its set
    for subject in &parsed.subject {
        if !CATEGORIES.contains(&subject.get_ref().category.as_str()) {
            return Err(err(
                line_of(source, subject.span().start),
                format!(
                    "subject {:?} has category {:?}; expected one of {}",
                    subject.get_ref().name,
                    subject.get_ref().category,
                    CATEGORIES.join(", ")
                ),
            ));
        }
    }
    for note in &parsed.term_note {
        if !TERM_NOTE_KINDS.contains(&note.get_ref().kind.as_str()) {
            return Err(err(
                line_of(source, note.span().start),
                format!(
                    "term_note kind {:?}; expected one of {}",
                    note.get_ref().kind,
                    TERM_NOTE_KINDS.join(", ")
                ),
            ));
        }
    }

    // 5. every days string parses
    let mut subject_days: Vec<String> = Vec::with_capacity(parsed.subject.len());
    for subject in &parsed.subject {
        let raw = subject.get_ref().days.as_deref().unwrap_or(DEFAULT_DAYS);
        match canonical_days(raw) {
            Ok(days) => subject_days.push(days),
            Err(why) => {
                return Err(err(
                    line_of(source, subject.span().start),
                    format!("subject {:?} days {raw:?}: {why}", subject.get_ref().name),
                ))
            }
        }
    }
    let mut assignment_days: Vec<Option<String>> = Vec::with_capacity(parsed.assignment.len());
    for assignment in &parsed.assignment {
        match assignment.get_ref().days.as_deref() {
            None => assignment_days.push(None),
            Some(raw) => match canonical_days(raw) {
                Ok(days) => assignment_days.push(Some(days)),
                Err(why) => {
                    return Err(err(
                        line_of(source, assignment.span().start),
                        format!(
                            "assignment for {:?} week {} days {raw:?}: {why}",
                            assignment.get_ref().subject,
                            assignment.get_ref().week
                        ),
                    ))
                }
            },
        }
    }

    // 6. subject names are unique
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for subject in &parsed.subject {
        if !names.insert(subject.get_ref().name.as_str()) {
            return Err(err(
                line_of(source, subject.span().start),
                format!(
                    "subject name {:?} appears more than once",
                    subject.get_ref().name
                ),
            ));
        }
    }

    // 7. every assignment.subject resolves, by name, to a subject in this file
    for assignment in &parsed.assignment {
        if !names.contains(assignment.get_ref().subject.as_str()) {
            return Err(err(
                line_of(source, assignment.span().start),
                format!(
                    "assignment names subject {:?}, which no [[subject]] in this file declares",
                    assignment.get_ref().subject
                ),
            ));
        }
    }

    // 8. every assignment week is inside the curriculum
    for assignment in &parsed.assignment {
        let week = assignment.get_ref().week;
        if !(1..=weeks).contains(&week) {
            return Err(err(
                line_of(source, assignment.span().start),
                format!(
                    "assignment for {:?} is week {week}, but the curriculum has {weeks} weeks",
                    assignment.get_ref().subject
                ),
            ));
        }
    }

    // 9. (subject, week, ordinal) is unique
    let mut triples: BTreeSet<(String, i64, i64)> = BTreeSet::new();
    for assignment in &parsed.assignment {
        let spec = assignment.get_ref();
        let ordinal = spec.ordinal.unwrap_or(1);
        if ordinal < 1 {
            return Err(err(
                line_of(source, assignment.span().start),
                format!(
                    "assignment for {:?} week {} has ordinal {ordinal}; ordinals start at 1",
                    spec.subject, spec.week
                ),
            ));
        }
        if !triples.insert((spec.subject.clone(), spec.week, ordinal)) {
            return Err(err(
                line_of(source, assignment.span().start),
                format!(
                    "two assignments share (subject {:?}, week {}, ordinal {ordinal})",
                    spec.subject, spec.week
                ),
            ));
        }
    }

    // 10. every term is inside the curriculum
    let terms = term_count(weeks, term_weeks);
    for note in &parsed.term_note {
        let term = note.get_ref().term;
        if !(1..=terms).contains(&term) {
            return Err(err(
                line_of(source, note.span().start),
                format!("term_note is term {term}, but the curriculum has {terms} terms"),
            ));
        }
    }

    // Everything holds: apply the defaults and hand back a total value.
    let subjects = parsed
        .subject
        .iter()
        .zip(subject_days)
        .enumerate()
        .map(|(index, (subject, days))| {
            let spec = subject.get_ref();
            ValidatedSubject {
                name: spec.name.clone(),
                category: spec.category.clone(),
                source: spec.source.clone(),
                days,
                // H5: reading/weekly are read aloud to everyone, daily is per
                // boy, unless the file says otherwise (§4 default 5).
                shared: spec
                    .shared
                    .unwrap_or(spec.category != "daily" && spec.category != "free_read"),
                icon_name: spec.icon.clone(),
                sort_order: spec.sort_order.unwrap_or(index as i64 + 1),
            }
        })
        .collect();

    let assignments = parsed
        .assignment
        .iter()
        .zip(assignment_days)
        .map(|(assignment, days)| {
            let spec = assignment.get_ref();
            ValidatedAssignment {
                subject: spec.subject.clone(),
                week: spec.week,
                ordinal: spec.ordinal.unwrap_or(1),
                text: spec.text.clone(),
                detail: spec.detail.clone(),
                days,
            }
        })
        .collect();

    let term_notes = parsed
        .term_note
        .iter()
        .map(|note| {
            let spec = note.get_ref();
            ValidatedTermNote {
                term: spec.term,
                kind: spec.kind.clone(),
                text: spec.text.clone(),
                sort_order: spec.sort_order.unwrap_or(0),
            }
        })
        .collect();

    Ok(ValidatedCurriculum {
        slug,
        name: parsed.curriculum.name.clone(),
        weeks,
        term_weeks,
        source_note: parsed.curriculum.source_note.clone(),
        subjects,
        assignments,
        term_notes,
    })
}

/// Read and validate a file from disk.
pub fn read_curriculum(path: &Path) -> Result<ValidatedCurriculum, LoadError> {
    let source = std::fs::read_to_string(path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok(parse_curriculum(&name, &source)?)
}

// ---------------------------------------------------------------------------
// Insert missing rows only
// ---------------------------------------------------------------------------

/// What one insert-missing-only pass actually wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InsertReport {
    pub curriculum_id: i64,
    pub subjects_inserted: usize,
    pub assignments_inserted: usize,
    pub term_notes_inserted: usize,
}

/// Insert every row of `curriculum` that is not in the database already, keyed
/// on `(slug, subject name, week, ordinal)` (H5).
///
/// Never updates and never deletes: a parent's inline edit to
/// `assignments.text` survives every subsequent boot, which is the whole point
/// of the rule. One transaction, so a file is either wholly applied or wholly
/// absent.
pub async fn insert_missing(
    pool: &SqlitePool,
    curriculum: &ValidatedCurriculum,
) -> Result<InsertReport, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO curricula (slug, name, weeks, term_weeks, source_note) \
         VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (slug) DO NOTHING",
    )
    .bind(&curriculum.slug)
    .bind(&curriculum.name)
    .bind(curriculum.weeks)
    .bind(curriculum.term_weeks)
    .bind(curriculum.source_note.as_deref())
    .execute(&mut *tx)
    .await?;

    let (curriculum_id,): (i64,) = sqlx::query_as("SELECT id FROM curricula WHERE slug = ?1")
        .bind(&curriculum.slug)
        .fetch_one(&mut *tx)
        .await?;

    let mut report = InsertReport {
        curriculum_id,
        ..InsertReport::default()
    };

    for subject in &curriculum.subjects {
        let result = sqlx::query(
            "INSERT INTO subjects \
                 (curriculum_id, name, category, source, days, shared, icon_name, sort_order) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT (curriculum_id, name) DO NOTHING",
        )
        .bind(curriculum_id)
        .bind(&subject.name)
        .bind(&subject.category)
        .bind(subject.source.as_deref())
        .bind(&subject.days)
        .bind(i64::from(subject.shared))
        .bind(subject.icon_name.as_deref())
        .bind(subject.sort_order)
        .execute(&mut *tx)
        .await?;
        report.subjects_inserted += result.rows_affected() as usize;
    }

    let subject_ids = subject_ids_by_name(&mut tx, curriculum_id).await?;

    for assignment in &curriculum.assignments {
        let Some(subject_id) = subject_ids.get(&assignment.subject) else {
            // Unreachable after validation rule 7, but a missing subject must
            // never be a panic on the boot path.
            continue;
        };
        let result = sqlx::query(
            "INSERT INTO assignments (subject_id, week, ordinal, text, detail, days) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (subject_id, week, ordinal) DO NOTHING",
        )
        .bind(subject_id)
        .bind(assignment.week)
        .bind(assignment.ordinal)
        .bind(&assignment.text)
        .bind(assignment.detail.as_deref())
        .bind(assignment.days.as_deref())
        .execute(&mut *tx)
        .await?;
        report.assignments_inserted += result.rows_affected() as usize;
    }

    for note in &curriculum.term_notes {
        // `term_notes` carries no unique constraint (H1), so "already there" is
        // an explicit NOT EXISTS on the natural key rather than ON CONFLICT.
        let result = sqlx::query(
            "INSERT INTO term_notes (curriculum_id, term, kind, text, sort_order) \
             SELECT ?1, ?2, ?3, ?4, ?5 \
             WHERE NOT EXISTS (SELECT 1 FROM term_notes \
                 WHERE curriculum_id = ?1 AND term = ?2 AND kind = ?3 AND text = ?4)",
        )
        .bind(curriculum_id)
        .bind(note.term)
        .bind(&note.kind)
        .bind(&note.text)
        .bind(note.sort_order)
        .execute(&mut *tx)
        .await?;
        report.term_notes_inserted += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(report)
}

async fn subject_ids_by_name(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    curriculum_id: i64,
) -> Result<BTreeMap<String, i64>, sqlx::Error> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM subjects WHERE curriculum_id = ?1")
            .bind(curriculum_id)
            .fetch_all(&mut **tx)
            .await?;
    Ok(rows.into_iter().map(|(id, name)| (name, id)).collect())
}

// ---------------------------------------------------------------------------
// --replace
// ---------------------------------------------------------------------------

/// What a `--replace` actually changed, printed by `import-curriculum` (H5,
/// W-8: "rows whose subject vanished are deleted and counted in the output").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplaceReport {
    pub curriculum_id: i64,
    pub subjects_written: usize,
    pub subjects_removed: usize,
    pub assignments_written: usize,
    pub assignments_removed: usize,
    pub term_notes_written: usize,
    pub lesson_logs_removed: usize,
}

impl std::fmt::Display for ReplaceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} subjects written, {} removed; {} assignments written, {} removed; \
             {} term notes; {} lesson log rows removed with them",
            self.subjects_written,
            self.subjects_removed,
            self.assignments_written,
            self.assignments_removed,
            self.term_notes_written,
            self.lesson_logs_removed
        )
    }
}

/// Replace one curriculum's `subjects` / `assignments` / `term_notes` from the
/// file, in one transaction (H5's bulk-fix path).
///
/// Subjects and assignments that still exist in the file are **updated in
/// place**, never dropped and re-inserted: both cascade to `lesson_log`, so
/// re-inserting would silently erase the boys' history. Only rows the file no
/// longer contains are deleted, and the `lesson_log` rows that go with them are
/// counted before the delete so the output can report them.
pub async fn replace_curriculum(
    pool: &SqlitePool,
    curriculum: &ValidatedCurriculum,
) -> Result<ReplaceReport, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO curricula (slug, name, weeks, term_weeks, source_note) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT (slug) DO UPDATE SET \
             name = excluded.name, weeks = excluded.weeks, \
             term_weeks = excluded.term_weeks, source_note = excluded.source_note",
    )
    .bind(&curriculum.slug)
    .bind(&curriculum.name)
    .bind(curriculum.weeks)
    .bind(curriculum.term_weeks)
    .bind(curriculum.source_note.as_deref())
    .execute(&mut *tx)
    .await?;

    let (curriculum_id,): (i64,) = sqlx::query_as("SELECT id FROM curricula WHERE slug = ?1")
        .bind(&curriculum.slug)
        .fetch_one(&mut *tx)
        .await?;

    let mut report = ReplaceReport {
        curriculum_id,
        ..ReplaceReport::default()
    };

    // Subjects: upsert every one the file names...
    for subject in &curriculum.subjects {
        sqlx::query(
            "INSERT INTO subjects \
                 (curriculum_id, name, category, source, days, shared, icon_name, sort_order) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT (curriculum_id, name) DO UPDATE SET \
                 category = excluded.category, source = excluded.source, \
                 days = excluded.days, shared = excluded.shared, \
                 icon_name = excluded.icon_name, sort_order = excluded.sort_order",
        )
        .bind(curriculum_id)
        .bind(&subject.name)
        .bind(&subject.category)
        .bind(subject.source.as_deref())
        .bind(&subject.days)
        .bind(i64::from(subject.shared))
        .bind(subject.icon_name.as_deref())
        .bind(subject.sort_order)
        .execute(&mut *tx)
        .await?;
        report.subjects_written += 1;
    }

    // ...then drop the ones it does not, counting the log rows that go with
    // them first (the FK cascade would otherwise take them away silently).
    let keep_names: BTreeSet<&str> = curriculum
        .subjects
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    let existing: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM subjects WHERE curriculum_id = ?1")
            .bind(curriculum_id)
            .fetch_all(&mut *tx)
            .await?;
    for (subject_id, name) in &existing {
        if keep_names.contains(name.as_str()) {
            continue;
        }
        let (orphaned,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM lesson_log WHERE subject_id = ?1")
                .bind(subject_id)
                .fetch_one(&mut *tx)
                .await?;
        report.lesson_logs_removed += orphaned as usize;
        sqlx::query("DELETE FROM subjects WHERE id = ?1")
            .bind(subject_id)
            .execute(&mut *tx)
            .await?;
        report.subjects_removed += 1;
    }

    let subject_ids = subject_ids_by_name(&mut tx, curriculum_id).await?;

    // Assignments: same shape, keyed on (subject_id, week, ordinal) so a row
    // that survives keeps its id and therefore its lesson_log rows.
    let mut keep_assignments: BTreeSet<(i64, i64, i64)> = BTreeSet::new();
    for assignment in &curriculum.assignments {
        let Some(subject_id) = subject_ids.get(&assignment.subject).copied() else {
            continue;
        };
        sqlx::query(
            "INSERT INTO assignments (subject_id, week, ordinal, text, detail, days) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (subject_id, week, ordinal) DO UPDATE SET \
                 text = excluded.text, detail = excluded.detail, days = excluded.days",
        )
        .bind(subject_id)
        .bind(assignment.week)
        .bind(assignment.ordinal)
        .bind(&assignment.text)
        .bind(assignment.detail.as_deref())
        .bind(assignment.days.as_deref())
        .execute(&mut *tx)
        .await?;
        report.assignments_written += 1;
        keep_assignments.insert((subject_id, assignment.week, assignment.ordinal));
    }

    let existing_assignments: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT a.id, a.subject_id, a.week, a.ordinal FROM assignments a \
         JOIN subjects s ON s.id = a.subject_id WHERE s.curriculum_id = ?1",
    )
    .bind(curriculum_id)
    .fetch_all(&mut *tx)
    .await?;
    for (id, subject_id, week, ordinal) in existing_assignments {
        if keep_assignments.contains(&(subject_id, week, ordinal)) {
            continue;
        }
        let (orphaned,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM lesson_log WHERE assignment_id = ?1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        report.lesson_logs_removed += orphaned as usize;
        sqlx::query("DELETE FROM assignments WHERE id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        report.assignments_removed += 1;
    }

    // Term notes carry no log rows, so they really are replaced wholesale.
    sqlx::query("DELETE FROM term_notes WHERE curriculum_id = ?1")
        .bind(curriculum_id)
        .execute(&mut *tx)
        .await?;
    for note in &curriculum.term_notes {
        sqlx::query(
            "INSERT INTO term_notes (curriculum_id, term, kind, text, sort_order) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(curriculum_id)
        .bind(note.term)
        .bind(&note.kind)
        .bind(&note.text)
        .bind(note.sort_order)
        .execute(&mut *tx)
        .await?;
        report.term_notes_written += 1;
    }

    tx.commit().await?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// The boot-time loader
// ---------------------------------------------------------------------------

/// What one scan of the curricula directory did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub loaded: usize,
    pub skipped: usize,
}

/// Every `*.toml` in `dir`, in a stable (lexicographic) order.
fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort();
    files
}

/// Scan `dir` and insert every missing row of every file it holds.
///
/// A bad file is logged at `warn` and skipped; a good sibling still loads (H5).
/// The directory not existing is not an error either — it is created first.
pub async fn load_directory(pool: &SqlitePool, dir: &Path) -> LoadReport {
    if let Err(err) = std::fs::create_dir_all(dir) {
        tracing::warn!(
            curricula_dir = %dir.display(),
            %err,
            "could not create the curricula directory; no curriculum will be loaded"
        );
        return LoadReport::default();
    }

    tracing::info!(
        curricula_dir = %dir.display(),
        "scanning the curricula directory"
    );

    let mut report = LoadReport::default();
    for path in toml_files(dir) {
        match load_one(pool, &path).await {
            Ok(inserted) => {
                report.loaded += 1;
                tracing::info!(
                    file = %path.display(),
                    curriculum_id = inserted.curriculum_id,
                    subjects = inserted.subjects_inserted,
                    assignments = inserted.assignments_inserted,
                    term_notes = inserted.term_notes_inserted,
                    "loaded a curriculum file"
                );
            }
            Err(err) => {
                report.skipped += 1;
                tracing::warn!(
                    file = %path.display(),
                    error = %err,
                    "skipped a curriculum file; every other file still loads"
                );
            }
        }
    }

    tracing::info!(
        loaded = report.loaded,
        skipped = report.skipped,
        "finished loading curricula"
    );
    report
}

async fn load_one(pool: &SqlitePool, path: &Path) -> Result<InsertReport, LoadError> {
    let curriculum = read_curriculum(path)?;
    Ok(insert_missing(pool, &curriculum).await?)
}

/// The boot-time entry point the Boss wires into `db::pools()` /
/// `db::migrate()` (HS1's one handoff; `docs/HANDOFF.md`).
///
/// Scans `config.curricula_dir()`, inserts every missing row, then runs the
/// Isaiah enrollment seed. A bad curriculum file can never fail this call — the
/// hub must boot for the rest of the family whatever state one TOML is in.
pub async fn load_and_seed(pool: &SqlitePool, config: &FamilyHubConfig) -> Result<(), sqlx::Error> {
    load_directory(pool, &config.curricula_dir()).await;
    seed_enrollments(pool).await
}

// ---------------------------------------------------------------------------
// The Isaiah enrollment seed
// ---------------------------------------------------------------------------

/// Enroll Isaiah at week 1 if he is not enrolled already (§5 Q1, D-6).
///
/// Uses the server's own local date as both `week_started_on` and `started_on`,
/// with no Monday arithmetic: `date_for` pushes a Monday or Tuesday into the
/// following week of the 7-day span, which is exactly what H2 intends.
pub async fn seed_enrollments(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    seed_enrollments_on(pool, &today).await
}

/// [`seed_enrollments`] with the run date injected, so the "a second boot
/// changes nothing" and "a renamed profile is skipped" cases are testable
/// without touching the clock.
///
/// `INSERT … ON CONFLICT (profile_id) DO NOTHING`, **never** an upsert: a
/// reboot must not reset the week a parent has advanced.
pub async fn seed_enrollments_on(pool: &SqlitePool, date: &str) -> Result<(), sqlx::Error> {
    let profiles: Vec<(i64,)> = sqlx::query_as("SELECT id FROM profiles WHERE name = ?1")
        .bind(SEED_PROFILE_NAME)
        .fetch_all(pool)
        .await?;
    let [(profile_id,)] = profiles.as_slice() else {
        tracing::warn!(
            name = SEED_PROFILE_NAME,
            matches = profiles.len(),
            "enrollment seed: expected exactly one profile with this name; skipping"
        );
        return Ok(());
    };

    let curriculum: Option<(i64,)> = sqlx::query_as("SELECT id FROM curricula WHERE slug = ?1")
        .bind(SEED_CURRICULUM_SLUG)
        .fetch_optional(pool)
        .await?;
    let Some((curriculum_id,)) = curriculum else {
        tracing::info!(
            slug = SEED_CURRICULUM_SLUG,
            "enrollment seed: no such curriculum is loaded yet; skipping"
        );
        return Ok(());
    };

    let result = sqlx::query(
        "INSERT INTO enrollments \
             (profile_id, curriculum_id, current_week, week_started_on, school_days, started_on) \
         VALUES (?1, ?2, 1, ?3, ?4, ?3) \
         ON CONFLICT (profile_id) DO NOTHING",
    )
    .bind(profile_id)
    .bind(curriculum_id)
    .bind(date)
    .bind(DEFAULT_DAYS)
    .execute(pool)
    .await?;

    if result.rows_affected() == 1 {
        tracing::info!(
            profile_id = *profile_id,
            curriculum_id,
            week_started_on = date,
            "enrollment seed: enrolled {SEED_PROFILE_NAME} at week 1"
        );
    } else {
        tracing::info!(
            profile_id = *profile_id,
            "enrollment seed: already enrolled; leaving the week pointer alone"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// import-curriculum (family-hub.exe)
// ---------------------------------------------------------------------------

/// `family-hub.exe import-curriculum <path> [--replace]` (H5).
///
/// Validates first, copies second: a bad path or a rejected file writes
/// **nothing** — not the destination copy, not a database row (§3 HS1 accept
/// (g)). `--replace` then rewrites that slug's rows in one transaction, keeping
/// every `lesson_log` row whose subject and assignment still exist.
///
/// Returns the human-readable summary the CLI prints, or the message it prints
/// to stderr before exiting non-zero.
pub async fn import_curriculum(
    config: &FamilyHubConfig,
    source_path: &Path,
    replace: bool,
) -> Result<String, String> {
    let curriculum = read_curriculum(source_path).map_err(|err| err.to_string())?;

    // Open the database *before* the copy (QA_HS_ROUND_1 QH1-08). Opening the
    // pool runs the boot loader over `curricula_dir()`, so if the copy landed
    // first the boot loader would insert this curriculum's rows and the
    // command's own `insert_missing` would then honestly report "0 subjects, 0
    // assignments, 0 term notes inserted" on a first import. Running the boot
    // loader against the directory as it was keeps the printed summary true.
    // A rejected file still never gets here: `read_curriculum` fails above.
    let pool = crate::server::db::pool()
        .await
        .map_err(|err| format!("could not open the database: {err}"))?;

    let dir = config.curricula_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;

    let file_name = source_path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source_path.display()))?;
    let destination = dir.join(file_name);
    if destination != source_path {
        std::fs::copy(source_path, &destination).map_err(|err| {
            format!(
                "could not copy {} to {}: {err}",
                source_path.display(),
                destination.display()
            )
        })?;
    }

    let summary = if replace {
        let report = replace_curriculum(pool, &curriculum)
            .await
            .map_err(|err| format!("could not replace {}: {err}", curriculum.slug))?;
        format!(
            "replaced {} from {}: {report}",
            curriculum.slug,
            destination.display()
        )
    } else {
        let report = insert_missing(pool, &curriculum)
            .await
            .map_err(|err| format!("could not import {}: {err}", curriculum.slug))?;
        format!(
            "imported {} from {}: {} subjects, {} assignments, {} term notes inserted \
             (existing rows left untouched)",
            curriculum.slug,
            destination.display(),
            report.subjects_inserted,
            report.assignments_inserted,
            report.term_notes_inserted
        )
    };

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Unit tests (pure — the database-backed acceptance suite is
// tests/homeschool_db_tests.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_days_returns_the_letters_in_the_fixed_order() {
        assert_eq!(parse_days("MTWRF").unwrap(), vec!['M', 'T', 'W', 'R', 'F']);
        assert_eq!(parse_days("FM").unwrap(), vec!['M', 'F']);
        assert_eq!(parse_days("U").unwrap(), vec!['U']);
    }

    #[test]
    fn parse_days_rejects_unknown_letters_repeats_and_the_empty_string() {
        assert!(parse_days("Th").is_err(), "h is not a day letter");
        assert!(parse_days("MM").is_err(), "M twice is a typo, not a Monday");
        assert!(parse_days("X").is_err());
        assert!(parse_days("").is_err());
    }

    #[test]
    fn line_of_counts_newlines_before_the_offset() {
        let source = "a\nbb\nccc\n";
        assert_eq!(line_of(source, 0), 1);
        assert_eq!(line_of(source, 2), 2);
        assert_eq!(line_of(source, 5), 3);
        assert_eq!(line_of(source, 9_999), 4);
    }

    #[test]
    fn term_count_is_weeks_divided_by_term_weeks_rounded_up() {
        assert_eq!(term_count(36, 12), 3);
        assert_eq!(term_count(3, 1), 3);
        assert_eq!(term_count(37, 12), 4);
        assert_eq!(term_count(1, 12), 1);
    }

    #[test]
    fn slug_is_valid_matches_lowercase_digits_and_hyphens_only() {
        assert!(slug_is_valid("sample-year"));
        assert!(slug_is_valid("ao-year-1"));
        assert!(!slug_is_valid("Sample-Year"));
        assert!(!slug_is_valid("sample year"));
        assert!(!slug_is_valid(""));
        assert!(!slug_is_valid(&"a".repeat(65)));
    }

    #[test]
    fn shared_defaults_to_true_for_reading_and_weekly_and_false_for_daily() {
        let source = "\
[curriculum]
slug = \"defaults\"
name = \"Defaults\"
weeks = 2

[[subject]]
name = \"Sums\"
category = \"daily\"

[[subject]]
name = \"Stories\"
category = \"reading\"

[[subject]]
name = \"Painting\"
category = \"weekly\"
";
        let parsed = parse_curriculum("defaults.toml", source).expect("valid file");
        assert_eq!(parsed.term_weeks, DEFAULT_TERM_WEEKS);
        assert!(!parsed.subjects[0].shared);
        assert!(parsed.subjects[1].shared);
        assert!(parsed.subjects[2].shared);
        assert_eq!(parsed.subjects[0].days, DEFAULT_DAYS);
        assert_eq!(parsed.subjects[0].sort_order, 1);
        assert_eq!(parsed.subjects[2].sort_order, 3);
    }

    #[test]
    fn an_unknown_key_is_rejected_with_the_file_name_and_a_line_number() {
        let source = "\
[curriculum]
slug = \"typo\"
name = \"Typo\"
weeks = 2
weekz = 3
";
        let err = parse_curriculum("typo.toml", source).expect_err("weekz is not a key");
        assert_eq!(err.file, "typo.toml");
        assert!(
            err.to_string().contains("line 5"),
            "expected the offending line, got {err}"
        );
    }
}
