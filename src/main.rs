use axum::{
    Router,
    extract::Query,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::net::SocketAddr;

// Map the file "svg-process.rs" to the module "svg_process"
#[path = "svg-process.rs"]
mod svg_process;

#[derive(Deserialize)]
struct BitmapParams {
    lat: f64,
    lng: f64,
    location: String,
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/fetch_bitmap", get(fetch_bitmap));

    // Bind to all interfaces so devices on the LAN can reach this server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn fetch_bitmap(Query(params): Query<BitmapParams>) -> Response {
    let svg = match svg_process::build_weather_svg(params.lat, params.lng, &params.location).await {
        Ok(svg) => svg,
        Err(e) => {
            eprintln!("Error building weather SVG: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build weather SVG: {}", e),
            )
                .into_response();
        }
    };

    match svg_process::svg_to_bmp(&svg) {
        Ok(image_data) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "image/bmp")],
            image_data,
        )
            .into_response(),
        Err(e) => {
            eprintln!("Error rendering BMP: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render BMP: {}", e),
            )
                .into_response()
        }
    }
}
