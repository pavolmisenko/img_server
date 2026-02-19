use image::ImageEncoder;
use open_meteo_rs::forecast::ForecastResult;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};

// Display template
const TEMPLATE_SVG_PATH: &str = "weather_template.svg";

// Descriptions / icon mapping
const DESCRIPTIONS_PATH: &str = "descriptions.json";
const FALLBACK_ICON: &str = "icons/cloud_rain_heavy.svg";

/// Fetches weather data and fills the SVG template, returning the processed SVG string.
pub async fn build_weather_svg(lat: f64, lng: f64, location: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Read template.svg
    let template_path = Path::new(TEMPLATE_SVG_PATH);
    let svg_content = fs::read_to_string(template_path)?;

    // Fetch weather data from open-meteo
    let weather = fetch_weather(lat, lng).await?;

    let daily = weather.daily.as_deref().unwrap_or(&[]);
    let icon_map = load_icon_map()?;

    // Replace location placeholder
    let mut processed_svg = svg_content.replace("{{location}}", location);

    // Fill in day0..day6 placeholders from daily forecast
    for (i, day) in daily.iter().enumerate().take(7) {
        let weather_code = day
            .values
            .get("weather_code")
            .and_then(|item| item.value.as_f64())
            .unwrap_or(0.0) as u32;
        let temp_max = day
            .values
            .get("temperature_2m_max")
            .and_then(|item| item.value.as_f64())
            .unwrap_or(0.0);
        let temp_min = day
            .values
            .get("temperature_2m_min")
            .and_then(|item| item.value.as_f64())
            .unwrap_or(0.0);

        let day_name = day.date.format("%a").to_string();
        let icon = weather_code_to_icon(weather_code, &icon_map);
        let prefix = format!("day{}", i);

        processed_svg = processed_svg
            .replace(&format!("{{{{{}-day}}}}", prefix), &day_name)
            .replace(
                &format!("{{{{{}-minmax}}}}", prefix),
                &format!("{:.0}°C / {:.0}°C", temp_max, temp_min),
            );

        let icon_rect_id = format!("{}-icon", prefix);
        processed_svg = replace_rect_with_svg(&processed_svg, &icon_rect_id, icon)?;
    }

    Ok(processed_svg)
}

/// Renders an SVG string to a BMP-encoded byte buffer.
///
/// tiny-skia uses premultiplied RGBA8888. For opaque images (common here)
/// this is equivalent to straight alpha, so no explicit demultiplication is needed.
/// The `image` crate encodes the result as a BMP compatible with embedded_graphics tinybmp.
pub fn svg_to_bmp(svg_content: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_system_fonts();

    let mut opt = Options::default();
    opt.fontdb = Arc::new(fontdb);

    let tree = Tree::from_str(svg_content, &opt)?;

    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("Failed to create pixmap")?;

    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());

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

/// Fetches weather forecast data from open-meteo for the given coordinates.
async fn fetch_weather(lat: f64, lng: f64) -> Result<ForecastResult, Box<dyn std::error::Error>> {
    let client = open_meteo_rs::Client::new();
    let mut opts = open_meteo_rs::forecast::Options::default();

    opts.location = open_meteo_rs::Location { lat, lng };

    // Timezone
    opts.time_zone = Some("Europe/Berlin".into());

    // Daily weather fields
    opts.daily.push("temperature_2m_max".into());
    opts.daily.push("temperature_2m_min".into());
    opts.daily.push("weather_code".into());

    let res = client.forecast(opts).await?;
    Ok(res)
}

/// Loads the weather code → icon path mapping from descriptions.json.
fn load_icon_map() -> Result<HashMap<u32, String>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(DESCRIPTIONS_PATH)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    let mut map = HashMap::new();
    if let Some(obj) = json.as_object() {
        for (key, val) in obj {
            if let Ok(code) = key.parse::<u32>() {
                if let Some(image) = val.pointer("/day/image").and_then(|v| v.as_str()) {
                    map.insert(code, image.to_string());
                }
            }
        }
    }
    Ok(map)
}

/// Returns the icon SVG path for the given WMO weather code, falling back to FALLBACK_ICON.
fn weather_code_to_icon<'a>(code: u32, icon_map: &'a HashMap<u32, String>) -> &'a str {
    icon_map
        .get(&code)
        .map(|s| s.as_str())
        .unwrap_or(FALLBACK_ICON)
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