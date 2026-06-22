# Battery Voltage Parameter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `bat_volt` query parameter to the `/fetch_bitmap` endpoint and substitute its value into the `{{bat-volt}}` placeholder in the SVG template.

**Architecture:** The endpoint's query-param struct gains a new `bat_volt: f64` field. It is validated, passed into `build_weather_svg`, and substituted directly into the SVG string — exactly as the existing `{{location-day}}`, `{{tmp}}`, etc. replacements work. The SVG template already contains the `{{bat-volt}}` placeholder.

**Tech Stack:** Rust, Axum 0.8, Serde (Deserialize), `str::replace`

## Global Constraints

- Do not add any external crates — all substitution uses `str::replace` already present in `svg_process.rs`.
- Follow the existing code style: no comments unless non-obvious, no extra error handling, no feature flags.
- The `bat_volt` field is required (not `Option`), consistent with the other existing fields.
- Format the voltage as `{:.2}V` (e.g. `3.70V`) in the substitution.

---

### Task 1: Add battery voltage parameter to endpoint and wire into SVG substitution

**Files:**
- Modify: `src/main.rs:15-20` — `BitmapParams` struct and `fetch_bitmap` handler
- Modify: `src/svg_process.rs:54-59` — `build_weather_svg` signature and substitution block

**Interfaces:**
- `BitmapParams` gains field: `bat_volt: f64`
- `build_weather_svg` signature changes from `(lat: f64, lng: f64, location: &str, state: &AppState)` to `(lat: f64, lng: f64, location: &str, bat_volt: f64, state: &AppState)`

- [ ] **Step 1: Write the failing test**

Add a `#[cfg(test)]` block at the bottom of `src/svg_process.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it passes (this is a pure logic test, no external deps)**

Run: `cargo test bat_volt_placeholder_is_replaced -- --nocapture`
Expected: PASS — the test validates the substitution format before wiring it in.

- [ ] **Step 3: Update `BitmapParams` in `src/main.rs`**

Change:
```rust
#[derive(Deserialize)]
struct BitmapParams {
    lat: f64,
    lng: f64,
    location: String,
}
```
To:
```rust
#[derive(Deserialize)]
struct BitmapParams {
    lat: f64,
    lng: f64,
    location: String,
    bat_volt: f64,
}
```

- [ ] **Step 4: Add validation for `bat_volt` in `fetch_bitmap` in `src/main.rs`**

After the existing `location` validation block (line ~69), add:

```rust
if !(-5.0..=30.0).contains(&params.bat_volt) {
    return (StatusCode::BAD_REQUEST, "bat_volt must be between -5 and 30").into_response();
}
```

- [ ] **Step 5: Update the `info!` log line and `build_weather_svg` call in `fetch_bitmap`**

Change:
```rust
info!(lat = params.lat, lng = params.lng, location = %params.location, "Handling bitmap request");

let svg = match svg_process::build_weather_svg(params.lat, params.lng, &params.location, &state).await {
```
To:
```rust
info!(lat = params.lat, lng = params.lng, location = %params.location, bat_volt = params.bat_volt, "Handling bitmap request");

let svg = match svg_process::build_weather_svg(params.lat, params.lng, &params.location, params.bat_volt, &state).await {
```

- [ ] **Step 6: Update `build_weather_svg` signature in `src/svg_process.rs`**

Change:
```rust
pub async fn build_weather_svg(
    lat: f64,
    lng: f64,
    location: &str,
    state: &AppState,
) -> Result<String, Box<dyn std::error::Error>> {
```
To:
```rust
pub async fn build_weather_svg(
    lat: f64,
    lng: f64,
    location: &str,
    bat_volt: f64,
    state: &AppState,
) -> Result<String, Box<dyn std::error::Error>> {
```

- [ ] **Step 7: Add `{{bat-volt}}` substitution inside `build_weather_svg`**

After the existing `.replace("{{minmax}}", ...)` call (around line 80), extend the chain:

Change:
```rust
    svg = svg
        .replace("{{location-day}}", &format!("{location}, {day_label}"))
        .replace("{{tmp}}", &format!("{:.0}°C", current_temp))
        .replace("{{tmp-fl}}", &format!("Feels like {:.0}°C", current_apparent))
        .replace("{{minmax}}", &format!("High {:.0}°C   Low {:.0}°C", today_max, today_min));
```
To:
```rust
    svg = svg
        .replace("{{location-day}}", &format!("{location}, {day_label}"))
        .replace("{{tmp}}", &format!("{:.0}°C", current_temp))
        .replace("{{tmp-fl}}", &format!("Feels like {:.0}°C", current_apparent))
        .replace("{{minmax}}", &format!("High {:.0}°C   Low {:.0}°C", today_max, today_min))
        .replace("{{bat-volt}}", &format!("{:.2}V", bat_volt));
```

- [ ] **Step 8: Build to verify no compiler errors**

Run: `cargo build`
Expected: Compiles with no errors or warnings about unused variables.

- [ ] **Step 9: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 10: Manual smoke test**

Run the server locally and send a request with the new parameter:

```bash
cargo run &
curl "http://localhost:3000/fetch_bitmap?lat=48.1&lng=17.1&location=Bratislava&bat_volt=3.72" \
  --output /tmp/test.bmp && echo "OK"
```

Expected: `OK` printed, `/tmp/test.bmp` is a valid BMP file.

Also verify validation rejects out-of-range values:
```bash
curl -v "http://localhost:3000/fetch_bitmap?lat=48.1&lng=17.1&location=Bratislava&bat_volt=99"
```
Expected: `400 Bad Request` with body `bat_volt must be between -5 and 30`.

Also verify missing parameter returns an error:
```bash
curl -v "http://localhost:3000/fetch_bitmap?lat=48.1&lng=17.1&location=Bratislava"
```
Expected: `400 Bad Request` (Axum rejects missing required query fields automatically).

- [ ] **Step 11: Commit**

```bash
git add src/main.rs src/svg_process.rs
git commit -m "feat: add bat_volt query parameter and substitute {{bat-volt}} in SVG template"
```
