# Asks the running program to reconnect to the controller and shows its state.
Invoke-RestMethod -Method Post http://127.0.0.1:7878/api/reconnect | Out-Null
Start-Sleep 4
& (Join-Path $PSScriptRoot 'status.ps1')
