# Autostart — the agent launches when the laptop boots

Goal: open the laptop and the agent is already there, so the
phone works from anywhere without anyone having to "open the
app" first. Browsers and chat apps autostart; this one does too
— but a network-facing remote-control agent is not a browser,
so the mechanism and the trust model are deliberately boring
and deliberately narrow.

## Install / remove

```powershell
# Presence only (safe default): heartbeat, ready to pair/receive
.\scripts\install-autostart.ps1

# LAN mode: also autostart the local API on the current LAN address
.\scripts\install-autostart.ps1 -IncludeApi

# Unattended remote desktop (read the warning below first)
.\scripts\install-autostart.ps1 -Remote -TrustPaired

# Unattended remote desktop WITH input control
.\scripts\install-autostart.ps1 -Remote -TrustPaired -AllowInput

# Remove everything again
.\scripts\uninstall-autostart.ps1
```

What the installer creates: one shortcut per component in the
**current user's Startup folder** (`shell:startup`), launching
minimized. That is the entire mechanism. Verify it yourself:
open `shell:startup` and look at it.

## Why this mechanism and not the others

| Mechanism | Verdict | Reason |
|---|---|---|
| Startup-folder shortcut (chosen) | Yes | User-visible, user-removable, runs as the normal user, in the interactive session (capture/input work). |
| Registry Run key | No | Same effect but hidden from casual inspection; a shortcut the user can see beats a key they cannot. |
| Task Scheduler, logon trigger | No | Tempting, but it invites "run with highest privileges", which would elevate a network-facing agent on every boot. |
| Windows Service | No | Services run in session 0: no interactive desktop, so no screen capture and no input injection. Wrong tool, not just a heavier one. |
| Agent self-installing persistence | Never | The binary never writes Startup, registry, or tasks by itself. Only the installer script, run deliberately by you, creates persistence. This is what keeps the security audit's "no self-persistence" claim true. |

## Modes — what autostart actually runs

| Autostart flags | What happens at boot | Market use |
|---|---|---|
| (none) | Presence heartbeat if paired; nothing else | Phone sees the laptop online; files/sessions need a manual agent run |
| `-Remote` | Also polls for remote sessions, **every one prompts at this keyboard** | Useless unattended: the prompt rejects when nobody is here |
| `-Remote -TrustPaired` | Sessions from the **already-paired controller only** auto-accept; anyone else is rejected | **This is the market mode**: laptop at home, owner away, phone drives the screen |
| `+ -AllowInput` | Above, plus input injection | Full unattended control — only with eyes open (below) |

Two separations are load-bearing and stay separate on purpose:

- `--trust-paired` never implies `--allow-input`. Auto-accepting
  a *view* and auto-accepting *keystrokes* are different trusts.
- Pairing approval is untouched. Autostart never pairs a new
  device; it only serves the controller that was already
  approved face-to-face (well, screen-to-screen) at pairing time.

## Loophole audit — what could still bite, honestly

| Loophole | Severity | Mitigation in place | Residual risk / your job |
|---|---|---|---|
| Unattended auto-accept becomes blanket trust | High if misunderstood | Scoped to the single paired controller id; anyone else is rejected, never prompted; startup banner says UNATTENDED MODE; session indicator + toast still fire per session; revoke kills live sessions instantly | You must understand: pairing = trust from then on. Pair only your own phone. |
| Unattended input on a logged-in, unlocked desktop | High | Input needs its own flag on top of trust; banner warns; lock screen note below | Lock the laptop (Win+L) when you leave it. An unlocked session + trusted phone = full control **by design**. |
| Input at the lock screen | Medium | Nothing in the agent distinguishes lock-screen input from desktop input — that is Windows behaviour, not an Abhiverse bypass | Do not combine `-AllowInput` autostart with a weak/absent Windows password. Physical access already implies this. |
| Duplicate agents (autostart + manual launch) | Low | Installer refuses a second shortcut unless `-Force`; docs warn against double runs | No in-code single-instance mutex yet: two processes double-heartbeat (harmless) and double-poll sessions (first accept wins, second errors — annoying, not a hole). A mutex is a future hardening item. |
| Stale shortcut after moving/deleting the binary | Low | Installer resolves the exe path at install time and refuses a missing binary | If you move the repo, re-run the installer. A dead shortcut fails silently at logon — check `shell:startup` after a move. |
| Autostart without a local API (LAN mode) | Low | Heartbeats retry forever on transient failure; nothing crashes | In LAN mode, use `-IncludeApi` or start the API yourself. In hosted/tunnel mode, no local API is wanted at all. |
| Tunnel URL change (tunnel path) | Low | Documented in `docs/remote-access.md` | A changed tunnel URL needs re-pointing + re-pairing; use a named tunnel or the VPS path for stability. |
| Orphaned autostart after "uninstalling" | Low | The uninstaller removes exactly what the installer created; nothing hides elsewhere | Verify in `shell:startup` after uninstalling. There is no other persistence to hunt. |
| Stolen laptop / stolen phone | High if unprepared | Revoke from either side, instantly, including live sessions; keys never leave devices | Revoke immediately on loss. Autostart does not change this — it only changes *when* the agent is running, not *who* it trusts. |

## What autostart does NOT do

- It never pairs, never approves a new device, never enables
  input by itself, and never elevates privileges.
- It never phones home: the agent talks only to the API URL in
  the pairing record (LAN address or your hosted URL).
- It never survives uninstall: delete the shortcut and the next
  boot is clean. A running agent must still be stopped with
  Ctrl+C in its window.

## First-run checklist for autostart

1. Pair first, autostart second (autostart without a pairing
   just prints "not paired yet" every boot).
2. Start with no flags. Confirm presence works from the phone.
3. Add `-Remote -TrustPaired` only when you actually need
   unattended viewing. Add `-AllowInput` only when you need
   unattended control — and lock the machine when you walk away.
4. Test the revoke path once: revoke from the phone, confirm
   the next session offer is dead. If revoke ever stops working,
   autostart comes off first and questions come second.
