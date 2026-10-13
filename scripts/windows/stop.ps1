[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'common.ps1')
$statePath = Join-Path $script:RuntimeRoot 'processes.json'
if (-not (Test-Path -LiteralPath $statePath)) { Write-Host 'No managed process record. Nothing was stopped.'; return }
$state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
if ([IO.Path]::GetFullPath($state.workspace) -ne $script:ProjectRoot) { throw 'Workspace mismatch; refusing to stop any process.' }
$unresolved = @()
foreach ($entry in @($state.processes)) {
    if (-not (Get-Process -Id ([int]$entry.pid) -ErrorAction SilentlyContinue)) { continue }
    if (-not (Test-ProcessIdentity $entry)) {
        Write-Warning "PID $($entry.pid) identity differs; it was not stopped."
        $unresolved += $entry
        continue
    }
    Stop-Process -Id ([int]$entry.pid) -ErrorAction Stop
    Write-Host "Stopped verified project process $($entry.pid)."
}
if ($unresolved.Count -gt 0) {
    $state.processes = $unresolved
    Write-RuntimeState $state
    Write-Warning 'Unmatched records retained for diagnosis. No process tree or unrelated process was killed.'
} else {
    Remove-Item -LiteralPath $statePath
    Write-Host 'Project stopped. SQLite WAL recovery will run automatically at next startup.'
}
