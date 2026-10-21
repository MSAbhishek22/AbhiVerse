# ABHIVERSE — Architecture (v0.1)

Three parts:

1. **Controller** (`apps/controller`, Flutter) — dashboard, pairing UI,
   file browser and remote-session UI. Android first.
2. **Agent** (`agents/windows`, Rust) — runs on the device being
   accessed; exposes only explicitly approved capabilities.
3. **Coordination API** (`server/api`, Rust + Axum) — pairing,
   presence and connection negotiation. It must never need to see
   file contents or screen data.

Shared payloads live in `crates/protocol` and are the contract
between all three.

## Connection flow (target, phases 1+)

1. Agent registers its device identity (public key) with the API.
2. Controller requests a short-lived, single-use pairing challenge.
3. User approves the pairing *on the target device*.
4. Devices authenticate with their keys — the pairing code is a
   bootstrap, never a permanent secret.
5. Sessions negotiate a direct encrypted peer connection (WebRTC),
   falling back to a TURN relay when NAT prevents a direct path.

## Task 001 boundary

Only `GET /health` exists. No pairing, files, screen, input, shell
or power actions. The API binds to `127.0.0.1:8080` by default.
