# Abhiverse — remove agent autostart (Windows, current user only).
#
# Deletes the Startup-folder shortcuts created by
# scripts/install-autostart.ps1. Nothing else is touched: your
# pairing, keys, and state file stay exactly where they are, and
# a currently running agent keeps running until you stop it
# (Ctrl+C in its window). Removing autostart only stops the
# NEXT logon from launching it again.
$ErrorActionPreference = "Stop"
$startup = [Environment]::GetFolderPath("Startup")
$names = @("Abhiverse Agent.lnk", "Abhiverse API (LAN).lnk")
$removed = 0
foreach ($name in $names) {
    $path = Join-Path $startup $name
    if (Test-Path $path) {
        Remove-Item $path -Force
        Write-Host "Removed: $path"
        $removed++
    }
}
if ($removed -eq 0) {
    Write-Host "No Abhiverse autostart entries found in $startup."
} else {
    Write-Host "Autostart removed. Stop a running agent with Ctrl+C in its window."
}
