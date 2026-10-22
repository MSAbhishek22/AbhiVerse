# Abhiverse — one-command real-device test runner (Windows).
#
# Builds the API, agent and APK from this checkout, starts the
# API, opens the agent pairing window, and prints the ordered
# test steps from docs/real-device-test.md. It cannot approve
# the pairing or judge video quality for you — those are the
# human parts of the run.
#
# Usage:
#   .\scripts\run-real-device-test.ps1                # LAN mode
#   .\scripts\run-real-device-test.ps1 -UseAdbReverse # USB only, no LAN exposure
#   .\scripts\run-real-device-test.ps1 -SkipApkBuild  # reuse the last APK
[CmdletBinding()]
param(
    [string]$LanIp,
    [switch]$UseAdbReverse,
    [switch]$SkipApkBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Need($cmd, $hint) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        throw "Missing '$cmd'. $hint"
    }
}
Need cargo "Install Rust from https://rustup.rs"
Need flutter "Install Flutter: https://docs.flutter.dev/get-started/install/windows"

Write-Host "== Building API + agent (release) =="
cargo build --release -p abhiverse-api -p abhiverse-agent-windows
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$apiExe = "$root\target\release\abhiverse-api.exe"
$agentExe = "$root\target\release\abhiverse-agent.exe"

if ($UseAdbReverse) {
    $apiBase = "http://127.0.0.1:8080"
    $bind = "127.0.0.1:8080"
    Need adb "Install Android platform-tools or enable Flutter's bundled adb."
} else {
    if (-not $LanIp) {
        $LanIp = (Get-NetIPAddress -AddressFamily IPv4 |
            Where-Object { $_.IPAddress -match '^(192\.168\.|10\.|172\.(1[6-9]|2\d|3[01])\.)' -and $_.PrefixOrigin -ne 'WellKnown' } |
            Select-Object -First 1).IPAddress
    }
    if (-not $LanIp) { throw "Could not detect a LAN IPv4 address; pass -LanIp x.x.x.x" }
    $apiBase = "http://${LanIp}:8080"
    $bind = "${LanIp}:8080"
    Write-Host "LAN mode: API at $apiBase (trusted network only!)"
}

if (-not $SkipApkBuild) {
    Write-Host "== Building APK (API base: $apiBase) =="
    Push-Location "$root\apps\controller"
    flutter build apk --release --dart-define=ABHIVERSE_API_BASE_URL=$apiBase
    if ($LASTEXITCODE -ne 0) { throw "flutter build apk failed" }
    Pop-Location
}
$apk = "$root\apps\controller\build\app\outputs\flutter-apk\app-release.apk"

$adbDevice = $null
if (Get-Command adb -ErrorAction SilentlyContinue) {
    $adbDevice = (adb devices | Select-String "`tdevice$").Line.Split("`t")[0]
}
if ($adbDevice) {
    Write-Host "== Installing APK on $adbDevice =="
    adb -s $adbDevice install -r $apk | Out-Host
    if ($UseAdbReverse) { adb -s $adbDevice reverse tcp:8080 tcp:8080 | Out-Host }
} else {
    Write-Host "No adb device found. Install the APK manually: $apk"
}

Write-Host "== Starting API (bind $bind) =="
$envBlock = "`$env:ABHIVERSE_BIND_ADDR='$bind'; `$env:ABHIVERSE_STATE_FILE='$root\abhiverse-state.json'; & '$apiExe'"
Start-Process powershell -ArgumentList "-NoExit", "-Command", $envBlock
for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Milliseconds 500
    try {
        $h = Invoke-RestMethod -Uri "http://127.0.0.1:8080/health" -TimeoutSec 2
        if ($h.status -eq "ok") { break }
    } catch { }
}
Write-Host "API health: $($h | ConvertTo-Json -Compress)"

Write-Host "== Opening agent pairing window =="
Start-Process powershell -NoExit -ArgumentList "-Command", "& '$agentExe' --pair"

Write-Host @"

NEXT — the human part (docs/real-device-test.md):
  1. On the phone, open Abhiverse, check http://$($apiBase.TrimStart('http://'))/health
     loads in the phone browser first (firewall: allow Private networks).
  2. In the app: pair using the QR in the agent window, approve with 'y'.
  3. Work the matrix top to bottom: transfer, video, input
     (restart the agent with --remote --allow-input for the input step),
     network switch, failure cases.
  4. Record results in the matrix; 'untested' is an honest answer.

Stop everything: close the API/agent windows (Ctrl+C).
Revoke: app device menu, or  & '$agentExe' --revoke
"@
