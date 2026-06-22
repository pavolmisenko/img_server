use chrono::{Datelike, NaiveDate, NaiveDateTime};
use image::ImageEncoder;
use plotters::prelude::*;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};
use tracing::warn;
use usvg::{Options, Tree};

const TEMPLATE_SVG_PATH: &str = "weather_template.svg";
const DESCRIPTIONS_PATH: &str = "descriptions.json";
const FALLBACK_ICON: &str = "icons/cloud_rain_heavy.svg";

/// Static assets loaded once at startup and shared across all requests.
pub struct AppState {
    pub template_svg: String,
    pub icon_map: HashMap<u32, String>,
    pub icon_contents: HashMap<String, String>,
}

impl AppState {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let template_svg = fs::read_to_string(TEMPLATE_SVG_PATH)
            .map_err(|e| format!("Failed to load {TEMPLATE_SVG_PATH}: {e}"))?;

        let icon_map = load_icon_map()?;

        let mut icon_contents = HashMap::new();

        let fallback = fs::read_to_string(FALLBACK_ICON)
            .map_err(|e| format!("Failed to load fallback icon {FALLBACK_ICON}: {e}"))?;
        icon_contents.insert(FALLBACK_ICON.to_string(), fallback);

        for icon_path in icon_map.values() {
            if !icon_contents.contains_key(icon_path) {
                match fs::read_to_string(icon_path) {
                    Ok(c) => {
                        icon_contents.insert(icon_path.clone(), c);
                    }
                    Err(e) => warn!("Could not load icon '{icon_path}': {e}"),
                }
            }
        }

        Ok(Self { template_svg, icon_map, icon_contents })
    }
}

/// Fetches weather data and fills the SVG template, returning the processed SVG string.
pub async fn build_weather_svg(
    lat: f64,
    lng: f64,
    location: &str,
    bat_volt: f64,
    state: &AppState,
) -> Result<String, Box<dyn std::error::Error>> {
    let weather = fetch_weather(lat, lng).await?;

    let current_temp = get_number(&weather, &["current", "temperature_2m"]).unwrap_or(0.0);
    let current_apparent =
        get_number(&weather, &["current", "apparent_temperature"]).unwrap_or(current_temp);
    let current_code = get_number(&weather, &["current", "weather_code"]).unwrap_or(0.0) as u32;

    let today_max = get_daily_number(&weather, "temperature_2m_max", 0).unwrap_or(0.0);
    let today_min = get_daily_number(&weather, "temperature_2m_min", 0).unwrap_or(0.0);

    let day_label = get_daily_string(&weather, "time", 0)
        .and_then(|d| parse_weekday_short(&d))
        .unwrap_or_else(|| "Today".to_string());

    let mut svg = state.template_svg.clone();

    svg = svg
        .replace("{{location-day}}", &format!("{location}, {day_label}"))
        .replace("{{tmp}}", &format!("{:.0}°C", current_temp))
        .replace("{{tmp-fl}}", &format!("Feels like {:.0}°C", current_apparent))
        .replace("{{minmax}}", &format!("High {:.0}°C   Low {:.0}°C", today_max, today_min))
        .replace("{{bat-volt}}", &format!("{:.2}V", bat_volt));

    let actual_icon = weather_code_to_icon(current_code, &state.icon_map);
    let icon_id = if svg.contains("id=\"actual-icon\"") { "actual-icon" } else { "day0-icon" };
    svg = replace_rect_with_svg(&svg, icon_id, actual_icon, &state.icon_contents)?;

    // Build 24-hour hourly plot entirely in memory (no temp files → safe under concurrency)
    let current_time_str = weather["current"]["time"].as_str().unwrap_or("");
    let current_dt = NaiveDateTime::parse_from_str(current_time_str, "%Y-%m-%dT%H:%M")
        .unwrap_or_else(|_| chrono::Local::now().naive_local());

    let hourly_times: Vec<Value> =
        weather["hourly"]["time"].as_array().cloned().unwrap_or_default();
    let hourly_temps: Vec<Value> =
        weather["hourly"]["temperature_2m"].as_array().cloned().unwrap_or_default();
    let hourly_precip: Vec<Value> =
        weather["hourly"]["precipitation"].as_array().cloned().unwrap_or_default();

    let mask: Vec<bool> = hourly_times
        .iter()
        .map(|t| {
            NaiveDateTime::parse_from_str(t.as_str().unwrap_or(""), "%Y-%m-%dT%H:%M")
                .map(|dt| dt >= current_dt && dt <= current_dt + chrono::Duration::hours(24))
                .unwrap_or(false)
        })
        .collect();

    let temp_data: Vec<f64> = hourly_temps
        .iter()
        .zip(&mask)
        .filter_map(|(v, &m)| if m { v.as_f64() } else { None })
        .collect();

    let precip_data: Vec<f64> = hourly_precip
        .iter()
        .zip(&mask)
        .filter_map(|(v, &m)| if m { v.as_f64() } else { None })
        .collect();

    let hours: Vec<String> = hourly_times
        .iter()
        .zip(&mask)
        .filter_map(|(t, &m)| {
            if m {
                t.as_str().and_then(|s| {
                    let h: u32 = s.split('T').nth(1)?.split(':').next()?.parse().ok()?;
                    Some(match h {
                        0 => "12am".to_string(),
                        1..=11 => format!("{}am", h),
                        12 => "12pm".to_string(),
                        h => format!("{}pm", h - 12),
                    })
                })
            } else {
                None
            }
        })
        .collect();

    let hour_strs: Vec<&str> = hours.iter().map(String::as_str).collect();
    let plot_svg = plot(&hour_strs, &temp_data, &precip_data)?;
    svg = replace_rect_with_svg_content(&svg, "today-plot", &plot_svg)?;

    // Fill days 1–7 from daily forecast
    for day_num in 1..=7 {
        let temp_max = get_daily_number(&weather, "temperature_2m_max", day_num).unwrap_or(0.0);
        let temp_min = get_daily_number(&weather, "temperature_2m_min", day_num).unwrap_or(0.0);
        let code = get_daily_number(&weather, "weather_code", day_num).unwrap_or(0.0) as u32;
        let precip_mm = get_daily_number(&weather, "precipitation_sum", day_num).unwrap_or(0.0);
        let precip_pct =
            get_daily_number(&weather, "precipitation_probability_max", day_num).unwrap_or(0.0);

        let day_name = get_daily_string(&weather, "time", day_num)
            .and_then(|d| parse_weekday_short(&d))
            .unwrap_or_else(|| "--".to_string());

        let icon = weather_code_to_icon(code, &state.icon_map);
        let prefix = format!("day{day_num}");

        svg = svg
            .replace(&format!("{{{{{prefix}-day}}}}"), &day_name)
            .replace(
                &format!("{{{{{prefix}-minmax}}}}"),
                &format!("{:.0}°C / {:.0}°C", temp_max, temp_min),
            )
            .replace(
                &format!("{{{{{prefix}-precip}}}}"),
                &format!("{:.0}% [{:.1}]", precip_pct, precip_mm),
            );

        svg = replace_rect_with_svg(&svg, &format!("{prefix}-icon"), icon, &state.icon_contents)?;
    }

    Ok(svg)
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

async fn fetch_weather(lat: f64, lng: f64) -> Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://api.open-meteo.com/v1/forecast")
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
                "relative_humidity_2m,temperature_2m,apparent_temperature,weather_code"
                    .to_string(),
            ),
            ("hourly", "temperature_2m,precipitation_probability,precipitation".to_string()),
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
    json.get("daily")?.get(key)?.as_array()?.get(idx)?.as_f64()
}

fn get_daily_string(json: &Value, key: &str, idx: usize) -> Option<String> {
    json.get("daily")?.get(key)?.as_array()?.get(idx)?.as_str().map(ToString::to_string)
}

fn parse_weekday_short(date_str: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    Some(date.weekday().to_string().chars().take(3).collect())
}

fn load_icon_map() -> Result<HashMap<u32, String>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(DESCRIPTIONS_PATH)
        .map_err(|e| format!("Failed to load {DESCRIPTIONS_PATH}: {e}"))?;
    let json: Value = serde_json::from_str(&content)?;
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

fn weather_code_to_icon<'a>(code: u32, icon_map: &'a HashMap<u32, String>) -> &'a str {
    icon_map.get(&code).map(|s| s.as_str()).unwrap_or(FALLBACK_ICON)
}

/// Looks up icon content from the cache and delegates to `replace_rect_with_svg_content`.
fn replace_rect_with_svg(
    svg_content: &str,
    rect_id: &str,
    icon_path: &str,
    icon_contents: &HashMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
    let content = icon_contents
        .get(icon_path)
        .or_else(|| icon_contents.get(FALLBACK_ICON))
        .ok_or_else(|| format!("Icon not found and fallback missing: {icon_path}"))?;
    replace_rect_with_svg_content(svg_content, rect_id, content)
}

/// Replaces a `<rect id="…">` placeholder with an inlined, positioned SVG icon.
fn replace_rect_with_svg_content(
    svg_content: &str,
    rect_id: &str,
    icon_content: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let rect_pattern = format!(
        r#"<rect([^>]*id=["']{}["'][^>]*)(?:/>|></rect>)"#,
        regex::escape(rect_id)
    );
    let rect_regex = Regex::new(&rect_pattern)?;

    let rect_match = rect_regex
        .find(svg_content)
        .ok_or_else(|| format!("Rectangle with id '{rect_id}' not found"))?;
    let rect_element = rect_match.as_str();

    let x = extract_attribute(rect_element, "x")?;
    let y = extract_attribute(rect_element, "y")?;
    let width = extract_attribute(rect_element, "width")?;
    let height = extract_attribute(rect_element, "height")?;

    let viewbox_regex = Regex::new(r#"viewBox=["']([^"']+)["']"#)?;
    let viewbox = viewbox_regex
        .captures(icon_content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or("0 0 48 48");

    let content_regex = Regex::new(r"<svg[^>]*>([\s\S]*?)<\/svg>")?;
    let icon_inner = content_regex
        .captures(icon_content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .ok_or("Could not extract icon inner content")?;

    let replacement = format!(
        r#"<g id="{rect_id}" transform="translate({x}, {y})"><svg width="{width}" height="{height}" viewBox="{viewbox}">{icon_inner}</svg></g>"#
    );

    Ok(rect_regex.replace(svg_content, replacement.as_str()).to_string())
}

fn extract_attribute(element: &str, attr_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let pattern = format!(r#"[\s]{}=["']([^"']+)["']"#, regex::escape(attr_name));
    let regex = Regex::new(&pattern)?;
    regex
        .captures(element)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim_end_matches("px").to_string())
        .ok_or_else(|| format!("Attribute '{attr_name}' not found in element").into())
}

/// Generates the 24-hour temperature/precipitation chart as an SVG string (no disk I/O).
fn plot(
    hours: &[&str],
    temp_data: &[f64],
    precip_data: &[f64],
) -> Result<String, Box<dyn std::error::Error>> {
    if temp_data.is_empty() {
        return Err("No hourly temperature data available for plot".into());
    }

    let mut svg_buf = String::new();
    {
        let root = SVGBackend::with_string(&mut svg_buf, (770, 130)).into_drawing_area();
        root.fill(&WHITE)?;

        let temp_min = temp_data.iter().cloned().fold(f64::INFINITY, f64::min);
        let temp_max = temp_data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let precip_max = precip_data.iter().cloned().fold(0.0f64, f64::max);

        let temp_min = (temp_min - 1.0).floor();
        let temp_max = (temp_max + 1.0).ceil();
        let precip_max = (precip_max + 0.25).ceil();

        let num_hours = hours.len();

        let mut chart = ChartBuilder::on(&root)
            .x_label_area_size(20)
            .y_label_area_size(35)
            .right_y_label_area_size(35)
            .build_cartesian_2d(0..num_hours, temp_min..temp_max)?
            .set_secondary_coord(0..num_hours, 0.0..precip_max);

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(num_hours)
            .x_label_formatter(&|idx| hours.get(*idx).copied().unwrap_or("").to_string())
            .x_label_style(("sans-serif", 14).into_font().color(&BLACK))
            .y_label_style(("sans-serif", 14).into_font().color(&RED))
            .axis_desc_style(("sans-serif", 14).into_font().color(&RED))
            .draw()?;

        chart
            .configure_secondary_axes()
            .label_style(("sans-serif", 14).into_font().color(&BLACK))
            .draw()?;

        chart.draw_secondary_series(
            Histogram::vertical(&chart.borrow_secondary())
                .style(BLACK.mix(0.5).filled())
                .margin(2)
                .data(precip_data.iter().enumerate().map(|(i, &v)| (i, v))),
        )?;

        chart.draw_series(LineSeries::new(
            temp_data.iter().enumerate().map(|(i, &v)| (i, v)),
            RED.stroke_width(2),
        ))?;

        root.present()?;
    }
    Ok(svg_buf)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bat_volt_placeholder_is_replaced() {
        let svg = "voltage: {{bat-volt}} end";
        let bat_volt = 3.7_f64;
        let result = svg.replace("{{bat-volt}}", &format!("{:.2}V", bat_volt));
        assert!(!result.contains("{{bat-volt}}"));
        assert!(result.contains("3.70V"));
    }
}
