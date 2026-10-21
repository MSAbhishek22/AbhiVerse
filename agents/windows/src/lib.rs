//! ABHIVERSE Windows agent library.
//!
//! Task 001 scope: report version / platform and shut down cleanly.
//! No screen capture, input injection, shell or power controls yet —
//! those land in later, explicitly permissioned phases.

use abhiverse_protocol::AgentInfo;

pub fn agent_info() -> AgentInfo {
    AgentInfo::current(env!("CARGO_PKG_VERSION"))
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
}

pub mod platform;
