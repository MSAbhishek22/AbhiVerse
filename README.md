# ABHIVERSE

**Your devices. One ecosystem. Total control.**

An open-source platform for securely connecting, accessing and
managing your own devices from anywhere — a personal control centre
for your phone, laptop and tablet, across operating systems and
across networks.

> **Status: v0.5 — Tasks 001–006 + controller keys & persistence.**
> Device identity (Ed25519) for agents *and* controllers, pairing
> with signed approval (code or QR), presence, revocation signed by
> either side, chunked file transfer with SHA-256 integrity and
> resume, persistent API state, and remote desktop: WebRTC
> signalling with signed requests/answers, H.264 screen streaming
> (DXGI capture on Windows), `SendInput` injection behind an
> explicit agent flag, and a Flutter viewer with touch/keyboard
> input. Media/input verification on real hardware is the remaining
> step — the dev sandbox blocks UDP; see `docs/remote-desktop.md`.

## What exists today

| Component | Path | State |
|---|---|---|
| Coordination API (Rust + Axum) | `server/api` | Health, pairing, presence, revocation, file-transfer and remote-desktop signalling endpoints; binds to localhost; optional persistent state via `ABHIVERSE_STATE_FILE`, chunk store on disk; verifies agent *and* controller signatures |
| Windows agent (Rust) | `agents/windows` | Device identity, `--pair` flow with QR code + on-device approval, signed heartbeats, `--send-file` / `--receive`, `--remote` WebRTC sessions (approval prompt, H.264 screen stream via DXGI, session banner), `--allow-input` to enable `SendInput` injection, `--revoke` |
| Android controller (Flutter) | `apps/controller` | Own Ed25519 identity in secure storage (signs transfers, session requests, revocations), device list with presence, pairing by code or QR scan, send/download & verify files, remote-desktop viewer with touch + keyboard input, revoke pairing |
| Shared protocol types | `crates/protocol` | Health, device, pairing, presence, transfer, session and input payloads + canonical signing messages, protocol v1 |
| Remote input & capture (Rust) | `crates/win-input` | `InputEvent` → platform actions mapping, Windows `SendInput` injection, DXGI desktop duplication capture, BGRA→I420 conversion (Windows code type-checked in CI on the Windows target) |
| Device identity (Rust) | `crates/identity` | Ed25519 key generation/storage (0600), fingerprint device IDs, sign/verify |
| File transfer (Rust) | `crates/file-transfer` | 256 KiB chunk plans, SHA-256 integrity, resumable assembly, safe file names |

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
# {"app":"ABHIVERSE","service":"abhiverse-api","status":"ok","version":"0.2.0","protocol_version":1}
```

Override the bind address with `ABHIVERSE_BIND_ADDR`, e.g.
`ABHIVERSE_BIND_ADDR=127.0.0.1:9000 cargo run -p abhiverse-api`.
**Do not expose this unauthenticated Task 001 API to the internet.**

### 2. Device agent

```sh
cargo run -p abhiverse-agent-windows -- --version   # abhiverse-agent 0.2.0
cargo run -p abhiverse-agent-windows -- --identity  # device ID + public identity
cargo run -p abhiverse-agent-windows -- --pair --api http://127.0.0.1:8080
cargo run -p abhiverse-agent-windows                # runs until Ctrl+C (heartbeats if paired)
cargo run -p abhiverse-agent-windows -- --revoke    # unpair/revoke this device
cargo run -p abhiverse-agent-windows -- --send-file ~/photo.jpg   # to the paired phone
cargo run -p abhiverse-agent-windows -- --receive   # download files sent to this computer
cargo run -p abhiverse-agent-windows -- --remote    # answer remote-desktop sessions (asks first)
```

On first run the agent creates an Ed25519 key pair in its data
directory (`ABHIVERSE_DATA_DIR`, default `~/.abhiverse/agent`). The
device ID is the fingerprint of the public key. **Back up / protect
`keypair.json` like a password — it *is* the device's identity.**

To pair: start the API, run the agent with `--pair`, then enter the
6-digit code in the controller app and approve the prompt on the
computer. The code expires in 5 minutes, is single-use, and the
approval is a signature from the agent's private key.

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

Verified for v0.4 (Linux dev host, Rust 1.99, Flutter 3.47):
Rust — 22 tests passing, clippy + rustfmt clean. Flutter —
`flutter analyze` clean, 11 tests passing. Live: the complete
remote-desktop signalling exchange (probe peer → session →
on-device approval → signed answer → ICE exchange) plus all v0.3
file-transfer end-to-end runs. One honest exception: the WebRTC
data-channel connection itself could not complete inside the dev
sandbox, which blocks all UDP sends (verified independently,
even to localhost) — it needs a first real run on actual devices.
Android APK builds require the Android SDK (`flutter doctor`).

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
- **Task 002** — Device identity & key generation ✅
- **Task 003** — Pairing challenges & target-device approval ✅ (QR payload is the same code; in-app QR scanning is a later UX add)
- **Task 004** — Authenticated presence & revocation ✅
- **Task 005** — Secure file transfer (integrity + resume)
- **Task 006** — Remote desktop engine & input

See `docs/architecture.md` and `docs/threat-model.md` before
contributing anything security-sensitive.

## License

MIT — see `LICENSE`.
