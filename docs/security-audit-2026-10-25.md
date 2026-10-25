# Security audit — 2026-10-25

Scope: the Abhiverse tree as of v0.5.3 (Rust API + Windows agent,
Flutter controller, shared crates). Method: source review of
permissions, dependencies, network surface, key handling, logging
and execution paths — no dynamic testing on hardware.

## Verdict: clean, with one advisory

No evidence of data theft, background collection, third-party
app installation, or silent uploads was found. The app cannot
install other apps, cannot run shell commands, and has no
telemetry of any kind.

## What was checked

| Area | Finding |
|---|---|
| Android permissions | Only `CAMERA` (QR pairing; hardware not required) and `INTERNET`. No storage, location, contacts, SMS, microphone, or install-packages permissions. |
| Dependencies | `http`, `crypto`, `cryptography`, `flutter_secure_storage`, `mobile_scanner`, `flutter_webrtc`, `path_provider`, `cupertino_icons`. No analytics, ads, crash-reporting or tracking SDKs anywhere in the tree. |
| Network surface (app) | Talks only to the API base URL configured by the user (`--dart-define=ABHIVERSE_API_BASE_URL`, default the emulator address). No hard-coded third-party hosts. QR scanning via mobile_scanner runs on-device (ML Kit local). |
| Network surface (agent) | Outbound HTTPS/HTTP only to the configured API (`/pairing`, `/devices`, `/transfers`, `/sessions`) and WebRTC peers via the configured STUN/TURN. The API binds `127.0.0.1` by default; LAN exposure is an explicit, trusted-network choice. |
| Execution | No process spawning, no shell, no autostart/registry persistence anywhere in the agent. Input injection exists only behind the explicit `--allow-input` flag and an approved session. |
| Keys | Phone: Ed25519 seed in Android secure storage. Windows: `keypair.json` with a current-user-only DACL, fail-closed (file deleted if the ACL cannot be set). Private keys are never transmitted, logged or backed up by the app. |
| Logging | Agent/API logs contain device IDs, statuses and errors only. No keys, codes (beyond the single-use pairing code shown on the laptop's own console), signatures or TURN secrets are logged. |
| Authorisation | Signed requests with replay rejection (v0.5.2), per-pairing authorisation, pairing-claim throttling, transfer size cap, path traversal blocked, chunk bounds enforced, revocation ending live sessions. |
| Guard behaviour | Anything unsigned, unpaired or unapproved is rejected by the API and visible in logs; the laptop owner must manually approve a pairing/session or revoke a device. Nothing in the system can override the owner's approval. |

## Advisory (non-blocking)

- The Android app sets `usesCleartextTraffic=true` so plain-HTTP
  LAN testing works. For any hosted setup, always build the app
  with an `https://` API base URL (the `deploy/` stack provides
  TLS via Caddy). A future release should gate cleartext to
  debug builds.

## What this audit does not cover

Real-device runtime behaviour (WebRTC media path, DXGI capture,
SendInput) is still pending the first hardware run, and no
third-party security firm has reviewed the code. The audit above
is a source review by the maintainer's assistant, not a
certification.
