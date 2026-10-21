//! GET /config/rtc with TURN env configured mints ephemeral
//! credentials (own test binary so env vars cannot race others).

use abhiverse_api::build_app;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn rtc_config_mints_ephemeral_turn_credentials() {
    std::env::set_var("ABHIVERSE_TURN_SECRET", "integration-secret");
    std::env::set_var(
        "ABHIVERSE_TURN_URLS",
        "turn:turn.example.com:3478, turns:turn.example.com:5349",
    );
    let app = build_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/config/rtc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let cfg: abhiverse_protocol::RtcConfig = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        cfg.ice_servers.len(),
        2,
        "stun + turn: {:?}",
        cfg.ice_servers
    );
    let turn = &cfg.ice_servers[1];
    assert_eq!(turn.urls.len(), 2);
    let username: i64 = turn.username.as_ref().unwrap().parse().unwrap();
    let now = abhiverse_protocol::now_unix();
    assert!(
        username > now && username <= now + 3600,
        "username is the expiry: {username} vs now {now}"
    );
    let credential = turn.credential.as_ref().unwrap();
    assert_eq!(credential.len(), 28, "base64 SHA-1 digest: {credential}");
    std::env::remove_var("ABHIVERSE_TURN_SECRET");
    std::env::remove_var("ABHIVERSE_TURN_URLS");
}
