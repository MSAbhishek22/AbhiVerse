# Changelog

Abhiverse is pre-1.0: minor versions ship features and hardening
together, and every release stays a **pre-release** until the
real-device run (physical Windows laptop + Android phone) verifies
the WebRTC media path, DXGI capture, SendInput injection, and the
APK build. Dates are IST.

## Unreleased

Controller-app audit + add-ons (four-lens review: user, security,
investor, competitor):

- **Real file sending.** The send dialog used to send typed text
  only; it now picks real files via the system file picker
  (file_selector, the Flutter team's own picker — file_picker was
  dropped after CI run #75 proved its Android module pins Kotlin 1.8,
  which cannot build under the app's AGP 9 / Gradle 9 toolchain),
  and flutter_webrtc 0.12 → 1.6 (the old module compiled against
  Android API 31, which CI run #80 proved cannot consume the
  exifinterface 1.4.1 the graph now resolves; 1.6 targets API 36
  and ships a much newer WebRTC — a security win too),
  with a 1 GB client-side guard matching the
  server cap and chunk progress as before.
- **In-app server settings.** The API URL is no longer
  build-time-only: the app saves a user-chosen server URL in
  secure storage and rebuilds its clients around it without a
  restart (reset to the build default included). This is what
  makes a release APK usable outside the emulator.
- **Endpoint guard.** Public `http` URLs are refused before any
  request goes out; LAN `http` shows a warning; `https` is
  unrestricted. The guard is unit-tested across loopback,
  private, link-local and public ranges.
- **Download-name sanitization.** Incoming file names are
  reduced to a basename before any disk write (defense in depth
  beside the server's own sanitization).
- **Server errors surfaced.** The client used to replace every
  error body with a bare HTTP status; it now shows the server's
  own message (including the clock-sync hint).
- **QR parser tightened.** Only the agent's two shapes are
  accepted (bare code, `abhiverse://pair?code=`); arbitrary text
  containing six digits no longer starts a pairing claim. After
  a claim, a dialog names the target computer and warns the user
  to approve only on their own machine.
- **Responsive tests.** The home screen and dialogs are now
  widget-tested at six sizes (320×568 small phone through
  1280×800 tablet landscape) with zero overflow; dialogs scroll
  on small screens.
- **Docs:** `docs/windows-defender.md` (Defender/SmartScreen/
  Play Protect posture + signing path), `PITCH.md` (honest
  one-pager: real numbers, real gaps).
- Tests: Flutter 19 → 37 (+1 skipped); analyze clean.

## v0.5.4 — 2026-10-25 — production hardening

Security audit fixes, from a full in-depth audit (code + tests +
live HTTP E2E) of the v0.5.3 tree:

- **Signed reads everywhere it matters.** Session, transfer and
  RTC-config reads now require a paired device/controller
  signature over the exact path + query (`abhiverse-get:` message).
  Reads are verified but not replay-recorded, so sub-second
  polling loops cannot false-positive as replays. Cross-pair
  reads are forbidden (403); revoked callers are locked out (401).
- **Signed ICE trickle, both sides.** A stolen session id alone
  can no longer inject ICE candidates.
- **Closed the v0.3 unsigned-controller exceptions.** Controller
  chunk uploads, downloads, completes and session ends are now
  signed, like agent operations. (Controllers have had Ed25519
  keys since v0.5.0 / Task 007.)
- **Timeouts.** Agent HTTP: 10 s control plane, 120 s chunk
  transfers; signalling: 10 s. A hung connection can no longer
  stall heartbeats or pairing forever.
- **Viewer ICE failure UX.** Failed/disconnected WebRTC now shows
  a clear error with a Retry button (which ends the dead session
  first) instead of an eternal spinner.
- **Anti-spam / resource bounds.** One live session offer per
  device (409); 100 pending pairings (429) with expired pruning;
  24 h session prune; 7-day transfer retention prune including
  relay chunk files.
- **Clock-sync hint** in stale-timestamp errors (signatures allow
  120 s of skew).
- Tests: 50 Rust (new `auth_reads.rs`), 19 Flutter + 1 skipped;
  clippy/fmt/analyze clean; live cross-language E2E v5 green.

## v0.5.3 — 2026-10-22 — public-beta preparation

Issue templates, rewritten SECURITY.md, fuller CONTRIBUTING,
one-command real-device test runner
(`scripts/run-real-device-test.ps1`), staging bundle
(`server/api/Dockerfile` + `deploy/` Caddy → API + coturn
compose, syntax-checked, not deployed). CI green on GitHub
Actions; v0.5.3 published as a GitHub pre-release.

## v0.5.2 — 2026-10-21 — validation / security pass

- Replayable signed requests fixed: the API records accepted
  signatures; creates carry a signed `request_id`
  (**protocol v1 → v2**).
- Revoke now ends live remote-desktop sessions.
- Pairing claims throttled (10 failures/min → 429).
- Transfer size cap (default 1 GiB, `ABHIVERSE_MAX_TRANSFER_MB`).
- Docs: `docs/real-device-setup.md`,
  `docs/real-device-test.md`, `docs/known-limitations.md`,
  `docs/release.md`.

## v0.5.1 — 2026-10-21

Adaptive video quality tiers from WebRTC stats, session
indicator (console title + toast), Windows ACL hardening on
`keypair.json` (fail-closed), ephemeral TURN credentials
(`GET /config/rtc` mints coturn `--use-auth-secret` credentials),
CI workflow.

## v0.5.0 — 2026-10-21

Remote desktop video (DXGI capture → OpenH264 640×360@10fps over
WebRTC), input injection (`SendInput`, gated behind
`--allow-input`), TURN guide + session banner, controller
Ed25519 identity in Flutter secure storage with API signature
verification, API state persistence, QR pairing.

## v0.4 — 2026-10-21

Remote-desktop design + WebRTC signalling (paired-controller-only
sessions, 120 s offers, signed answer/reject/end, ICE trickle),
agent `--remote` peer, headless `rtc_probe`, Flutter signalling
client.

## v0.3 — 2026-10-21

Chunked file transfer (256 KiB chunks, whole-file SHA-256,
per-chunk signatures for agent uploads, receiver verification,
resume).

## v0.2 — 2026-10-21

Device identity (Ed25519, fingerprint device IDs), pairing
(6-digit codes, 5-min, single-use, agent terminal approval),
signed heartbeats/presence, revocation.

## v0.1 — 2026-10-21

Foundation: `GET /health`, agent skeleton, Flutter status
screen. (Project renamed NEXUS → Abhiverse the same day.)
