//! GET /config/rtc with TURN env configured mints ephemeral
//! credentials (own test binary so env vars cannot race others).
//! The endpoint is signed: only a paired device or its controller
//! may fetch TURN credentials.

use abhiverse_api::build_app;
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{now_unix, pairing_approve_message, signed_get_message, DeviceKind};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

async fn call(
    app: axum::Router,
    uri: &str,
    headers: Vec<(String, String)>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let res = app
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, bytes)
}

fn signed_get(id: &DeviceIdentity, path: &str) -> Vec<(String, String)> {
    let ts = now_unix();
    vec![
        ("x-abhiverse-timestamp".into(), ts.to_string()),
        (
            "x-abhiverse-signature".into(),
            id.sign(&signed_get_message(path, &id.info.device_id, ts)),
        ),
    ]
}

#[tokio::test]
async fn rtc_config_mints_ephemeral_turn_credentials() {
    std::env::set_var("ABHIVERSE_TURN_SECRET", "integration-secret");
    std::env::set_var(
        "ABHIVERSE_TURN_URLS",
        "turn:turn.example.com:3478, turns:turn.example.com:5349",
    );
    let dir = std::env::temp_dir().join(format!("abhiverse-rtc-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
    let ctrl_dir =
        std::env::temp_dir().join(format!("abhiverse-rtc-test-ctrl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ctrl_dir);
    let controller = ensure_identity(&ctrl_dir, "Pixel", DeviceKind::Controller).unwrap();
    let app = build_app();

    // Pair agent + controller so the agent is a valid caller.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pairing/requests")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "agent": agent.info }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let pairing_id = created["pairing_id"].as_str().unwrap();
    let code = created["code"].as_str().unwrap();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/pairing/claim")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "code": code, "controller": controller.info }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let sig = agent.sign(&pairing_approve_message(pairing_id, code));
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/pairing/{pairing_id}/approve"))
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "signature": sig }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Unsigned fetch is refused: TURN credentials mint relay access.
    let (status, _) = call(app.clone(), "/config/rtc", vec![]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Signed fetch mints the ephemeral credential.
    let path = format!("/config/rtc?as={}", agent.info.device_id);
    let (status, bytes) = call(app.clone(), &path, signed_get(&agent, &path)).await;
    assert_eq!(status, StatusCode::OK);
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
