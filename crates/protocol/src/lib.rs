//! Shared protocol types for ABHIVERSE.
//!
//! This crate is the single source of truth for payloads exchanged
//! between the controller, the coordination API and device agents.
//! Keep it free of platform-specific and transport-specific code.

use serde::{Deserialize, Serialize};

/// Human-facing product name.
pub const APP_NAME: &str = "ABHIVERSE";
/// Machine name of the coordination API service.
pub const API_SERVICE_NAME: &str = "abhiverse-api";
/// Machine name of the Windows agent service.
pub const AGENT_SERVICE_NAME: &str = "abhiverse-agent-windows";
/// Protocol version. Bump on breaking payload changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Response body for `GET /health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Product name, e.g. "ABHIVERSE".
    pub app: String,
    /// Service that answered, e.g. "abhiverse-api".
    pub service: String,
    /// Service status, currently always "ok" when reachable.
    pub status: String,
    /// Service crate version (semver).
    pub version: String,
    /// Wire protocol version implemented by the service.
    pub protocol_version: u32,
}

impl HealthResponse {
    pub fn ok(service: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            app: APP_NAME.to_string(),
            service: service.into(),
            status: "ok".to_string(),
            version: version.into(),
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

/// Basic identity/status payload reported by a device agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub app: String,
    pub service: String,
    pub version: String,
    pub protocol_version: u32,
    pub platform: String,
}

impl AgentInfo {
    pub fn current(version: impl Into<String>) -> Self {
        Self {
            app: APP_NAME.to_string(),
            service: AGENT_SERVICE_NAME.to_string(),
            version: version.into(),
            protocol_version: PROTOCOL_VERSION,
            platform: std::env::consts::OS.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_round_trips_as_json() {
        let health = HealthResponse::ok(API_SERVICE_NAME, "0.1.0");
        let json = serde_json::to_string(&health).unwrap();
        let parsed: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, health);
        assert_eq!(parsed.status, "ok");
        assert_eq!(parsed.app, "ABHIVERSE");
    }

    #[test]
    fn input_event_round_trips_as_tagged_json() {
        let event = InputEvent::MouseMove { x: 0.5, y: 0.25 };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, "{\"type\":\"mouse_move\",\"x\":0.5,\"y\":0.25}");
        let back: InputEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
        let key: InputEvent =
            serde_json::from_str("{\"type\":\"key_down\",\"code\":\"KeyA\"}").unwrap();
        assert_eq!(
            key,
            InputEvent::KeyDown {
                code: "KeyA".to_string()
            }
        );
    }

    #[test]
    fn agent_info_reports_current_platform() {
        let info = AgentInfo::current("0.1.0");
        assert_eq!(info.service, AGENT_SERVICE_NAME);
        assert!(!info.platform.is_empty());
    }
}
// ---------- Task 002: device identity ----------

/// What a device is in the ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Agent,
    Controller,
}

/// Public identity of a device. The private key never leaves the device
/// and is never part of this payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Fingerprint of the public key (first 16 bytes of SHA-256, hex).
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub kind: DeviceKind,
    /// Ed25519 public key, hex-encoded (64 chars).
    pub public_key: String,
}

// ---------- Task 003: pairing ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    Pending,
    Claimed,
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePairingRequest {
    pub agent: DeviceInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePairingResponse {
    pub pairing_id: String,
    /// 6-digit code. Short-lived and single-use; it bootstraps trust,
    /// it is never a credential by itself.
    pub code: String,
    /// Unix seconds when the code stops working.
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimPairingRequest {
    pub code: String,
    pub controller_id: String,
    pub controller_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingView {
    pub pairing_id: String,
    pub status: PairingStatus,
    pub agent: DeviceInfo,
    pub controller_id: Option<String>,
    pub controller_name: Option<String>,
    pub expires_at: i64,
}

/// Signed decision sent by the agent being paired. The signature
/// proves possession of the agent's private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingDecisionRequest {
    /// Hex Ed25519 signature over `pairing_approve_message` or
    /// `pairing_reject_message`.
    pub signature: String,
}

pub fn pairing_approve_message(pairing_id: &str, code: &str) -> String {
    format!("abhiverse-pairing-approve:{pairing_id}:{code}")
}

pub fn pairing_reject_message(pairing_id: &str, code: &str) -> String {
    format!("abhiverse-pairing-reject:{pairing_id}:{code}")
}

// ---------- Task 004: presence & revocation ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub timestamp: i64,
    /// Hex Ed25519 signature over `heartbeat_message`.
    pub signature: String,
}

pub fn heartbeat_message(device_id: &str, timestamp: i64) -> String {
    format!("abhiverse-heartbeat:{device_id}:{timestamp}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeRequest {
    pub timestamp: i64,
    /// Hex Ed25519 signature over `revoke_message`, made by the
    /// device being revoked (self-revocation).
    pub signature: String,
}

pub fn revoke_message(device_id: &str, timestamp: i64) -> String {
    format!("abhiverse-revoke:{device_id}:{timestamp}")
}

/// A paired device as shown to controllers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    pub info: DeviceInfo,
    pub paired_with_name: String,
    /// Unix seconds of the last accepted heartbeat, if any.
    pub last_seen: Option<i64>,
    /// True when the last heartbeat is fresher than the server's
    /// online window (30s).
    pub online: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceListResponse {
    pub devices: Vec<DeviceView>,
}

/// Current unix time in seconds.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
// ---------- Task 005: file transfer ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    /// Created; chunks are being uploaded.
    Receiving,
    /// All chunks arrived and the server verified the file hash.
    Complete,
    /// Hash verification failed at completion.
    Failed,
}

/// Create a transfer. For agent senders this must be accompanied by
/// the signature headers; controller senders are unauthenticated in
/// v0.3 (controller keys are a documented future task).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTransferRequest {
    pub from_id: String,
    pub to_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    /// SHA-256 of the whole file, hex. The end-to-end integrity anchor.
    pub sha256: String,
    pub total_chunks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferView {
    pub transfer_id: String,
    pub from_id: String,
    pub to_id: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub total_chunks: u32,
    /// Indexes the server currently holds — the resume source of truth.
    pub received_chunks: Vec<u32>,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferListResponse {
    pub transfers: Vec<TransferView>,
}

pub fn transfer_create_message(from_id: &str, to_id: &str, sha256: &str, timestamp: i64) -> String {
    format!("abhiverse-transfer-create:{from_id}:{to_id}:{sha256}:{timestamp}")
}

pub fn chunk_upload_message(
    transfer_id: &str,
    index: u32,
    chunk_sha256: &str,
    timestamp: i64,
) -> String {
    format!("abhiverse-chunk:{transfer_id}:{index}:{chunk_sha256}:{timestamp}")
}

pub fn chunk_download_message(transfer_id: &str, index: u32, timestamp: i64) -> String {
    format!("abhiverse-download:{transfer_id}:{index}:{timestamp}")
}

pub fn transfer_complete_message(transfer_id: &str, timestamp: i64) -> String {
    format!("abhiverse-transfer-complete:{transfer_id}:{timestamp}")
}

// ---------- Task 006: remote desktop signalling ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Offered,
    Answered,
    Rejected,
    Ended,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub device_id: String,
    pub controller_id: String,
    pub sdp_offer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionView {
    pub session_id: String,
    pub device_id: String,
    pub controller_id: String,
    pub status: SessionStatus,
    pub sdp_offer: String,
    pub sdp_answer: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSessionRequest {
    pub sdp_answer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceCandidateRequest {
    pub from_id: String,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceCandidateView {
    pub seq: u64,
    pub from_id: String,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceCandidateListResponse {
    pub candidates: Vec<IceCandidateView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndSessionRequest {
    pub from_id: String,
}

pub fn session_answer_message(session_id: &str, answer_sha256: &str, timestamp: i64) -> String {
    format!("abhiverse-session-answer:{session_id}:{answer_sha256}:{timestamp}")
}

pub fn session_reject_message(session_id: &str, timestamp: i64) -> String {
    format!("abhiverse-session-reject:{session_id}:{timestamp}")
}

pub fn session_end_message(session_id: &str, timestamp: i64) -> String {
    format!("abhiverse-session-end:{session_id}:{timestamp}")
}

// ---------- RTC configuration ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtcConfig {
    pub ice_servers: Vec<IceServerConfig>,
}

impl RtcConfig {
    /// Default: a public STUN server only (no relay). Operators add
    /// TURN via `ABHIVERSE_ICE_SERVERS_JSON` on the API.
    pub fn default_stun() -> Self {
        Self {
            ice_servers: vec![IceServerConfig {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                username: None,
                credential: None,
            }],
        }
    }
}

// ---------- Input events (data channel) ----------

/// Label of the WebRTC data channel that carries input events.
pub const INPUT_CHANNEL_LABEL: &str = "abhiverse-input";

/// Input events sent by the controller on the input data channel.
/// Coordinates are normalised to 0.0..=1.0 of the remote frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputEvent {
    MouseMove { x: f64, y: f64 },
    MouseDown { button: u8 },
    MouseUp { button: u8 },
    Scroll { dx: f64, dy: f64 },
    KeyDown { code: String },
    KeyUp { code: String },
    Text { text: String },
}
