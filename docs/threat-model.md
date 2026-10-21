# ABHIVERSE — Threat model (initial)

A remote-access product can expose an entire computer, so these
are non-negotiable from Phase 1 onward:

- Each installation generates its own cryptographic key pair;
  private keys never leave the device and are never stored by the
  coordination server.
- Pairing codes are short-lived, single-use, and only bootstrap a
  key exchange. Approval happens on the target device.
- Permissions are per-capability: files, clipboard, screen view,
  input, terminal and power actions are separate grants.
- File access is limited to approved directories with path
  validation (no `..` escapes, safe symlink handling).
- Any device can be revoked; revocation invalidates future sessions.
- Remote command execution is disabled by default.
- Releases are signed; vulnerabilities are reported via SECURITY.md.

Known limitation (v0.1): Task 001 has no authentication because it
exposes no capabilities and binds to localhost only. Do not deploy
it publicly.


## Implemented in v0.2 (Tasks 002–004)

- Device IDs are public-key fingerprints — an ID cannot be forged
  without the private key.
- Pairing approval/rejection, heartbeats and revocation all require
  Ed25519 signatures from the device key; timestamps bound replay.
- Agent private keys live in `keypair.json`, protected with `0600`
  permissions on Unix and a current-user-only DACL on Windows
  (since v0.5.1; the agent fails closed if the ACL cannot be set).

Assessed, not a gap: QR codes are not signed — deliberately. A QR
only carries the 6-digit pairing code, which is single-use,
expires in 5 minutes, and is worthless without the target device's
signed approval at its own prompt; a swapped QR can at worst point
a phone at a code whose pairing the laptop owner still has to
approve, seeing the controller's name first.

Closed in v0.5: controllers hold Ed25519 keys (device ID =
fingerprint of the controller key, registered at pairing), so
controller-originated transfers, session requests and
controller-initiated revocation are all signature-verified; API
state persists via `ABHIVERSE_STATE_FILE`. Remaining known gaps: session/pairing state is
kept in memory by design (short-lived), and hosted deployment
issues are tracked separately. Both were
scheduled before any hosted/public deployment.


## File transfer notes (v0.3)

- Agent-originated chunks are individually signature-checked
  against the chunk's own SHA-256, so a relay (or MITM of the relay
  API) cannot substitute bytes silently; the final whole-file hash
  is verified by the server *and* independently by the receiver.
- Agents only ever pull files addressed to them, into their own
  `received/` directory, under sanitised base names — nothing can
  write outside it or choose arbitrary paths on the device.
- Controller-originated transfers are not yet cryptographically
  authenticated since v0.5 (controllers hold Ed25519 keys and sign
  transfer creates; chunk integrity additionally rests on the
  declared whole-file SHA-256, verified by the API at completion
  and again by the receiver).

## v0.5.2 security review (2026-10-21)

A focused review of the authorisation boundaries, with automated
regression tests in `server/api/tests/security.rs`:

- **Replay (fixed).** Signed requests were previously accepted any
  number of times inside the 120 s timestamp window. The API now
  records accepted signatures and rejects repeats; create requests
  additionally carry a signed `request_id` (protocol v2) so
  legitimate duplicates are not mistaken for replays.
- **Revocation vs. live sessions (fixed).** Revoking a device did
  not touch its active remote-desktop sessions. Revocation now
  ends them; the agent sees `Ended` on its next poll (≤ ~1 s) and
  stops streaming and injecting, and closed sessions refuse new
  ICE. Input injection also stops with the peer connection.
- **Pairing-code guessing (mitigated).** Failed claims are
  throttled globally (10 failures/minute locks claiming briefly).
  A claimed code still needs the device's signed approval, and a
  claim squats the code only until the 5-minute expiry.
- **Transfer size (bounded).** Creates above a size cap (default
  1 GiB, `ABHIVERSE_MAX_TRANSFER_MB`) are refused, bounding relay
  disk use. Chunk indexes and lengths were already strictly
  checked; file names are reduced to safe base names.
- **Checked, no change needed.** Session/ICE endpoints verify
  participation; cross-pairing requests (controller A → device B)
  are refused (tested); revocation persists across API restarts
  (tested); TURN credentials are minted per request with a
  1-hour expiry and the secret never leaves the API; no private
  keys or secrets are written to logs.
- **Residual.** Controller-origin chunk uploads are unsigned by
  design — integrity rests on the declared whole-file SHA-256,
  verified at completion and again by the receiver. Session
  offers expire after 120 s; established sessions end only via
  end/revoke/peer-close, which matches the revocation test above.
