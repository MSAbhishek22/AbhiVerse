use abhiverse_file_transfer::sha256_hex;
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{
    ice_candidate_message, now_unix, pairing_approve_message, session_answer_message,
    session_end_message, session_reject_message, signed_get_message, DeviceKind,
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

fn ice_headers(id: &DeviceIdentity, sid: &str, candidate: &str) -> Vec<(String, String)> {
    let ts = now_unix();
    signed_at(
        id,
        &ice_candidate_message(
            sid,
            &id.info.device_id,
            &sha256_hex(candidate.as_bytes()),
            ts,
        ),
        ts,
    )
}

fn make_controller(tag: &str) -> DeviceIdentity {
    let dir = std::env::temp_dir().join(format!(
        "abhiverse-session-test-{tag}-ctrl-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    ensure_identity(&dir, "Pixel", DeviceKind::Controller).unwrap()
}

async fn paired_app(tag: &str) -> (axum::Router, DeviceIdentity, DeviceIdentity) {
    let dir = std::env::temp_dir().join(format!(
        "abhiverse-session-test-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
    let controller = make_controller(tag);
    let app = abhiverse_api::build_app();
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
    (app, agent, controller)
}

fn session_headers(
    controller: &DeviceIdentity,
    device_id: &str,
    offer: &str,
) -> (Vec<(String, String)>, String) {
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let request_id = format!("test-req-{n}");
    let ts = now_unix();
    let msg = abhiverse_protocol::session_create_message(
        device_id,
        &controller.info.device_id,
        &sha256_hex(offer.as_bytes()),
        ts,
        &request_id,
    );
    (signed_at(controller, &msg, ts), request_id)
}

const OFFER: &str = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\n";

#[tokio::test]
async fn full_signalling_flow() {
    let (app, agent, controller) = paired_app("flow").await;
    let dev = agent.info.device_id.clone();

    // RTC config defaults to STUN (signed: TURN credentials are sensitive).
    let rtc_path = format!("/config/rtc?as={}", controller.info.device_id);
    let (s, cfg) = call(
        app.clone(),
        Method::GET,
        &rtc_path,
        None,
        signed_get(&controller, &rtc_path),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(cfg["ice_servers"][0]["urls"][0]
        .as_str()
        .unwrap()
        .starts_with("stun:"));

    // A stranger cannot open a session.
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": "mallory", "sdp_offer": OFFER, "request_id": "mallory-1",
        })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // The paired controller can — with its signature.
    let (headers, rid) = session_headers(&controller, &dev, OFFER);
    let (s, view) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id, "sdp_offer": OFFER, "request_id": rid,
        })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(view["status"], "offered");
    let sid = view["session_id"].as_str().unwrap().to_string();

    // Unknown callers cannot read the session (no registered key).
    let (s, _) = call(
        app.clone(),
        Method::GET,
        &format!("/sessions/{sid}?as=mallory"),
        None,
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // The agent discovers it via the for-list (signed).
    let list_path = format!("/sessions?for={dev}&as={dev}");
    let (_, list) = call(
        app.clone(),
        Method::GET,
        &list_path,
        None,
        signed_get(&agent, &list_path),
    )
    .await;
    assert_eq!(list["sessions"].as_array().unwrap().len(), 1);

    // Unsigned answer is refused; signed answer is accepted.
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/answer"),
        Some(json!({ "sdp_answer": "answer-sdp" })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let ts = now_unix();
    let headers = signed_at(
        &agent,
        &session_answer_message(&sid, &sha256_hex(b"answer-sdp"), ts),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/answer"),
        Some(json!({ "sdp_answer": "answer-sdp" })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "answered");
    assert_eq!(view["sdp_answer"], "answer-sdp");

    // ICE flows both ways; each side sees only the other's candidates.
    let (s, r) = call(app.clone(), Method::POST, &format!("/sessions/{sid}/ice"), Some(json!({
        "from_id": controller.info.device_id, "candidate": "candidate:ctrl", "sdp_mid": "0", "sdp_mline_index": 0,
    })), ice_headers(&controller, &sid, "candidate:ctrl")).await;
    assert_eq!(s, StatusCode::OK);
    let seq_ctrl = r["seq"].as_u64().unwrap();
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/ice"),
        Some(json!({
            "from_id": dev, "candidate": "candidate:agent", "sdp_mid": "0", "sdp_mline_index": 0,
        })),
        ice_headers(&agent, &sid, "candidate:agent"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let ice_path = format!("/sessions/{sid}/ice?as={dev}");
    let (_, ice) = call(
        app.clone(),
        Method::GET,
        &ice_path,
        None,
        signed_get(&agent, &ice_path),
    )
    .await;
    let cands = ice["candidates"].as_array().unwrap();
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0]["candidate"], "candidate:ctrl");

    let ice_path = format!(
        "/sessions/{sid}/ice?as={}&after={seq_ctrl}",
        controller.info.device_id
    );
    let (_, ice) = call(
        app.clone(),
        Method::GET,
        &ice_path,
        None,
        signed_get(&controller, &ice_path),
    )
    .await;
    let cands = ice["candidates"].as_array().unwrap();
    assert_eq!(cands.len(), 1);
    assert_eq!(cands[0]["candidate"], "candidate:agent");

    // Controller ends the session (signed); further ICE is a conflict.
    let ts = now_unix();
    let end_headers = signed_at(&controller, &session_end_message(&sid, ts), ts);
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/end"),
        Some(json!({ "from_id": controller.info.device_id })),
        end_headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "ended");
    let (s, _) = call(app.clone(), Method::POST, &format!("/sessions/{sid}/ice"), Some(json!({
        "from_id": controller.info.device_id, "candidate": "candidate:late", "sdp_mid": null, "sdp_mline_index": null,
    })), ice_headers(&controller, &sid, "candidate:late")).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test]
async fn agent_can_reject_and_end_with_signature() {
    let (app, agent, controller) = paired_app("reject").await;
    let dev = agent.info.device_id.clone();

    let (headers, rid) = session_headers(&controller, &dev, OFFER);
    let (_, view) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id, "sdp_offer": OFFER, "request_id": rid,
        })),
        headers,
    )
    .await;
    let sid = view["session_id"].as_str().unwrap().to_string();

    // Reject needs the device's signature.
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/reject"),
        None,
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let ts = now_unix();
    let headers = signed_at(&agent, &session_reject_message(&sid, ts), ts);
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/reject"),
        None,
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "rejected");

    // A second session: agent ends it himself after answering.
    let (headers, rid) = session_headers(&controller, &dev, OFFER);
    let (_, view) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(json!({
            "device_id": dev, "controller_id": controller.info.device_id, "sdp_offer": OFFER, "request_id": rid,
        })),
        headers,
    )
    .await;
    let sid2 = view["session_id"].as_str().unwrap().to_string();
    let ts = now_unix();
    let headers = signed_at(&agent, &session_end_message(&sid2, ts), ts);
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid2}/end"),
        Some(json!({ "from_id": dev })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "ended");
}
