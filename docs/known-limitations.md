# Known limitations (v0.5.2)

Honest list, also linked from the README. Items marked
**hardware-pending** are implemented and verified everywhere the
dev sandbox allows, but still await the real-device run in
`docs/real-device-test.md`.

## Verification status

- **WebRTC media path — hardware-pending.** Signalling, session
  authorisation and the encoder are tested; the DTLS media/data
  channel connection itself has never run outside the sandbox
  (which blocks UDP). DXGI capture and `SendInput` injection are
  Windows-target type-checked and unit-tested, not yet observed
  on a live desktop.
- **APK build — hardware-pending.** The Flutter app is analysed
  and unit-tested; no Android SDK exists in the dev sandbox, so
  the release APK build is first exercised on a real machine.

## Product

- One controller per device; one device per pairing (multi-device
  dashboard is future work).
- Stream quality tops out at 640×360 / 10 fps (adaptive tiers go
  down from there); no audio, no multi-monitor selection.
- Every transfer chunk upload/download, session read, ICE trickle
  and RTC-config fetch is signed by the sending device/controller
  (closed 2026-10-25: previously controller uploads/downloads and
  all session reads were unsigned). Whole-file SHA-256 is still
  verified at completion and on receipt as the integrity backstop.
- Pairing and remote sessions are intentionally not persisted;
  pairings and transfers are (with `ABHIVERSE_STATE_FILE`).
- The Android launcher artwork is the stock Flutter icon;
  dimensions are correct at all densities. A branded icon is a
  pre-beta cosmetic task.
- No automatic updates and no signed installers yet (see
  `docs/release.md`); releases are source + manually built
  binaries/APK with published SHA-256 checksums.

## Operations

- The API binds to localhost by default; LAN exposure is a manual,
  trusted-network choice (`ABHIVERSE_BIND_ADDR`). There is no TLS
  in the app/API today — the hosted deployment (reverse proxy +
  TLS) is the next milestone, and TURN credentials minted by the
  API expire after 1 hour.
- Pairing-claim throttling is global (10 failures/minute), which
  suits a personal server; a multi-tenant host would need
  per-source limits.
- GitHub: `.github/workflows/ci.yml` cannot be pushed by the
  project's automation (GitHub Apps lack workflow permission) —
  it is added via the web UI; see `docs/release.md`.
