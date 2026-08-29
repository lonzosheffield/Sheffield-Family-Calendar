//! T0.0 acceptance test — device-ID gate (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.0).
//!
//! Pure file assertions; no server/db features needed, so this runs under any
//! `cargo test` invocation (including `--features server`).

use std::fs;
use std::path::Path;

fn read_doc(relative_path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
    fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!("expected {relative_path} to exist and be readable: {e}"))
}

#[test]
fn fire_tv_doc_exists_with_status_line() {
    let content = read_doc("docs/FIRE_TV.md");
    let first_line = content
        .lines()
        .next()
        .expect("docs/FIRE_TV.md must not be empty");

    // Mirrors the regex `^STATUS: (FIRE_OS|VEGA_OS|UNKNOWN)` from PURPLE_TEAM.md §P3.
    let matches_status_line = first_line == "STATUS: FIRE_OS"
        || first_line == "STATUS: VEGA_OS"
        || first_line == "STATUS: UNKNOWN";
    assert!(
        matches_status_line,
        "docs/FIRE_TV.md line 1 must match `^STATUS: (FIRE_OS|VEGA_OS|UNKNOWN)`, got: {first_line:?}"
    );
}

#[test]
fn fire_tv_doc_documents_all_three_branches() {
    let content = read_doc("docs/FIRE_TV.md");
    assert!(
        content.contains("## Branch A"),
        "docs/FIRE_TV.md is missing the `## Branch A` heading"
    );
    assert!(
        content.contains("## Branch B"),
        "docs/FIRE_TV.md is missing the `## Branch B` heading"
    );
    assert!(
        content.contains("## Branch B\u{2032}"),
        "docs/FIRE_TV.md is missing the `## Branch B\u{2032}` (B-prime) heading"
    );
}

#[test]
fn owner_checklist_has_a_device_row() {
    let content = read_doc("docs/OWNER_CHECKLIST.md");
    assert!(
        content.lines().any(|line| line.contains("Device")),
        "docs/OWNER_CHECKLIST.md must contain a row identifying the `Device`"
    );
}
