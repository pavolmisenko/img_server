use image::ImageEncoder;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

const TEMPLATE_SVG_PATH: &str = "weather_template.svg";

pub fn generate_image() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Read template.svg
    let template_path = Path::new(TEMPLATE_SVG_PATH);
    let svg_content = fs::read_to_string(template_path)?;

    // download weather data and process it  (use open-meteo-rs crate)


    //
    let processed_svg = svg_content.replace("PLACEHOLDER", "REPLACED_TEXT");

    // Parse SVG
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();

    let mut opt = Options::default();
    opt.fontdb = Arc::new(fontdb);

    let tree = Tree::from_str(&processed_svg, &opt)?;

    // Render to Pixmap
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("Failed to create pixmap")?;

    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());

    // Convert to BMP
    // tiny-skia utilizes premultiplied RGBA8888.
    // Ideally, we should demultiply, but efficient demultiplication is non-trivial without extra deps
    // or iteration. For opaque images (common in this usecase), it's identical.
    // We use the `image` crate to encode to BMP, which is compatible with embedded_graphics tinybmp.
    let mut bmp_data = Vec::new();
    let mut cursor = Cursor::new(&mut bmp_data);

    let encoder = image::codecs::bmp::BmpEncoder::new(&mut cursor);
    encoder.write_image(
        pixmap.data(),
        pixmap.width(),
        pixmap.height(),
        image::ExtendedColorType::Rgba8,
    )?;

    Ok(bmp_data)
}
