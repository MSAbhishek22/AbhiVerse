# Real-device validation — test matrix

The release blocker for the public beta. The dev sandbox blocks
all UDP, so WebRTC media, DXGI capture and `SendInput` have only
ever been verified at signalling/type-check level. This run is
the first real proof. Record everything; an honest "untested"
beats a guessed "pass".

**Run metadata (fill in):**

- Date / tester:
- App version (APK): 0.5.2+6 / commit:
- Agent + API version: 0.5.2 / commit:
- Phone: model, Android version:
- Laptop: model, Windows version:
- Network case(s): same Wi-Fi / different networks (direct) /
  different networks (relay)

## Pre-flight

- [ ] Built agent, API and APK from the exact same commit
      (`docs/real-device-setup.md`)
- [ ] `keypair.json` ACL verified (current user only)
- [ ] Agent running as the normal Windows user, not admin
- [ ] No unsaved work on the laptop; test folder with
      non-sensitive files prepared
- [ ] Stop/revoke paths known (setup doc §5)
- [ ] Launcher icon on the phone shows the expected icon
      (note: current artwork is the stock Flutter icon)

## Ordered run

1. **Pairing** — QR scan from the app, approve at the laptop.
2. **File transfer** — small file phone → laptop, then back;
   compare SHA-256 (`Get-FileHash` / app shows verified).
3. **Remote viewing** — desktop appears; note orientation,
   colours, resolution (expect 640×360 initially), latency feel.
4. **Input** (only now add `--allow-input`) — in a text editor:
   tap, drag, right-click (long-press), typing, special keys.
   End the session and confirm input stops.
5. **Network switch** — mid-session, phone Wi-Fi → mobile data
   (API must be reachable on the new path for this case; record
   exactly what happens: reconnect, clean failure, or hang).
6. **Failure cases** — stop the agent mid-session; revoke the
   controller mid-session; restart the API; kill the phone's
   network. After each: no stale control of the laptop.

## Matrix

| Test | Pass condition | Result | Notes (direct/relay, measurements) |
|---|---|---|---|
| Pairing | Only the explicitly approved device becomes trusted | | |
| File transfer | Both directions work, SHA-256 hashes match | | |
| Video | Stable image, acceptable latency, no persistent corruption | | fps/res measured: |
| Input | Mouse + keyboard work only during an authorised session | | |
| Wi-Fi → mobile data | Session reconnects or fails with a clear, recoverable error | | |
| Direct connection unavailable | TURN fallback works with the relay configured | | mark *untested* if it can't be forced |
| Revocation | Revoked device cannot create transfers or sessions; live session ends | | |
| API restart | Pairings/transfers survive; no unauthorised sessions reappear | | |
| Laptop restart | Agent returns to its documented startup state | | |
| Agent shutdown | Remote input stops immediately | | |
| Session indicator | Console title + toast show the session; title restored after | | |
| Adaptive quality | Stream steps down/up on constrained bandwidth (optional: throttle the link) | | |

Rules: a case you cannot force (notably relay-only) is recorded
**untested**, never passed by assumption. Any failure gets: what
happened, logs from both sides, and whether it reproduced.

Findings from this run go into `docs/known-limitations.md` and
GitHub issues before the beta is published.
