[CmdletBinding()]
param([string]$RunId, [switch]$Force)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$runId = Resolve-SoakId $RunId; $directory = Get-SoakDirectory $runId
$state = Read-SoakJson (Join-Path $directory 'state.json')
if ($state.workspace -ne $script:ProjectRoot) { throw 'Workspace mismatch.' }
if (-not (Test-SoakWorkerIdentity $state.worker)) {
    Write-Warning 'No matching live observer. No process was stopped.'
    $null = Write-SoakReport $runId
    return
}
[IO.File]::WriteAllText((Join-Path $directory 'stop.request'), [DateTime]::UtcNow.ToString('o'))
for ($attempt=0; $attempt -lt 30; $attempt++) {
    if (-not (Test-SoakWorkerIdentity $state.worker)) { break }
    Start-Sleep -Milliseconds 500
}
if (Test-SoakWorkerIdentity $state.worker) {
    if (-not $Force) { Write-Warning 'Stop requested; observer is finishing a network read. Recheck status, or use -Force to stop only this verified observer.'; return }
    Stop-Process -Id ([int]$state.worker.pid)
    $state.status='STOPPED_FORCED'; $state.ended_at_utc=[DateTime]::UtcNow.ToString('o')
    Write-SoakJson (Join-Path $directory 'state.json') $state
}
$null = Write-SoakReport $runId
Write-Host 'Observer stopped. Rust monitoring and frontend were not stopped.'
