//! Platform adapters live here. Windows-specific API calls must be
//! `#[cfg(windows)]`-gated so the crate still builds and tests on
//! Linux/macOS CI runners.
