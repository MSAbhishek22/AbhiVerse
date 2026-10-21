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
    chunk_download_message, chunk_upload_message, now_unix, session_answer_message,
    session_end_message, session_reject_message, transfer_complete_message,
    transfer_create_message, AnswerSessionRequest, ClaimPairingRequest, CreatePairingRequest,
    CreatePairingResponse, CreateSessionRequest, CreateTransferRequest, DeviceInfo,
    DeviceListResponse, DeviceView, EndSessionRequest, HealthResponse, HeartbeatRequest,
    IceCandidateListResponse, IceCandidateRequest, IceCandidateView, PairingDecisionRequest,
    PairingStatus, PairingView, RevokeRequest, RtcConfig, SessionListResponse, SessionStatus,
    SessionView, TransferListResponse, TransferStatus, TransferView, API_SERVICE_NAME,
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
use serde::{Deserialize, Serialize};

const PAIRING_TTL_SECS: i64 = 300;
const ONLINE_WINDOW_SECS: i64 = 30;
const MAX_CLOCK_SKEW_SECS: i64 = 120;

#[derive(Clone)]
struct Session {
    pairing_id: String,
    code: String,
    agent: DeviceInfo,
    controller: Option<DeviceInfo>,
    status: PairingStatus,
    expires_at: i64,
}

#[derive(Clone)]
struct TrustedDevice {
    info: DeviceInfo,
    paired_with_name: String,
    controller_id: String,
    /// The paired controller's public identity (Task 007): its key
    /// verifies controller-originated requests.
    controller_info: DeviceInfo,
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
    rd_sessions: HashMap<String, RdSession>,
    /// When set, mutations snapshot the store to this JSON file.
    persist_path: Option<PathBuf>,
}

// ---- Persistence (Task 008) ----
// Pairings and transfers survive API restarts when ABHIVERSE_STATE_FILE
// points at a JSON snapshot. Pairing *sessions* and remote-desktop
// sessions stay ephemeral on purpose (short-lived by design).

#[derive(Serialize, Deserialize)]
struct PersistedDevice {
    info: DeviceInfo,
    paired_with_name: String,
    controller_id: String,
    controller_info: DeviceInfo,
    last_seen: Option<i64>,
    revoked: bool,
}

#[derive(Serialize, Deserialize)]
struct PersistedTransfer {
    from_id: String,
    to_id: String,
    file_name: String,
    size_bytes: u64,
    sha256: String,
    total_chunks: u32,
    received: Vec<u32>,
    status: TransferStatus,
    from_is_agent: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Snapshot {
    devices: Vec<PersistedDevice>,
    transfers: HashMap<String, PersistedTransfer>,
}

impl Store {
    /// Snapshot to the persist path, if one is configured.
    fn persist(&self) {
        if let Some(path) = &self.persist_path {
            self.save(path);
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            devices: self
                .devices
                .values()
                .map(|d| PersistedDevice {
                    info: d.info.clone(),
                    paired_with_name: d.paired_with_name.clone(),
                    controller_id: d.controller_id.clone(),
                    controller_info: d.controller_info.clone(),
                    last_seen: d.last_seen,
                    revoked: d.revoked,
                })
                .collect(),
            transfers: self
                .transfers
                .iter()
                .map(|(id, r)| {
                    (
                        id.clone(),
                        PersistedTransfer {
                            from_id: r.from_id.clone(),
                            to_id: r.to_id.clone(),
                            file_name: r.file_name.clone(),
                            size_bytes: r.size_bytes,
                            sha256: r.sha256.clone(),
                            total_chunks: r.total_chunks,
                            received: r.received.iter().copied().collect(),
                            status: r.status,
                            from_is_agent: r.from_is_agent,
                        },
                    )
                })
                .collect(),
        }
    }

    fn save(&self, path: &std::path::Path) {
        let Ok(json) = serde_json::to_string(&self.snapshot()) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    fn load(path: &std::path::Path) -> Store {
        let mut store = Store {
            persist_path: Some(path.to_path_buf()),
            ..Store::default()
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return store;
        };
        let Ok(snap) = serde_json::from_str::<Snapshot>(&text) else {
            return store;
        };
        for d in snap.devices {
            store.devices.insert(
                d.info.device_id.clone(),
                TrustedDevice {
                    info: d.info,
                    paired_with_name: d.paired_with_name,
                    controller_id: d.controller_id,
                    controller_info: d.controller_info,
                    last_seen: d.last_seen,
                    revoked: d.revoked,
                },
            );
        }
        for (id, r) in snap.transfers {
            store.transfers.insert(
                id,
                TransferRec {
                    from_id: r.from_id,
                    to_id: r.to_id,
                    file_name: r.file_name,
                    size_bytes: r.size_bytes,
                    sha256: r.sha256,
                    total_chunks: r.total_chunks,
                    received: r.received.into_iter().collect(),
                    status: r.status,
                    from_is_agent: r.from_is_agent,
                },
            );
        }
        store
    }
}

#[derive(Clone)]
struct RdSession {
    device_id: String,
    controller_id: String,
    status: SessionStatus,
    sdp_offer: String,
    sdp_answer: Option<String>,
    created_at: i64,
    expires_at: i64,
    ice: Vec<IceCandidateView>,
    next_seq: u64,
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
    let state_file = std::env::var("ABHIVERSE_STATE_FILE")
        .ok()
        .map(PathBuf::from);
    build_app_with_paths(state_file, transfer_dir())
}

/// Build the router with explicit storage paths (used by tests and
/// by `build_app`, which reads `ABHIVERSE_STATE_FILE` /
/// `ABHIVERSE_TRANSFER_DIR`).
pub fn build_app_with_paths(state_file: Option<PathBuf>, chunk_dir: PathBuf) -> Router {
    let mut initial = state_file.as_deref().map(Store::load).unwrap_or_default();
    if initial.persist_path.is_none() {
        initial.persist_path = state_file.clone();
    }
    let store: Shared = Arc::new(AppState {
        store: Mutex::new(initial),
        chunk_dir,
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
        .route("/config/rtc", get(rtc_config))
        .route("/sessions", post(create_session).get(list_sessions))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/answer", post(answer_session))
        .route("/sessions/{id}/reject", post(reject_session))
        .route("/sessions/{id}/ice", post(post_ice).get(get_ice))
        .route("/sessions/{id}/end", post(end_session))
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

fn session_controller(store: &Store, pairing_id: &str) -> Option<DeviceInfo> {
    store
        .sessions
        .get(pairing_id)
        .and_then(|s| s.controller.clone())
}

fn session_view(session: &Session) -> PairingView {
    PairingView {
        pairing_id: session.pairing_id.clone(),
        status: session.status,
        agent: session.agent.clone(),
        controller_id: session.controller.as_ref().map(|c| c.device_id.clone()),
        controller_name: session.controller.as_ref().map(|c| c.name.clone()),
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
        controller: None,
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
    if !valid_device_info(&req.controller) {
        return err(
            StatusCode::BAD_REQUEST,
            "controller device_id does not match its public key fingerprint",
        );
    }
    let session = store.sessions.get_mut(&id).unwrap();
    session.controller = Some(req.controller);
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
        let Some(controller_info) = session_controller(&store, &view.pairing_id) else {
            return err(StatusCode::CONFLICT, "pairing has no controller identity");
        };
        let device = TrustedDevice {
            info: view.agent.clone(),
            paired_with_name: view.controller_name.clone().unwrap_or_default(),
            controller_id: view.controller_id.clone().unwrap_or_default(),
            controller_info,
            last_seen: None,
            revoked: false,
        };
        store.devices.insert(device.info.device_id.clone(), device);
        store.persist();
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
    // Either the device itself, or its paired controller, may revoke.
    let by_device = verify(&device.info.public_key, &message, &req.signature);
    let by_controller =
        !by_device && verify(&device.controller_info.public_key, &message, &req.signature);
    if !by_device && !by_controller {
        return err(
            StatusCode::UNAUTHORIZED,
            "signature does not match device or paired controller key",
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
    // Sender classification: a paired agent (sending to its paired
    // controller), or the paired controller of the target agent.
    // Both must sign the create request with their device key.
    let from_is_agent = store.devices.get(&req.from_id).is_some_and(|d| !d.revoked);
    let ts = header(&headers, "x-abhiverse-timestamp")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let message = transfer_create_message(&req.from_id, &req.to_id, &req.sha256, ts);
    if from_is_agent {
        let device = store.devices.get(&req.from_id).unwrap();
        if device.controller_id != req.to_id {
            return err(
                StatusCode::FORBIDDEN,
                "agents can only send to their paired controller",
            );
        }
        if let Some(resp) = signature_error(&headers, &device.info.public_key, &message) {
            return resp;
        }
    } else {
        let Some(target) = store.devices.get(&req.to_id) else {
            return err(
                StatusCode::FORBIDDEN,
                "sender is not a paired device or its controller",
            );
        };
        if target.revoked || target.controller_info.device_id != req.from_id {
            return err(
                StatusCode::FORBIDDEN,
                "sender is not the paired controller of the target",
            );
        }
        let pk = target.controller_info.public_key.clone();
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
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
    {
        let mut store = state.store.lock().unwrap();
        store.transfers.insert(transfer_id, rec);
        store.persist();
    }
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
    let (received, total) = {
        let rec = store.transfers.get_mut(&id).unwrap();
        rec.received.insert(index);
        (rec.received.len(), rec.total_chunks)
    };
    store.persist();
    Json(serde_json::json!({ "received": received, "total": total })).into_response()
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
    let view = {
        let rec_mut = store.transfers.get_mut(&id).unwrap();
        if actual == rec.sha256 {
            rec_mut.status = TransferStatus::Complete;
            Some(transfer_view(&id, rec_mut))
        } else {
            rec_mut.status = TransferStatus::Failed;
            None
        }
    };
    store.persist();
    match view {
        Some(view) => Json(view).into_response(),
        None => err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "assembled file hash does not match declared sha256",
        ),
    }
}

// ---------- Task 006: remote desktop signalling ----------

const SESSION_TTL_SECS: i64 = 120;

async fn rtc_config() -> Json<RtcConfig> {
    if let Ok(raw) = std::env::var("ABHIVERSE_ICE_SERVERS_JSON") {
        if let Ok(servers) = serde_json::from_str::<Vec<abhiverse_protocol::IceServerConfig>>(&raw)
        {
            return Json(RtcConfig {
                ice_servers: servers,
            });
        }
    }
    Json(RtcConfig::default_stun())
}

fn rd_view(id: &str, rec: &RdSession) -> SessionView {
    SessionView {
        session_id: id.to_string(),
        device_id: rec.device_id.clone(),
        controller_id: rec.controller_id.clone(),
        status: rec.status,
        sdp_offer: rec.sdp_offer.clone(),
        sdp_answer: rec.sdp_answer.clone(),
        created_at: rec.created_at,
        expires_at: rec.expires_at,
    }
}

fn refresh_rd_expiry(rec: &mut RdSession) {
    if rec.status == SessionStatus::Offered && now_unix() > rec.expires_at {
        rec.status = SessionStatus::Expired;
    }
}

async fn create_session(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<CreateSessionRequest>,
) -> Response {
    if req.sdp_offer.is_empty() || req.sdp_offer.len() > 100_000 {
        return err(StatusCode::BAD_REQUEST, "sdp_offer missing or too large");
    }
    {
        let store = state.store.lock().unwrap();
        let Some(device) = store.devices.get(&req.device_id) else {
            return err(
                StatusCode::FORBIDDEN,
                "only the paired controller can request a session",
            );
        };
        if device.revoked || device.controller_info.device_id != req.controller_id {
            return err(
                StatusCode::FORBIDDEN,
                "only the paired controller can request a session",
            );
        }
        // The controller signs its session request (Task 007).
        let offer_hash = abhiverse_file_transfer::sha256_hex(req.sdp_offer.as_bytes());
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = abhiverse_protocol::session_create_message(
            &req.device_id,
            &req.controller_id,
            &offer_hash,
            ts,
        );
        let pk = device.controller_info.public_key.clone();
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
        }
    }
    let mut rng = rand::thread_rng();
    let id_bytes: [u8; 16] = rng.gen();
    let session_id = hex_encode(&id_bytes);
    let now = now_unix();
    let rec = RdSession {
        device_id: req.device_id,
        controller_id: req.controller_id,
        status: SessionStatus::Offered,
        sdp_offer: req.sdp_offer,
        sdp_answer: None,
        created_at: now,
        expires_at: now + SESSION_TTL_SECS,
        ice: Vec::new(),
        next_seq: 1,
    };
    let view = rd_view(&session_id, &rec);
    state
        .store
        .lock()
        .unwrap()
        .rd_sessions
        .insert(session_id, rec);
    (StatusCode::CREATED, Json(view)).into_response()
}

#[derive(serde::Deserialize)]
struct ParticipantQuery {
    #[serde(rename = "as")]
    participant: Option<String>,
    after: Option<u64>,
    #[serde(rename = "for")]
    recipient: Option<String>,
}

async fn list_sessions(
    State(state): State<Shared>,
    Query(q): Query<ParticipantQuery>,
) -> Json<SessionListResponse> {
    let mut store = state.store.lock().unwrap();
    let sessions = store
        .rd_sessions
        .iter_mut()
        .filter(|(_, rec)| q.recipient.as_deref().is_none_or(|r| r == rec.device_id))
        .map(|(id, rec)| {
            refresh_rd_expiry(rec);
            rd_view(id, rec)
        })
        .collect();
    Json(SessionListResponse { sessions })
}

async fn get_session(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<ParticipantQuery>,
) -> Response {
    let mut store = state.store.lock().unwrap();
    let Some(rec) = store.rd_sessions.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown session");
    };
    refresh_rd_expiry(rec);
    let participant = q.participant.as_deref().unwrap_or("");
    if participant != rec.device_id && participant != rec.controller_id {
        return err(StatusCode::FORBIDDEN, "not a participant of this session");
    }
    Json(rd_view(&id, rec)).into_response()
}

async fn answer_session(
    State(state): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<AnswerSessionRequest>,
) -> Response {
    let device_pk = {
        let mut store = state.store.lock().unwrap();
        let Some(rec) = store.rd_sessions.get_mut(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown session");
        };
        refresh_rd_expiry(rec);
        let device_id = rec.device_id.clone();
        let pk = store
            .devices
            .get(&device_id)
            .map(|d| d.info.public_key.clone());
        pk
    };
    let Some(pk) = device_pk else {
        return err(StatusCode::NOT_FOUND, "session device no longer exists");
    };
    let ts = header(&headers, "x-abhiverse-timestamp")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let answer_hash = abhiverse_file_transfer::sha256_hex(req.sdp_answer.as_bytes());
    let message = session_answer_message(&id, &answer_hash, ts);
    if let Some(resp) = signature_error(&headers, &pk, &message) {
        return resp;
    }
    let mut store = state.store.lock().unwrap();
    let rec = store.rd_sessions.get_mut(&id).unwrap();
    match rec.status {
        SessionStatus::Expired => return err(StatusCode::GONE, "session offer expired"),
        SessionStatus::Offered => {}
        _ => return err(StatusCode::CONFLICT, "session is not waiting for an answer"),
    }
    rec.sdp_answer = Some(req.sdp_answer);
    rec.status = SessionStatus::Answered;
    Json(rd_view(&id, rec)).into_response()
}

async fn reject_session(
    State(state): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let device_pk = {
        let store = state.store.lock().unwrap();
        let Some(rec) = store.rd_sessions.get(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown session");
        };
        store
            .devices
            .get(&rec.device_id)
            .map(|d| d.info.public_key.clone())
    };
    let Some(pk) = device_pk else {
        return err(StatusCode::NOT_FOUND, "session device no longer exists");
    };
    let ts = header(&headers, "x-abhiverse-timestamp")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    let message = session_reject_message(&id, ts);
    if let Some(resp) = signature_error(&headers, &pk, &message) {
        return resp;
    }
    let mut store = state.store.lock().unwrap();
    let rec = store.rd_sessions.get_mut(&id).unwrap();
    refresh_rd_expiry(rec);
    if rec.status != SessionStatus::Offered {
        return err(StatusCode::CONFLICT, "session is not waiting for an answer");
    }
    rec.status = SessionStatus::Rejected;
    Json(rd_view(&id, rec)).into_response()
}

async fn post_ice(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Json(req): Json<IceCandidateRequest>,
) -> Response {
    let mut store = state.store.lock().unwrap();
    let Some(rec) = store.rd_sessions.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown session");
    };
    refresh_rd_expiry(rec);
    if req.from_id != rec.device_id && req.from_id != rec.controller_id {
        return err(StatusCode::FORBIDDEN, "not a participant of this session");
    }
    if matches!(
        rec.status,
        SessionStatus::Ended | SessionStatus::Rejected | SessionStatus::Expired
    ) {
        return err(StatusCode::CONFLICT, "session is closed");
    }
    let seq = rec.next_seq;
    rec.next_seq += 1;
    rec.ice.push(IceCandidateView {
        seq,
        from_id: req.from_id,
        candidate: req.candidate,
        sdp_mid: req.sdp_mid,
        sdp_mline_index: req.sdp_mline_index,
    });
    Json(serde_json::json!({ "seq": seq })).into_response()
}

async fn get_ice(
    State(state): State<Shared>,
    Path(id): Path<String>,
    Query(q): Query<ParticipantQuery>,
) -> Response {
    let store = state.store.lock().unwrap();
    let Some(rec) = store.rd_sessions.get(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown session");
    };
    let participant = q.participant.as_deref().unwrap_or("");
    if participant != rec.device_id && participant != rec.controller_id {
        return err(StatusCode::FORBIDDEN, "not a participant of this session");
    }
    let after = q.after.unwrap_or(0);
    let candidates = rec
        .ice
        .iter()
        .filter(|c| c.from_id != participant && c.seq > after)
        .cloned()
        .collect();
    Json(IceCandidateListResponse { candidates }).into_response()
}

async fn end_session(
    State(state): State<Shared>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<EndSessionRequest>,
) -> Response {
    let (device_id, controller_id, device_pk) = {
        let store = state.store.lock().unwrap();
        let Some(rec) = store.rd_sessions.get(&id) else {
            return err(StatusCode::NOT_FOUND, "unknown session");
        };
        let pk = store
            .devices
            .get(&rec.device_id)
            .map(|d| d.info.public_key.clone());
        (rec.device_id.clone(), rec.controller_id.clone(), pk)
    };
    if req.from_id != device_id && req.from_id != controller_id {
        return err(StatusCode::FORBIDDEN, "not a participant of this session");
    }
    if req.from_id == device_id {
        let Some(pk) = device_pk else {
            return err(StatusCode::NOT_FOUND, "session device no longer exists");
        };
        let ts = header(&headers, "x-abhiverse-timestamp")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);
        let message = session_end_message(&id, ts);
        if let Some(resp) = signature_error(&headers, &pk, &message) {
            return resp;
        }
    }
    let mut store = state.store.lock().unwrap();
    let rec = store.rd_sessions.get_mut(&id).unwrap();
    rec.status = SessionStatus::Ended;
    Json(rd_view(&id, rec)).into_response()
}
