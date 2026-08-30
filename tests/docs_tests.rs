//! T0.0 acceptance test — device-ID gate (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.0).
//!
//! Pure file assertions; no server/db features needed, so this runs under any
//! `cargo test` invocation (including `--features server`).

use image::ImageDecoder;
use std::fs;
use std::path::{Path, PathBuf};

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

// ---------------------------------------------------------------------------
// T0.7 acceptance tests — PWA icons and asset fixtures
// (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.7).
// ---------------------------------------------------------------------------

#[test]
fn test_pwa_icons_are_generated_with_correct_dimensions() {
    use std::fs;
    use std::path::Path;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let icons_dir = manifest_dir.join("assets/icons");

    // Check that all required icon files exist
    let required_icons = vec![
        ("icon-192.png", 192, 192),
        ("icon-192-maskable.png", 192, 192),
        ("icon-512.png", 512, 512),
        ("icon-512-maskable.png", 512, 512),
    ];

    for (filename, expected_width, expected_height) in required_icons {
        let path = icons_dir.join(filename);
        assert!(
            path.exists(),
            "Expected icon file not found: {}",
            path.display()
        );

        // Verify the dimensions by reading the PNG
        let data = fs::read(&path).unwrap_or_else(|e| panic!("Failed to read {}: {}", filename, e));
        let decoder = image::codecs::png::PngDecoder::new(std::io::Cursor::new(&data))
            .expect("Failed to create PNG decoder");
        let (width, height) = decoder.dimensions();
        assert_eq!(
            width, expected_width,
            "{} width mismatch: expected {}, got {}",
            filename, expected_width, width
        );
        assert_eq!(
            height, expected_height,
            "{} height mismatch: expected {}, got {}",
            filename, expected_height, height
        );
    }
}

#[test]
fn test_photo_fixture_has_sufficient_resolution() {
    use std::fs;
    use std::path::Path;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("tests/fixtures/photo_12mp.jpg");

    assert!(
        fixture_path.exists(),
        "Expected photo fixture not found: {}",
        fixture_path.display()
    );

    // Verify the dimensions
    let data = fs::read(&fixture_path).expect("Failed to read photo_12mp.jpg");
    let decoder = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&data))
        .expect("Failed to create JPEG decoder");
    let (width, height) = decoder.dimensions();

    assert!(
        width >= 4000 && height >= 3000,
        "Photo fixture dimensions must be at least 4000x3000, got {}x{}",
        width,
        height
    );
}

#[test]
fn test_screensaver_assets_exist() {
    use std::fs;
    use std::path::Path;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let screensaver_dir = manifest_dir.join("assets/screensaver");

    assert!(
        screensaver_dir.exists(),
        "Screensaver directory not found: {}",
        screensaver_dir.display()
    );

    // Check for at least 3 screensaver JPEGs
    let entries = fs::read_dir(&screensaver_dir).expect("Failed to read screensaver directory");
    let jpg_count = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
        })
        .count();

    assert!(
        jpg_count >= 3,
        "Expected at least 3 screensaver JPEGs, found {}",
        jpg_count
    );
}

/// T0.7 acceptance: the maskable icons must keep >= 10 % safe-zone padding
/// on every side (W3C maskable spec: only the inner 80 % is guaranteed to
/// survive the launcher mask). Asserts the outer 10 % band of each maskable
/// PNG is solid monogram background, and — so the assertion cannot pass
/// vacuously — that the matching non-maskable icon does paint artwork inside
/// that same band.
#[test]
fn test_maskable_icons_have_ten_percent_safe_zone_padding() {
    const BACKGROUND: [u8; 4] = [0x26, 0x72, 0xB3, 0xFF];

    let icons_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let band_pixels = |name: &str| -> (Vec<[u8; 4]>, u32) {
        let data =
            fs::read(icons_dir.join(name)).unwrap_or_else(|e| panic!("Failed to read {name}: {e}"));
        let img = image::load_from_memory(&data)
            .unwrap_or_else(|e| panic!("Failed to decode {name}: {e}"))
            .to_rgba8();
        let (w, h) = img.dimensions();
        let pad = (w as f32 * 0.10).floor() as u32;
        let band = img
            .enumerate_pixels()
            .filter(|(x, y, _)| *x < pad || *x >= w - pad || *y < pad || *y >= h - pad)
            .map(|(_, _, p)| p.0)
            .collect();
        (band, pad)
    };

    for size in [192u32, 512] {
        let (maskable, pad) = band_pixels(&format!("icon-{size}-maskable.png"));
        assert!(
            pad >= 19,
            "safe-zone band for {size}px must be >= 10 %, got {pad}px"
        );
        let off = maskable.iter().filter(|p| **p != BACKGROUND).count();
        assert_eq!(
            off, 0,
            "icon-{size}-maskable.png paints {off} non-background pixels inside its outer 10 % band"
        );

        let (regular, _) = band_pixels(&format!("icon-{size}.png"));
        assert!(
            regular.iter().any(|p| *p != BACKGROUND),
            "icon-{size}.png should paint artwork within the outer 10 % band (otherwise the maskable assertion is vacuous)"
        );
    }
}

// ---------------------------------------------------------------------------
// T3.2 acceptance tests — the runbooks
// (`docs/reviews/PURPLE_TEAM.md` §P3, row T3.2 / PLAN v2 §3 Phase 3).
//
// "A `#[test]` asserts: every doc exists; `FIRE_TV.md` covers, by string
// match, `sleep_timeout`, `HDMI-CEC`, `SYSTEM_ALERT_WINDOW`,
// `GET_USAGE_STATS`, `Screensaver`, `Silk`, and the Fully Kiosk PLUS price;
// `OWNER_CHECKLIST.md` contains >= 8 numbered steps each with an explicit
// pass criterion; `RECOVERY.md` covers >= 4 named failure modes; every
// internal doc link resolves (link checker test)."
// ---------------------------------------------------------------------------

/// The five runbooks T3.2 delivers, plus the operator-facing docs they link
/// to. Every backticked repo path in these files must resolve — these are
/// what a human follows with a remote in one hand.
const RUNBOOK_DOCS: &[&str] = &[
    "docs/FIRE_TV.md",
    "docs/OWNER_CHECKLIST.md",
    "docs/DEV_WINDOWS.md",
    "docs/PWA.md",
    "docs/RECOVERY.md",
    "docs/NON_RUST.md",
    "docs/PROTOCOL.md",
    "docs/BASELINE.md",
];

/// Planning records. They legitimately name deliverables of tasks that have
/// not run yet, so an unresolved reference from one of these is only allowed
/// when it is one of [`PLANNED_ARTEFACTS`]; anything else is a typo and
/// fails.
const PLANNING_DOCS: &[&str] = &["docs/PLAN.md", "docs/HANDOFF.md"];

/// Documents PLAN v2 promises but that a later task (or a failure that never
/// happened) creates: T3.3 writes `VERIFICATION.md`, T3.5 the `qa/` round
/// files, and `BLOCKED.md`/`RESIDUAL.md` only exist if a task blocks.
const PLANNED_ARTEFACTS: &[&str] = &[
    "docs/VERIFICATION.md",
    "docs/BLOCKED.md",
    "docs/RESIDUAL.md",
];

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Markdown outside fenced code blocks. Everything the link checker looks at
/// — headings, `[text](target)` links, backticked paths — is prose, never a
/// PowerShell snippet that happens to mention `Cargo.toml`.
fn without_code_fences(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// GitHub's heading-anchor rule: lower-case, drop everything that is not
/// alphanumeric or a space/hyphen, then spaces become hyphens. `## Failure
/// mode 1 — The television is blank` becomes
/// `failure-mode-1--the-television-is-blank` (the em dash vanishes and leaves
/// its two spaces behind, which is exactly what GitHub does).
fn slugify_heading(heading: &str) -> String {
    let text = heading.trim_start_matches('#').trim();
    let mut slug = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            slug.push('-');
        }
    }
    slug
}

fn heading_slugs(markdown: &str) -> Vec<String> {
    without_code_fences(markdown)
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(slugify_heading)
        .collect()
}

/// Every markdown file the checker covers: `docs/**/*.md` plus the repo's
/// own `README.md`.
fn all_markdown_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                found.push(path);
            }
        }
    }

    let mut found = vec![repo_root().join("README.md")];
    walk(&repo_root().join("docs"), &mut found);
    found.sort();
    found
}

/// The inline-code spans of a line: the odd-indexed pieces of a split on
/// backticks.
fn code_spans(line: &str) -> Vec<&str> {
    line.split('`')
        .enumerate()
        .filter_map(|(i, piece)| (i % 2 == 1).then_some(piece))
        .collect()
}

/// The same text with every inline-code span emptied out. `[text](target)`
/// written inside backticks is markdown *syntax being discussed*, not a link
/// — `docs/HANDOFF.md` does exactly that when it describes this checker.
fn without_code_spans(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    for line in markdown.lines() {
        for (index, piece) in line.split('`').enumerate() {
            if index % 2 == 0 {
                out.push_str(piece);
            }
        }
        out.push('\n');
    }
    out
}

/// Does this inline-code span look like a path into this repository?
fn looks_like_repo_path(span: &str) -> bool {
    let Some((_, extension)) = span.rsplit_once('.') else {
        return false;
    };
    // Only `.md`/`.toml`: source paths in the reviews are quoted with line
    // numbers and refer to files that have since been split up, and the
    // reviews are frozen historical records.
    if !matches!(extension, "md" | "toml") {
        return false;
    }
    span.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
}

/// Resolve a reference from `source`: repo-root-relative first (how every
/// `docs/...` reference is written), then relative to the referring file (so
/// a bare `PURPLE_TEAM.md` inside `docs/reviews/` resolves too).
fn resolve_reference(source: &Path, reference: &str) -> Option<PathBuf> {
    let from_root = repo_root().join(reference);
    if from_root.is_file() {
        return Some(from_root);
    }
    let sibling = source.parent()?.join(reference);
    sibling.is_file().then_some(sibling)
}

fn relative_to_repo(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn t3_2_every_runbook_doc_exists_and_is_substantial() {
    for doc in RUNBOOK_DOCS {
        let content = read_doc(doc);
        assert!(
            content.lines().count() >= 20,
            "{doc} exists but is a stub ({} lines)",
            content.lines().count()
        );
    }
}

#[test]
fn t3_2_fire_tv_covers_every_required_string() {
    let content = read_doc("docs/FIRE_TV.md");

    // The six strings named verbatim in PURPLE_TEAM.md §P3 row T3.2.
    for required in [
        "sleep_timeout",
        "HDMI-CEC",
        "SYSTEM_ALERT_WINDOW",
        "GET_USAGE_STATS",
        "Screensaver",
        "Silk",
    ] {
        assert!(
            content.contains(required),
            "docs/FIRE_TV.md must document `{required}`"
        );
    }

    // ... "and the Fully Kiosk PLUS price" — both currencies, as
    // `docs/NON_RUST.md` prices it.
    assert!(
        content.contains("PLUS"),
        "docs/FIRE_TV.md must name the Fully Kiosk PLUS licence"
    );
    for price in ["\u{20ac}8.90", "$10.99"] {
        assert!(
            content.contains(price),
            "docs/FIRE_TV.md must state the Fully Kiosk PLUS price {price}"
        );
    }

    // T0.0's contract, re-asserted here because T3.2 rewrote the file: the
    // STATUS line and all three branches survive finalisation.
    assert!(content.starts_with("STATUS: FIRE_OS"));
    for branch in ["## Branch A", "## Branch B", "## Branch B\u{2032}"] {
        assert!(
            content.contains(branch),
            "docs/FIRE_TV.md lost the `{branch}` heading"
        );
    }

    // A2/the task statement: this display is a television, not a stick, so
    // the HDMI-CEC step is explicitly replaced by disabling the television's
    // own sleep/power-saver timers.
    let lowered = content.to_lowercase();
    assert!(
        lowered.contains("power-saver") || lowered.contains("power saver"),
        "docs/FIRE_TV.md must give the television's own sleep/power-saver timers as the \
         replacement for the HDMI-CEC step"
    );
    assert!(
        content.contains("NS-50F301NA22") && content.contains("Fire OS 7.7.1.5"),
        "docs/FIRE_TV.md must identify the detected device"
    );
}

#[test]
fn t3_2_owner_checklist_has_eight_numbered_steps_each_with_a_pass_criterion() {
    let content = read_doc("docs/OWNER_CHECKLIST.md");
    let stripped = without_code_fences(&content);

    // A step is `### <n>. <title>`; its body runs to the next such heading.
    let mut steps: Vec<(usize, String, String)> = Vec::new();
    for line in stripped.lines() {
        let step_heading = line
            .strip_prefix("### ")
            .and_then(|rest| rest.split_once('.'))
            .and_then(|(number, title)| number.parse::<usize>().ok().map(|n| (n, title)));
        match step_heading {
            Some((number, title)) => steps.push((number, title.trim().to_string(), String::new())),
            None => {
                if let Some(last) = steps.last_mut() {
                    last.2.push_str(line);
                    last.2.push('\n');
                }
            }
        }
    }

    assert!(
        steps.len() >= 8,
        "docs/OWNER_CHECKLIST.md must contain at least 8 numbered steps, found {}",
        steps.len()
    );

    // Numbered, in order, from 1 — a checklist you work down.
    for (index, (number, title, _)) in steps.iter().enumerate() {
        assert_eq!(
            *number,
            index + 1,
            "docs/OWNER_CHECKLIST.md step numbering jumps at `{title}`"
        );
    }

    for (number, title, body) in &steps {
        assert!(
            body.contains("**Pass criterion"),
            "docs/OWNER_CHECKLIST.md step {number} ({title}) has no explicit **Pass criterion:**"
        );
        assert!(
            body.trim().len() > 120,
            "docs/OWNER_CHECKLIST.md step {number} ({title}) is too thin to follow"
        );
    }

    // PLAN v2 Appendix A A3, and the task statement: the elevated install.
    assert!(
        content.contains("family-hub.exe install"),
        "docs/OWNER_CHECKLIST.md must give the `family-hub.exe install` command"
    );
    assert!(
        content.to_lowercase().contains("elevated"),
        "docs/OWNER_CHECKLIST.md must say `family-hub.exe install` needs an elevated prompt"
    );
}

#[test]
fn t3_2_recovery_covers_at_least_four_named_failure_modes() {
    let content = read_doc("docs/RECOVERY.md");
    let stripped = without_code_fences(&content);

    let modes: Vec<&str> = stripped
        .lines()
        .filter_map(|line| line.strip_prefix("## Failure mode "))
        .collect();

    assert!(
        modes.len() >= 4,
        "docs/RECOVERY.md must cover at least 4 named failure modes, found {}: {modes:?}",
        modes.len()
    );

    // Each one is *named*, not just numbered.
    for mode in &modes {
        let (number, name) = mode.split_once(char::is_whitespace).unwrap_or_else(|| {
            panic!("failure mode heading is not `## Failure mode <n> - <name>`: {mode:?}")
        });
        assert!(
            number.trim().parse::<usize>().is_ok(),
            "failure mode heading is not numbered: {mode:?}"
        );
        assert!(
            name.trim_start_matches(['\u{2014}', '-', ' ']).len() >= 10,
            "failure mode {number} has no descriptive name: {mode:?}"
        );
    }

    // The three the acceptance row names explicitly ("what to do when the TV
    // is blank, the cert expired, the DB is corrupt").
    let lowered = content.to_lowercase();
    for topic in [
        "television is blank",
        "trusting the hub",
        "database is corrupt",
    ] {
        assert!(
            lowered.contains(topic),
            "docs/RECOVERY.md must cover the `{topic}` failure mode"
        );
    }
    assert!(
        content.matches("**Verify:").count() >= 4,
        "each failure mode in docs/RECOVERY.md needs a way to confirm the fix worked"
    );
}

/// The link checker. Three passes, none of which may find a broken target:
///
/// 1. every `[text](target)` link in `docs/**/*.md` and `README.md` — the
///    file must exist, and a `#fragment` must match a real heading;
/// 2. every backticked repo path (`.md`/`.toml`) in the runbook set;
/// 3. every backticked `docs/*.md` path in the planning records, which may
///    additionally name a [`PLANNED_ARTEFACTS`] file that a later task
///    creates.
#[test]
fn t3_2_every_internal_doc_link_resolves() {
    let files = all_markdown_files();
    let mut broken: Vec<String> = Vec::new();
    let mut checked_links = 0usize;
    let mut checked_anchors = 0usize;
    let mut checked_paths = 0usize;

    for file in &files {
        let raw =
            fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
        let content = without_code_fences(&raw);
        let here = relative_to_repo(file);

        // --- (1) markdown links ------------------------------------------
        let prose = without_code_spans(&content);
        let mut rest = prose.as_str();
        while let Some(open) = rest.find("](") {
            rest = &rest[open + 2..];
            let Some(close) = rest.find(')') else { break };
            let target = &rest[..close];
            rest = &rest[close + 1..];

            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
                || target.is_empty()
            {
                continue;
            }
            checked_links += 1;

            let (path_part, anchor) = match target.split_once('#') {
                Some((path, anchor)) => (path, Some(anchor)),
                None => (target, None),
            };

            let resolved = if path_part.is_empty() {
                Some(file.clone())
            } else {
                let found = resolve_reference(file, path_part);
                if found.is_none() {
                    broken.push(format!("{here}: link target `{target}` does not exist"));
                }
                found
            };

            if let (Some(resolved), Some(anchor)) = (resolved, anchor) {
                checked_anchors += 1;
                let target_doc = fs::read_to_string(&resolved).unwrap_or_default();
                if !heading_slugs(&target_doc).iter().any(|slug| slug == anchor) {
                    broken.push(format!(
                        "{here}: link `{target}` points at no heading in {}",
                        relative_to_repo(&resolved)
                    ));
                }
            }
        }

        // --- (2) and (3) backticked repo paths ---------------------------
        let is_runbook = RUNBOOK_DOCS.contains(&here.as_str());
        let is_planning = PLANNING_DOCS.contains(&here.as_str());
        if !is_runbook && !is_planning {
            continue;
        }

        for line in content.lines() {
            for span in code_spans(line) {
                if !looks_like_repo_path(span) {
                    continue;
                }
                // A planning record is only held to its `docs/` references;
                // it also cites upstream repositories by path, and names
                // artefacts a later task will write.
                if is_planning && (!span.starts_with("docs/") || PLANNED_ARTEFACTS.contains(&span))
                {
                    continue;
                }
                checked_paths += 1;
                if resolve_reference(file, span).is_none() {
                    broken.push(format!("{here}: `{span}` does not exist"));
                }
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} broken internal doc reference(s):\n  {}",
        broken.len(),
        broken.join("\n  ")
    );

    // Guard against the checker silently doing nothing.
    assert!(
        files.len() >= 10,
        "the link checker found only {} markdown files",
        files.len()
    );
    assert!(
        checked_links >= 5 && checked_anchors >= 5 && checked_paths >= 50,
        "the link checker is vacuous: {checked_links} links, {checked_anchors} anchors, \
         {checked_paths} paths"
    );
}

/// The runbooks are a set, not five islands: each of the five links to the
/// others by name, so whichever one the owner opens first leads to the rest.
#[test]
fn t3_2_the_runbooks_cross_reference_each_other() {
    let set = [
        "docs/FIRE_TV.md",
        "docs/OWNER_CHECKLIST.md",
        "docs/DEV_WINDOWS.md",
        "docs/PWA.md",
        "docs/RECOVERY.md",
    ];
    for doc in set {
        let content = read_doc(doc);
        let links: Vec<&str> = set
            .iter()
            .filter(|other| **other != doc && content.contains(*other))
            .copied()
            .collect();
        assert!(
            links.len() >= 3,
            "{doc} links to only {} of the other four runbooks: {links:?}",
            links.len()
        );
    }
}

// ---------------------------------------------------------------------------
// T3.3 acceptance tests — Verification pass
// (`docs/reviews/PURPLE_TEAM.md` §P3, row T3.3).
//
// "One row per task ID, none FAIL; screenshots embedded/linked."
// This test asserts that every task ID from docs/PLAN.md §3 (Phases 0–3,
// excluding T3.5) appears exactly once in docs/VERIFICATION.md.
// ---------------------------------------------------------------------------

#[test]
fn t3_3_every_task_id_appears_exactly_once_in_verification() {
    let verification_content = read_doc("docs/VERIFICATION.md");

    // All task IDs from PLAN.md §3 (Phases 0–3, excluding T3.5)
    let expected_tasks = vec![
        "T0.0", "T0.1", "T0.2", "T0.3", "T0.4", "T0.5", "T0.6", "T0.7", "T0.8",
        "T1.1", "T1.2", "T1.3", "T1.4", "T1.5", "T1.6", "T1.7",
        "T2.1", "T2.2", "T2.3", "T2.4", "T2.5", "T2.6", "T2.7",
        "T3.1", "T3.2", "T3.3", "T3.4",
    ];

    // Each task ID must appear exactly once in a table row: "| <task_id> | <status> |"
    for task_id in &expected_tasks {
        let pattern = format!("| {} |", task_id);
        let count = verification_content.matches(&pattern).count();
        assert_eq!(
            count, 1,
            "task {} appears {} times in a table row in docs/VERIFICATION.md (as `| {} |`), expected exactly 1",
            task_id, count, task_id
        );
    }
}
