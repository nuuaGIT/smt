use anyhow::{Context, Result};
use image::imageops::FilterType;
use std::path::Path;

fn main() -> Result<()> {
    let input = Path::new("data/map_highres.png");
    let output = Path::new("data/map_highres_2x.png");
    let image = image::open(input)
        .with_context(|| format!("Karte konnte nicht gelesen werden: {}", input.display()))?;
    let resized = image::imageops::resize(&image, 16_384, 16_384, FilterType::Triangle);
    resized.save(output).with_context(|| {
        format!(
            "2x-Karte konnte nicht gespeichert werden: {}",
            output.display()
        )
    })?;
    println!(
        "{}x{} -> {}",
        image.width(),
        image.height(),
        output.display()
    );
    Ok(())
}
