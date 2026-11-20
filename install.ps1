# Builds the release version and installs it to %LOCALAPPDATA%\Programs\akp03e.
# Autostart (enabled in the panel or the tray menu) points to the installed copy,
# so it does not depend on the source folder.
#
# Usage:  .\install.ps1

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

# MinGW and cargo may have been added to PATH after this terminal was opened.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine')

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }

$target = Join-Path $env:LOCALAPPDATA 'Programs\akp03e'
New-Item -ItemType Directory -Force $target | Out-Null
$exe = Join-Path $target 'akp03e-panel.exe'

# Stop the running instance (the .exe is locked while the program runs).
$running = Get-Process akp03e, akp03e-panel -ErrorAction SilentlyContinue
if ($running) {
    Write-Host 'Stopping the running program...'
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

# Copy next to it and swap – never delete the working version before the copy succeeds.
$tmp = "$exe.new"
Copy-Item .\target\release\akp03e.exe $tmp -Force
for ($i = 1; ; $i++) {
    try { Move-Item $tmp $exe -Force; break }
    catch { if ($i -ge 20) { throw }; Start-Sleep -Milliseconds 500 }
}
Write-Host "Installed: $exe"

# Start through Explorer so the program is not a child of this terminal
# (some terminals freeze or close their child processes).
Start-Process explorer.exe -ArgumentList "`"$exe`""
Write-Host 'Started. The icon is in the system tray, panel: http://127.0.0.1:7878'
