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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use abhiverse_identity::{fingerprint, hex_decode, hex_encode, verify};
use abhiverse_protocol::{
    now_unix, ClaimPairingRequest, CreatePairingRequest, CreatePairingResponse, DeviceInfo,
    DeviceListResponse, DeviceView, HealthResponse, HeartbeatRequest, PairingDecisionRequest,
    PairingStatus, PairingView, RevokeRequest, API_SERVICE_NAME,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
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
    last_seen: Option<i64>,
    revoked: bool,
}

#[derive(Default)]
struct Store {
    sessions: HashMap<String, Session>,
    devices: HashMap<String, TrustedDevice>,
}

type Shared = Arc<Mutex<Store>>;

/// Build the HTTP router (fresh in-memory state). Extracted from
/// `main` so integration tests can exercise it without a socket.
pub fn build_app() -> Router {
    let store: Shared = Arc::new(Mutex::new(Store::default()));
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
    let mut store = store.lock().unwrap();
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
    let mut store = store.lock().unwrap();
    let Some(session) = store.sessions.get_mut(&id) else {
        return err(StatusCode::NOT_FOUND, "unknown pairing_id");
    };
    refresh_expiry(session);
    Json(session_view(session)).into_response()
}

fn decide(store: &Shared, id: &str, signature: &str, approve: bool) -> Response {
    let mut store = store.lock().unwrap();
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
    let store = store.lock().unwrap();
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
    let mut store = store.lock().unwrap();
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
    let mut store = store.lock().unwrap();
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
