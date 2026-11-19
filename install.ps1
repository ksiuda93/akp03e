# Buduje wersję release i instaluje ją do %LOCALAPPDATA%\Programs\akp03e.
# Autostart (włączany w panelu albo w menu ikony) wskazuje na zainstalowaną kopię,
# więc działa niezależnie od folderu z kodem.
#
# Użycie:  .\install.ps1

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

# MinGW i cargo mogły zostać dodane do PATH po otwarciu tego terminala.
$env:Path = [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + [Environment]::GetEnvironmentVariable('Path', 'Machine')

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'Kompilacja nie powiodła się' }

$target = Join-Path $env:LOCALAPPDATA 'Programs\akp03e'
New-Item -ItemType Directory -Force $target | Out-Null
$exe = Join-Path $target 'akp03e-panel.exe'

# Zatrzymaj działającą instancję (plik .exe jest zablokowany, gdy program działa).
$running = Get-Process akp03e, akp03e-panel -ErrorAction SilentlyContinue
if ($running) {
    Write-Host 'Zatrzymuję działający program…'
    $running | Stop-Process -Force
    $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
}

# Kopiujemy obok i podmieniamy – nigdy nie kasujemy działającej wersji przed udanym kopiowaniem.
$tmp = "$exe.new"
Copy-Item .\target\release\akp03e.exe $tmp -Force
for ($i = 1; ; $i++) {
    try { Move-Item $tmp $exe -Force; break }
    catch { if ($i -ge 20) { throw }; Start-Sleep -Milliseconds 500 }
}
Write-Host "Zainstalowano: $exe"

# Uruchom przez Eksploratora, żeby program nie był potomkiem tego terminala
# (niektóre terminale zamrażają lub zamykają procesy potomne).
Start-Process explorer.exe -ArgumentList "`"$exe`""
Write-Host 'Uruchomiono. Ikona jest w zasobniku systemowym, panel: http://127.0.0.1:7878'
