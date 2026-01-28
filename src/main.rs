use axum::{
    routing::get,
    Router,
    response::{IntoResponse, Response},
    http::{header, StatusCode},
};
use std::net::SocketAddr;

// Map the file "svg-process.rs" to the module "svg_process"
#[path = "svg-process.rs"]
mod svg_process;

#[tokio::main]
async fn main() {
    // Build our application with a single route
    let app = Router::new()
        .route("/path/refresh_image", get(refresh_image));

    // Run it
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn refresh_image() -> Response {
    match svg_process::generate_image() {
        Ok(image_data) => {
            (
                StatusCode::OK,
                // BMP content type
                [(header::CONTENT_TYPE, "image/bmp")],
                image_data
            ).into_response()
        },
        Err(e) => {
            eprintln!("Error generating image: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate image"
            ).into_response()
        }
    }
}