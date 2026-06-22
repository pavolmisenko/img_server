# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust HTTP server (Axum) that builds a weather dashboard image for e-ink / embedded displays. On a `GET /fetch_bitmap` request it fetches live forecast data from the Open-Meteo API (no key required), fills an SVG template, rasterises it, and returns a BMP.

## Commands

```bash
cargo build --release        # build
cargo run --release          # run (binds 0.0.0.0:3000, override with PORT env var)
cargo test                   # run tests
cargo test bat_volt_placeholder_is_replaced   # run a single test by name
RUST_LOG=img_server=debug cargo run            # verbose tracing
```

The binary reads `weather_template.svg`, `descriptions.json`, and `icons/` **from the current working directory** at startup (paths are relative — see constants in `svg_process.rs`). Run from the repo root, or these assets must be alongside the binary (the Docker image copies them into `/app`).

## Request contract

`GET /fetch_bitmap?lat=<f64>&lng=<f64>&location=<string>&bat_volt=<f64>`

All four params are **required** (serde will 400 on a missing one) and range-validated in `main.rs`: lat ∈ [-90,90], lng ∈ [-180,180], location 1–100 chars, bat_volt ∈ [-5,30]. Note the README's endpoint table is stale and omits `bat_volt`. Also exposes `GET /health` (returns 200, used by the Docker healthcheck).

## Architecture

Two source files:

- **`src/main.rs`** — Axum router, the two routes, param parsing/validation, and content-type wiring. Thin.
- **`src/svg_process.rs`** — everything else, in three stages:
  1. **`AppState::load()`** reads the template, the WMO-code→icon map (`descriptions.json`), and every referenced icon's SVG text into memory **once at startup**. Wrapped in an `Arc` and shared across requests, so per-request handling does no disk I/O.
  2. **`build_weather_svg()`** fetches Open-Meteo (`fetch_weather`), then produces the final SVG by string substitution into a clone of the template. Two substitution mechanisms coexist:
     - `{{placeholder}}` text tokens → `String::replace` (e.g. `{{tmp}}`, `{{bat-volt}}`, `{{dayN-day}}`).
     - `<rect id="...">` placeholders → replaced with an inlined, repositioned `<svg>` via regex (`replace_rect_with_svg` / `replace_rect_with_svg_content`). This is how weather icons and the chart get embedded. The rect's x/y/width/height become a `<g transform>` + nested `<svg viewBox>`.
  3. **`svg_to_bmp()`** rasterises via usvg/resvg/tiny-skia and BMP-encodes RGBA8 with the `image` crate.
- The 24-hour temp/precip chart (`plot()`) is rendered **to an SVG string in memory** with plotters' `SVGBackend` and inlined — deliberately no temp files, so it's safe under concurrent requests.

Adding a new template field: add the `{{token}}` (or `<rect id>`) to `weather_template.svg`, then add the matching `.replace(...)` / `replace_rect_with_svg(...)` call in `build_weather_svg`. Unmatched placeholders are silently left in the output SVG.

## Data / assets

- `descriptions.json` maps WMO weather codes → `icons/*.svg`. Unmapped codes fall back to `FALLBACK_ICON` (`icons/cloud_rain_heavy.svg`).
- `scripts/convert.py` is a **standalone offline utility** (PIL) for converting an image to a 1-bit dithered 800×480 BMP for the display — it is not part of the server runtime.

## Deployment workflow — important

Pushing to `main` that touches source/assets triggers `.github/workflows/docker.yml`, which builds and pushes `ghcr.io/pavolmisenko/img_server:latest` — i.e. **a merge to `main` deploys to the live Unraid container.** Do all development on a feature branch and only merge to `main` when complete and tested; partial work on `main` ships broken images. A `v*.*.*` tag additionally produces pinned semver image tags.

## Notes

- Output BMP is RGBA8, consumed by `embedded_graphics`/`tinybmp` on the device. tiny-skia uses premultiplied alpha, treated as straight alpha here since images are opaque (see comment in `svg_to_bmp`).
- SVG text rendering needs system fonts; the Docker runtime installs `fonts-dejavu-core` for this reason. The binary loads system fonts at request time (`load_system_fonts`).
