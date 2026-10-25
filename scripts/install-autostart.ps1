# Abhiverse — install agent autostart (Windows, current user only).
#
# Creates a shortcut in YOUR Startup folder, so the agent launches
# at logon, minimized, as the normal user. That is the whole
# mechanism — no admin rights, no registry keys, no Windows
# service, and nothing the agent does by itself:
#
#   - A service cannot touch the interactive desktop (no screen
#     capture, no input injection), so a service is the wrong
#     tool for this agent, not just a heavier one.
#   - An admin autostart would run the agent elevated on every
#     boot — a privilege hole for a network-facing tool. Never.
#   - The agent binary never installs persistence on its own.
#     Only this script, run deliberately by you, creates it.
#     Undo it any time with scripts/uninstall-autostart.ps1
#     (or by deleting the shortcut from shell:startup).
#
# Usage (from the repo root):
#   .\scripts\install-autostart.ps1
#   .\scripts\install-autostart.ps1 -IncludeApi
#   .\scripts\install-autostart.ps1 -Remote -TrustPaired
#
# Modes — pick deliberately, widest last:
#   default            presence heartbeat only. Safe to autostart.
#   -Remote            also answers remote desktop sessions, but
#                      every session still prompts at this keyboard.
#                      (Unattended, that prompt rejects: nobody is
#                      here to press y. See -TrustPaired.)
#   -Remote -TrustPaired  unattended mode: sessions from your
#                      ALREADY-PAIRED controller are auto-accepted.
#                      Anyone else is rejected. This is the mode
#                      that makes "laptop at home, owner in the
#                      market" work — and it must be chosen, never
#                      defaulted. Revoke the pairing to stop it.
#   -AllowInput        needs -Remote. Input injection on autostart.
#                      Combine with -TrustPaired only if you mean
#                      full unattended control, and lock this
#                      computer (Win+L) when you leave it.
[CmdletBinding()]
param(
    [string]$AgentExe,
    [string]$ApiExe,
    [switch]$IncludeApi,
    [switch]$Remote,
    [switch]$TrustPaired,
    [switch]$AllowInput,
    [switch]$Force
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $AgentExe) { $AgentExe = Join-Path $root "target\release\abhiverse-agent.exe" }
if (-not (Test-Path $AgentExe)) {
    throw "Agent binary not found: $AgentExe. Build first: cargo build --release -p abhiverse-agent-windows"
}
if ($TrustPaired -and -not $Remote) {
    throw "-TrustPaired needs -Remote. It only affects remote sessions."
}
if ($AllowInput -and -not $Remote) {
    throw "-AllowInput needs -Remote. It only affects remote sessions."
}

$startup = [Environment]::GetFolderPath("Startup")
$shell = New-Object -ComObject WScript.Shell

function Install-Shortcut($name, $target, $arguments) {
    $path = Join-Path $startup $name
    if ((Test-Path $path) -and -not $Force) {
        throw "Already installed: $path (delete it or re-run with -Force to replace it)."
    }
    $sc = $shell.CreateShortcut($path)
    $sc.TargetPath = $target
    $sc.Arguments = $arguments
    $sc.WorkingDirectory = Split-Path $target -Parent
    $sc.WindowStyle = 7  # minimized: no console popping up every logon
    $sc.Description = "Abhiverse agent (autostart)"
    $sc.Save()
    Write-Host "Installed: $path"
    Write-Host "  -> $target $arguments"
}

$agentArgs = @()
if ($Remote) { $agentArgs += "--remote" }
if ($TrustPaired) { $agentArgs += "--trust-paired" }
if ($AllowInput) { $agentArgs += "--allow-input" }
Install-Shortcut "Abhiverse Agent.lnk" $AgentExe ($agentArgs -join " ")

if ($IncludeApi) {
    # LAN mode only: a hosted/tunnel setup needs no local API.
    # The API shortcut runs the LAN starter, which detects the
    # current LAN address at logon (DHCP changes it otherwise).
    $apiStarter = Join-Path $root "scripts\start-api-lan.ps1"
    $ps = (Get-Command powershell.exe -ErrorAction SilentlyContinue)
    if (-not $ps) { throw "powershell.exe not found; cannot install the API shortcut." }
    Install-Shortcut "Abhiverse API (LAN).lnk" $ps.Source "-WindowStyle Minimized -ExecutionPolicy Bypass -File `"$apiStarter`""
}

Write-Host ""
Write-Host "Autostart installed for the current user only."
if ($TrustPaired) {
    Write-Host "UNATTENDED MODE: sessions from your paired controller will auto-accept on this machine." -ForegroundColor Yellow
    Write-Host "Anyone else is rejected. Revoke the pairing to stop this instantly." -ForegroundColor Yellow
}
if ($AllowInput) {
    Write-Host "INPUT ENABLED on autostart. Lock this computer (Win+L) when you leave it." -ForegroundColor Yellow
}
if ($AllowInput -and -not $TrustPaired) {
    Write-Host "Note: --allow-input without --trust-paired has no unattended effect — sessions still prompt at this keyboard."
}
if ($IncludeApi) {
    Write-Host "Note: the API shortcut is for LAN mode. Hosted/tunnel setups must NOT also start a local API."
}
Write-Host "Undo: .\scripts\uninstall-autostart.ps1  (or delete the shortcut from shell:startup)"
