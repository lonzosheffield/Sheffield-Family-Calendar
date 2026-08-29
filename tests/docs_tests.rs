//! T0.0 acceptance test — device-ID gate (`docs/reviews/PURPLE_TEAM.md` §P3, row T0.0).
//!
//! Pure file assertions; no server/db features needed, so this runs under any
//! `cargo test` invocation (including `--features server`).

use image::ImageDecoder;
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
