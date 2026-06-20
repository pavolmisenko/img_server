# img_server

A lightweight Rust HTTP server that generates weather display images in BMP format. It fetches live forecast data from [Open-Meteo](https://open-meteo.com/), renders it into an SVG template, and returns a BMP-encoded image — useful for e-ink displays or embedded devices.

## How It Works

1. Client sends a GET request to `/fetch_bitmap` with coordinates and a location name.
2. Server fetches weather data from the Open-Meteo API (no API key required).
3. Weather values are injected into `weather_template.svg` (current conditions, 7-day forecast, icons).
4. A 24-hour temperature and precipitation chart is generated with `plotters` and embedded into the SVG.
5. The SVG is rasterised to RGBA pixels via `resvg` / `tiny-skia` and encoded as a BMP.
6. The BMP bytes are returned in the HTTP response (`image/bmp`).

## Endpoint

```
GET /fetch_bitmap?lat=<latitude>&lng=<longitude>&location=<name>
```

| Parameter  | Type   | Description                             |
| ---------- | ------ | --------------------------------------- |
| `lat`      | float  | Latitude of the location                |
| `lng`      | float  | Longitude of the location               |
| `location` | string | Display name shown on the weather image |

**Example:**

```
http://0.0.0.0:3000/fetch_bitmap?lat=49.1952&lng=16.608&location=Brno
```

## Project Structure

```
img_server/
├── src/
│   ├── main.rs          # Axum server, route definitions
│   └── svg_process.rs   # Weather fetching, SVG templating, BMP rendering
├── icons/               # SVG weather condition icons
├── descriptions.json    # WMO weather code → icon mapping
├── weather_template.svg # SVG layout template with {{placeholders}}
└── Cargo.toml
```

### Template Placeholders

The following placeholders in `weather_template.svg` are replaced at runtime:

| Placeholder          | Content                                 |
| -------------------- | --------------------------------------- |
| `{{location-day}}`   | Location name and current weekday       |
| `{{tmp}}`            | Current temperature (°C)                |
| `{{tmp-fl}}`         | Feels-like temperature                  |
| `{{minmax}}`         | Today's high / low temperatures         |
| `{{dayN-day}}`       | Weekday name for forecast day N         |
| `{{dayN-minmax}}`    | High/low for forecast day N             |
| `{{dayN-precip}}`    | Precipitation probability and amount    |
| `actual-icon` (rect) | Current conditions weather icon         |
| `dayN-icon` (rect)   | Forecast day N weather icon             |
| `today-plot` (rect)  | 24-hour temperature/precipitation chart |

## Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- `weather_template.svg` in the working directory
- `descriptions.json` in the working directory
- `icons/` directory with SVG icon files

## Build & Run

```bash
# Build
cargo build --release

# Run (binds to 0.0.0.0:3000, accessible on the local network)
cargo run --release
```

The server listens on **port 3000** and binds to all interfaces, so other devices on the LAN can reach it.

## Dependencies

| Crate                  | Purpose                                  |
| ---------------------- | ---------------------------------------- |
| `axum`                 | HTTP server framework                    |
| `tokio`                | Async runtime                            |
| `reqwest`              | HTTP client for Open-Meteo API calls     |
| `serde` / `serde_json` | JSON (de)serialisation                   |
| `usvg`                 | SVG parsing                              |
| `resvg`                | SVG rasterisation                        |
| `tiny-skia`            | Pixel buffer and rendering backend       |
| `image`                | BMP encoding                             |
| `plotters`             | 24-hour forecast chart generation        |
| `chrono`               | Date/time parsing and weekday formatting |
| `regex`                | SVG content manipulation                 |
