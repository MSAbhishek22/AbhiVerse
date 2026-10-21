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
    fn agent_info_reports_current_platform() {
        let info = AgentInfo::current("0.1.0");
        assert_eq!(info.service, AGENT_SERVICE_NAME);
        assert!(!info.platform.is_empty());
    }
}
