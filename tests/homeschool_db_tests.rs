#![cfg(feature = "server")]
//! HS1 acceptance suite — migration `0005_homeschool.sql`, the queries over it,
//! the curriculum loader and the Isaiah enrollment seed
//! (`docs/homeschool/PLAN_HOMESCHOOL.md` §3, row HS1).
//!
//! | # | Assertion |
//! | - | --- |
//! | a | `family_v1.db` migrates to 5 with `[1,2,3,4,5]` recorded and every routine log row intact; a fresh DB reaches 5; `PRAGMA foreign_keys` = 1 |
//! | b | loading the fixture twice is a no-op; a parent edit survives a reload; `--replace` puts the file's text back and reports the count |
//! | c | six shapes of invalid file are rejected with the file name and a `line N`, and write nothing |
//! | d | `set_occurrence` is idempotent, `clear_occurrence` removes the row, a `NULL assignment_id` key dedupes, two weeks are two rows |
//! | e | enrolling twice replaces; deleting a profile cascades enrollment and log |
//! | f | `curricula_dir()` is absolute and created on demand; a bad file beside a good one loads exactly one curriculum and logs the path at `info` |
//! | g | *(in `tests/service_tests.rs`, which owns the real-binary harness)* `import-curriculum` with a bad path or bad TOML exits non-zero and writes nothing |
//! | h | §0 N1: no curriculum content is tracked anywhere in this repository |
//! | i | the `lesson_extras` CHECKs, `set_extra_status(None)`, `extras_between`, `add_extra`'s `sort_order`, and the `updated_at` bumps |
//!
//! Plus the post-A micro-commit's enrollment seed: a second boot changes
//! nothing, and a renamed profile is skipped.
//!
//! **§0 N1.** Every curriculum string in this file is invented. Nothing from
//! `docs/homeschool/curriculum/` (the family's licensed-for-family-use
//! schedules) appears here, and `no_curriculum_content_is_tracked_in_the_repo`
//! is the committed guard that keeps it that way.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use family_calendar::server::config::FamilyHubConfig;
use family_calendar::server::db;
use family_calendar::server::homeschool::db as hs;
use family_calendar::server::homeschool::loader;
use sqlx::SqlitePool;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

static SCRATCH_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, empty scratch directory for one test. Windows reuses PIDs, so the
/// directory is wiped before use exactly as `tests/profiles_tests.rs` does.
fn scratch_dir(name: &str) -> PathBuf {
    let n = SCRATCH_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("familyhub-hs1-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    repo_root().join("tests/fixtures/curricula/sample-year.toml")
}

/// sqlx wants forward slashes in a sqlite URL, even on Windows.
fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display().to_string().replace('\\', "/"))
}

/// A migrated, private, in-memory database. Every pragma `db::connect_options`
/// sets — `foreign_keys` above all — applies here exactly as it does to the
/// real file.
async fn memory_pool() -> SqlitePool {
    let pool = db::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    db::migrate(&pool).await.expect("migrations");
    pool
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|err| panic!("count {table}: {err}"));
    row.0
}

/// Row counts of every table this migration adds, as one comparable tuple.
async fn homeschool_counts(pool: &SqlitePool) -> (i64, i64, i64, i64) {
    (
        count(pool, "curricula").await,
        count(pool, "subjects").await,
        count(pool, "assignments").await,
        count(pool, "term_notes").await,
    )
}

fn read_fixture() -> loader::ValidatedCurriculum {
    loader::read_curriculum(&fixture_path()).expect("the committed fixture must validate")
}

async fn load_fixture(pool: &SqlitePool) -> loader::InsertReport {
    loader::insert_missing(pool, &read_fixture())
        .await
        .expect("insert the fixture")
}

async fn curriculum_id(pool: &SqlitePool, slug: &str) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT id FROM curricula WHERE slug = ?1")
        .bind(slug)
        .fetch_one(pool)
        .await
        .expect("curriculum by slug");
    row.0
}

async fn subject_id(pool: &SqlitePool, curriculum_id: i64, name: &str) -> i64 {
    let row: (i64,) =
        sqlx::query_as("SELECT id FROM subjects WHERE curriculum_id = ?1 AND name = ?2")
            .bind(curriculum_id)
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|err| panic!("subject {name}: {err}"));
    row.0
}

async fn assignment_text(pool: &SqlitePool, subject_id: i64, week: i64, ordinal: i64) -> String {
    let row: (String,) = sqlx::query_as(
        "SELECT text FROM assignments WHERE subject_id = ?1 AND week = ?2 AND ordinal = ?3",
    )
    .bind(subject_id)
    .bind(week)
    .bind(ordinal)
    .fetch_one(pool)
    .await
    .expect("assignment text");
    row.0
}

/// QA round 3, QH3-02: the second line under a reading row. The storage fn must
/// keep a `detail` it is handed, so that HS5's inline edit can carry the value
/// it already has in hand instead of nulling it.
async fn assignment_detail(
    pool: &SqlitePool,
    subject_id: i64,
    week: i64,
    ordinal: i64,
) -> Option<String> {
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT detail FROM assignments WHERE subject_id = ?1 AND week = ?2 AND ordinal = ?3",
    )
    .bind(subject_id)
    .bind(week)
    .bind(ordinal)
    .fetch_one(pool)
    .await
    .expect("assignment detail");
    row.0
}

/// A config rooted at a scratch directory — never the owner's real data dir.
fn scratch_config(data_dir: PathBuf) -> FamilyHubConfig {
    FamilyHubConfig {
        data_dir,
        http_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        tls_addr: "127.0.0.1:0".parse().expect("valid socket address"),
        screensaver_schedule_hour: None,
        log_level: None,
    }
}

// ---------------------------------------------------------------------------
// A tracing subscriber that records what was logged
// ---------------------------------------------------------------------------

/// A hand-rolled `tracing::Subscriber` that records every event as
/// `"<LEVEL> field=value ..."`, so a test can prove the loader really logged
/// the absolute curricula path at `info` (§3 HS1 accept (f)).
///
/// Hand-rolled for the same reason `src/server/config.rs`'s `CountingSubscriber`
/// is: `tracing-subscriber` is not a dependency of this crate, and adding one
/// to prove a log line fires would be a poor trade.
#[derive(Clone, Default)]
struct RecordingSubscriber(Arc<Mutex<Vec<String>>>);

impl RecordingSubscriber {
    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("recorded lines").clone()
    }
}

struct FieldCollector(String);

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!(" {}={:?}", field.name(), value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
}

impl tracing::Subscriber for RecordingSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = FieldCollector(format!("{}", event.metadata().level()));
        event.record(&mut collector);
        if let Ok(mut lines) = self.0.lock() {
            lines.push(collector.0);
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

// ---------------------------------------------------------------------------
// (a) the migration
// ---------------------------------------------------------------------------

/// HS1 (a), first half: a fresh database reaches version 5, every table this
/// migration adds exists, and the connection really is enforcing foreign keys
/// (every `ON DELETE CASCADE` in 0005 depends on it).
#[tokio::test]
async fn a_fresh_database_migrates_to_version_five_with_foreign_keys_enforced() {
    let pool = memory_pool().await;

    assert_eq!(
        db::migration_version(&pool).await.expect("version"),
        Some(5),
        "0005_homeschool must be the newest embedded migration"
    );

    let applied: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("_sqlx_migrations");
    assert_eq!(
        applied.iter().map(|(v,)| *v).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );

    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("sqlite_master");
    let tables: Vec<String> = tables.into_iter().map(|(n,)| n).collect();
    for expected in [
        "assignments",
        "curricula",
        "enrollments",
        "lesson_extras",
        "lesson_log",
        "subjects",
        "term_notes",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "0005 must create {expected}; got {tables:?}"
        );
    }

    let (foreign_keys,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .expect("PRAGMA foreign_keys");
    assert_eq!(foreign_keys, 1, "every cascade in 0005 depends on this");

    // The expression unique index is what makes an occurrence's identity real.
    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name IN \
         ('lesson_log', 'lesson_extras') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .expect("indexes");
    let indexes: Vec<String> = indexes.into_iter().map(|(n,)| n).collect();
    for expected in [
        "lesson_extras_day",
        "lesson_log_occurrence",
        "lesson_log_week",
    ] {
        assert!(
            indexes.iter().any(|i| i == expected),
            "0005 must create the {expected} index; got {indexes:?}"
        );
    }
}

/// HS1 (a), second half: the owner's real pre-migration `family.db` reaches
/// version 5 with `[1,2,3,4,5]` recorded and **every** routine log row intact.
#[tokio::test]
async fn the_v1_fixture_migrates_to_version_five_and_keeps_every_routine_log_row() {
    let fixture = repo_root().join("tests/fixtures/family_v1.db");
    assert!(
        fixture.is_file(),
        "missing v1 fixture {}",
        fixture.display()
    );

    let dir = scratch_dir("v1");
    let db_path = dir.join("family.db");
    std::fs::copy(&fixture, &db_path).expect("copy the v1 fixture");
    let url = sqlite_url(&db_path);

    let before = db::connect(&url).await.expect("open the fixture");
    let logs_before: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT user_id, template_id, date_logged FROM daily_routine_logs \
         ORDER BY user_id, template_id, date_logged",
    )
    .fetch_all(&before)
    .await
    .expect("v1 logs");
    assert!(
        logs_before.len() >= 10,
        "the fixture must carry real history, got {}",
        logs_before.len()
    );
    before.close().await;

    let pools = db::open_pools(&url).await.expect("pools");
    db::migrate(&pools.write).await.expect("migrate to 0005");

    let applied: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pools.read)
            .await
            .expect("_sqlx_migrations");
    assert_eq!(
        applied.iter().map(|(v,)| *v).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "0001 baselined, 0002..0005 applied on top"
    );
    assert_eq!(
        db::migration_version(&pools.read).await.expect("version"),
        Some(5)
    );

    let logs_after: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT user_id, template_id, date_logged FROM daily_routine_logs \
         ORDER BY user_id, template_id, date_logged",
    )
    .fetch_all(&pools.read)
    .await
    .expect("migrated logs");
    assert_eq!(
        logs_after, logs_before,
        "0005 must not touch a single routine log row"
    );

    pools.close().await;
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// (b) the loader is insert-missing-only, and --replace is the bulk fix
// ---------------------------------------------------------------------------

/// HS1 (b), first half: loading the same file twice writes nothing the second
/// time. The fixture's own shape is pinned here too (P-15), since five later
/// tasks build their expectations on it.
#[tokio::test]
async fn loading_the_fixture_twice_leaves_the_row_counts_identical() {
    let pool = memory_pool().await;

    let first = load_fixture(&pool).await;
    assert_eq!(first.subjects_inserted, 7, "the fixture has seven subjects");
    assert_eq!(first.assignments_inserted, 9);
    assert_eq!(first.term_notes_inserted, 3);

    let after_first = homeschool_counts(&pool).await;
    assert_eq!(after_first, (1, 7, 9, 3));

    let second = load_fixture(&pool).await;
    assert_eq!(second.curriculum_id, first.curriculum_id);
    assert_eq!(second.subjects_inserted, 0);
    assert_eq!(second.assignments_inserted, 0);
    assert_eq!(second.term_notes_inserted, 0);
    assert_eq!(
        homeschool_counts(&pool).await,
        after_first,
        "a second boot must write nothing at all"
    );
}

/// HS1 (b), second half: a parent's inline edit survives every reboot — that is
/// the entire reason the loader is insert-missing-only — and `--replace` is the
/// documented way to put the file's own text back, reporting what it removed.
#[tokio::test]
async fn a_parent_edit_survives_a_reload_and_replace_restores_the_file_text() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;

    let curriculum = curriculum_id(&pool, "sample-year").await;
    let old_tales = subject_id(&pool, curriculum, "Old Tales").await;
    let from_file = assignment_text(&pool, old_tales, 2, 1).await;

    // The parent retypes week 2 on the phone, handing back the `detail` the row
    // already carried (QA round 3, QH3-02: the client has it in `LessonOccurrence`).
    hs::upsert_assignment(
        &pool,
        old_tales,
        2,
        1,
        "ch. 2 (only the first half)",
        Some("stop at the bridge"),
        None,
    )
    .await
    .expect("parent edit");
    assert_eq!(
        assignment_text(&pool, old_tales, 2, 1).await,
        "ch. 2 (only the first half)"
    );
    assert_eq!(
        assignment_detail(&pool, old_tales, 2, 1).await,
        Some("stop at the bridge".to_string()),
        "the storage fn must keep a detail it is handed (QH3-02)"
    );

    // Reboot: the loader must leave it exactly as the parent left it.
    load_fixture(&pool).await;
    assert_eq!(
        assignment_text(&pool, old_tales, 2, 1).await,
        "ch. 2 (only the first half)",
        "the boot-time loader must never overwrite a parent's edit"
    );

    // `--replace` is the bulk fix, and it reports what it did.
    let report = loader::replace_curriculum(&pool, &read_fixture())
        .await
        .expect("replace");
    assert_eq!(assignment_text(&pool, old_tales, 2, 1).await, from_file);
    assert_eq!(report.subjects_written, 7);
    assert_eq!(report.assignments_written, 9);
    assert_eq!(report.term_notes_written, 3);
    assert_eq!(report.subjects_removed, 0);
    assert_eq!(report.assignments_removed, 0);
    assert_eq!(
        report.lesson_logs_removed, 0,
        "nothing vanished, so no log row may be counted as removed"
    );
    assert!(
        report.to_string().contains("0 lesson log rows removed"),
        "the CLI output must report the count: {report}"
    );
    assert_eq!(
        homeschool_counts(&pool).await,
        (1, 7, 9, 3),
        "a replace of the same file changes no row count"
    );
}

/// The half of `--replace` that W-8 actually asks for: rows whose subject
/// vanished from the file are deleted, and the `lesson_log` rows that go with
/// them are **counted in the output** rather than disappearing silently down
/// the foreign-key cascade. Every other boy's history survives.
#[tokio::test]
async fn replace_deletes_a_vanished_subject_and_counts_the_log_rows_it_took() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;

    let curriculum = curriculum_id(&pool, "sample-year").await;
    let painting = subject_id(&pool, curriculum, "Painting").await;
    let old_tales = subject_id(&pool, curriculum, "Old Tales").await;
    let old_tales_week_2: (i64,) = sqlx::query_as(
        "SELECT id FROM assignments WHERE subject_id = ?1 AND week = 2 AND ordinal = 1",
    )
    .bind(old_tales)
    .fetch_one(&pool)
    .await
    .expect("Old Tales week 2");

    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 2, painting, None, "2026-09-11"),
        "done",
        None,
        "2026-09-11",
    )
    .await
    .expect("tick Painting");
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 2, old_tales, Some(old_tales_week_2.0), "2026-09-07"),
        "done",
        None,
        "2026-09-07",
    )
    .await
    .expect("tick Old Tales");
    assert_eq!(count(&pool, "lesson_log").await, 2);

    // The parent's corrected file no longer has Painting at all.
    let mut trimmed = read_fixture();
    trimmed.subjects.retain(|s| s.name != "Painting");

    let report = loader::replace_curriculum(&pool, &trimmed)
        .await
        .expect("replace");
    assert_eq!(report.subjects_removed, 1);
    assert_eq!(
        report.lesson_logs_removed, 1,
        "the one tick against the vanished subject must be counted, got {report}"
    );
    assert_eq!(
        count(&pool, "lesson_log").await,
        1,
        "the surviving subject's history stays"
    );
}

// ---------------------------------------------------------------------------
// (c) validation
// ---------------------------------------------------------------------------

const VALID_HEADER: &str = "\
[curriculum]
slug = \"broken\"
name = \"Broken\"
weeks = 2
term_weeks = 1

[[subject]]
name = \"Stories\"
category = \"reading\"
days = \"MW\"
";

/// HS1 (c): every shape of invalid file H5 names is rejected, the message
/// carries the file name and a `line N`, and **nothing** is written.
#[tokio::test]
async fn every_invalid_curriculum_file_is_rejected_by_name_and_line_and_writes_nothing() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let before = homeschool_counts(&pool).await;

    let cases: [(&str, String); 6] = [
        (
            "a week past the end of the curriculum",
            format!("{VALID_HEADER}\n[[assignment]]\nsubject = \"Stories\"\nweek = 7\ntext = \"too far\"\n"),
        ),
        (
            "an unknown category",
            "[curriculum]\nslug = \"broken\"\nname = \"Broken\"\nweeks = 2\n\n\
             [[subject]]\nname = \"Stories\"\ncategory = \"narration\"\n"
                .to_string(),
        ),
        (
            "an unknown term_note kind",
            format!("{VALID_HEADER}\n[[term_note]]\nterm = 1\nkind = \"motto\"\ntext = \"nope\"\n"),
        ),
        (
            "a day letter outside MTWRFSU",
            "[curriculum]\nslug = \"broken\"\nname = \"Broken\"\nweeks = 2\n\n\
             [[subject]]\nname = \"Stories\"\ncategory = \"reading\"\ndays = \"Th\"\n"
                .to_string(),
        ),
        (
            "a duplicate (subject, week, ordinal)",
            format!(
                "{VALID_HEADER}\n[[assignment]]\nsubject = \"Stories\"\nweek = 1\nordinal = 1\n\
                 text = \"one\"\n\n[[assignment]]\nsubject = \"Stories\"\nweek = 1\nordinal = 1\n\
                 text = \"two\"\n"
            ),
        ),
        (
            "an assignment naming no subject",
            format!("{VALID_HEADER}\n[[assignment]]\nsubject = \"Nowhere\"\nweek = 1\ntext = \"x\"\n"),
        ),
    ];

    for (what, source) in cases {
        let err = loader::parse_curriculum("broken.toml", &source)
            .expect_err(&format!("{what} must be rejected"));
        let rendered = err.to_string();
        assert_eq!(err.file, "broken.toml", "{what}: {rendered}");
        assert!(
            rendered.contains("broken.toml"),
            "{what}: the message must name the file, got {rendered}"
        );
        assert!(
            rendered.contains(&format!("line {}", err.line)),
            "{what}: the message must carry `line N`, got {rendered}"
        );
        assert!(
            err.line >= 1,
            "{what}: the line must be 1-based, got {rendered}"
        );
    }

    assert_eq!(
        homeschool_counts(&pool).await,
        before,
        "a rejected file must never write a row"
    );
}

/// Validation is all-or-nothing: a file whose *last* assignment is bad leaves
/// the earlier, perfectly good ones out of the database too.
#[tokio::test]
async fn a_file_with_one_bad_row_at_the_end_is_rejected_whole() {
    let pool = memory_pool().await;
    let source = format!(
        "{VALID_HEADER}\n[[assignment]]\nsubject = \"Stories\"\nweek = 1\ntext = \"fine\"\n\n\
         [[assignment]]\nsubject = \"Stories\"\nweek = 99\ntext = \"not fine\"\n"
    );

    assert!(loader::parse_curriculum("half-good.toml", &source).is_err());
    assert_eq!(homeschool_counts(&pool).await, (0, 0, 0, 0));
}

// ---------------------------------------------------------------------------
// (d) lesson_log occurrence identity
// ---------------------------------------------------------------------------

/// HS1 (d): the expression unique index is the occurrence's identity. Ticking
/// twice leaves one row; unticking leaves none; a daily subject's `NULL`
/// assignment id dedupes exactly like a reading's real one; and the same
/// subject on the same date in two different weeks is two rows, not one.
#[tokio::test]
async fn the_occurrence_key_dedupes_ticks_including_the_null_assignment_case() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;
    let sums = subject_id(&pool, curriculum, "Sums").await;
    let old_tales = subject_id(&pool, curriculum, "Old Tales").await;
    let reading: (i64,) = sqlx::query_as(
        "SELECT id FROM assignments WHERE subject_id = ?1 AND week = 1 AND ordinal = 1",
    )
    .bind(old_tales)
    .fetch_one(&pool)
    .await
    .expect("Old Tales week 1");

    // A reading, ticked twice.
    for _ in 0..2 {
        hs::set_occurrence(
            &pool,
            &hs::OccurrenceKey::new(1, 1, old_tales, Some(reading.0), "2026-09-07"),
            "done",
            None,
            "2026-09-07",
        )
        .await
        .expect("tick");
    }
    assert_eq!(
        count(&pool, "lesson_log").await,
        1,
        "one occurrence, one row"
    );

    // A daily subject with no assignment row: the NULL case.
    for _ in 0..2 {
        hs::set_occurrence(
            &pool,
            &hs::OccurrenceKey::new(1, 1, sums, None, "2026-09-07"),
            "done",
            None,
            "2026-09-07",
        )
        .await
        .expect("tick the daily");
    }
    assert_eq!(
        count(&pool, "lesson_log").await,
        2,
        "the NULL assignment_id occurrence must dedupe on its own key"
    );

    // The same subject and date in week 3 is a different occurrence.
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 3, sums, None, "2026-09-07"),
        "done",
        None,
        "2026-09-07",
    )
    .await
    .expect("tick week 3");
    assert_eq!(
        count(&pool, "lesson_log").await,
        3,
        "week is part of the key, so weeks 1 and 3 are two rows"
    );

    // Unticking deletes, and matches the NULL key.
    let removed = hs::clear_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, sums, None, "2026-09-07"),
    )
    .await
    .expect("untick");
    assert_eq!(removed, 1);
    assert_eq!(count(&pool, "lesson_log").await, 2);

    let removed = hs::clear_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, old_tales, Some(reading.0), "2026-09-07"),
    )
    .await
    .expect("untick the reading");
    assert_eq!(removed, 1);
    assert_eq!(count(&pool, "lesson_log").await, 1);

    // Unticking something that was never ticked is a no-op, not an error.
    let removed = hs::clear_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, old_tales, Some(reading.0), "2026-09-07"),
    )
    .await
    .expect("untick twice");
    assert_eq!(removed, 0);
}

/// `logs()` and `log_counts_between()` are what the Today view and the Month
/// grid are built from; both must carry the `skipped` state, not flatten it.
#[tokio::test]
async fn logs_and_log_counts_report_done_and_skipped_separately() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;
    let sums = subject_id(&pool, curriculum, "Sums").await;
    let copywork = subject_id(&pool, curriculum, "Copywork").await;

    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, sums, None, "2026-09-07"),
        "done",
        None,
        "2026-09-07",
    )
    .await
    .expect("tick");
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, copywork, None, "2026-09-07"),
        "skipped",
        Some("we were out"),
        "2026-09-07",
    )
    .await
    .expect("skip");
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(1, 1, sums, None, "2026-09-08"),
        "done",
        None,
        "2026-09-08",
    )
    .await
    .expect("tick tuesday");

    let rows = hs::logs(&pool, 1, 1).await.expect("logs");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].scheduled_date, "2026-09-07");
    assert!(rows
        .iter()
        .any(|row| row.status == "skipped" && row.note.as_deref() == Some("we were out")));

    let counts = hs::log_counts_between(&pool, 1, "2026-09-07", "2026-09-07")
        .await
        .expect("counts");
    assert_eq!(counts.len(), 1, "inclusive on both ends");
    assert_eq!(counts[0].scheduled_date, "2026-09-07");
    assert_eq!(counts[0].done, 1);
    assert_eq!(counts[0].skipped, 1);

    let counts = hs::log_counts_between(&pool, 1, "2026-09-07", "2026-09-08")
        .await
        .expect("counts");
    assert_eq!(counts.len(), 2);
    assert_eq!(counts[1].done, 1);
    assert_eq!(counts[1].skipped, 0);
}

// ---------------------------------------------------------------------------
// (e) enrollment
// ---------------------------------------------------------------------------

/// HS1 (e): `profile_id` is `UNIQUE`, so re-enrolling a boy moves his existing
/// row instead of giving him two — and `started_on` still remembers the day
/// school actually began.
#[tokio::test]
async fn enrolling_the_same_boy_twice_replaces_his_row_and_keeps_started_on() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;

    hs::upsert_enrollment(&pool, 1, curriculum, 1, "MTWRF", "2026-09-07")
        .await
        .expect("first enrollment");
    hs::upsert_enrollment(&pool, 1, curriculum, 2, "MTWR", "2026-09-14")
        .await
        .expect("second enrollment");

    assert_eq!(count(&pool, "enrollments").await, 1, "never a duplicate");
    let enrollment = hs::enrollment(&pool, 1)
        .await
        .expect("enrollment")
        .expect("Isaiah is enrolled");
    assert_eq!(enrollment.current_week, 2);
    assert_eq!(enrollment.school_days, "MTWR");
    assert_eq!(enrollment.week_started_on, "2026-09-14");
    assert_eq!(
        enrollment.started_on, "2026-09-07",
        "re-enrolling must not forget the day school began"
    );
    assert_eq!(enrollment.weeks, 3, "joined from the curriculum");
    assert_eq!(enrollment.term_weeks, 1);
    assert!(!enrollment.paused);

    // Not enrolled is `None`, never an error (H6's empty state).
    assert!(hs::enrollment(&pool, 4).await.expect("query").is_none());
}

/// HS1 (e): deleting a profile takes its enrollment **and** its log with it —
/// that is the `ON DELETE CASCADE` the `foreign_keys` pragma makes real.
#[tokio::test]
async fn deleting_a_profile_cascades_its_enrollment_its_log_and_its_extras() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;
    let sums = subject_id(&pool, curriculum, "Sums").await;

    hs::upsert_enrollment(&pool, 3, curriculum, 1, "MTWRF", "2026-09-07")
        .await
        .expect("enroll");
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(3, 1, sums, None, "2026-09-07"),
        "done",
        None,
        "2026-09-07",
    )
    .await
    .expect("tick");
    hs::add_extra(&pool, 3, "2026-09-07", "Read to Mum", "reading", None)
        .await
        .expect("extra");

    assert_eq!(count(&pool, "enrollments").await, 1);
    assert_eq!(count(&pool, "lesson_log").await, 1);
    assert_eq!(count(&pool, "lesson_extras").await, 1);

    sqlx::query("DELETE FROM profiles WHERE id = 3")
        .execute(&pool)
        .await
        .expect("delete the profile");

    assert_eq!(count(&pool, "enrollments").await, 0);
    assert_eq!(count(&pool, "lesson_log").await, 0);
    assert_eq!(count(&pool, "lesson_extras").await, 0);
}

/// Unenrolling is the *other* half of §4 default 14: the enrollment goes, the
/// log stays. `set_week` and `set_paused` are checked here too — they are the
/// only two ways the week pointer and the summer switch ever move (H2).
#[tokio::test]
async fn unenrolling_keeps_the_log_and_the_week_pointer_only_moves_when_told() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;
    let sums = subject_id(&pool, curriculum, "Sums").await;

    hs::upsert_enrollment(&pool, 2, curriculum, 1, "MTWRF", "2026-09-07")
        .await
        .expect("enroll");
    hs::set_occurrence(
        &pool,
        &hs::OccurrenceKey::new(2, 1, sums, None, "2026-09-07"),
        "done",
        None,
        "2026-09-07",
    )
    .await
    .expect("tick");

    hs::set_week(&pool, 2, 2, "2026-09-14")
        .await
        .expect("finish week");
    let enrollment = hs::enrollment(&pool, 2)
        .await
        .expect("q")
        .expect("enrolled");
    assert_eq!(enrollment.current_week, 2);
    assert_eq!(
        enrollment.week_started_on, "2026-09-14",
        "H2: every move stamps a new anchor"
    );

    hs::set_paused(&pool, 2, true).await.expect("pause");
    assert!(
        hs::enrollment(&pool, 2)
            .await
            .expect("q")
            .expect("e")
            .paused
    );
    assert_eq!(
        count(&pool, "lesson_log").await,
        1,
        "pausing must not touch the log"
    );

    hs::unenroll(&pool, 2).await.expect("unenroll");
    assert!(hs::enrollment(&pool, 2).await.expect("q").is_none());
    assert_eq!(
        count(&pool, "lesson_log").await,
        1,
        "unenroll keeps the log (default 14)"
    );
}

/// The Together group is "every enrollment sharing `(curriculum_id,
/// current_week)`" (H4) — a boy on a different week is simply a different
/// group, never a member of this one.
#[tokio::test]
async fn together_group_is_every_enrollment_sharing_a_curriculum_and_week() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;

    for (profile, week) in [(1, 2), (2, 2), (3, 1)] {
        hs::upsert_enrollment(&pool, profile, curriculum, week, "MTWRF", "2026-09-07")
            .await
            .expect("enroll");
    }

    let week_two = hs::together_group(&pool, curriculum, 2)
        .await
        .expect("group");
    assert_eq!(
        week_two.iter().map(|e| e.profile_id).collect::<Vec<_>>(),
        vec![1, 2]
    );

    let week_one = hs::together_group(&pool, curriculum, 1)
        .await
        .expect("group");
    assert_eq!(
        week_one.iter().map(|e| e.profile_id).collect::<Vec<_>>(),
        vec![3]
    );

    assert_eq!(
        hs::all_enrollments(&pool).await.expect("all").len(),
        3,
        "all_enrollments spans every group"
    );
}

/// `week_plan` fetches exactly one week and one term: every subject (the
/// occurrence rule needs them all), **this** week's assignments, and **this**
/// term's notes.
#[tokio::test]
async fn week_plan_returns_every_subject_but_only_this_weeks_rows() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;

    let week_two = hs::week_plan(&pool, curriculum, 2)
        .await
        .expect("week_plan")
        .expect("the fixture is loaded");
    assert_eq!(week_two.subjects.len(), 7);
    assert_eq!(
        week_two.term, 2,
        "term_weeks = 1, so week 2 is term 2 (default 4)"
    );
    // Week 2 holds one Old Tales reading and both Twice Told readings, and —
    // this is the "-----" case — nothing at all for Fables.
    assert_eq!(week_two.assignments.len(), 3);
    let fables = subject_id(&pool, curriculum, "Fables").await;
    assert!(
        !week_two.assignments.iter().any(|a| a.subject_id == fables),
        "Fables has no reading in week 2"
    );
    assert!(
        week_two.term_notes.is_empty(),
        "every fixture note is term 1"
    );

    let week_one = hs::week_plan(&pool, curriculum, 1)
        .await
        .expect("week_plan")
        .expect("loaded");
    assert_eq!(week_one.term, 1);
    assert_eq!(week_one.term_notes.len(), 3, "one note of each kind");
    assert_eq!(week_one.assignments.len(), 3);

    assert!(hs::week_plan(&pool, 9_999, 1).await.expect("q").is_none());
    assert_eq!(hs::count_curricula(&pool).await.expect("count"), 1);
}

/// `set_subject_schedule` is one of the two server-side writers of a `days`
/// string (H7); the schema's `GLOB` is the belt to `parse_days`' braces.
#[tokio::test]
async fn set_subject_schedule_writes_days_and_shared_and_the_check_rejects_rubbish() {
    let pool = memory_pool().await;
    load_fixture(&pool).await;
    let curriculum = curriculum_id(&pool, "sample-year").await;
    let sums = subject_id(&pool, curriculum, "Sums").await;

    hs::set_subject_schedule(&pool, sums, "MWF", true)
        .await
        .expect("reschedule");
    let plan = hs::week_plan(&pool, curriculum, 1)
        .await
        .expect("q")
        .expect("loaded");
    let sums_row = plan
        .subjects
        .iter()
        .find(|s| s.id == sums)
        .expect("Sums is still there");
    assert_eq!(sums_row.days, "MWF");
    assert!(sums_row.shared);

    assert!(
        loader::parse_days("Th").is_err(),
        "the loader's own guard rejects a Thursday spelled 'Th'"
    );
    assert!(
        hs::set_subject_schedule(&pool, sums, "", true)
            .await
            .is_err(),
        "the schema's GLOB rejects an empty days string"
    );
}

// ---------------------------------------------------------------------------
// (f) the curricula directory and the boot-time scan
// ---------------------------------------------------------------------------

/// HS1 (f): the loader's directory is absolute, is created when missing, and
/// its resolved path is logged at `info` — the one line an owner grep for
/// "where does it look for my files?" lands on.
#[tokio::test]
async fn a_bad_file_beside_a_good_one_loads_exactly_one_curriculum_and_logs_the_path() {
    let pool = memory_pool().await;
    let data_dir = scratch_dir("curricula");
    let config = scratch_config(data_dir.clone());

    let dir = config.curricula_dir();
    assert!(
        dir.is_absolute(),
        "curricula_dir() must be absolute: {}",
        dir.display()
    );
    assert_eq!(dir, data_dir.join("curricula"));
    assert!(!dir.exists(), "the loader, not the test, creates it");

    // First scan: the directory does not exist yet, so the loader creates it
    // and finds nothing. That is a normal first boot, not an error.
    let empty = loader::load_directory(&pool, &dir).await;
    assert!(
        dir.is_dir(),
        "a missing curricula directory must be created"
    );
    assert_eq!(
        empty,
        loader::LoadReport {
            loaded: 0,
            skipped: 0
        }
    );

    // Second scan: the good file, and a sibling that is not valid TOML at all.
    std::fs::copy(fixture_path(), dir.join("sample-year.toml")).expect("copy the fixture");
    std::fs::write(dir.join("bad.toml"), "[curriculum\nslug = \"nope\"\n").expect("write bad.toml");

    let recorder = RecordingSubscriber::default();
    let guard = tracing::subscriber::set_default(recorder.clone());
    let report = loader::load_directory(&pool, &dir).await;
    drop(guard);

    assert_eq!(
        report,
        loader::LoadReport {
            loaded: 1,
            skipped: 1
        },
        "a bad file is skipped and its good sibling still loads"
    );
    assert_eq!(hs::count_curricula(&pool).await.expect("count"), 1);
    assert_eq!(homeschool_counts(&pool).await, (1, 7, 9, 3));

    let lines = recorder.lines();
    let needle = dir.display().to_string();
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("INFO") && line.contains(&needle)),
        "the resolved curricula path must appear in an INFO line; got {lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("WARN") && line.contains("bad.toml")),
        "the skipped file must be logged at WARN; got {lines:#?}"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// `load_and_seed` is the single call the Boss wires into `db::pools()`. It
/// must return `Ok` with a broken file in the directory — a curriculum a parent
/// mistyped can never stop the hub booting for the rest of the family.
#[tokio::test]
async fn load_and_seed_is_ok_even_when_every_file_in_the_directory_is_bad() {
    let pool = memory_pool().await;
    let data_dir = scratch_dir("load-and-seed");
    let config = scratch_config(data_dir.clone());
    let dir = config.curricula_dir();
    std::fs::create_dir_all(&dir).expect("curricula dir");
    std::fs::write(dir.join("bad.toml"), "not even toml =\n").expect("write bad.toml");

    loader::load_and_seed(&pool, &config)
        .await
        .expect("a bad file must never fail the boot path");
    assert_eq!(hs::count_curricula(&pool).await.expect("count"), 0);
    assert_eq!(count(&pool, "enrollments").await, 0);

    let _ = std::fs::remove_dir_all(&data_dir);
}

// ---------------------------------------------------------------------------
// The Isaiah enrollment seed (post-A micro-commit, D-6)
// ---------------------------------------------------------------------------

/// The seed enrolls Isaiah at week 1 exactly once. A second boot — the case
/// that matters, because the hub reboots and a parent has since finished a week
/// — must change nothing at all.
#[tokio::test]
async fn the_enrollment_seed_enrolls_isaiah_once_and_a_second_boot_changes_nothing() {
    let pool = memory_pool().await;
    let mut ao = read_fixture();
    ao.slug = loader::SEED_CURRICULUM_SLUG.to_string();
    loader::insert_missing(&pool, &ao).await.expect("insert");

    loader::seed_enrollments_on(&pool, "2026-09-02")
        .await
        .expect("first boot");
    let first = hs::enrollment(&pool, 1)
        .await
        .expect("q")
        .expect("Isaiah is profile 1 after 0004_name_the_boys");
    assert_eq!(first.current_week, 1);
    assert_eq!(first.school_days, "MTWRF");
    assert_eq!(first.week_started_on, "2026-09-02");
    assert_eq!(
        first.started_on, "2026-09-02",
        "week_started_on = started_on = the run date, no Monday arithmetic"
    );
    assert_eq!(first.curriculum_slug, loader::SEED_CURRICULUM_SLUG);

    // The parent finishes week 1...
    hs::set_week(&pool, 1, 2, "2026-09-09")
        .await
        .expect("finish");

    // ...and the hub reboots the next morning.
    loader::seed_enrollments_on(&pool, "2026-09-10")
        .await
        .expect("second boot");
    let second = hs::enrollment(&pool, 1)
        .await
        .expect("q")
        .expect("enrolled");
    assert_eq!(count(&pool, "enrollments").await, 1);
    assert_eq!(
        second.current_week, 2,
        "ON CONFLICT DO NOTHING — a reboot must never reset the week"
    );
    assert_eq!(second.week_started_on, "2026-09-09");
}

/// The seed finds the profile **by name**, so a family that renamed the boy is
/// simply skipped — with a warning, never a panic and never the wrong boy.
#[tokio::test]
async fn the_enrollment_seed_skips_a_renamed_profile_and_a_missing_curriculum() {
    let pool = memory_pool().await;
    let mut ao = read_fixture();
    ao.slug = loader::SEED_CURRICULUM_SLUG.to_string();

    // No `ao-year-1` loaded yet: skipped, and not an error.
    loader::seed_enrollments_on(&pool, "2026-09-02")
        .await
        .expect("no curriculum is not an error");
    assert_eq!(count(&pool, "enrollments").await, 0);

    loader::insert_missing(&pool, &ao).await.expect("insert");

    sqlx::query("UPDATE profiles SET name = 'Izzy' WHERE name = ?1")
        .bind(loader::SEED_PROFILE_NAME)
        .execute(&pool)
        .await
        .expect("rename");
    loader::seed_enrollments_on(&pool, "2026-09-02")
        .await
        .expect("a renamed profile is skipped, not an error");
    assert_eq!(
        count(&pool, "enrollments").await,
        0,
        "no name match, so nothing is enrolled"
    );

    // Two profiles with the name is also "skip", not "guess".
    sqlx::query("UPDATE profiles SET name = ?1 WHERE id IN (1, 2)")
        .bind(loader::SEED_PROFILE_NAME)
        .execute(&pool)
        .await
        .expect("rename two");
    loader::seed_enrollments_on(&pool, "2026-09-02")
        .await
        .expect("ambiguous is skipped, not an error");
    assert_eq!(count(&pool, "enrollments").await, 0);
}

// ---------------------------------------------------------------------------
// (i) extras
// ---------------------------------------------------------------------------

/// HS1 (i), first half: the schema's own CHECKs bound what a parent can add.
#[tokio::test]
async fn an_extra_with_too_long_a_title_or_an_unknown_category_is_rejected() {
    let pool = memory_pool().await;

    let too_long = "x".repeat(81);
    assert!(
        hs::add_extra(&pool, 1, "2026-09-07", &too_long, "daily", None)
            .await
            .is_err(),
        "an 81-character title must violate the CHECK"
    );
    assert!(
        hs::add_extra(&pool, 1, "2026-09-07", "", "daily", None)
            .await
            .is_err(),
        "an empty title must violate the CHECK too"
    );
    assert!(
        hs::add_extra(&pool, 1, "2026-09-07", "Copywork", "poetry", None)
            .await
            .is_err(),
        "'poetry' is a term_note kind, not an extra category"
    );
    assert_eq!(count(&pool, "lesson_extras").await, 0);

    let ok = hs::add_extra(&pool, 1, "2026-09-07", &"x".repeat(80), "weekly", None)
        .await
        .expect("exactly 80 characters is allowed");
    assert_eq!(ok.title.len(), 80);
}

/// HS1 (i): `sort_order` is `MAX + 1` **within `(profile_id,
/// scheduled_date)`** — a second boy, or a second day, starts at 1 again.
#[tokio::test]
async fn add_extra_numbers_sort_order_within_the_profile_and_the_date() {
    let pool = memory_pool().await;

    let first = hs::add_extra(&pool, 1, "2026-09-07", "Copywork", "daily", None)
        .await
        .expect("first");
    let second = hs::add_extra(
        &pool,
        1,
        "2026-09-07",
        "Read aloud",
        "reading",
        Some("ch. 4"),
    )
    .await
    .expect("second");
    let other_day = hs::add_extra(&pool, 1, "2026-09-08", "Nature walk", "weekly", None)
        .await
        .expect("other day");
    let other_boy = hs::add_extra(&pool, 2, "2026-09-07", "Copywork", "daily", None)
        .await
        .expect("other boy");

    assert_eq!(first.sort_order, 1);
    assert_eq!(second.sort_order, 2);
    assert_eq!(other_day.sort_order, 1, "a new date starts again at 1");
    assert_eq!(other_boy.sort_order, 1, "so does a new boy");
    assert_eq!(second.text.as_deref(), Some("ch. 4"));
    assert!(second.status.is_none(), "a new extra is still to do");
    assert!(second.completed_on.is_none());
}

/// HS1 (i): `extras_between` is inclusive on both ends and ordered by
/// `(scheduled_date, sort_order, id)` — the order the day sheet renders in.
#[tokio::test]
async fn extras_between_is_inclusive_on_both_ends_and_ordered_by_date_then_sort_order() {
    let pool = memory_pool().await;

    for (date, title) in [
        ("2026-09-06", "Before"),
        ("2026-09-07", "Monday one"),
        ("2026-09-07", "Monday two"),
        ("2026-09-09", "Wednesday"),
        ("2026-09-10", "After"),
    ] {
        hs::add_extra(&pool, 1, date, title, "daily", None)
            .await
            .expect("add");
    }

    let window = hs::extras_between(&pool, 1, "2026-09-07", "2026-09-09")
        .await
        .expect("extras_between");
    assert_eq!(
        window.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
        vec!["Monday one", "Monday two", "Wednesday"],
        "both ends inclusive, ordered by (date, sort_order, id)"
    );

    // A single-day window is the day itself.
    let one_day = hs::extras_between(&pool, 1, "2026-09-07", "2026-09-07")
        .await
        .expect("extras_between");
    assert_eq!(one_day.len(), 2);

    // Another boy's extras are never in this boy's window.
    assert!(hs::extras_between(&pool, 2, "2026-09-01", "2026-12-31")
        .await
        .expect("extras_between")
        .is_empty());
}

/// HS1 (i): ticking stamps `completed_on`/`completed_at`; unticking
/// (`status = None`) clears **both**, so an untick leaves no trace. `updated_at`
/// moves on every edit and every status change.
#[tokio::test]
async fn setting_and_clearing_an_extras_status_manages_its_completion_stamps() {
    let pool = memory_pool().await;
    let extra = hs::add_extra(&pool, 1, "2026-09-07", "Copywork", "daily", None)
        .await
        .expect("add");

    hs::set_extra_status(&pool, extra.id, Some("done"), None, "2026-09-07")
        .await
        .expect("tick");
    let ticked = hs::extra(&pool, extra.id)
        .await
        .expect("q")
        .expect("still there");
    assert_eq!(ticked.status.as_deref(), Some("done"));
    assert_eq!(ticked.completed_on.as_deref(), Some("2026-09-07"));
    let (completed_at,): (Option<String>,) =
        sqlx::query_as("SELECT completed_at FROM lesson_extras WHERE id = ?1")
            .bind(extra.id)
            .fetch_one(&pool)
            .await
            .expect("completed_at");
    assert!(completed_at.is_some());

    hs::set_extra_status(
        &pool,
        extra.id,
        Some("skipped"),
        Some("out all day"),
        "2026-09-08",
    )
    .await
    .expect("skip");
    let skipped = hs::extra(&pool, extra.id).await.expect("q").expect("there");
    assert_eq!(skipped.status.as_deref(), Some("skipped"));
    assert_eq!(skipped.note.as_deref(), Some("out all day"));
    assert_eq!(skipped.completed_on.as_deref(), Some("2026-09-08"));

    hs::set_extra_status(&pool, extra.id, None, None, "2026-09-08")
        .await
        .expect("untick");
    let cleared = hs::extra(&pool, extra.id).await.expect("q").expect("there");
    assert!(cleared.status.is_none());
    assert!(
        cleared.completed_on.is_none(),
        "clearing the status must clear completed_on"
    );
    let (completed_at,): (Option<String>,) =
        sqlx::query_as("SELECT completed_at FROM lesson_extras WHERE id = ?1")
            .bind(extra.id)
            .fetch_one(&pool)
            .await
            .expect("completed_at");
    assert!(
        completed_at.is_none(),
        "clearing the status must clear completed_at too"
    );
}

/// Backdate an extra's `updated_at` by hand, so a bump is visible without a
/// one-second sleep (`CURRENT_TIMESTAMP` has second resolution).
async fn backdate(pool: &SqlitePool, extra_id: i64) {
    sqlx::query("UPDATE lesson_extras SET updated_at = '2000-01-01 00:00:00' WHERE id = ?1")
        .bind(extra_id)
        .execute(pool)
        .await
        .expect("backdate");
}

/// HS1 (i): both writers bump `updated_at`.
///
/// `CURRENT_TIMESTAMP` has one-second resolution, so the row's stamp is first
/// backdated by hand — proving the bump happened without a `sleep(1s)` in the
/// suite.
#[tokio::test]
async fn update_extra_and_set_extra_status_both_bump_updated_at() {
    let pool = memory_pool().await;
    let extra = hs::add_extra(&pool, 1, "2026-09-07", "Copywork", "daily", None)
        .await
        .expect("add");

    backdate(&pool, extra.id).await;
    hs::update_extra(
        &pool,
        extra.id,
        "Copywork (a page)",
        "daily",
        Some("neat hand"),
        "2026-09-08",
    )
    .await
    .expect("update");
    let updated = hs::extra(&pool, extra.id).await.expect("q").expect("there");
    assert_eq!(updated.title, "Copywork (a page)");
    assert_eq!(updated.scheduled_date, "2026-09-08");
    assert_eq!(updated.text.as_deref(), Some("neat hand"));
    assert!(
        updated.status.is_none(),
        "editing an extra must not tick it"
    );
    assert_ne!(
        hs::extra_updated_at(&pool, extra.id)
            .await
            .expect("q")
            .expect("there"),
        "2000-01-01 00:00:00",
        "update_extra must bump updated_at"
    );

    backdate(&pool, extra.id).await;
    hs::set_extra_status(&pool, extra.id, Some("done"), None, "2026-09-08")
        .await
        .expect("tick");
    assert_ne!(
        hs::extra_updated_at(&pool, extra.id)
            .await
            .expect("q")
            .expect("there"),
        "2000-01-01 00:00:00",
        "set_extra_status must bump updated_at"
    );

    assert_eq!(hs::delete_extra(&pool, extra.id).await.expect("delete"), 1);
    assert!(hs::extra(&pool, extra.id).await.expect("q").is_none());
    assert_eq!(
        hs::delete_extra(&pool, extra.id).await.expect("delete"),
        0,
        "deleting twice is a no-op, not an error"
    );
}

// ---------------------------------------------------------------------------
// (h) N1 — curriculum content is never committed
// ---------------------------------------------------------------------------

/// §0 N1, the committed guard: the family's curriculum files are licensed for
/// family use and the repository is public, so `docs/homeschool/curriculum/`
/// must hold **no tracked file at all**, and no tracked file under `src/`,
/// `tests/` or `assets/` may name the publisher.
///
/// The needle is assembled at runtime from two halves on purpose: spelling it
/// as one literal would put it in a tracked file under `tests/` and make this
/// very test fail.
#[test]
fn no_curriculum_content_is_tracked_in_the_repository() {
    let needle = format!("{}{}", "Amble", "side");

    let tracked_curriculum = git(&["ls-files", "--", "docs/homeschool/curriculum/"]);
    assert!(
        tracked_curriculum.trim().is_empty(),
        "N1: docs/homeschool/curriculum/ must hold no tracked file, found:\n{tracked_curriculum}"
    );

    let listing = git(&["ls-files", "--", "src", "tests", "assets"]);
    let mut offenders = Vec::new();
    for relative in listing.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = repo_root().join(relative);
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if String::from_utf8_lossy(&bytes).contains(&needle) {
            offenders.push(relative.to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "N1: no tracked file under src/, tests/ or assets/ may carry the publisher's \
         name; found it in {offenders:?}"
    );

    // And the gitignore rule that makes the first assertion stick is itself
    // committed — a future `git add -f` is then a deliberate act, not an
    // accident.
    let gitignore =
        std::fs::read_to_string(repo_root().join(".gitignore")).expect(".gitignore is readable");
    assert!(
        gitignore.contains("docs/homeschool/curriculum/"),
        "N1: .gitignore must exclude the curriculum directory"
    );
}

fn git(args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("running `git {}`: {err}", args.join(" ")));
    assert!(
        output.status.success(),
        "`git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The committed fixture is the only curriculum this repository carries, and it
/// must always validate — six later tasks build their expectations on it
/// (P-15).
#[test]
fn the_committed_fixture_is_the_shape_every_later_task_expects() {
    let fixture = read_fixture();
    assert_eq!(fixture.slug, "sample-year");
    assert_eq!(fixture.name, "Sample Year");
    assert_eq!(fixture.weeks, 3);
    assert_eq!(fixture.term_weeks, 1);
    assert_eq!(
        fixture
            .subjects
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Sums",
            "Copywork",
            "Old Tales",
            "Fables",
            "Twice Told",
            "Painting",
            "Reading Basket"
        ]
    );

    let by_name = |name: &str| {
        fixture
            .subjects
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("the fixture must carry {name}"))
            .clone()
    };
    assert_eq!(by_name("Sums").days, "MTWRF");
    assert!(!by_name("Sums").shared, "daily work is per boy");
    assert_eq!(by_name("Old Tales").days, "MW");
    assert!(by_name("Old Tales").shared, "readings are read aloud");
    assert_eq!(by_name("Fables").days, "TF");
    assert_eq!(by_name("Twice Told").days, "T");
    assert_eq!(by_name("Painting").days, "F");
    assert_eq!(by_name("Reading Basket").category, "free_read");

    let rows = |name: &str, week: i64| {
        fixture
            .assignments
            .iter()
            .filter(|a| a.subject == name && a.week == week)
            .count()
    };
    assert_eq!(rows("Old Tales", 1), 1);
    assert_eq!(rows("Old Tales", 2), 1);
    assert_eq!(rows("Old Tales", 3), 1);
    assert_eq!(rows("Fables", 1), 2);
    assert_eq!(rows("Fables", 2), 0, "the '-----' case");
    assert_eq!(rows("Fables", 3), 2);
    assert_eq!(rows("Twice Told", 2), 2, "both readings on one day");
    assert_eq!(fixture.term_notes.len(), 3);
}
