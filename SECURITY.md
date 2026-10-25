# Security policy

Abhiverse is pre-1.0 software. The coordination API binds to
localhost by default; exposing it beyond a trusted network is
supported only behind TLS with the deployment in `deploy/`
(see `docs/known-limitations.md` for what is and isn't verified).

## Reporting a vulnerability

Please do **not** open a public issue for exploitable flaws.
Use GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab → **Report a
   vulnerability**.
2. Include affected version/commit, reproduction steps, and the
   impact you observed.

You should get an acknowledgement within a few days. Fixes for
confirmed high-severity issues are prioritised over features,
and reporters are credited in the release notes unless they
prefer otherwise.

## Scope notes

- Device private keys never leave the device that generated
  them; a report claiming otherwise is high severity by default.
- Remote input requires the agent's explicit `--allow-input`
  flag and an approved session; any path that injects input
  without both is high severity.
- The threat model lives in `docs/threat-model.md`.

## Security model (v0.5.4)

- **Everything is signed except the documented public surface.**
  Every mutation and every sensitive read (sessions, transfers,
  ICE candidates, TURN config) requires an Ed25519 signature from
  a paired device or its paired controller, over a message that
  binds the resource, the caller, and a timestamp (120 s window).
  Mutations are replay-protected; idempotent reads are verified
  without replay recording so polling loops cannot
  false-positive.
- **Intentionally public:** `GET /health` (version only),
  `GET /devices` (names + presence, for unpaired discovery),
  and pairing create/poll (the device is unknown before pairing,
  so these cannot be signed — the pending set is capped at 100
  and pruned, and a claim still needs the laptop owner's terminal
  approval).
- **Revocation is total.** Revoking a device ends its live
  sessions and locks it (and its controller) out of reads too.
- **Resource bounds.** One live session offer per device,
  pairing-claim throttling, 1 GiB default transfer cap, 7-day
  transfer retention, 24 h session prune.
- **Transport.** The API binds localhost by default and ships no
  TLS; hosted setups must sit behind the TLS reverse proxy in
  `deploy/` and must use an HTTPS API base URL on the phone
  (cleartext is allowed only for LAN development).
