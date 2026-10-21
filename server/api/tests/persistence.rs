//! Task 008: devices and transfers survive an API restart when a
//! state file is configured.

use abhiverse_file_transfer::{sha256_hex, CHUNK_SIZE};
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{pairing_approve_message, transfer_create_message, DeviceKind};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::path::PathBuf;
use tower::ServiceExt;

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "abhiverse-persist-test-{tag}-{}",
        std::process::id()
    ))
}

async fn call(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    headers: Vec<(String, String)>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let request = if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let response = app.oneshot(request).await.unwrap();
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

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn signed_at(id: &DeviceIdentity, msg: &str, ts: i64) -> Vec<(String, String)> {
    vec![
        ("x-abhiverse-timestamp".into(), ts.to_string()),
        ("x-abhiverse-signature".into(), id.sign(msg)),
    ]
}

#[tokio::test]
async fn devices_and_transfers_survive_restart() {
    let dir = temp_dir("restart");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state_file = dir.join("state.json");
    let chunk_dir = dir.join("chunks");

    let agent_dir = temp_dir("restart-agent");
    let _ = std::fs::remove_dir_all(&agent_dir);
    let agent = ensure_identity(&agent_dir, "Laptop", DeviceKind::Agent).unwrap();
    let ctrl_dir = temp_dir("restart-ctrl");
    let _ = std::fs::remove_dir_all(&ctrl_dir);
    let controller = ensure_identity(&ctrl_dir, "Pixel", DeviceKind::Controller).unwrap();

    let paths = || (Some(state_file.clone()), chunk_dir.clone());

    // --- Boot 1: pair and start a 2-chunk transfer, upload chunk 0.
    let (sf, cd) = paths();
    let app = abhiverse_api::build_app_with_paths(sf, cd);
    let (_, created) = call(
        app.clone(),
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": agent.info })),
        vec![],
    )
    .await;
    let pairing_id = created["pairing_id"].as_str().unwrap().to_string();
    let code = created["code"].as_str().unwrap().to_string();
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": code, "controller": controller.info })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let sig = agent.sign(&pairing_approve_message(&pairing_id, &code));
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/pairing/{pairing_id}/approve"),
        Some(json!({ "signature": sig })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let data: Vec<u8> = (0..CHUNK_SIZE + 10).map(|i| (i % 251) as u8).collect();
    let hash = sha256_hex(&data);
    let ts = now_unix();
    let headers = signed_at(
        &controller,
        &transfer_create_message(
            &controller.info.device_id,
            &agent.info.device_id,
            &hash,
            ts,
            "persist-req-1",
        ),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": controller.info.device_id, "to_id": agent.info.device_id,
            "file_name": "big.bin", "size_bytes": data.len(), "sha256": hash, "total_chunks": 2, "request_id": "persist-req-1",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let tid = view["transfer_id"].as_str().unwrap().to_string();
    // Upload only chunk 0 (raw bytes; controller uploads are unsigned by design,
    // integrity comes from the declared whole-file hash).
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/transfers/{tid}/chunks/0"))
        .body(Body::from(data[..CHUNK_SIZE as usize].to_vec()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    drop(app);
    assert!(state_file.exists(), "state file must have been written");

    // --- Boot 2: same paths. Device and partial transfer are back.
    let (sf, cd) = paths();
    let app = abhiverse_api::build_app_with_paths(sf, cd);
    let (_, list) = call(app.clone(), Method::GET, "/devices", None, vec![]).await;
    let devices = list["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["info"]["device_id"], agent.info.device_id);

    let (_, view) = call(
        app.clone(),
        Method::GET,
        &format!("/transfers/{tid}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(view["status"], "receiving");
    assert_eq!(view["received_chunks"], json!([0]));

    // Finish the transfer on the new boot: chunk 1 + signed complete.
    let req = Request::builder()
        .method(Method::PUT)
        .uri(format!("/transfers/{tid}/chunks/1"))
        .body(Body::from(data[CHUNK_SIZE as usize..].to_vec()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ts = now_unix();
    let headers = signed_at(
        &controller,
        &abhiverse_protocol::transfer_complete_message(&tid, ts),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid}/complete"),
        None,
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "complete");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&agent_dir);
    let _ = std::fs::remove_dir_all(&ctrl_dir);
}

#[tokio::test]
async fn corrupt_state_file_starts_empty_instead_of_crashing() {
    let dir = temp_dir("corrupt");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state_file = dir.join("state.json");
    std::fs::write(&state_file, "{not json").unwrap();
    let app = abhiverse_api::build_app_with_paths(Some(state_file), dir.join("chunks"));
    let (s, list) = call(app, Method::GET, "/devices", None, vec![]).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list["devices"].as_array().unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
