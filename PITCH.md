# Abhiverse — pitch one-pager

**One line:** Your phone is the remote for your laptop — screen,
keyboard, files and power, free and open source, no account, no
subscription.

## The problem

- Apple Continuity works only inside Apple's walled garden.
- Samsung DeX works only on Samsung hardware.
- AnyDesk / TeamViewer are closed, subscription-priced, and need
  an account on someone else's servers.
- LocalSend moves files only — no screen, no input, no power.
- Nobody offers the whole loop (see it, type on it, move files
  to it) as free, open-source software you can self-host.

## The product (today, v0.5.4 pre-release)

- **Pair in seconds:** 6-digit code or QR, approved on the
  computer's terminal. No account, no email, no password.
- **Remote desktop:** WebRTC video + touch/keyboard input, with
  ICE-failure recovery and retry.
- **File transfer:** chunked, resumable, SHA-256 verified both
  ends, from real files on the phone.
- **Presence & control:** live online/offline, revoke any
  pairing from the phone, per-session approval on the computer.
- **Self-hostable:** the coordination server is a small Rust
  binary with an optional TURN relay; the phone's server address
  is now configurable in-app.

## Why it wins (the moat)

1. **Signed everything.** Every API call is Ed25519-signed by
   the calling device; sessions, transfers and ICE are signed;
   replays are rejected; revocation locks reads too. There is no
   password database to leak because there are no passwords.
2. **No telemetry by design.** No analytics, no tracking SDKs,
   no account. The product cannot sell data it never collects.
3. **Open source, auditable.** Public code, public security
   audit, published release checksums.
4. **Price:** free. Support is optional (UPI on the website).

## Maturity — real numbers, no inflation

- 50 Rust tests + 37 Flutter tests (+1 skipped), clippy/fmt/
  analyze clean, GitHub Actions green (Rust, Windows target,
  Flutter).
- Live cross-language end-to-end green (pairing → signed
  transfer → signed session → revoke).
- Full in-depth security audit completed; all findings fixed
  (see CHANGELOG.md).
- CI builds a release APK artifact on every push to main.

## Honest gaps (what funding closes)

- **Real-device run pending:** WebRTC media, DXGI capture,
  SendInput and the APK on physical hardware are verified by
  test matrix, not yet by a live run — the release blocker.
- **Play Store listing** needs a release keystore + data-safety
  form (no data collected).
- **Code signing** (OV/EV certificate) to retire SmartScreen
  warnings — see `docs/windows-defender.md`.
- **Hosted staging** needs a VPS/domain; the deploy bundle is
  ready (Caddy + API + coturn).
- **Roadmap:** power controls, clipboard sync, multi-monitor,
  audio, LAN auto-discovery, multi-device dashboard.

## Competition

| | Abhiverse | AnyDesk / TeamViewer | LocalSend | KDE Connect |
|---|---|---|---|---|
| Open source | Yes | No | Yes | Yes |
| Screen + input | Yes | Yes | No | Partial |
| Files | Yes | Yes | Yes | Yes |
| Account needed | No | Yes | No | No |
| Windows-first agent | Yes | Yes | Yes | No |
| Price | Free | Subscription | Free | Free |

## Business model

Free core, forever. Optional user support today. Later, an
optional hosted relay for non-technical users (freemium) and
self-host support for teams — the core stays free and
self-hostable, so the paid tier can never hold users hostage.

## The ask

Pre-seed to fund: code signing, Play listing, hosted staging,
and a real-device QA pass across phones and laptops — turning a
verified, audited pre-release into a stable 1.0.
