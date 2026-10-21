use abhiverse_protocol::HealthResponse;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok_json() {
    let app = abhiverse_api::build_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let health: HealthResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(health.app, "ABHIVERSE");
    assert_eq!(health.service, "abhiverse-api");
    assert_eq!(health.status, "ok");
    assert_eq!(health.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(health.protocol_version, 1);
}

#[tokio::test]
async fn unknown_route_is_404() {
    let app = abhiverse_api::build_app();
    let response = app
        .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
