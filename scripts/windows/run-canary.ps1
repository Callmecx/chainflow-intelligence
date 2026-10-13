[CmdletBinding()]
param([ValidateRange(15,30)][int]$DurationMinutes = 15)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$state = New-SoakRun ($DurationMinutes * 60) 'REAL_LOCAL_CANARY_V1_1'
Write-Host "Canary run ID: $($state.run_id). This command only observes an existing local service."
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'soak-worker.ps1') -RunId $state.run_id
if ($LASTEXITCODE -ne 0) { throw "Canary observer failed. Review .runtime/soaks/$($state.run_id)/report.json." }
Write-Host "Report: .runtime/soaks/$($state.run_id)/report.md"
