//! Abhiverse Windows agent library.
//!
//! Task 001: version reporting.
//! Task 002: loads (or creates) the device identity.
//! Task 003/004: pairing record storage used by the pair flow and
//! the heartbeat loop.
//!
//! Still no screen capture, input injection, shell or power
//! controls — those land in later, explicitly permissioned phases.

use std::path::{Path, PathBuf};

use abhiverse_identity::DeviceIdentity;
use abhiverse_protocol::{AgentInfo, DeviceKind};
use serde::{Deserialize, Serialize};

pub fn agent_info() -> AgentInfo {
    AgentInfo::current(env!("CARGO_PKG_VERSION"))
}

/// Human name for this device: hostname when available.
pub fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "My Computer".to_string())
}

/// Load this device's identity, creating a key pair on first run.
pub fn load_identity() -> Result<DeviceIdentity, Box<dyn std::error::Error>> {
    let dir = abhiverse_identity::default_data_dir();
    abhiverse_identity::ensure_identity(&dir, &device_name(), DeviceKind::Agent)
}

/// Record of a completed pairing, stored next to the key pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRecord {
    pub api_url: String,
    pub pairing_id: String,
    pub controller_name: String,
    #[serde(default)]
    pub controller_id: String,
}

pub fn pairing_path(data_dir: &Path) -> PathBuf {
    data_dir.join("pairing.json")
}

pub fn load_pairing(data_dir: &Path) -> Option<PairingRecord> {
    let text = std::fs::read_to_string(pairing_path(data_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_pairing(data_dir: &Path, record: &PairingRecord) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(
        pairing_path(data_dir),
        serde_json::to_string_pretty(record)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_matches_package_version() {
        let info = agent_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.app, "ABHIVERSE");
    }

    #[test]
    fn pairing_record_round_trips() {
        let dir = std::env::temp_dir().join(format!("abhiverse-agent-test-{}", std::process::id()));
        let record = PairingRecord {
            api_url: "http://127.0.0.1:8080".into(),
            pairing_id: "abc".into(),
            controller_name: "Pixel".into(),
            controller_id: "ctrl-1".into(),
        };
        save_pairing(&dir, &record).unwrap();
        assert_eq!(load_pairing(&dir), Some(record));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub mod platform;
