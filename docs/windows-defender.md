# Windows Defender, SmartScreen & Play Protect — what can flag Abhiverse

Honest summary first: **no unsigned remote-access tool can promise
zero warnings.** AnyDesk, TeamViewer and friends get flagged too,
sometimes. What this document does is explain exactly what
Defender/SmartScreen/Play Protect look at, what Abhiverse already
does to stay on the right side of those heuristics, and what the
remaining steps are (code signing, reputation, exclusions).

## Why a remote-desktop tool looks suspicious to scanners

Three behaviours are classic dual-use signals:

1. **Screen capture** (DXGI desktop duplication) — also what
   spyware does.
2. **Input injection** (`SendInput`) — also what keyloggers and
   RATs do.
3. **An unsigned binary from the internet** — SmartScreen's
   reputation system has no history for it, so it warns.

Abhiverse does all three for legitimate, user-initiated reasons —
but scanners cannot read intent, only behaviour.

## What Abhiverse already does to reduce flags

- **No self-installed persistence, no hidden process.** The agent
  is a visible console window. It never installs a service, never
  adds itself to startup, and does not hide. Autostart exists
  only as an explicit user-run installer
  (`scripts/install-autostart.ps1`, Startup-folder shortcut,
  current user, minimized console) and is fully removable with
  `scripts/uninstall-autostart.ps1` — see `docs/autostart.md`.
- **No silent input.** Input injection requires the agent's
  explicit `--allow-input` flag AND an approved session. Without
  both, the viewer is view-only.
- **No silent sessions — by default.** Every remote session needs
  the person at the computer to approve it at the agent prompt,
  and the console title switches to a persistent ACTIVE warning
  while a session runs. The one exception is the explicit
  `--trust-paired` opt-in (autostart's unattended mode), which
  auto-accepts sessions ONLY from the already-paired controller,
  prints an UNATTENDED MODE banner at startup, and is revoked
  instantly like any pairing. See `docs/autostart.md`.
- **No packing or obfuscation.** Release binaries are plain,
  unpacked Rust builds. Packers are one of the strongest
  heuristic triggers; Abhiverse does not use them.
- **Public source, signed releases.** Anyone — including
  Microsoft's analysts — can read every line, and releases ship
  with published SHA-256 checksums.
- **Minimal Android permissions.** The controller app asks only
  for CAMERA (QR pairing) and INTERNET. No accessibility service,
  no device admin, no overlay, no SMS/call/location permissions —
  the exact permissions that get remote tools flagged by Play
  Protect. The app also refuses cleartext (`http`) connections to
  public servers in-app; LAN `http` is allowed with a warning.
- **No telemetry, no data collection.** There is nothing to
  exfiltrate and no account database to breach.

## What still triggers warnings today (and the fix for each)

| Warning | Cause | Fix |
|---|---|---|
| SmartScreen "Unknown publisher" on first download | Binary is unsigned; no reputation history | OV/EV code-signing certificate + signed releases, then reputation builds with download volume |
| Defender false positive on the agent | Heuristic match on capture + injection | Submit the binary to Microsoft's malware analysis portal (free) for allow-listing; keep behaviour visible as above |
| Android "unknown sources" prompt on sideload | Any APK outside Play shows this | Expected Android behaviour, not a block; a Play Store listing removes it |
| Play "data safety" questions | Play requires a data-safety form | The honest answer is "no data collected, no data shared" — the app has no analytics, accounts or tracking |

## Code signing (the real fix, needs funding)

1. Buy an OV certificate (~$200–500/year) or EV certificate
   (more, with hardware token) from a public CA.
2. Sign the agent `.exe` (and later the installer) in CI with
   the certificate; publish the signature alongside the SHA-256.
3. Submit each release to Microsoft Defender's sample submission
   portal so false positives get cleared proactively.
4. Expect SmartScreen warnings to fade as signed-download
   reputation accumulates — this takes weeks of real downloads,
   not a single switch flip.

Until signing is funded, the supported workaround is a Defender
exclusion for the folder containing the official agent binary —
and only that folder, never a general exclusion. Steps:
Windows Security → Virus & threat protection → Manage settings →
Exclusions → Add a folder → the folder with `abhiverse-agent.exe`.
Verify the binary's SHA-256 against the release notes first.

## What Abhiverse will never do to dodge scanners

No packing, no obfuscation, no process hiding, no disabling or
evading security software, no silent persistence. Dodging
scanners that way would make the product indistinguishable from
malware — the opposite of the trust this project is built on.
