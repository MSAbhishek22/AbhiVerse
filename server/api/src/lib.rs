//! Abhiverse coordination API.
//!
//! Task 001: `GET /health`.
//! Task 003: pairing — an agent creates a short-lived 6-digit code, a
//! controller claims it, and the agent approves by signing with its
//! device private key. The code alone never grants trust.
//! Task 004: presence — paired devices send signed heartbeats; any
//! paired device can be revoked, after which its signatures are
//! refused.
//!
//! State is in-memory for now (a restart clears pairings); persistent
//! storage lands with the hosted-service phase. Bind to localhost.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use abhiverse_identity::{fingerprint, hex_decode, hex_encode, verify};
use abhiverse_protocol::{
    chunk_download_message, chunk_upload_message, now_unix, transfer_complete_message,
    transfer_create_message, ClaimPairingRequest, CreatePairingRequest, CreatePairingResponse,
    CreateTransferRequest, DeviceInfo, DeviceListResponse, DeviceView, HealthResponse,
    HeartbeatRequest, PairingDecisionRequest, PairingStatus, PairingView, RevokeRequest,
    TransferListResponse, TransferStatus, TransferView, API_SERVICE_NAME,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use rand::Rng;

const PAIRING_TTL_SECS: i64 = 300;
const ONLINE_WINDOW_SECS: i64 = 30;
const MAX_CLOCK_SKEW_SECS: i64 = 120;

#[derive(Clone)]
struct Session {
    pairing_id: String,
    code: String,
    agent: DeviceInfo,
    controller_id: Option<String>,
    controller_name: Option<String>,
    status: PairingStatus,
    expires_at: i64,
}

#[derive(Clone)]
struct TrustedDevice {
    info: DeviceInfo,
    paired_with_name: String,
    controller_id: String,
    last_seen: Option<i64>,
    revoked: bool,
}

#[derive(Clone)]
struct TransferRec {
    from_id: String,
    to_id: String,
    file_name: String,
    size_bytes: u64,
    sha256: String,
    total_chunks: u32,
    received: BTreeSet<u32>,
    status: TransferStatus,
    from_is_agent: bool,
}

#[derive(Default)]
struct Store {
    sessions: HashMap<String, Session>,
    devices: HashMap<String, TrustedDevice>,
    transfers: HashMap<String, TransferRec>,
}

struct AppState {
    store: Mutex<Store>,
    chunk_dir: PathBuf,
}

type Shared = Arc<AppState>;

fn transfer_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ABHIVERSE_TRANSFER_DIR") {
        return PathBuf::from(dir);
    }
    std::env::temp_dir().join(format!("abhiverse-transfers-{}", std::process::id()))
}

/// Build the HTTP router (fresh in-memory state). Extracted from
/// `main` so integration tests can exercise it without a socket.
pub fn build_app() -> Router {
    let store: Shared = Arc::new(AppState {
        store: Mutex::new(Store::default()),
        chunk_dir: transfer_dir(),
    });
    Router::new()
        .route("/health", get(health))
        .route("/pairing/requests", post(create_pairing))
        .route("/pairing/claim", post(claim_pairing))
        .route("/pairing/{id}", get(get_pairing))
        .route("/pairing/{id}/approve", post(approve_pairing))
        .route("/pairing/{id}/reject", post(reject_pairing))
        .route("/devices", get(list_devices))
        .route("/devices/{id}/heartbeat", post(heartbeat))
        .route("/devices/{id}", delete(revoke_device))
        .route("/transfers", post(create_transfer).get(list_transfers))
        .route("/transfers/{id}", get(get_transfer))
        .route(
            "/transfers/{id}/chunks/{index}",
            put(upload_chunk).get(download_chunk),
        )
        .route("/transfers/{id}/complete", post(complete_transfer))
        .with_state(store)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse::ok(
        API_SERVICE_NAME,
        env!("CARGO_PKG_VERSION"),
    ))
}

fn err(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn refresh_expiry(session: &mut Session) {
    if matches!(
        session.status,
        PairingStatus::Pending | PairingStatus::Claimed
    ) && now_unix() > session.expires_at
    {
        session.status = PairingStatus::Expired;
    }
}

fn session_view(session: &Session) -> PairingView {
    PairingView {
        pairing_id: session.pairing_id.clone(),
        status: session.status,
        agent: session.agent.clone(),
        controller_id: session.controller_id.clone(),
        controller_name: session.controller_name.clone(),
        expires_at: session.expires_at,
    }
}

fn valid_device_info(info: &DeviceInfo) -> bool {
    let Some(pk) = hex_decode(&info.public_key) else {
        return false;
    };
    let Ok(pk): Result<[u8; 32], _> = pk.try_into() else {
        return false;
    };
    fingerprint(&pk) == info.device_id
}

async fn create_pairing(
    State(store): State<Shared>,
    Json(req): Json<CreatePairingRequest>,
) -> Response {
    if !valid_device_info(&req.agent) {
        return err(
            StatusCode::BAD_REQUEST,
            "device_id does not match public_key fingerprint",
        );
    }
    let mut rng = rand::thread_rng();
    let code = format!("{:06}", rng.gen_range(0..1_000_000));
    let id_bytes: [u8; 16] = rng.gen();
    let pairing_id = hex_encode(&id_bytes);
    let expires_at = now_unix() + PAIRING_TTL_SECS;
    let session = Session {
        pairing_id: pairing_id.clone(),
        code: code.clone(),
        agent: req.agent,
        controller_id: None,
        controller_name: None,
        status: PairingStatus::Pending,
        expires_at,
    };
    store
        .store
        .lock()
        .unwrap()
        .sessions
        .insert(pairing_id.clone(), session);
    (
        StatusCode::CREATED,
        Json(CreatePairingResponse {
            pairing_id,
            code,
            expires_at,
        }),
    )
        .into_response()
}

async fn claim_pairing(
    State(store): State<Shared>,
    Json(req): Json<ClaimPairingRequest>,
) -> Response {
    let mut store = store.store.lock().unwrap();
    // First pass: does any live session carry this code?
    let mut found: Option<String> = None;
    let mut conflict = false;
    for session in store.sessions.values_mut() {
        refresh_expiry(session);
        if session.code == req.code {
            match session.status {
                PairingStatus::Pending => found = Some(session.pairing_id.clone()),
                PairingStatus::Expired => {}
                _ => conflict = true,
            }
        }
    }
    if conflict {
        return err(StatusCode::CONFLICT, "pairing code already used");
    }
    let Some(id) = found else {
        return err(StatusCode::NOT_FOUND, "unknown or expired pairing code");
    };
    let session = store.sessions.get_mut(&id).unwrap();
    session.controller_id = Some(req.controller_id);
    session.controller_name = Some(req.controller_name);
    session.status = PairingStatus::Claimed;
    Json(session_view(session)).into_response()
}

async fn get_pairing(State(store): State<Shared>, Path(id): Path<String>) -> Response {
    let mut store = store.store.lock().unwrap();
    let Some(session) = store.sessions.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown pairing_id");
    };
    refresh_expiry(session);
    Json(session_view(session)).into_response()
}

fn decide(store: &Shared, id: &str, signature: &str, approve: bool) -> Response {
    let mut store = store.store.lock().unwrap();
    let Some(session) = store.sessions.get_mut(id) else {
        return err(StatusCode::NOT_FOUND, "unknown pairing_id");
    };
    refresh_expiry(session);
    if session.status == PairingStatus::Expired {
        return err(StatusCode::GONE, "pairing code expired");
    }
    if session.status != PairingStatus::Claimed {
        return err(StatusCode::CONFLICT, "pairing is not waiting for approval");
    }
    let message = if approve {
        abhiverse_protocol::pairing_approve_message(id, &session.code)
    } else {
        abhiverse_protocol::pairing_reject_message(id, &session.code)
    };
    if !verify(&session.agent.public_key, &message, signature) {
        return err(
            StatusCode::UNAUTHORIZED,
            "signature does not match agent key",
        );
    }
    session.status = if approve {
        PairingStatus::Approved
    } else {
        PairingStatus::Rejected
    };
    let view = session_view(session);
    if approve {
        let device = TrustedDevice {
            info: view.agent.clone(),
            paired_with_name: view.controller_name.clone().unwrap_or_default(),
            controller_id: view.controller_id.clone().unwrap_or_default(),
            last_seen: None,
            revoked: false,
        };
        store.devices.insert(device.info.device_id.clone(), device);
    }
    Json(view).into_response()
}

async fn approve_pairing(
    State(store): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<PairingDecisionRequest>,
) -> Response {
    decide(&store, &id, &req.signature, true)
}

async fn reject_pairing(
    State(store): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<PairingDecisionRequest>,
) -> Response {
    decide(&store, &id, &req.signature, false)
}

async fn list_devices(State(store): State<Shared>) -> Json<DeviceListResponse> {
    let store = store.store.lock().unwrap();
    let now = now_unix();
    let devices = store
        .devices
        .values()
        .filter(|d| !d.revoked)
        .map(|d| DeviceView {
            info: d.info.clone(),
            paired_with_name: d.paired_with_name.clone(),
            last_seen: d.last_seen,
            online: d
                .last_seen
                .is_some_and(|seen| now - seen <= ONLINE_WINDOW_SECS),
        })
        .collect();
    Json(DeviceListResponse { devices })
}

fn check_timestamp(timestamp: i64) -> bool {
    (now_unix() - timestamp).abs() <= MAX_CLOCK_SKEW_SECS
}

async fn heartbeat(
    State(store): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<HeartbeatRequest>,
) -> Response {
    let mut store = store.store.lock().unwrap();
    let Some(device) = store.devices.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown device");
    };
    if device.revoked {
        return err(StatusCode::FORBIDDEN, "device has been revoked");
    }
    if !check_timestamp(req.timestamp) {
        return err(
            StatusCode::BAD_REQUEST,
            "heartbeat timestamp outside allowed window",
        );
    }
    let message = abhiverse_protocol::heartbeat_message(&id, req.timestamp);
    if !verify(&device.info.public_key, &message, &req.signature) {
        return err(
            StatusCode::UNAUTHORIZED,
            "signature does not match device key",
        );
    }
    device.last_seen = Some(now_unix());
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

async fn revoke_device(
    State(store): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<RevokeRequest>,
) -> Response {
    let mut store = store.store.lock().unwrap();
    let Some(device) = store.devices.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown device");
    };
    if device.revoked {
        return err(StatusCode::NOT_FOUND, "unknown device");
    }
    if !check_timestamp(req.timestamp) {
        return err(
            StatusCode::BAD_REQUEST,
            "revoke timestamp outside allowed window",
        );
    }
    let message = abhiverse_protocol::revoke_message(&id, req.timestamp);
    if !verify(&device.info.public_key, &message, &req.signature) {
        return err(
            StatusCode::UNAUTHORIZED,
            "signature does not match device key",
        );
    }
    device.revoked = true;
    Json(serde_json::json!({ "status": "revoked" })).into_response()
}

/// Address the server binds to. Override with `ABHIVERSE_BIND_ADDR`;
/// defaults to localhost only — never `0.0.0.0` by default.
pub fn bind_addr() -> String {
    std::env::var("ABHIVERSE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string())
}

// ---------- Task 005: file transfer ----------

fn header(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Verify a signed request (`x-abhiverse-timestamp` +
/// `x-abhiverse-signature`). Returns `Some(error_response)` when the
/// signature is missing, stale or wrong; `None` when it checks out.
fn signature_error(headers: &HeaderMap, public_key: &str, message: &str) -> Option<Response> {
    let Some(ts) = header(headers, "x-abhiverse-timestamp").and_then(|v| v.parse::<i64>().ok())
    else {
        return Some(err(
            StatusCode::UNAUTHORIZED,
            "missing or invalid signature timestamp",
        ));
    };
    if !check_timestamp(ts) {
        return Some(err(
            StatusCode::BAD_REQUEST,
            "signature timestamp outside allowed window",
        ));
    }
    let Some(sig) = header(headers, "x-abhiverse-signature") else {
        return Some(err(StatusCode::UNAUTHORIZED, "missing signature"));
    };
    if !verify(public_key, message, &sig) {
        return Some(err(
            StatusCode::UNAUTHORIZED,
            "signature does not match device key",
        ));
    }
    None
}

fn transfer_view(id: &str, rec: &TransferRec) -> TransferView {
    TransferView {
        transfer_id: id.to_string(),
        from_id: rec.from_id.clone(),
        to_id: rec.to_id.clone(),
        file_name: rec.file_name.clone(),
        size_bytes: rec.size_bytes,
        sha256: rec.sha256.clone(),
        total_chunks: rec.total_chunks,
        received_chunks: rec.received.iter().copied().collect(),
        status: rec.status,
    }
}

async fn create_transfer(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<CreateTransferRequest>,
) -> Response {
    let Some(file_name) = abhiverse_file_transfer::sanitize_file_name(&req.file_name) else {
        return err(StatusCode::BAD_REQUEST, "unsafe file name");
    };
    if req.total_chunks != abhiverse_file_transfer::total_chunks(req.size_bytes) {
        return err(
            StatusCode::BAD_REQUEST,
            "total_chunks does not match size_bytes",
        );
    }
    if hex_decode(&req.sha256).map(|b| b.len()) != Some(32) {
        return err(StatusCode::BAD_REQUEST, "sha256 must be 64 hex characters");
    }
    let store = state.store.lock().unwrap();
    // Sender classification: a paired agent, or the paired controller
    // of the target agent.
    let from_is_agent = store.devices.get(&req.from_id).is_some_and(|d| !d.revoked);
    if from_is_agent {
        let device = store.devices.get(&req.from_id).unwrap();
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = transfer_create_message(&req.from_id, &req.to_id, &req.sha256, ts);
        if let Some(resp) = signature_error(&headers, &device.info.public_key, &message) {
            return resp;
        }
    } else {
        let is_paired_controller = store.devices.get(&req.to_id).is_some_and(|d| {
            !d.revoked && d.controller_id == req.from_id && !req.from_id.is_empty()
        });
        if !is_paired_controller {
            return err(
                StatusCode::FORBIDDEN,
                "sender is not a paired device or its controller",
            );
        }
    }
    drop(store);
    let mut rng = rand::thread_rng();
    let id_bytes: [u8; 16] = rng.gen();
    let transfer_id = hex_encode(&id_bytes);
    let rec = TransferRec {
        from_id: req.from_id,
        to_id: req.to_id,
        file_name,
        size_bytes: req.size_bytes,
        sha256: req.sha256,
        total_chunks: req.total_chunks,
        received: BTreeSet::new(),
        status: TransferStatus::Receiving,
        from_is_agent,
    };
    let view = transfer_view(&transfer_id, &rec);
    state
        .store
        .lock()
        .unwrap()
        .transfers
        .insert(transfer_id, rec);
    (StatusCode::CREATED, Json(view)).into_response()
}

#[derive(serde::Deserialize)]
struct ForQuery {
    #[serde(rename = "for")]
    recipient: Option<String>,
}

async fn list_transfers(
    State(state): State<Shared>,
    Query(q): Query<ForQuery>,
) -> Json<TransferListResponse> {
    let store = state.store.lock().unwrap();
    let transfers = store
        .transfers
        .iter()
        .filter(|(_, rec)| q.recipient.as_deref().is_none_or(|r| r == rec.to_id))
        .map(|(id, rec)| transfer_view(id, rec))
        .collect();
    Json(TransferListResponse { transfers })
}

async fn get_transfer(State(state): State<Shared>, Path(id): Path<String>) -> Response {
    let store = state.store.lock().unwrap();
    match store.transfers.get(&id) {
        Some(rec) => Json(transfer_view(&id, rec)).into_response(),
        None => err(StatusCode::NOT_FOUND, "unknown transfer"),
    }
}

async fn upload_chunk(
    State(state): State<Shared>,
    Path((id, index)): Path<(String, u32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let from_pk = {
        let store = state.store.lock().unwrap();
        let Some(rec) = store.transfers.get(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown transfer");
        };
        if rec.status != TransferStatus::Receiving {
            return err(StatusCode::CONFLICT, "transfer is not receiving chunks");
        }
        if index >= rec.total_chunks {
            return err(StatusCode::BAD_REQUEST, "chunk index out of range");
        }
        if body.len() as u64 != abhiverse_file_transfer::chunk_len(rec.size_bytes, index) {
            return err(StatusCode::BAD_REQUEST, "chunk has wrong length");
        }
        if rec.from_is_agent {
            store
                .devices
                .get(&rec.from_id)
                .map(|d| d.info.public_key.clone())
        } else {
            None
        }
    };
    if let Some(pk) = from_pk {
        let chunk_hash = abhiverse_file_transfer::sha256_hex(&body);
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = chunk_upload_message(&id, index, &chunk_hash, ts);
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
        }
    }
    if let Err(e) = abhiverse_file_transfer::write_chunk(&state.chunk_dir, &id, index, &body) {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not store chunk: {e}"),
        );
    }
    let mut store = state.store.lock().unwrap();
    let rec = store.transfers.get_mut(&id).unwrap();
    rec.received.insert(index);
    Json(serde_json::json!({ "received": rec.received.len(), "total": rec.total_chunks }))
        .into_response()
}

async fn download_chunk(
    State(state): State<Shared>,
    Path((id, index)): Path<(String, u32)>,
    headers: HeaderMap,
) -> Response {
    let (rec, recipient_pk) = {
        let store = state.store.lock().unwrap();
        let Some(rec) = store.transfers.get(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown transfer");
        };
        if !rec.received.contains(&index) {
            return err(StatusCode::NOT_FOUND, "chunk not uploaded yet");
        }
        let pk = store
            .devices
            .get(&rec.to_id)
            .map(|d| d.info.public_key.clone());
        (rec.clone(), pk)
    };
    // Agent recipients must sign; controller recipients use the
    // (unguessable) transfer ID as the capability in v0.3.
    if let Some(pk) = recipient_pk {
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = chunk_download_message(&id, index, ts);
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
        }
    }
    let len = abhiverse_file_transfer::chunk_len(rec.size_bytes, index) as usize;
    let path = abhiverse_file_transfer::part_path(&state.chunk_dir, &id);
    let read = (|| -> std::io::Result<Vec<u8>> {
        let mut f = std::fs::File::open(&path)?;
        f.seek(SeekFrom::Start(
            index as u64 * abhiverse_file_transfer::CHUNK_SIZE,
        ))?;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf)?;
        Ok(buf)
    })();
    match read {
        Ok(bytes) => (StatusCode::OK, bytes).into_response(),
        Err(e) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not read chunk: {e}"),
        ),
    }
}

async fn complete_transfer(
    State(state): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (rec, from_pk) = {
        let store = state.store.lock().unwrap();
        let Some(rec) = store.transfers.get(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown transfer");
        };
        let pk = if rec.from_is_agent {
            store
                .devices
                .get(&rec.from_id)
                .map(|d| d.info.public_key.clone())
        } else {
            None
        };
        (rec.clone(), pk)
    };
    if let Some(pk) = from_pk {
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = transfer_complete_message(&id, ts);
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
        }
    }
    if rec.received.len() as u32 != rec.total_chunks {
        return err(StatusCode::CONFLICT, "not all chunks have been uploaded");
    }
    let actual = if rec.total_chunks == 0 {
        abhiverse_file_transfer::sha256_hex(&[])
    } else {
        let path = abhiverse_file_transfer::part_path(&state.chunk_dir, &id);
        match abhiverse_file_transfer::sha256_file(&path) {
            Ok(h) => h,
            Err(e) => {
                return err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("could not hash assembled file: {e}"),
                )
            }
        }
    };
    let mut store = state.store.lock().unwrap();
    let rec_mut = store.transfers.get_mut(&id).unwrap();
    if actual == rec.sha256 {
        rec_mut.status = TransferStatus::Complete;
        Json(transfer_view(&id, rec_mut)).into_response()
    } else {
        rec_mut.status = TransferStatus::Failed;
        err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "assembled file hash does not match declared sha256",
        )
    }
}
