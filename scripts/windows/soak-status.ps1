[CmdletBinding()]
param([string]$RunId)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$runId = Resolve-SoakId $RunId
$result = Write-SoakReport $runId
$result | Select-Object run_id,kind,status,started_at_utc,ended_at_utc,resume_count,summary | ConvertTo-Json -Depth 12
