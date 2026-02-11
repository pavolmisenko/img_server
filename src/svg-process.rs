use image::ImageEncoder;
use regex::Regex;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

// Display template
const TEMPLATE_SVG_PATH: &str = "weather_template.svg";

// Icons templates
const CLOUD_SNOW: &str = "cloud_snow.svg";
const CLOUD_RAIN: &str = "cloud_rain.svg";

pub fn generate_image() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Read template.svg
    let template_path = Path::new(TEMPLATE_SVG_PATH);
    let svg_content = fs::read_to_string(template_path)?;

    // TBD - download weather data and process it (use open-meteo-rs crate)

    // Replace text placeholders
    let mut processed_svg = svg_content
        .replace("{{Den}}", "Pondelol")
        .replace("{{Teplota}}", "15°C");

    // Replace the day1-icon rectangle with the snow icon
    processed_svg = replace_rect_with_svg(&processed_svg, "day1-icon", "snow-icon.svg")?;

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


/// Replaces a rectangle element with an SVG icon, scaled to match the rectangle's dimensions
fn replace_rect_with_svg(
    svg_content: &str,
    rect_id: &str,
    icon_svg_path: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Read the icon SVG
    let icon_content = fs::read_to_string(icon_svg_path)?;

    // Extract the rect element attributes using regex
    // Match entire rect element (both self-closing and with closing tag)
    let rect_pattern = format!(
        r#"<rect([^>]*id=["']{}["'][^>]*)(?:/>|></rect>)"#,
        regex::escape(rect_id)
    );
    let rect_regex = Regex::new(&rect_pattern)?;

    let rect_match = rect_regex
        .find(svg_content)
        .ok_or("Rectangle with specified id not found")?;
    let rect_element = rect_match.as_str();

    // Extract x, y, width, height from rect
    let x = extract_attribute(rect_element, "x")?;
    let y = extract_attribute(rect_element, "y")?;
    let width = extract_attribute(rect_element, "width")?;
    let height = extract_attribute(rect_element, "height")?;

    // Extract viewBox from icon SVG
    let viewbox_regex = Regex::new(r#"viewBox=["']([^"']+)["']"#)?;
    let viewbox = viewbox_regex
        .captures(&icon_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .unwrap_or("0 0 48 48");

    // Extract the inner content of the icon (everything between opening and closing svg tags)
    let content_regex = Regex::new(r"<svg[^>]*>([\s\S]*?)<\/svg>")?;
    let icon_inner = content_regex
        .captures(&icon_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str())
        .ok_or("Could not extract icon content")?;

    // Build the replacement group with embedded SVG
    let replacement = format!(
        r#"<g id="{}" transform="translate({}, {})"><svg width="{}" height="{}" viewBox="{}">{}</svg></g>"#,
        rect_id, x, y, width, height, viewbox, icon_inner
    );

    // Replace the rect element with the new group
    Ok(rect_regex.replace(svg_content, replacement.as_str()).to_string())
}

/// Helper function to extract attribute value from an element string
fn extract_attribute(element: &str, attr_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Use space or equals sign before attribute name to avoid partial matches
    // e.g., avoid matching "stroke-width" when looking for "width"
    let pattern = format!(r#"[\s]{}=["']([^"']+)["']"#, regex::escape(attr_name));
    let regex = Regex::new(&pattern)?;
    
    regex
        .captures(element)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().trim_end_matches("px").to_string())
        .ok_or_else(|| format!("Attribute '{}' not found", attr_name).into())
}