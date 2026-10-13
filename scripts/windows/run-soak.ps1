[CmdletBinding()]
param(
    [ValidateSet(6, 24)][int]$Hours = 6,
    [string]$Resume
)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
if ($Resume) {
    $runId = Resolve-SoakId $Resume; $directory = Get-SoakDirectory $runId
    $state = Read-SoakJson (Join-Path $directory 'state.json')
    if ($state.workspace -ne $script:ProjectRoot) { throw 'Workspace mismatch.' }
    if (Test-SoakWorkerIdentity $state.worker) { throw 'This run already has a verified live observer.' }
    if ($state.status -in @('COMPLETED','COMPLETED_WITH_GAPS')) { throw 'Completed runs are immutable. Start a new run.' }
    $state.resume_count++; $state.status='CREATED'; $state.ended_at_utc=$null
    Write-SoakJson (Join-Path $directory 'state.json') $state
    $stopPath=Join-Path $directory 'stop.request'; if(Test-Path -LiteralPath $stopPath){Remove-Item -LiteralPath $stopPath}
} else {
    $state = New-SoakRun ($Hours * 3600) 'REAL_LOCAL_SOAK'
    $runId = $state.run_id; $directory = Get-SoakDirectory $runId
}
$workerScript = Join-Path $PSScriptRoot 'soak-worker.ps1'
$shell = (Get-Command powershell.exe -ErrorAction Stop).Source
$stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$process = Start-Process -FilePath $shell -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$workerScript+'"'),'-RunId',$runId) -WorkingDirectory $script:ProjectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $directory "observer-$stamp.log") -RedirectStandardError (Join-Path $directory "observer-$stamp.error.log")
Start-Sleep -Milliseconds 500
if ($process.HasExited) { throw "Observer exited. Inspect .runtime/soaks/$runId/observer-$stamp.error.log." }
Write-Host "Detached observer started. Run ID: $runId"
Write-Host "Use soak-status.ps1 -RunId $runId and soak-stop.ps1 -RunId $runId."
Write-Host 'The existing Rust service remains independently managed. Closing this terminal does not stop the observer.'
