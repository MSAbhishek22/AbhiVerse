//! Security regression tests (v0.5.2 review): replay protection,
//! revocation terminating live sessions, pairing-claim throttling,
//! cross-pairing authorization, transfer size limits, and
//! revocation surviving an API restart. All hardware-free.

use abhiverse_api::{build_app, build_app_with_paths};
use abhiverse_file_transfer::sha256_hex;
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{
    heartbeat_message, now_unix, pairing_approve_message, revoke_message, session_answer_message,
    session_create_message, transfer_create_message, DeviceKind,
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

fn identity(tag: &str, name: &str, kind: DeviceKind) -> DeviceIdentity {
    let dir = std::env::temp_dir().join(format!("abhiverse-sec-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    ensure_identity(&dir, name, kind).unwrap()
}

/// Pair an agent + controller against `app`; returns the code used.
async fn pair(app: &axum::Router, agent: &DeviceIdentity, controller: &DeviceIdentity) -> String {
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
    code
}

async fn paired(tag: &str) -> (axum::Router, DeviceIdentity, DeviceIdentity) {
    let agent = identity(&format!("{tag}-agent"), "Laptop", DeviceKind::Agent);
    let controller = identity(&format!("{tag}-ctrl"), "Phone", DeviceKind::Controller);
    let app = build_app();
    pair(&app, &agent, &controller).await;
    (app, agent, controller)
}

fn session_create(
    controller: &DeviceIdentity,
    device_id: &str,
    rid: &str,
) -> (Value, Vec<(String, String)>) {
    let offer = "v=0 test-offer";
    let ts = now_unix();
    let msg = session_create_message(
        device_id,
        &controller.info.device_id,
        &sha256_hex(offer.as_bytes()),
        ts,
        rid,
    );
    (
        json!({
            "device_id": device_id,
            "controller_id": controller.info.device_id,
            "sdp_offer": offer,
            "request_id": rid,
        }),
        signed_at(controller, &msg, ts),
    )
}

#[tokio::test]
async fn heartbeat_replay_is_rejected() {
    let (app, agent, _c) = paired("hb").await;
    let id = &agent.info.device_id;
    let ts = now_unix();
    let body = json!({
        "timestamp": ts,
        "signature": agent.sign(&heartbeat_message(id, ts)),
    });
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{id}/heartbeat"),
        Some(body.clone()),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // The identical signed heartbeat, replayed inside its window.
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{id}/heartbeat"),
        Some(body),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "replayed heartbeat must fail");
}

#[tokio::test]
async fn transfer_create_replay_rejected_but_fresh_request_id_ok() {
    let (app, agent, controller) = paired("txreplay").await;
    let sha = sha256_hex(b"hello");
    let make = |rid: &str| {
        let ts = now_unix();
        let msg = transfer_create_message(
            &controller.info.device_id,
            &agent.info.device_id,
            &sha,
            ts,
            rid,
        );
        (
            json!({
                "from_id": controller.info.device_id,
                "to_id": agent.info.device_id,
                "file_name": "a.txt",
                "size_bytes": 5,
                "sha256": sha,
                "total_chunks": 1,
                "request_id": rid,
            }),
            signed_at(&controller, &msg, ts),
        )
    };
    let (body, headers) = make("req-1");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(body.clone()),
        headers.clone(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    // Byte-identical replay of the accepted create.
    let (s, _) = call(app.clone(), Method::POST, "/transfers", Some(body), headers).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "replayed create must fail");
    // The same file again, legitimately: new request_id, new signature.
    let (body, headers) = make("req-2");
    let (s, _) = call(app.clone(), Method::POST, "/transfers", Some(body), headers).await;
    assert_eq!(s, StatusCode::CREATED, "fresh duplicate create must work");
}

#[tokio::test]
async fn session_create_replay_is_rejected() {
    let (app, agent, controller) = paired("sessreplay").await;
    let (body, headers) = session_create(&controller, &agent.info.device_id, "s-req-1");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/sessions",
        Some(body.clone()),
        headers.clone(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = call(app.clone(), Method::POST, "/sessions", Some(body), headers).await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "replayed session create must fail"
    );
}

#[tokio::test]
async fn revoke_ends_active_session() {
    let (app, agent, controller) = paired("revokesess").await;
    let dev = &agent.info.device_id;
    let (body, headers) = session_create(&controller, dev, "rev-req-1");
    let (s, view) = call(app.clone(), Method::POST, "/sessions", Some(body), headers).await;
    assert_eq!(s, StatusCode::CREATED);
    let sid = view["session_id"].as_str().unwrap().to_string();

    // Agent answers -> session is live.
    let ts = now_unix();
    let answer = "v=0 test-answer";
    let headers = signed_at(
        &agent,
        &session_answer_message(&sid, &sha256_hex(answer.as_bytes()), ts),
        ts,
    );
    let (s, view) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/answer"),
        Some(json!({ "sdp_answer": answer })),
        headers,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "answered");

    // Controller revokes the device mid-session.
    let ts = now_unix();
    let (s, _) = call(
        app.clone(),
        Method::DELETE,
        &format!("/devices/{dev}"),
        Some(json!({
            "timestamp": ts,
            "signature": controller.sign(&revoke_message(dev, ts)),
        })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // The session is now Ended for both participants, and no new
    // ICE can be added to it.
    let (s, view) = call(
        app.clone(),
        Method::GET,
        &format!("/sessions/{sid}?as={}", controller.info.device_id),
        None,
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["status"], "ended", "revoke must end the live session");
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/sessions/{sid}/ice"),
        Some(json!({
            "from_id": dev,
            "candidate": "candidate:1 1 udp 1 127.0.0.1 5000 typ host",
            "sdp_mid": "0",
            "sdp_mline_index": 0,
        })),
        vec![],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "no ICE for a session killed by revocation"
    );
}

#[tokio::test]
async fn pairing_claim_throttle_kicks_in() {
    let agent = identity("throttle-agent", "Laptop", DeviceKind::Agent);
    let controller = identity("throttle-ctrl", "Phone", DeviceKind::Controller);
    let app = build_app();
    let (_, created) = call(
        app.clone(),
        Method::POST,
        "/pairing/requests",
        Some(json!({ "agent": agent.info })),
        vec![],
    )
    .await;
    let code = created["code"].as_str().unwrap().to_string();
    let wrong = if code == "999999" { "000000" } else { "999999" };
    for i in 0..10 {
        let (s, _) = call(
            app.clone(),
            Method::POST,
            "/pairing/claim",
            Some(json!({ "code": wrong, "controller": controller.info })),
            vec![],
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "wrong claim #{i}");
    }
    // After 10 failures inside a minute, even the CORRECT code is
    // throttled (the device-side approval remains the real gate).
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/pairing/claim",
        Some(json!({ "code": code, "controller": controller.info })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn cross_pairing_authorization_is_denied() {
    let (app, agent_a, controller_a) = paired("crossa").await;
    let agent_b = identity("crossb-agent", "Desktop", DeviceKind::Agent);
    let controller_b = identity("crossb-ctrl", "Tablet", DeviceKind::Controller);
    pair(&app, &agent_b, &controller_b).await;

    // Controller A is paired with A only: a session to B must fail.
    let (body, headers) = session_create(&controller_a, &agent_b.info.device_id, "cross-1");
    let (s, _) = call(app.clone(), Method::POST, "/sessions", Some(body), headers).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // A transfer from A to B likewise.
    let sha = sha256_hex(b"nope");
    let ts = now_unix();
    let msg = transfer_create_message(
        &controller_a.info.device_id,
        &agent_b.info.device_id,
        &sha,
        ts,
        "cross-2",
    );
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": controller_a.info.device_id,
            "to_id": agent_b.info.device_id,
            "file_name": "n.txt",
            "size_bytes": 4,
            "sha256": sha,
            "total_chunks": 1,
            "request_id": "cross-2",
        })),
        signed_at(&controller_a, &msg, ts),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let _ = agent_a;
}

#[tokio::test]
async fn oversized_transfer_is_rejected() {
    let (app, agent, controller) = paired("big").await;
    let size: u64 = 1024 * 1024 * 1024 + 1; // default cap is 1 GiB
    let total = size.div_ceil(262_144);
    let sha = sha256_hex(b"big");
    let ts = now_unix();
    let msg = transfer_create_message(
        &controller.info.device_id,
        &agent.info.device_id,
        &sha,
        ts,
        "big-1",
    );
    let (s, _) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": controller.info.device_id,
            "to_id": agent.info.device_id,
            "file_name": "big.bin",
            "size_bytes": size,
            "sha256": sha,
            "total_chunks": total,
            "request_id": "big-1",
        })),
        signed_at(&controller, &msg, ts),
    )
    .await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn revocation_survives_api_restart() {
    let root = std::env::temp_dir().join(format!("abhiverse-sec-restart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let state = root.join("state.json");
    let chunks = root.join("chunks");
    let agent = identity("restart-agent", "Laptop", DeviceKind::Agent);
    let controller = identity("restart-ctrl", "Phone", DeviceKind::Controller);

    let app = build_app_with_paths(Some(state.clone()), chunks.clone());
    pair(&app, &agent, &controller).await;
    let dev = &agent.info.device_id;
    let ts = now_unix();
    let (s, _) = call(
        app.clone(),
        Method::DELETE,
        &format!("/devices/{dev}"),
        Some(json!({
            "timestamp": ts,
            "signature": agent.sign(&revoke_message(dev, ts)),
        })),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    drop(app);

    // After a restart the device must still be revoked.
    let app = build_app_with_paths(Some(state), chunks);
    let ts = now_unix();
    let (s, _) = call(
        app.clone(),
        Method::POST,
        &format!("/devices/{dev}/heartbeat"),
        Some(json!({
            "timestamp": ts,
            "signature": agent.sign(&heartbeat_message(dev, ts)),
        })),
        vec![],
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "revoked device must stay revoked after restart"
    );
}
