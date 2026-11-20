# Diagnostics for the installed program: processes, autostart, log, panel.
Write-Host '--- processes'
Get-CimInstance Win32_Process -Filter "Name like 'akp03e%'" | ForEach-Object {
    $parent = (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.ParentProcessId)").Name
    Write-Host ("pid {0} started {1:HH:mm:ss} parent {2}: {3}" -f $_.ProcessId, $_.CreationDate, $parent, $_.CommandLine)
}
Write-Host '--- autostart (HKCU Run)'
Write-Host (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name AKP03E -ErrorAction SilentlyContinue).AKP03E
Write-Host '--- log'
Get-Content (Join-Path $env:APPDATA 'akp03e\akp03e.log') -ErrorAction SilentlyContinue | Select-Object -Last 15 | ForEach-Object { Write-Host $_ }
Write-Host '--- panel'
try { Write-Host (Invoke-WebRequest http://127.0.0.1:7878/api/state -UseBasicParsing -TimeoutSec 3).Content } catch { Write-Host "no response: $($_.Exception.Message)" }
