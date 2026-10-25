# Abhiverse — start the API on the LAN address (Windows).
#
# Used by the autostart API shortcut (see install-autostart.ps1
# -IncludeApi). Detects the current LAN IPv4 at launch, because
# DHCP moves it between boots and a hardcoded address rots.
# LAN mode only: hosted/tunnel setups must not run a local API.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$apiExe = Join-Path $root "target\release\abhiverse-api.exe"
if (-not (Test-Path $apiExe)) {
    throw "API binary not found: $apiExe. Build first: cargo build --release -p abhiverse-api"
}
$lanIp = (Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -match '^(192\.168\.|10\.|172\.(1[6-9]|2\d|3[01])\.)' -and $_.PrefixOrigin -ne 'WellKnown' } |
    Select-Object -First 1).IPAddress
if (-not $lanIp) { throw "Could not detect a LAN IPv4 address; connect to a trusted network first." }
$env:ABHIVERSE_BIND_ADDR = "${lanIp}:8080"
$env:ABHIVERSE_STATE_FILE = Join-Path $root "abhiverse-state.json"
Write-Host "Abhiverse API on http://${lanIp}:8080 (trusted network only!)"
& $apiExe
