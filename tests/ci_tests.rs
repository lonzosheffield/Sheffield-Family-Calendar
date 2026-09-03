//! T0.8 acceptance test — CI workflow (`docs/reviews/PURPLE_TEAM.md` §P3, row
//! T0.8; `docs/PLAN.md` §3, row T0.8).
//!
//! `act` is not available on this box, so the acceptance test is a plain-text
//! parse of `.github/workflows/ci.yml` (GitHub Actions YAML is itself a
//! declared non-Rust exception, `docs/NON_RUST.md`) asserting the 7 named
//! steps exist and that no step string names `aarch64`. Each command those
//! steps run (`cargo fmt --check`, both `cargo clippy` invocations, and the
//! Tailwind rebuild-and-diff) is additionally run for real by the agent
//! outside this test suite, per the acceptance note in PURPLE_TEAM.md.

use std::fs;
use std::path::Path;

fn workflow_content() -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/ci.yml");
    fs::read_to_string(&full)
        .unwrap_or_else(|e| panic!(".github/workflows/ci.yml must exist and be readable: {e}"))
}

/// Every `- name: ...` line in the workflow, in file order.
fn step_names(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("- name:")
                .map(|rest| rest.trim().trim_matches('"').to_string())
        })
        .collect()
}

/// The 7 named steps from PLAN.md §3 / PURPLE_TEAM.md §P3 row T0.8:
/// fmt, clippy x2 (one step, two invocations), tests, dx build, Tailwind
/// rebuild fail-on-diff, `cargo tree -d` check, Windows-x64 release build.
const REQUIRED_STEPS: [&str; 7] = [
    "cargo fmt --check",
    "cargo clippy (server + web, -D warnings)",
    "cargo test --features server",
    "dx build --platform web --release",
    "Tailwind rebuild (fail on diff)",
    "cargo tree -d duplicate check",
    "Windows-x64 release build",
];

#[test]
fn ci_workflow_has_the_seven_named_steps() {
    let content = workflow_content();
    let names = step_names(&content);

    for required in REQUIRED_STEPS {
        assert!(
            names.iter().any(|n| n == required),
            "ci.yml is missing the required step named {required:?}; found steps: {names:?}"
        );
    }
}

#[test]
fn ci_workflow_has_no_aarch64_step() {
    let content = workflow_content();
    // Exclude `#`-comment lines: a comment documenting the *absence* of an
    // aarch64 job (as this file's header does) is not a step string. What
    // must never appear is `aarch64` inside actual YAML (job/step/matrix)
    // content.
    let non_comment: String = content
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !non_comment.to_ascii_lowercase().contains("aarch64"),
        "ci.yml must not reference 'aarch64' in any job/step/matrix (no aarch64 job, per R-25/R-26/E6)"
    );
}

#[test]
fn ci_workflow_clippy_step_runs_both_invocations() {
    let content = workflow_content();
    // Isolate the clippy step's body: from its `- name:` line to the next
    // `- name:` line (or EOF).
    let start = content
        .find("- name: cargo clippy (server + web, -D warnings)")
        .expect("clippy step must exist");
    let after = &content[start..];
    let end = after[1..]
        .find("- name:")
        .map(|i| i + 1)
        .unwrap_or(after.len());
    let body = &after[..end];

    assert!(
        body.contains("--features server") && body.contains("-D warnings"),
        "clippy step must run the server-target invocation with -D warnings"
    );
    assert!(
        body.contains("--features web")
            && body.contains("wasm32-unknown-unknown")
            && body.contains("-D warnings"),
        "clippy step must also run the web-target (wasm32-unknown-unknown) invocation with -D warnings"
    );
}

#[test]
fn ci_workflow_is_windows_only_single_job() {
    let content = workflow_content();
    let runs_on_lines: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("runs-on:"))
        .collect();

    assert_eq!(
        runs_on_lines.len(),
        1,
        "ci.yml must define exactly one job (Windows-x64 only, no aarch64/ubuntu/macos jobs); found: {runs_on_lines:?}"
    );
    assert!(
        runs_on_lines[0].contains("windows"),
        "the single job must run on a windows runner; found: {:?}",
        runs_on_lines[0]
    );
}

#[test]
fn ci_workflow_pins_tailwind_and_dx_versions() {
    let content = workflow_content();
    assert!(
        content.contains("dioxus-cli@0.7.10"),
        "ci.yml must pin dx to exactly 0.7.10 (PURPLE_TEAM.md §P5.4)"
    );
    assert!(
        content.contains("v3.4.17") && content.contains("tailwindcss-windows-x64"),
        "ci.yml must pin the Tailwind standalone binary to tailwindcss-windows-x64 v3.4.17"
    );
}

/// T0.7 QA fix Q2-07: validate that resvg, usvg, and tiny-skia are pinned
/// exactly in xtask/Cargo.toml (PURPLE_TEAM.md §P5.4, Cargo.toml/Cargo.lock
/// are Boss-serialised).
#[test]
fn xtask_crate_versions_are_pinned_exactly() {
    let xtask_cargo = Path::new(env!("CARGO_MANIFEST_DIR")).join("xtask/Cargo.toml");
    let content = fs::read_to_string(&xtask_cargo)
        .unwrap_or_else(|e| panic!("xtask/Cargo.toml must exist and be readable: {e}"));

    let crates_to_check = ["resvg", "usvg", "tiny-skia"];
    for crate_name in &crates_to_check {
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| line.contains(crate_name))
            .collect();

        assert!(
            !lines.is_empty(),
            "xtask/Cargo.toml must contain a line for {crate_name}"
        );

        for line in lines {
            assert!(
                line.contains("\"="),
                "xtask/Cargo.toml: every {crate_name} line must contain \"= (exact pin). Found: {line:?}"
            );
        }
    }
}

/// QH3-01 (`docs/qa/QA_HS_ROUND_3.md`): `assets/tailwind.css` is committed and nothing in
/// `cargo build` / `dx build` regenerates it, so a wave that introduces new utility classes
/// without a rebuild ships a stylesheet with no rule for them. Every size/layout utility a
/// component names must have a rule in the committed CSS.
#[test]
fn every_tailwind_utility_named_under_components_has_a_rule_in_the_committed_css() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let css =
        std::fs::read_to_string(root.join("assets/tailwind.css")).expect("assets/tailwind.css");
    const PREFIXES: [&str; 22] = [
        "grid-cols-",
        "min-h-",
        "min-w-",
        "max-h-",
        "max-w-",
        "rounded",
        "overflow-",
        "w-",
        "h-",
        "p-",
        "px-",
        "py-",
        "m-",
        "mt-",
        "mb-",
        "ml-",
        "mr-",
        "gap-",
        "tracking-",
        "items-",
        "self-",
        "list-",
    ];
    let mut missing = Vec::new();
    for path in walk_rs(&root.join("src/client/components")) {
        let source = std::fs::read_to_string(&path).expect("component source");
        for literal in source.split('"').skip(1).step_by(2) {
            for token in literal.split_whitespace() {
                if !PREFIXES.iter().any(|p| token.starts_with(p)) || token.contains('{') {
                    continue;
                }
                let escaped: String = token
                    .chars()
                    .flat_map(|c| {
                        // CSS escapes these with a backslash (char 92); spelled as a code
                        // so no shell on the way here can eat the escape.
                        if "[]./:%()".contains(c) {
                            vec![char::from(92u8), c]
                        } else {
                            vec![c]
                        }
                    })
                    .collect();
                if !css.contains(&format!(".{escaped}")) {
                    missing.push(format!("{} in {}", token, path.display()));
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "assets/tailwind.css is stale — rebuild it with `tailwindcss -i input.css -o assets/tailwind.css --minify` and commit the diff. Missing rules: {missing:#?}"
    );
}

fn walk_rs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("component directory") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(walk_rs(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}
