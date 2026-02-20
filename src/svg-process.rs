use image::ImageEncoder;
use chrono::{Datelike, NaiveDate};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use usvg::{Options, Tree};
use plotters::prelude::*;

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
    let icon_map = load_icon_map()?;

    // Replace location placeholder
    let mut processed_svg = svg_content;

    // Current weather values
    let current_temp = get_number(&weather, &["current", "temperature_2m"]).unwrap_or(0.0);
    let current_apparent = get_number(&weather, &["current", "apparent_temperature"]).unwrap_or(current_temp);
    let current_code = get_number(&weather, &["current", "weather_code"]).unwrap_or(0.0) as u32;

    let today_max = get_daily_number(&weather, "temperature_2m_max", 0).unwrap_or(0.0);
    let today_min = get_daily_number(&weather, "temperature_2m_min", 0).unwrap_or(0.0);

    let day_label = get_daily_string(&weather, "time", 0)
        .and_then(|d| parse_weekday_short(&d))
        .unwrap_or_else(|| "Today".to_string());

    processed_svg = processed_svg
        .replace("{{location-day}}", &format!("{location}, {day_label}"))
        .replace("{{tmp}}", &format!("{:.0}°C", current_temp))
        .replace("{{tmp-fl}}", &format!("Feels like {:.0}°C", current_apparent))
        .replace("{{minmax}}", &format!("High {:.0}°C   Low {:.0}°C", today_max, today_min));

    let actual_icon = weather_code_to_icon(current_code, &icon_map);
    processed_svg = if processed_svg.contains("id=\"actual-icon\"") {
        replace_rect_with_svg(&processed_svg, "actual-icon", actual_icon)?
    } else {
        replace_rect_with_svg(&processed_svg, "day0-icon", actual_icon)?
    };

    plot("plot.svg").unwrap();
    processed_svg = replace_rect_with_svg(&processed_svg, "today-plot", "plot.svg").unwrap();


    // Fill in day1..day7 placeholders from daily forecast (day1 = tomorrow)
    for day_num in 1..=7 {
        let temp_max = get_daily_number(&weather, "temperature_2m_max", day_num).unwrap_or(0.0);
        let temp_min = get_daily_number(&weather, "temperature_2m_min", day_num).unwrap_or(0.0);
        let weather_code = get_daily_number(&weather, "weather_code", day_num).unwrap_or(0.0) as u32;
        let precip_mm = get_daily_number(&weather, "precipitation_sum", day_num).unwrap_or(0.0);
        let precip_pct = get_daily_number(&weather, "precipitation_probability_max", day_num).unwrap_or(0.0);

        let day_name = get_daily_string(&weather, "time", day_num)
            .and_then(|d| parse_weekday_short(&d))
            .unwrap_or_else(|| "--".to_string());
        let icon = weather_code_to_icon(weather_code, &icon_map);
        let prefix = format!("day{}", day_num);

        processed_svg = processed_svg
            .replace(&format!("{{{{{}-day}}}}", prefix), &day_name)
            .replace(
                &format!("{{{{{}-minmax}}}}", prefix),
                &format!("{:.0}°C / {:.0}°C", temp_max, temp_min),
            )
            .replace(
                &format!("{{{{{}-precip}}}}", prefix),
                &format!("{:.0}% [{:.1}]", precip_pct, precip_mm),
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
async fn fetch_weather(lat: f64, lng: f64) -> Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = "https://api.open-meteo.com/v1/forecast";

    let response = client
        .get(url)
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lng.to_string()),
            ("forecast_days", "8".to_string()),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code,precipitation_probability_max".to_string(),
            ),
            (
                "current",
                "relative_humidity_2m,temperature_2m,apparent_temperature,weather_code".to_string(),
            ),
            (
                "hourly",
                "temperature_2m,precipitation_probability,precipitation".to_string(),
            ),
            ("timezone", "auto".to_string()),
        ])
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<Value>().await?)
}

fn get_number(json: &Value, path: &[&str]) -> Option<f64> {
    let mut node = json;
    for segment in path {
        node = node.get(*segment)?;
    }
    node.as_f64()
}

fn get_daily_number(json: &Value, key: &str, idx: usize) -> Option<f64> {
    json.get("daily")?
        .get(key)?
        .as_array()?
        .get(idx)?
        .as_f64()
}

fn get_daily_string(json: &Value, key: &str, idx: usize) -> Option<String> {
    json.get("daily")?
        .get(key)?
        .as_array()?
        .get(idx)?
        .as_str()
        .map(ToString::to_string)
}

fn parse_weekday_short(date_str: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    Some(date.weekday().to_string().chars().take(3).collect())
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

fn plot(plot_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(plot_path, (770, 130)).into_drawing_area();

    root.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root)
        .x_label_area_size(10)
        .y_label_area_size(10)
        //.margin(5)
        //.caption("Histogram Test", ("sans-serif", 50.0))
        .build_cartesian_2d((0u32..10u32).into_segmented(), 0u32..10u32)?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .bold_line_style(WHITE.mix(0.3))
        //.y_desc("Count")
        //.x_desc("Bucket")
        .axis_desc_style(("sans-serif", 15))
        .draw()?;

    let data = [
        0u32, 1, 1, 1, 4, 2, 5, 7, 8, 6, 4, 2, 1, 8, 3, 3, 3, 4, 4, 3, 3, 3,
    ];

    chart.draw_series(
        Histogram::vertical(&chart)
            .style(RED.mix(0.5).filled())
            .data(data.iter().map(|x: &u32| (*x, 1))),
    )?;

    root.present().expect("Unable to write result to file");

    Ok(())
}