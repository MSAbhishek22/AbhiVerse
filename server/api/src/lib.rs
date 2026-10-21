//! ABHIVERSE coordination API.
//!
//! Task 001 scope: a single `GET /health` endpoint. No device control,
//! no pairing, no public exposure. Bind to localhost by default.

use abhiverse_protocol::{HealthResponse, API_SERVICE_NAME};
use axum::{routing::get, Json, Router};

/// Build the HTTP router. Extracted from `main` so integration tests
/// can exercise it without opening a socket.
pub fn build_app() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok(
        API_SERVICE_NAME,
        env!("CARGO_PKG_VERSION"),
    ))
}

/// Address the server binds to. Override with `ABHIVERSE_BIND_ADDR`;
/// defaults to localhost only — never `0.0.0.0` by default.
pub fn bind_addr() -> String {
    std::env::var("ABHIVERSE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string())
}
