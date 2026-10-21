use abhiverse_file_transfer::{sha256_hex, total_chunks, CHUNK_SIZE};
use abhiverse_identity::{ensure_identity, DeviceIdentity};
use abhiverse_protocol::{
    chunk_download_message, chunk_upload_message, now_unix, pairing_approve_message,
    transfer_complete_message, transfer_create_message, DeviceKind,
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
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn call_bytes(
    app: axum::Router,
    uri: &str,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
) -> StatusCode {
    let mut builder = Request::builder().method(Method::PUT).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let response = app
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    response.status()
}

async fn get_bytes(
    app: axum::Router,
    uri: &str,
    headers: Vec<(String, String)>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let response = app
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, bytes)
}

fn signed_at(id: &DeviceIdentity, message: &str, ts: i64) -> Vec<(String, String)> {
    vec![
        ("x-abhiverse-timestamp".into(), ts.to_string()),
        ("x-abhiverse-signature".into(), id.sign(message)),
    ]
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "abhiverse-transfer-test-{tag}-{}",
        std::process::id()
    ))
}

/// Pair a fresh agent with controller "ctrl-1"; returns (app, identity).
async fn paired_app(tag: &str) -> (axum::Router, DeviceIdentity) {
    let dir = temp_dir(tag);
    let _ = std::fs::remove_dir_all(&dir);
    let agent = ensure_identity(&dir, "Laptop", DeviceKind::Agent).unwrap();
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
        Some(json!({ "code": code, "controller_id": "ctrl-1", "controller_name": "Pixel" })),
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
    (app, agent)
}

fn sample_data() -> Vec<u8> {
    (0..600_000u32).map(|i| (i * 17 % 253) as u8).collect() // 3 chunks
}

#[tokio::test]
async fn controller_to_agent_with_resume_and_signed_download() {
    let (app, agent) = paired_app("c2a").await;
    let data = sample_data();
    let hash = sha256_hex(&data);
    let total = total_chunks(data.len() as u64);
    assert_eq!(total, 3);

    // Controller creates the transfer (paired controller of the agent).
    let (status, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": "ctrl-1", "to_id": agent.info.device_id, "file_name": "notes.bin",
            "size_bytes": data.len(), "sha256": hash, "total_chunks": total,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let tid = view["transfer_id"].as_str().unwrap().to_string();

    // Upload chunks 0 and 2 only -> status shows the gap (resume source).
    for index in [0u32, 2] {
        let start = (index as u64 * CHUNK_SIZE) as usize;
        let end = (start + CHUNK_SIZE as usize).min(data.len());
        let s = call_bytes(
            app.clone(),
            &format!("/transfers/{tid}/chunks/{index}"),
            data[start..end].to_vec(),
            vec![],
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    let (_, view) = call(
        app.clone(),
        Method::GET,
        &format!("/transfers/{tid}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(view["received_chunks"], json!([0, 2]));

    // Completing early is a conflict.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid}/complete"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Upload the missing chunk, then complete: server verifies the hash.
    let s = call_bytes(
        app.clone(),
        &format!("/transfers/{tid}/chunks/1"),
        data[CHUNK_SIZE as usize..2 * CHUNK_SIZE as usize].to_vec(),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (status, view) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid}/complete"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "complete");

    // Agent downloads: unsigned is refused, signed works, bytes match.
    let (status, _) = get_bytes(app.clone(), &format!("/transfers/{tid}/chunks/0"), vec![]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let ts = now_unix();
    let headers = signed_at(&agent, &chunk_download_message(&tid, 0, ts), ts);
    let (status, bytes) =
        get_bytes(app.clone(), &format!("/transfers/{tid}/chunks/0"), headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, data[..CHUNK_SIZE as usize]);
}

#[tokio::test]
async fn agent_to_controller_requires_signatures() {
    let (app, agent) = paired_app("a2c").await;
    let data = sample_data();
    let hash = sha256_hex(&data);
    let total = total_chunks(data.len() as u64);
    let body = json!({
        "from_id": agent.info.device_id, "to_id": "ctrl-1", "file_name": "photo.bin",
        "size_bytes": data.len(), "sha256": hash, "total_chunks": total,
    });

    // Unsigned create from an agent identity is refused.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(body.clone()),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let ts = now_unix();
    let headers = signed_at(
        &agent,
        &transfer_create_message(&agent.info.device_id, "ctrl-1", &hash, ts),
        ts,
    );
    let (status, view) = call(app.clone(), Method::POST, "/transfers", Some(body), headers).await;
    assert_eq!(status, StatusCode::CREATED);
    let tid = view["transfer_id"].as_str().unwrap().to_string();

    // Chunk upload signed over the WRONG chunk hash is refused.
    let chunk0 = &data[..CHUNK_SIZE as usize];
    let ts = now_unix();
    let bad = signed_at(
        &agent,
        &chunk_upload_message(&tid, 0, &sha256_hex(b"nope"), ts),
        ts,
    );
    let s = call_bytes(
        app.clone(),
        &format!("/transfers/{tid}/chunks/0"),
        chunk0.to_vec(),
        bad,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Correct signatures for all chunks + complete.
    for index in 0..total {
        let start = (index as u64 * CHUNK_SIZE) as usize;
        let end = (start + CHUNK_SIZE as usize).min(data.len());
        let chunk = &data[start..end];
        let ts = now_unix();
        let headers = signed_at(
            &agent,
            &chunk_upload_message(&tid, index, &sha256_hex(chunk), ts),
            ts,
        );
        let s = call_bytes(
            app.clone(),
            &format!("/transfers/{tid}/chunks/{index}"),
            chunk.to_vec(),
            headers,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
    }
    let ts = now_unix();
    let headers = signed_at(&agent, &transfer_complete_message(&tid, ts), ts);
    let (status, view) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid}/complete"),
        None,
        headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "complete");

    // Controller downloads without a signature (transfer-ID capability).
    let (status, bytes) =
        get_bytes(app.clone(), &format!("/transfers/{tid}/chunks/2"), vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, data[2 * CHUNK_SIZE as usize..]);

    // Listing by recipient shows the transfer.
    let (_, list) = call(
        app.clone(),
        Method::GET,
        "/transfers?for=ctrl-1",
        None,
        vec![],
    )
    .await;
    assert_eq!(list["transfers"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn corrupt_upload_fails_completion_and_unknown_senders_are_rejected() {
    let (app, agent) = paired_app("corrupt").await;
    // Unknown sender cannot create transfers to the agent.
    let (status, _) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": "mallory", "to_id": agent.info.device_id, "file_name": "x.bin",
            "size_bytes": 3, "sha256": sha256_hex(b"abc"), "total_chunks": 1,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Declared hash is for "abc" but the chunk carries "xyz".
    let (status, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": "ctrl-1", "to_id": agent.info.device_id, "file_name": "x.bin",
            "size_bytes": 3, "sha256": sha256_hex(b"abc"), "total_chunks": 1,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let tid = view["transfer_id"].as_str().unwrap().to_string();
    let s = call_bytes(
        app.clone(),
        &format!("/transfers/{tid}/chunks/0"),
        b"xyz".to_vec(),
        vec![],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (status, _) = call(
        app.clone(),
        Method::POST,
        &format!("/transfers/{tid}/complete"),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, view) = call(
        app.clone(),
        Method::GET,
        &format!("/transfers/{tid}"),
        None,
        vec![],
    )
    .await;
    assert_eq!(view["status"], "failed");

    // Path-like file names are reduced to a safe base name.
    let (status, view) = call(
        app.clone(),
        Method::POST,
        "/transfers",
        Some(json!({
            "from_id": "ctrl-1", "to_id": agent.info.device_id, "file_name": "../../evil.sh",
            "size_bytes": 0, "sha256": sha256_hex(b""), "total_chunks": 0,
        })),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(view["file_name"], "evil.sh");
}
