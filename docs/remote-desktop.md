# Remote Desktop — design (Task 006)

Status: **signalling + agent peer implemented (v0.4, slice 006a).**
Screen capture, video encoding and input injection are *not*
implemented yet; they need a real Windows desktop to verify and are
scheduled as the slices listed at the end.

> **Verification note (006a):** the full signalling exchange was
> verified live — session created by a probe peer, approved at the
> agent's prompt, signed answer applied by the probe, ICE
> candidates exchanged through the API, both peers reaching ICE
> `checking`. The final step (an actual DTLS data-channel
> connection) could **not** be verified in the Linux CI sandbox used
> for development: that sandbox blocks all UDP sends (even to
> 127.0.0.1, confirmed independently), and WebRTC media/data runs
> over UDP. The data-channel code path compiles and follows the
> standard webrtc-rs patterns, but its first real run must happen
> on actual devices (Windows laptop + phone). Treat 006a as
> "signalling proven, transport pending real-device verification"
> until then.

## Goal

From the Android controller, see and control a paired Windows
laptop's desktop over the internet — not just on the same LAN —
with the same security posture as the rest of Abhiverse: nothing
is reachable until devices are paired, and taking control always
requires approval on the device being controlled.

## Architecture

Media and input travel **peer-to-peer over WebRTC** (DTLS-SRTP for
video, DTLS data channels for input). The coordination API never
sees screen content; it only brokers the introduction
("signalling") and stores nothing about a session once it ends.

```
Controller (Flutter)                API (Axum)              Agent (Rust, Windows)
        |  POST /sessions (offer) ---->|                        |
        |                              |<-- poll GET /sessions  |
        |                              |    (agent sees request, |
        |                              |     user approves       |
        |                              |     locally)            |
        |                              |<-- POST answer (signed)|
        |  GET /sessions/{id} -------->|                        |
        |  POST/GET .../ice  <========>|<=======>  .../ice      |
        |=========== WebRTC peer connection (direct/TURN) ======|
        |  video track (later) + "abhiverse-input" data channel |
```

### Why WebRTC

- NAT traversal (ICE/STUN) and relay fallback (TURN) are built in —
  required for "different networks", the project's core promise.
- Encryption is mandatory and end-to-end (DTLS), so even the TURN
  relay only forwards ciphertext.
- `flutter_webrtc` (controller) and the `webrtc` crate / webrtc-rs
  (agent) are mature, and both speak standard SDP/ICE.

## Session lifecycle

1. **Request.** The controller creates a session with its SDP
   offer: `POST /sessions {device_id, controller_id, sdp_offer}`.
   The API only accepts this from the device's *paired controller*
   (the same rule as controller-originated file transfers).
   Sessions expire after 120 seconds if not answered.
2. **Approval + answer.** The agent polls `GET /sessions?for={id}`.
   For each offered session it asks the local user (terminal prompt
   today; a Windows toast/dialog in the product slice) — exactly
   like pairing, a remote session is never silent. On approval the
   agent builds its WebRTC answer and posts it, signed with the
   device key: `POST /sessions/{id}/answer`. On refusal it posts a
   signed reject.
3. **ICE exchange.** Both sides trickle ICE candidates through
   `POST /sessions/{id}/ice` and poll the other side's with
   `GET /sessions/{id}/ice?as={participant}&after={seq}`. (HTTP
   polling for v1; a WebSocket upgrade is a drop-in improvement.)
4. **Connected.** Peers connect directly where NAT allows, else
   via TURN. The controller sends input on the data channel
   labelled `abhiverse-input`; video arrives on a media track
   (later slice).
5. **End.** Either side posts `/sessions/{id}/end`; the agent can
   also end a session locally at any time (and a future tray UI
   will show a persistent "being controlled" indicator — a session
   must never be invisible to the person at the keyboard).

## Signalling API (implemented in v0.4)

- `POST /sessions` → `201 SessionView` (status `offered`)
- `GET /sessions/{id}?as={participant_id}` → `SessionView`;
  non-participants get `403` (session IDs are not capabilities)
- `GET /sessions?for={device_id}` → sessions addressed to a device
- `POST /sessions/{id}/answer {sdp_answer}` — agent-signed:
  signature over `abhiverse-session-answer:{id}:{sha256(answer)}:{ts}`
- `POST /sessions/{id}/reject` — agent-signed
- `POST /sessions/{id}/ice {from_id, candidate, sdp_mid, sdp_mline_index}`
- `GET /sessions/{id}/ice?as={participant}&after={seq}` → the other
  side's candidates with `seq > after`
- `POST /sessions/{id}/end {from_id}` (agent-signed when the agent ends it)
- `GET /config/rtc` → `{ice_servers: [...]}` — STUN by default;
  operators override with `ABHIVERSE_ICE_SERVERS_JSON` (e.g. their
  own TURN server). TURN credentials are deployment config, never
  baked into clients.

## Input events (implemented as protocol types in v0.4)

JSON on the `abhiverse-input` data channel, coordinates normalised
to `0.0..=1.0` of the remote frame so resolution changes are safe:

- `mouse_move {x, y}` · `mouse_down {button}` · `mouse_up {button}`
- `scroll {dx, dy}` · `key_down {code}` · `key_up {code}` · `text {text}`

The agent maps these to Windows `SendInput` calls in the input
slice. Key codes use the web `KeyboardEvent.code` vocabulary so
Flutter and Windows agree without a translation table.

## Windows capture & input plan (later slices)

- **Capture:** DXGI Desktop Duplication (low latency, GPU frames),
  falling back to GDI `BitBlt` where DXGI is unavailable.
- **Encode:** hardware H.264 where available (Media Foundation),
  else VP8 software encoding from webrtc-rs. Start 720p/15fps,
  adaptive.
- **Input:** `SendInput` for mouse/keyboard; **the secure desktop
  (UAC prompts, lock screen) is out of scope** — a normal user-mode
  agent cannot and should not inject there.
- **Clipboard:** a separate, explicitly permissioned capability —
  not bundled into "remote desktop" silently.

## Security rules (non-negotiable)

- No pairing, no session: unknown or unpaired controllers cannot
  create sessions, and revoked devices lose everything at once.
- Every session needs a fresh on-device approval (an "unattended
  access" mode may come later, off by default, per-device, and
  clearly indicated while active).
- The agent's answer is signed; a hijacked API session record
  cannot substitute a different answer without the device key.
- Media/input are DTLS-encrypted end-to-end; the API and TURN see
  only ciphertext.
- Sessions are short-lived offers (120 s), single-answer, and
  ended sessions cannot be resumed — a new session is a new approval.

## Slice plan

- **006a (this slice):** this design, signalling API, protocol
  types, agent `--remote` answering approved sessions and logging
  input-channel messages, with a headless probe peer
  (`cargo run --example rtc_probe`) — signalling proven live; the
  data-channel connection itself awaits real-device verification
  (see the note at the top).
- **006b (done in v0.5, runtime verification pending real hardware):**
  agent video: DXGI Desktop Duplication capture (`crates/win-input`)
  → BGRA→I420 → OpenH264 → WebRTC video track at 640×360/10fps,
  with a synthetic test-pattern fallback when duplication is
  unavailable; controller `flutter_webrtc` viewer (touch = click/
  drag/right-click, special-keys row, type-text box). The capture
  and injection code type-checks against the Windows target; the
  encoder path is unit-tested with real encoded frames. First
  on-device run: pair, start the agent with `--remote`, open the
  viewer from the device list, approve at the agent prompt.
- **006c (done in v0.5):** input injection via `SendInput`
  (`crates/win-input`: web `KeyboardEvent.code` → virtual keys,
  normalised mouse → absolute pixels), gated behind the agent's
  `--allow-input` flag — without it, input events are only logged.
- **006d (done; TURN guide v0.5, the rest v0.5.1):** TURN
  deployment guide (`docs/turn.md`) incl. API-minted short-lived
  credentials; persistent "REMOTE SESSION ACTIVE" banner in the
  agent console for the whole session; Windows console-title
  warning + toast notification at session start/end
  (`crates/win-input` indicator); adaptive quality — the agent
  watches WebRTC stats (packet loss, estimated available bitrate)
  and steps the stream between 640×360@10fps/1.2 Mbps,
  640×360@5fps/700 kbps and 480×270@5fps/350 kbps with
  hysteresis.
