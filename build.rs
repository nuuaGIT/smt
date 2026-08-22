use std::env;
use std::fs::{self, File};
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=data/logo.png");
    println!("cargo:rerun-if-changed=data/map.svg");
    println!("cargo:rerun-if-changed=data/map_highres.png");

    if let Err(error) = build_svg_map_texture() {
        // Keep old checkouts/builds usable if the optional SVG asset is
        // missing or malformed. The runtime toggle then falls back to the
        // existing PNG texture rather than making the whole app unbuildable.
        println!("cargo:warning=SVG map unavailable, using PNG fallback: {error}");
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let fallback = manifest_dir.join("data/map_highres.png");
        let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join("smt_map_svg.png");
        fs::copy(fallback, output).expect("could not create SVG map fallback texture");
    }

    #[cfg(windows)]
    if let Err(error) = embed_windows_icon() {
        panic!("could not embed SMT Windows icon: {error}");
    }
}

const RASTER_SIZE: u32 = 8_192;
// The current SVG export has a small extra top margin compared with the
// established world-coordinate map. Keep the node projection untouched and
// compensate only while rasterizing the SVG background.
const SVG_VERTICAL_OFFSET_PIXELS: f32 = -32.0;

fn build_svg_map_texture() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let svg_path = manifest_dir.join("data/map.svg");
    let output_path = PathBuf::from(env::var("OUT_DIR")?).join("smt_map_svg.png");
    let svg_data = fs::read(&svg_path)?;
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(&svg_data, &options)?;
    let source_size = tree.size();
    let scale_x = RASTER_SIZE as f32 / source_size.width();
    let scale_y = RASTER_SIZE as f32 / source_size.height();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(RASTER_SIZE, RASTER_SIZE)
        .ok_or("could not allocate SVG map raster")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_row(
            scale_x,
            0.0,
            0.0,
            scale_y,
            0.0,
            SVG_VERTICAL_OFFSET_PIXELS,
        ),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(output_path)?;
    Ok(())
}

#[cfg(windows)]
fn embed_windows_icon() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let logo_path = manifest_dir.join("data/logo.png");
    let logo = image::open(logo_path)?.into_rgba8();

    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16_u32, 24, 32, 48, 64, 128, 256] {
        let resized =
            image::imageops::resize(&logo, size, size, image::imageops::FilterType::Lanczos3);
        let image = ico::IconImage::from_rgba_data(size, size, resized.into_raw());
        icon_dir.add_entry(ico::IconDirEntry::encode(&image)?);
    }

    let icon_path = PathBuf::from(env::var("OUT_DIR")?).join("smt.ico");
    icon_dir.write(File::create(&icon_path)?)?;

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon(icon_path.to_str().ok_or("invalid icon path")?)
        .set("ProductName", "SMT")
        .set("FileDescription", "Satisfactory Map Tracker")
        .compile()?;
    Ok(())
}
