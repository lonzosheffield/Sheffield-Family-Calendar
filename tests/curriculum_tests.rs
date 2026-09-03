#![cfg(feature = "server")]
//! HS2b acceptance suite — the transcribed `ao-year-1.toml` and its
//! `ao-year-1.expect.toml` (`docs/homeschool/PLAN_HOMESCHOOL.md` §3, row
//! HS2a/HS2b).
//!
//! | # | Assertion |
//! | - | --- |
//! | a | `.expect` counts hold: weeks, subject count, the "every week has" list |
//! | b | the chapter-sequence subject's chapters `first..=last` each appear exactly once, in non-decreasing week order |
//! | c | the two-ordinals rule holds for every week except the `.expect` file's listed exceptions |
//! | d | all six `[[spot]]` rows match their subject/week by `contains` |
//! | e | term-note counts hold (`geography`, `poetry`, `free_read` at least `free_read_min`) |
//! | f | every subject's `days` is a subset of `MTWRF` and `shared` matches the category default the notes table encodes |
//! | g | the HS1 loader accepts the file, both as a pure parse and as a real insert into a migrated database |
//! | h | §0 N1 guard: no text from the TOML's `[[assignment]]` rows appears in any git-tracked file |
//!
//! **§0 N1.** This file names no week->reading mapping. `docs/homeschool/curriculum/`
//! is gitignored; the TOML and its `.expect` file live there (or wherever
//! `FAMILY_HUB_AO_TOML` / `FAMILY_HUB_AO_EXPECT` point), and every test here
//! **skips, with a printed reason,** when they are absent — which is the normal
//! state of a fresh checkout of this public repository. Nothing read from those
//! files is ever written back into a tracked file, a panic message that could
//! land in CI logs some other test captures, or this file's own source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use family_calendar::server::db;
use family_calendar::server::homeschool::loader::{self, ValidatedCurriculum};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Locating the gitignored files
// ---------------------------------------------------------------------------

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn find_file(default_relative: &str, env_var: &str) -> Option<PathBuf> {
    let committed = repo_root().join(default_relative);
    if committed.is_file() {
        return Some(committed);
    }
    match std::env::var(env_var) {
        Ok(path) if !path.is_empty() && Path::new(&path).is_file() => Some(PathBuf::from(path)),
        _ => None,
    }
}

fn toml_path() -> Option<PathBuf> {
    find_file(
        "docs/homeschool/curriculum/ao-year-1.toml",
        "FAMILY_HUB_AO_TOML",
    )
}

fn expect_path() -> Option<PathBuf> {
    find_file(
        "docs/homeschool/curriculum/ao-year-1.expect.toml",
        "FAMILY_HUB_AO_EXPECT",
    )
}

// ---------------------------------------------------------------------------
// The `.expect.toml` shape (test-only; HS2b's own contract, per
// `ao-year-1.notes.md`'s "Expectations" section)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ExpectFile {
    weeks: i64,
    subjects: usize,
    every_week_has: Vec<String>,
    sha256: String,
    chapter_sequence: ChapterSequence,
    two_ordinals: TwoOrdinals,
    term_notes: TermNoteExpectations,
    spot: Vec<Spot>,
}

#[derive(Debug, Deserialize)]
struct ChapterSequence {
    subject: String,
    first: u32,
    last: u32,
}

#[derive(Debug, Deserialize)]
struct TwoOrdinals {
    subject: String,
    except_weeks: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct TermNoteExpectations {
    geography: usize,
    poetry: usize,
    free_read_min: usize,
}

#[derive(Debug, Deserialize)]
struct Spot {
    week: i64,
    subject: String,
    contains: String,
}

/// Loads both gitignored files, or prints why it could not and returns `None`
/// — every test calls this first and returns immediately on `None`, which is
/// this suite's "skip with a printed reason" (there is no dynamic `#[ignore]`
/// in stable Rust, so an early return that leaves the test green is the
/// standard shape for an optional-fixture suite).
fn load(test_name: &str) -> Option<(ValidatedCurriculum, ExpectFile)> {
    let Some(toml) = toml_path() else {
        println!(
            "SKIP {test_name}: docs/homeschool/curriculum/ao-year-1.toml is absent and \
             FAMILY_HUB_AO_TOML is not set to a valid path — this is normal for a fresh \
             checkout of this public repository; HS2b's files live outside version control."
        );
        return None;
    };
    let Some(expect) = expect_path() else {
        println!(
            "SKIP {test_name}: docs/homeschool/curriculum/ao-year-1.expect.toml is absent and \
             FAMILY_HUB_AO_EXPECT is not set to a valid path."
        );
        return None;
    };

    let curriculum = loader::read_curriculum(&toml)
        .unwrap_or_else(|err| panic!("{test_name}: ao-year-1.toml must validate: {err}"));

    let expect_source = std::fs::read_to_string(&expect)
        .unwrap_or_else(|err| panic!("{test_name}: reading {}: {err}", expect.display()));
    let expect: ExpectFile = toml::from_str(&expect_source)
        .unwrap_or_else(|err| panic!("{test_name}: parsing ao-year-1.expect.toml: {err}"));

    Some((curriculum, expect))
}

// ---------------------------------------------------------------------------
// (a) the .expect counts hold
// ---------------------------------------------------------------------------

#[test]
fn the_expect_file_s_weeks_subject_count_and_every_week_list_hold() {
    let Some((curriculum, expect)) =
        load("the_expect_file_s_weeks_subject_count_and_every_week_list_hold")
    else {
        return;
    };

    assert_eq!(curriculum.weeks, expect.weeks, "curriculum.weeks");
    assert_eq!(curriculum.subjects.len(), expect.subjects, "subject count");

    let subject_names: std::collections::BTreeSet<&str> = curriculum
        .subjects
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    for name in &expect.every_week_has {
        assert!(
            subject_names.contains(name.as_str()),
            "every_week_has names {name:?}, which is not a subject in the file"
        );
    }

    let by_subject_week: BTreeMap<(&str, i64), Vec<i64>> = {
        let mut map: BTreeMap<(&str, i64), Vec<i64>> = BTreeMap::new();
        for a in &curriculum.assignments {
            map.entry((a.subject.as_str(), a.week))
                .or_default()
                .push(a.ordinal);
        }
        map
    };
    for name in &expect.every_week_has {
        for week in 1..=curriculum.weeks {
            assert!(
                by_subject_week.contains_key(&(name.as_str(), week)),
                "{name:?} has no row in week {week}, but every_week_has names it"
            );
        }
    }

    if let Some(bytes) = toml_path().and_then(|p| std::fs::read(p).ok()) {
        let actual = sha256_hex(&bytes);
        if actual != expect.sha256 {
            println!(
                "WARN the_expect_file_s_weeks_subject_count_and_every_week_list_hold: \
                 ao-year-1.toml has drifted since the .expect file's sha256 was written \
                 (expected {}, got {actual}) — not a failure, just a heads-up.",
                expect.sha256
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (b) the chapter-sequence subject
// ---------------------------------------------------------------------------

#[test]
fn the_chapter_sequence_subject_s_chapters_appear_once_each_in_non_decreasing_week_order() {
    let Some((curriculum, expect)) = load(
        "the_chapter_sequence_subject_s_chapters_appear_once_each_in_non_decreasing_week_order",
    ) else {
        return;
    };

    let mut rows: Vec<(i64, &str)> = curriculum
        .assignments
        .iter()
        .filter(|a| a.subject == expect.chapter_sequence.subject)
        .map(|a| (a.week, a.text.as_str()))
        .collect();
    rows.sort_by_key(|(week, _)| *week);

    let mut chapters: Vec<u32> = Vec::with_capacity(rows.len());
    for (week, text) in &rows {
        let n = extract_leading_chapter_number(text).unwrap_or_else(|| {
            panic!(
                "{}: week {week} text {text:?} does not start with a chapter number",
                expect.chapter_sequence.subject
            )
        });
        chapters.push(n);
    }

    assert!(
        chapters.windows(2).all(|w| w[0] <= w[1]),
        "{}'s chapters are not in non-decreasing week order: {chapters:?}",
        expect.chapter_sequence.subject
    );

    let expected: Vec<u32> =
        (expect.chapter_sequence.first..=expect.chapter_sequence.last).collect();
    let mut sorted = chapters.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        expected,
        "{}'s chapters must be exactly {}..={}, each once",
        expect.chapter_sequence.subject,
        expect.chapter_sequence.first,
        expect.chapter_sequence.last
    );
}

/// `"chapter 14"` / `"chapter I \"Title\""` -> the leading Arabic or Roman
/// chapter number. Roman numerals appear only up to a handful in this
/// subject's rows in practice, but the parser is general for I..XX.
fn extract_leading_chapter_number(text: &str) -> Option<u32> {
    let rest = text.strip_prefix("chapter ")?;
    let token: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    if let Ok(n) = token.parse::<u32>() {
        return Some(n);
    }
    roman_to_u32(&token)
}

fn roman_to_u32(s: &str) -> Option<u32> {
    let value = |c: char| -> Option<u32> {
        match c.to_ascii_uppercase() {
            'I' => Some(1),
            'V' => Some(5),
            'X' => Some(10),
            'L' => Some(50),
            'C' => Some(100),
            _ => None,
        }
    };
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut total = 0i64;
    for i in 0..chars.len() {
        let v = value(chars[i])? as i64;
        if i + 1 < chars.len() && v < value(chars[i + 1])? as i64 {
            total -= v;
        } else {
            total += v;
        }
    }
    u32::try_from(total).ok()
}

// ---------------------------------------------------------------------------
// (c) two ordinals except the listed weeks
// ---------------------------------------------------------------------------

#[test]
fn the_two_ordinals_subject_has_two_rows_every_week_except_the_expect_file_s_exceptions() {
    let Some((curriculum, expect)) = load(
        "the_two_ordinals_subject_has_two_rows_every_week_except_the_expect_file_s_exceptions",
    ) else {
        return;
    };

    let mut ordinals_by_week: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for a in curriculum
        .assignments
        .iter()
        .filter(|a| a.subject == expect.two_ordinals.subject)
    {
        ordinals_by_week.entry(a.week).or_default().push(a.ordinal);
    }
    for ords in ordinals_by_week.values_mut() {
        ords.sort_unstable();
    }

    for week in 1..=curriculum.weeks {
        let ords = ordinals_by_week.get(&week).cloned().unwrap_or_default();
        if expect.two_ordinals.except_weeks.contains(&week) {
            assert_ne!(
                ords,
                vec![1, 2],
                "{}: week {week} is listed as an exception but has two ordinals",
                expect.two_ordinals.subject
            );
        } else {
            assert_eq!(
                ords,
                vec![1, 2],
                "{}: week {week} must have exactly ordinals 1 and 2",
                expect.two_ordinals.subject
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (d) the six spot checks
// ---------------------------------------------------------------------------

#[test]
fn every_spot_check_row_matches_its_week_and_subject_by_contains() {
    let Some((curriculum, expect)) =
        load("every_spot_check_row_matches_its_week_and_subject_by_contains")
    else {
        return;
    };

    assert_eq!(
        expect.spot.len(),
        6,
        ".expect.toml must carry exactly six [[spot]] rows"
    );

    for spot in &expect.spot {
        let found = curriculum.assignments.iter().any(|a| {
            a.week == spot.week && a.subject == spot.subject && a.text.contains(&spot.contains)
        });
        assert!(
            found,
            "no assignment in week {} for subject {:?} contains the spot-check text",
            spot.week, spot.subject
        );
    }
}

// ---------------------------------------------------------------------------
// (e) term-note counts
// ---------------------------------------------------------------------------

#[test]
fn term_note_counts_hold() {
    let Some((curriculum, expect)) = load("term_note_counts_hold") else {
        return;
    };

    let geography = curriculum
        .term_notes
        .iter()
        .filter(|n| n.kind == "geography")
        .count();
    let poetry = curriculum
        .term_notes
        .iter()
        .filter(|n| n.kind == "poetry")
        .count();
    let free_read = curriculum
        .term_notes
        .iter()
        .filter(|n| n.kind == "free_read")
        .count();

    assert_eq!(
        geography, expect.term_notes.geography,
        "geography term_note count"
    );
    assert_eq!(poetry, expect.term_notes.poetry, "poetry term_note count");
    assert!(
        free_read >= expect.term_notes.free_read_min,
        "free_read term_note count {free_read} is below the expected minimum {}",
        expect.term_notes.free_read_min
    );
}

// ---------------------------------------------------------------------------
// (f) days subset of MTWRF, shared matches the category default
// ---------------------------------------------------------------------------

#[test]
fn every_subject_s_days_are_within_the_five_day_school_week_and_shared_matches_its_category() {
    let Some((curriculum, _expect)) = load(
        "every_subject_s_days_are_within_the_five_day_school_week_and_shared_matches_its_category",
    ) else {
        return;
    };

    for subject in &curriculum.subjects {
        assert!(
            subject.days.chars().all(|c| "MTWRF".contains(c)),
            "subject {:?} has a day outside MTWRF: {:?}",
            subject.name,
            subject.days
        );
        // PLAN_HOMESCHOOL.md H4 (committed, not AO content) states the
        // family's `shared` rule in full: "reading/weekly -> shared, daily ->
        // not shared, poetry shared (the TOML says so)" — i.e. the loader's
        // category default (H5 default 5), with exactly one documented,
        // public exception: the daily "Poetry" subject is shared because it
        // is read aloud together. "Poetry" is a structural subject label the
        // plan itself names, not licensed AO content.
        let expected_shared = if subject.name == "Poetry" {
            true
        } else {
            subject.category != "daily" && subject.category != "free_read"
        };
        assert_eq!(
            subject.shared, expected_shared,
            "subject {:?} (category {:?}) has shared={}, expected {}",
            subject.name, subject.category, subject.shared, expected_shared
        );
    }
}

// ---------------------------------------------------------------------------
// (g) the HS1 loader accepts the file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_hs1_loader_accepts_the_file_as_both_a_parse_and_a_real_database_insert() {
    let Some(toml) = toml_path() else {
        println!(
            "SKIP the_hs1_loader_accepts_the_file_as_both_a_parse_and_a_real_database_insert: \
             ao-year-1.toml is absent."
        );
        return;
    };

    // Parse (already exercised by `load`, but this test stands alone so the
    // acceptance criterion "the loader accepts the file" is independently
    // verifiable without the .expect file).
    let curriculum = loader::read_curriculum(&toml).expect("ao-year-1.toml must validate");

    // And a real insert into a freshly migrated database.
    let pool = db::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    db::migrate(&pool).await.expect("migrations");
    let report = loader::insert_missing(&pool, &curriculum)
        .await
        .expect("insert ao-year-1 into a fresh database");
    assert_eq!(report.subjects_inserted, curriculum.subjects.len());
    assert_eq!(report.assignments_inserted, curriculum.assignments.len());
    assert_eq!(report.term_notes_inserted, curriculum.term_notes.len());

    // Insert-missing-only: loading the same file again inserts nothing more.
    let second = loader::insert_missing(&pool, &curriculum)
        .await
        .expect("second insert");
    assert_eq!(second.subjects_inserted, 0);
    assert_eq!(second.assignments_inserted, 0);
    assert_eq!(second.term_notes_inserted, 0);
}

// ---------------------------------------------------------------------------
// (h) N1 guard — no assignment text leaks into a tracked file
// ---------------------------------------------------------------------------

/// The HS2 contract's own N1 guard: every text from the TOML's
/// `[[assignment]]` rows, filtered to strings long enough that a coincidental
/// match elsewhere in the repository would be implausible, must not appear
/// verbatim in any git-tracked file. This is stricter than
/// `homeschool_db_tests.rs`'s single-needle N1 guard — it checks the actual
/// transcribed content, not just the publisher's name — and it never embeds
/// that content in this file's own source: every needle is read from the
/// gitignored TOML at test time.
#[test]
fn no_assignment_text_from_the_toml_appears_in_any_tracked_file() {
    let Some(toml) = toml_path() else {
        println!(
            "SKIP no_assignment_text_from_the_toml_appears_in_any_tracked_file: \
             ao-year-1.toml is absent."
        );
        return;
    };
    let curriculum = loader::read_curriculum(&toml).expect("ao-year-1.toml must validate");

    const MIN_NEEDLE_LEN: usize = 15;
    let mut needles: Vec<&str> = curriculum
        .assignments
        .iter()
        .map(|a| a.text.as_str())
        .filter(|t| t.len() >= MIN_NEEDLE_LEN)
        .collect();
    needles.sort_unstable();
    needles.dedup();
    assert!(
        !needles.is_empty(),
        "expected at least one assignment text long enough to check"
    );

    let listing = git(&["ls-files"]);
    let mut file_contents: Vec<(String, String)> = Vec::new();
    for relative in listing.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = repo_root().join(relative);
        if let Ok(bytes) = std::fs::read(&path) {
            file_contents.push((
                relative.to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
    }

    let mut offenders: Vec<(String, usize)> = Vec::new();
    for (relative, contents) in &file_contents {
        for (i, needle) in needles.iter().enumerate() {
            if contents.contains(needle) {
                offenders.push((relative.clone(), i));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "N1: {} tracked-file/needle pairs matched transcribed assignment text; \
         first offending file: {:?}",
        offenders.len(),
        offenders.first().map(|(f, _)| f)
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

// ---------------------------------------------------------------------------
// A small, self-contained SHA-256 (FIPS 180-4) — used only for the drift
// warning in test (a); the project takes no `sha2` dependency for one
// non-failing heads-up.
// ---------------------------------------------------------------------------

#[allow(clippy::many_single_char_names, clippy::needless_range_loop)]
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod sha256_self_test {
    use super::sha256_hex;

    #[test]
    fn sha256_matches_the_well_known_empty_and_abc_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
