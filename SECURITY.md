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
