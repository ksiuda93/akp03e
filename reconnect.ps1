# Prosi działający program o ponowne połączenie z kontrolerem i pokazuje stan.
Invoke-RestMethod -Method Post http://127.0.0.1:7878/api/reconnect | Out-Null
Start-Sleep 4
& (Join-Path $PSScriptRoot 'status.ps1')
