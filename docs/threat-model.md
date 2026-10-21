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
- Agent private keys live in `keypair.json` with `0600` permissions
  on Unix (Windows ACL hardening is still TODO).

Closed in v0.5: controllers hold Ed25519 keys (device ID =
fingerprint of the controller key, registered at pairing), so
controller-originated transfers, session requests and
controller-initiated revocation are all signature-verified; API
state persists via `ABHIVERSE_STATE_FILE`. Remaining known gaps:
the Windows keypair file relies on the user's profile ACLs
(explicit ACL hardening still TODO), session/pairing state is
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
