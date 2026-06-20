use axum::{
    Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};

mod svg_process;

#[derive(Deserialize)]
struct BitmapParams {
    lat: f64,
    lng: f64,
    location: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let state = Arc::new(svg_process::AppState::load().unwrap_or_else(|e| {
        error!("Failed to load application assets: {e}");
        std::process::exit(1);
    }));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let app = Router::new()
        .route("/fetch_bitmap", get(fetch_bitmap))
        .route("/health", get(|| async { StatusCode::OK }))
        .with_state(state);

    info!("Listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        error!("Failed to bind {addr}: {e}");
        std::process::exit(1);
    });

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        error!("Server exited with error: {e}");
        std::process::exit(1);
    });
}

async fn fetch_bitmap(
    State(state): State<Arc<svg_process::AppState>>,
    Query(params): Query<BitmapParams>,
) -> Response {
    if !(-90.0..=90.0).contains(&params.lat) {
        return (StatusCode::BAD_REQUEST, "lat must be between -90 and 90").into_response();
    }
    if !(-180.0..=180.0).contains(&params.lng) {
        return (StatusCode::BAD_REQUEST, "lng must be between -180 and 180").into_response();
    }
    if params.location.is_empty() || params.location.len() > 100 {
        return (StatusCode::BAD_REQUEST, "location must be 1–100 characters").into_response();
    }

    info!(lat = params.lat, lng = params.lng, location = %params.location, "Handling bitmap request");

    let svg = match svg_process::build_weather_svg(params.lat, params.lng, &params.location, &state).await {
        Ok(svg) => svg,
        Err(e) => {
            error!("Error building weather SVG: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build weather SVG")
                .into_response();
        }
    };

    match svg_process::svg_to_bmp(&svg) {
        Ok(bmp) => {
            (StatusCode::OK, [(header::CONTENT_TYPE, "image/bmp")], bmp).into_response()
        }
        Err(e) => {
            error!("Error rendering BMP: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to render BMP").into_response()
        }
    }
}
