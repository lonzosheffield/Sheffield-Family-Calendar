use anyhow::Result;
use image::ImageBuffer;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: xtask <command>");
        eprintln!("Commands:");
        eprintln!("  icons - Generate PWA icons");
        eprintln!("  assets - Generate screensaver and fixture assets");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "icons" => generate_icons()?,
        "assets" => generate_assets()?,
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn generate_icons() -> Result<()> {
    // Read the SVG monogram
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let svg_path = manifest_dir.join("src/monogram.svg");
    let svg_content = fs::read_to_string(&svg_path)?;

    // Create output directory in assets/icons
    let root = manifest_dir.parent().unwrap();
    let output_dir = root.join("assets/icons");
    fs::create_dir_all(&output_dir)?;

    // Generate icons at different sizes
    let sizes = vec![192, 512];

    for size in sizes {
        // Regular icon
        let icon_data = render_svg_to_png(&svg_content, size, size, false)?;
        let icon_path = output_dir.join(format!("icon-{}.png", size));
        icon_data.save(&icon_path)?;
        println!("Generated: {}", icon_path.display());

        // Maskable icon
        let maskable_data = render_svg_to_png(&svg_content, size, size, true)?;
        let maskable_path = output_dir.join(format!("icon-{}-maskable.png", size));
        maskable_data.save(&maskable_path)?;
        println!("Generated: {}", maskable_path.display());
    }

    Ok(())
}

fn generate_assets() -> Result<()> {
    // Create output directories
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap();
    let screensaver_dir = root.join("assets/screensaver");
    let fixtures_dir = root.join("tests/fixtures");

    fs::create_dir_all(&screensaver_dir)?;
    fs::create_dir_all(&fixtures_dir)?;

    // Generate 3 screensaver JPEGs with different Sheffield palette colors
    let screensaver_configs = vec![
        ("screensaver-1.jpg", 0x2672B3u32, 0xF4D03Fu32), // Dark blue + sun
        ("screensaver-2.jpg", 0x8BB5DAu32, 0xE86A58u32), // Light blue + accent
        ("screensaver-3.jpg", 0xE86A58u32, 0x2672B3u32), // Accent + dark blue
    ];

    for (filename, color1, color2) in screensaver_configs {
        let img = create_gradient_image(1280, 720, color1, color2)?;
        let path = screensaver_dir.join(filename);
        img.save(&path)?;
        println!("Generated: {}", path.display());
    }

    // Generate 12MP photo fixture (4000x3000 pixels)
    let fixture_img = create_gradient_image(4000, 3000, 0x8BB5DA, 0xE86A58)?;
    let fixture_path = fixtures_dir.join("photo_12mp.jpg");
    fixture_img.save(&fixture_path)?;
    println!("Generated: {}", fixture_path.display());

    Ok(())
}

fn create_gradient_image(
    width: u32,
    height: u32,
    color1: u32,
    color2: u32,
) -> Result<ImageBuffer<image::Rgb<u8>, Vec<u8>>> {
    let mut img = ImageBuffer::new(width, height);

    let (r1, g1, b1) = (
        (color1 >> 16) as u8,
        ((color1 >> 8) & 0xFF) as u8,
        (color1 & 0xFF) as u8,
    );
    let (r2, g2, b2) = (
        (color2 >> 16) as u8,
        ((color2 >> 8) & 0xFF) as u8,
        (color2 & 0xFF) as u8,
    );

    for y in 0..height {
        for x in 0..width {
            let t = y as f32 / height as f32;
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;

            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }

    Ok(img)
}

fn render_svg_to_png(
    svg_content: &str,
    width: u32,
    height: u32,
    _is_maskable: bool,
) -> Result<ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
    // Parse SVG using usvg
    let fontdb = usvg::fontdb::Database::new();
    let rtree = usvg::Tree::from_str(svg_content, &usvg::Options::default(), &fontdb)?;

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

    let scale = width as f32 / rtree.size().width();
    let transform = tiny_skia::Transform::from_scale(scale, scale);

    resvg::render(&rtree, transform, &mut pixmap.as_mut());

    // Convert tiny_skia pixmap to image buffer
    let data = pixmap.data().to_vec();
    let img = ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(width, height, data)
        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;

    Ok(img)
}
