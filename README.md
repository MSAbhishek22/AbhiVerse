# ABHIVERSE

**Your devices. One ecosystem. Total control.**

An open-source platform for securely connecting, accessing and
managing your own devices from anywhere — a personal control centre
for your phone, laptop and tablet, across operating systems and
across networks.

> **Status: v0.1 — Task 001 foundation.** This release proves local
> communication between the Flutter controller, the Rust coordination
> API and the Rust device agent. Pairing, file transfer and remote
> desktop are *not* implemented yet — see the roadmap below.

## What exists today

| Component | Path | State |
|---|---|---|
| Coordination API (Rust + Axum) | `server/api` | `GET /health` only, binds to localhost |
| Windows agent (Rust) | `agents/windows` | Reports version/platform, clean Ctrl+C shutdown. No device capabilities yet |
| Android controller (Flutter) | `apps/controller` | Home screen showing live connection status + backend version, with error/retry |
| Shared protocol types | `crates/protocol` | `HealthResponse`, `AgentInfo`, protocol v1 |

## Prerequisites

- Git
- Rust toolchain (stable): https://rustup.rs
- Flutter SDK (stable): https://docs.flutter.dev/get-started/install
- For Android builds: Android Studio or the Android SDK command-line tools
- For the Windows agent on Windows: Visual Studio Build Tools with
  the "Desktop development with C++" workload

Check your setup:

```sh
git --version
rustc --version
cargo --version
flutter doctor
```

## Run it locally

### 1. Coordination API

```sh
cargo run -p abhiverse-api
# listening on http://127.0.0.1:8080
curl http://127.0.0.1:8080/health
# {"app":"ABHIVERSE","service":"abhiverse-api","status":"ok","version":"0.1.0","protocol_version":1}
```

Override the bind address with `ABHIVERSE_BIND_ADDR`, e.g.
`ABHIVERSE_BIND_ADDR=127.0.0.1:9000 cargo run -p abhiverse-api`.
**Do not expose this unauthenticated Task 001 API to the internet.**

### 2. Device agent

```sh
cargo run -p abhiverse-agent-windows -- --version   # abhiverse-agent 0.1.0
cargo run -p abhiverse-agent-windows -- --once      # JSON status line
cargo run -p abhiverse-agent-windows                # runs until Ctrl+C
```

The agent builds and runs on Windows, Linux and macOS today;
Windows-specific capabilities arrive in later phases behind
platform adapters.

### 3. Controller (Flutter)

```sh
cd apps/controller
flutter pub get
flutter run
```

The app defaults to `http://10.0.2.2:8080` (Android emulator →
host localhost). Point it elsewhere with:

```sh
# Physical device on the same LAN as your computer:
flutter run --dart-define=ABHIVERSE_API_BASE_URL=http://192.168.1.20:8080

# Desktop / web / iOS simulator against a local server:
flutter run --dart-define=ABHIVERSE_API_BASE_URL=http://127.0.0.1:8080
```

## Tests & checks

```sh
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Flutter
cd apps/controller
flutter analyze
flutter test
```

Verified for Task 001 (Linux dev host, Rust 1.99, Flutter 3.47):
Rust — 5 tests passing, clippy + rustfmt clean, `GET /health`
returns the documented JSON live, agent and API both exit cleanly
on Ctrl+C. Flutter — `flutter analyze` clean, 5 tests passing, and
a live integration check passed with the Flutter client talking to
the real Rust API on `127.0.0.1:8080`. Android APK builds require
the Android SDK (`flutter doctor`) on your machine.

## Repository layout

```
abhiverse/
├── apps/controller/     # Flutter controller (Android first)
├── agents/windows/      # Rust device agent
├── crates/protocol/     # Shared Rust protocol types
├── server/api/          # Rust coordination API (Axum)
├── docs/                # Architecture, protocol, threat model
└── .github/workflows/   # CI: Rust checks + Flutter analysis/tests
```

## Roadmap

- **Task 001 — Foundation** ✅ local health check + builds
- **Task 002** — Device identity & key generation
- **Task 003** — Pairing challenges, QR codes, target-device approval
- **Task 004** — Authenticated presence & revocation
- **Task 005** — Secure file transfer (integrity + resume)
- **Task 006** — Remote desktop engine & input

See `docs/architecture.md` and `docs/threat-model.md` before
contributing anything security-sensitive.

## License

MIT — see `LICENSE`.
