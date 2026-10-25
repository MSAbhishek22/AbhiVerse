//! Signed-read enforcement + anti-spam caps: every session /
//! transfer / RTC read needs a paired device or controller signature,
//! cross-pair reads are forbidden, and pending offers / pairing
//! requests are bounded.

use abhiverse_file_transfer::sha256_hex;
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{
    chunk_download_message, chunk_upload_message, ice_candidate_message, now_unix,
    pairing_approve_message, session_create_message, signed_get_message, transfer_complete_message,
    transfer_create_message, DeviceKind,
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
    headers: Vec<(String, String)>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn call_bytes(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    response.status()
}

fn signed_at(id: &DeviceIdentity, message: &str, ts: i64) -> Vec<(String, String)> {
    vec![
        ("x-abhiverse-timestamp".into(), ts.to_string()),
        ("x-abhiverse-signature".into(), id.sign(message)),
    ]
}

fn signed_get(id: &DeviceIdentity, path: &str) -> Vec<(String, String)> {
    let ts = now_unix();
    signed_at(id, &signed_get_message(path, &id.info.device_id, ts), ts)
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("abhiverse-auth-test-{tag}-{}", std::process::id()))
}

/// Pair one agent + controller into a fresh app; returns all three.
async fn paired_app(tag: &str) -> (axum::Router, DeviceIdentity, DeviceIdentity) {
    let dir = temp_dir(tag);
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
    let ctrl_dir = temp_dir(&format!("{tag}-ctrl"));
    let _ = std::fs::remove_dir_all(&ctrl_dir);
    let controller = ensure_identity(&ctrl_dir, "Pixel", DeviceKind::Controller).unwrap();
    let app = abhiverse_api::build_app();
    pair_into(&app, &agent, &controller).await;
    (app, agent, controller)
}

/// Pair an additional agent + controller into an existing app.
async fn pair_into(app: &axum::Router, agent: &DeviceIdentity, controller: &DeviceIdentity) {
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
}

fn session_headers(
    controller: &DeviceIdentity,
    device_id: &str,
    offer: &str,
    request_id: &str,
) -> Vec<(String, String)> {
    let ts = now_unix();
    signed_at(
        controller,
        &session_create_message(
            device_id,
            &controller.info.device_id,
            &sha256_hex(offer.as_bytes()),
            ts,
            request_id,
        ),
        ts,
    )
}

const OFFER: &str = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\n";

/// One live session + one completed transfer + one receiving transfer.
async fn setup_live(
    app: axum::Router,
    agent: &DeviceIdentity,
    controller: &DeviceIdentity,
) -> (String, String, String) {
    let dev = agent.info.device_id.clone();
    let headers = session_headers(controller, &dev, OFFER, "auth-req-1");
    let (s, view) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id,
            "sdp_offer": OFFER, "request_id": "auth-req-1",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let sid = view["session_id"].as_str().unwrap().to_string();

    // Completed transfer (controller -> agent, 3 bytes).
    let data = b"abc";
    let hash = sha256_hex(data);
    let ts = now_unix();
    let headers = signed_at(
        controller,
        &transfer_create_message(&controller.info.device_id, &dev, &hash, ts, "auth-req-2"),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": controller.info.device_id, "to_id": dev, "file_name": "a.bin",
            "size_bytes": 3, "sha256": hash, "total_chunks": 1, "request_id": "auth-req-2",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let tid_done = view["transfer_id"].as_str().unwrap().to_string();
    let ts = now_unix();
    let headers = signed_at(
        controller,
        &chunk_upload_message(&tid_done, 0, &sha256_hex(data), ts),
        ts,
    );
    let s = call_bytes(
        app.clone(),
        Method::PUT,
        &format!("/transfers/{tid_done}/chunks/0"),
        data.to_vec(),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let ts = now_unix();
    let headers = signed_at(controller, &transfer_complete_message(&tid_done, ts), ts);
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid_done}/complete"),
        None,
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Receiving transfer (nothing uploaded yet).
    let ts = now_unix();
    let headers = signed_at(
        controller,
        &transfer_create_message(
            &controller.info.device_id,
            &dev,
            &sha256_hex(b"xyz"),
            ts,
            "auth-req-3",
        ),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": controller.info.device_id, "to_id": dev, "file_name": "b.bin",
            "size_bytes": 3, "sha256": sha256_hex(b"xyz"), "total_chunks": 1, "request_id": "auth-req-3",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let tid_open = view["transfer_id"].as_str().unwrap().to_string();
    (sid, tid_done, tid_open)
}

#[tokio::test]
async fn unsigned_reads_and_writes_are_rejected() {
    let (app, agent, controller) = paired_app("unsigned").await;
    let dev = agent.info.device_id.clone();
    let ctrl = controller.info.device_id.clone();
    let (sid, tid_done, tid_open) = setup_live(app.clone(), &agent, &controller).await;

    // Reads without a signature.
    for (method, uri) in [
        (Method::GET, format!("/sessions?for={dev}")),
        (Method::GET, format!("/sessions/{sid}?as={ctrl}")),
        (Method::GET, format!("/sessions/{sid}/ice?as={ctrl}")),
        (Method::GET, format!("/transfers?for={ctrl}")),
        (Method::GET, format!("/transfers/{tid_done}")),
        (Method::GET, "/config/rtc".to_string()),
    ] {
        let (s, _) = call(app.clone(), method, &uri, None, vec![]).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned {uri}");
    }

    // Writes without a signature.
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/ice"),
        Some(json!({
            "from_id": ctrl, "candidate": "candidate:x", "sdp_mid": "0", "sdp_mline_index": 0,
        })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned ICE post");
    let s = call_bytes(
        app.clone(),
        Method::PUT,
        &format!("/transfers/{tid_open}/chunks/0"),
        b"xyz".to_vec(),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned chunk upload");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid_open}/complete"),
        None,
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned complete");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/end"),
        Some(json!({ "from_id": ctrl })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned end");
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &format!("/transfers/{tid_done}/chunks/0"),
        None,
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unsigned chunk download");

    // Positive controls: the same calls work when signed.
    let path = format!("/sessions/{sid}?as={ctrl}");
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &path,
        None,
        signed_get(&controller, &path),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "signed session read");
    let ts = now_unix();
    let headers = signed_at(
        &controller,
        &ice_candidate_message(&sid, &ctrl, &sha256_hex(b"candidate:x"), ts),
        ts,
    );
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/ice"),
        Some(json!({
            "from_id": ctrl, "candidate": "candidate:x", "sdp_mid": "0", "sdp_mline_index": 0,
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "signed ICE post");
    let ts = now_unix();
    let headers = signed_at(&agent, &chunk_download_message(&tid_done, 0, ts), ts);
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &format!("/transfers/{tid_done}/chunks/0"),
        None,
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "signed chunk download");
}

#[tokio::test]
async fn cross_pair_reads_are_forbidden() {
    let (app, agent, controller) = paired_app("cross").await;
    let dev = agent.info.device_id.clone();
    let (sid, tid_done, _) = setup_live(app.clone(), &agent, &controller).await;

    // Second, unrelated pair on the same server.
    let dir2 = temp_dir("cross-2");
    let _ = std::fs::remove_dir_all(&dir2);
    let agent2 = ensure_identity(&dir2, "Laptop2", DeviceKind::Agent).unwrap();
    let ctrl2_dir = temp_dir("cross-2-ctrl");
    let _ = std::fs::remove_dir_all(&ctrl2_dir);
    let controller2 = ensure_identity(&ctrl2_dir, "Pixel2", DeviceKind::Controller).unwrap();
    pair_into(&app, &agent2, &controller2).await;
    let ctrl2 = controller2.info.device_id.clone();

    let path = format!("/sessions/{sid}?as={ctrl2}");
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &path,
        None,
        signed_get(&controller2, &path),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "other pair's session");
    let path = format!("/sessions?for={dev}&as={ctrl2}");
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &path,
        None,
        signed_get(&controller2, &path),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "other pair's session list");
    let path = format!("/transfers/{tid_done}?as={ctrl2}");
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &path,
        None,
        signed_get(&controller2, &path),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "other pair's transfer");
}

#[tokio::test]
async fn second_pending_session_offer_is_rejected() {
    let (app, agent, controller) = paired_app("cap").await;
    let dev = agent.info.device_id.clone();
    let headers = session_headers(&controller, &dev, OFFER, "cap-req-1");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id,
            "sdp_offer": OFFER, "request_id": "cap-req-1",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let headers = session_headers(&controller, &dev, OFFER, "cap-req-2");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id,
            "sdp_offer": OFFER, "request_id": "cap-req-2",
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "one live offer per device");
}

#[tokio::test]
async fn pairing_request_cap_is_enforced() {
    let app = abhiverse_api::build_app();
    let dir = temp_dir("cap-pair");
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
    for _ in 0..100 {
        let (s, _) = call(
            app.clone(),
            Method::POST,
            "/pairing/requests",
            Some(json!({ "agent": agent.info })),
            vec![],
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
    }
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": agent.info })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "101st pending pairing");
}
