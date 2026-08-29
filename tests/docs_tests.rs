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

// ---------------------------------------------------------------------------
// T0.1 acceptance tests — docs/NON_RUST.md, docs/DEV_WINDOWS.md, tailwind.config.js
// (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.1).
// ---------------------------------------------------------------------------

#[test]
fn test_non_rust_md_exists_with_required_content() {
    let content = read_doc("docs/NON_RUST.md");

    // Count only data rows (skip header and separator)
    let data_rows = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('|')
                && !trimmed.contains("---")
                && !trimmed.starts_with("| Component")
        })
        .count();

    assert!(
        data_rows >= 9,
        "docs/NON_RUST.md must have at least 9 data rows; found {data_rows}"
    );

    // Check for required strings
    let required_strings = ["sw.js", "tailwindcss-windows-x64", "Fully Kiosk", "adb"];
    for required in &required_strings {
        assert!(
            content.contains(required),
            "docs/NON_RUST.md must contain the string '{required}'"
        );
    }
}

#[test]
fn test_tailwind_config_no_index_html() {
    let content = read_doc("tailwind.config.js");

    assert!(
        !content.contains("./index.html"),
        "tailwind.config.js must not contain the dead './index.html' glob"
    );

    // Also verify the content field still exists and has the main glob
    assert!(
        content.contains("./src/**/*.{rs,html,css}"),
        "tailwind.config.js must contain the main src glob pattern"
    );
}

#[test]
fn test_dev_windows_md_exists_with_path_prefix() {
    let content = read_doc("docs/DEV_WINDOWS.md");

    // Verify it's the first step
    assert!(
        content.contains("### Step 1: Update `$env:PATH`")
            || content.contains("### Step 1: Update $env:PATH"),
        "docs/DEV_WINDOWS.md step 1 must be the PATH prefix setup"
    );

    // Verify it contains the Windows PATH setup instructions
    assert!(
        content.contains("$env:PATH") && content.contains(".cargo") && content.contains("scoop"),
        "docs/DEV_WINDOWS.md step 1 must describe the PATH prefix with .cargo/bin and scoop"
    );

    // Verify Tailwind v3.4.17 is mentioned
    assert!(
        content.contains("3.4.17") || content.contains("tailwindcss-windows-x64"),
        "docs/DEV_WINDOWS.md must mention Tailwind v3.4.17"
    );
}
