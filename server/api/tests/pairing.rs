use abhiverse_identity::ensure_identity;
use abhiverse_protocol::{
    heartbeat_message, now_unix, pairing_approve_message, revoke_message, DeviceKind,
};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn call(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let response = app.oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("abhiverse-api-test-{tag}-{}", std::process::id()))
}

#[tokio::test]
async fn full_pairing_presence_revocation_flow() {
    let dir = temp_dir("flow");
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Test Laptop", DeviceKind::Agent).unwrap();
    let info = agent.info.clone();
    let app = abhiverse_api::build_app();

    // 1. Agent creates a pairing request.
    let (status, created) = call(
        app.clone(),
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": info })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let pairing_id = created["pairing_id"].as_str().unwrap().to_string();
    let code = created["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 6);

    // 2. Wrong code cannot claim.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": "999999", "controller_id": "c1", "controller_name": "Pixel" })),
    )
    .await;
    // 999999 could theoretically be the generated code; regenerate check:
    if code != "999999" {
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // 3. Controller claims with the right code.
    let (status, view) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": code, "controller_id": "c1", "controller_name": "Pixel" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "claimed");
    assert_eq!(view["agent"]["device_id"], info.device_id);

    // 4. Code cannot be reused.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": code, "controller_id": "c2", "controller_name": "Other" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 5. Approval needs the agent's real signature.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/pairing/{pairing_id}/approve"),
        Some(json!({ "signature": "00".repeat(64) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let sig = agent.sign(&pairing_approve_message(&pairing_id, &code));
    let (status, view) = call(
        app.clone(),
        Method::POST,
        &format!("/pairing/{pairing_id}/approve"),
        Some(json!({ "signature": sig })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "approved");

    // 6. Device is listed, offline until first heartbeat.
    let (status, list) = call(app.clone(), Method::GET, "/devices", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["devices"].as_array().unwrap().len(), 1);
    assert_eq!(list["devices"][0]["online"], false);
    assert_eq!(list["devices"][0]["paired_with_name"], "Pixel");

    // 7. Heartbeat needs a valid fresh signature.
    let ts = now_unix();
    let bad = agent.sign(&heartbeat_message(&info.device_id, ts - 10_000));
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{}/heartbeat", info.device_id),
        Some(json!({ "timestamp": ts - 10_000, "signature": bad })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST); // stale timestamp

    let good = agent.sign(&heartbeat_message(&info.device_id, ts));
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{}/heartbeat", info.device_id),
        Some(json!({ "timestamp": ts, "signature": good })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, list) = call(app.clone(), Method::GET, "/devices", None).await;
    assert_eq!(list["devices"][0]["online"], true);

    // 8. Revocation needs the device's own signature.
    let ts = now_unix();
    let (status, _) = call(
        app.clone(),
        Method::DELETE,
        &format!("/devices/{}", info.device_id),
        Some(json!({ "timestamp": ts, "signature": "00".repeat(64) })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let sig = agent.sign(&revoke_message(&info.device_id, ts));
    let (status, _) = call(
        app.clone(),
        Method::DELETE,
        &format!("/devices/{}", info.device_id),
        Some(json!({ "timestamp": ts, "signature": sig })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 9. After revocation: gone from the list, heartbeats refused.
    let (_, list) = call(app.clone(), Method::GET, "/devices", None).await;
    assert_eq!(list["devices"].as_array().unwrap().len(), 0);

    let ts = now_unix();
    let sig = agent.sign(&heartbeat_message(&info.device_id, ts));
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{}/heartbeat", info.device_id),
        Some(json!({ "timestamp": ts, "signature": sig })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn pairing_request_rejects_forged_device_id() {
    let dir = temp_dir("forge");
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Real", DeviceKind::Agent).unwrap();
    let mut forged = agent.info.clone();
    forged.device_id = "0".repeat(32); // does not match the public key
    let app = abhiverse_api::build_app();
    let (status, _) = call(
        app,
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": forged })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn reject_flow_leaves_no_trusted_device() {
    let dir = temp_dir("reject");
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
    let app = abhiverse_api::build_app();
    let (_, created) = call(
        app.clone(),
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": agent.info })),
    )
    .await;
    let pairing_id = created["pairing_id"].as_str().unwrap().to_string();
    let code = created["code"].as_str().unwrap().to_string();
    let (status, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": code, "controller_id": "c9", "controller_name": "Stranger" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sig = agent.sign(&abhiverse_protocol::pairing_reject_message(
        &pairing_id,
        &code,
    ));
    let (status, view) = call(
        app.clone(),
        Method::POST,
        &format!("/pairing/{pairing_id}/reject"),
        Some(json!({ "signature": sig })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "rejected");
    let (_, list) = call(app, Method::GET, "/devices", None).await;
    assert_eq!(list["devices"].as_array().unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
