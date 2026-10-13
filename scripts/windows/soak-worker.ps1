[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$RunId)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$directory = Get-SoakDirectory $RunId
$statePath = Join-Path $directory 'state.json'
$state = Read-SoakJson $statePath
if ($state.workspace -ne $script:ProjectRoot) { throw 'Workspace mismatch.' }
$lock = [IO.File]::Open((Join-Path $directory 'worker.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try {
    $state.worker = Get-ProcessIdentity -ProcessId $PID -Marker $PSCommandPath
    if (-not $state.worker) { throw 'Unable to verify observer identity.' }
    $state.status = 'RUNNING'; $state.observer_error = $null
    Write-SoakJson $statePath $state
    $rows = [Collections.Generic.List[object]]::new()
    foreach ($existing in @(Get-SoakRows $directory)) { $rows.Add($existing) }
    while ($true) {
        if (Test-Path -LiteralPath (Join-Path $directory 'stop.request')) { $state.status='STOPPED'; break }
        $observation = Get-SoakObservation
        $rows.Add($observation)
        $line = ($observation | ConvertTo-Json -Compress -Depth 16) + [Environment]::NewLine
        [IO.File]::AppendAllText((Join-Path $directory 'observations.jsonl'), $line, [Text.UTF8Encoding]::new($false))
        $state.heartbeat_at_utc = [DateTime]::UtcNow.ToString('o'); Write-SoakJson $statePath $state
        Write-Host ("UTC {0} health={1} samples={2} pools={3}" -f $observation.at,$observation.health,$observation.samples,$observation.pools)
        $null = Write-SoakReport $RunId
        $span = (([DateTimeOffset]$observation.at)-([DateTimeOffset]$rows[0].at)).TotalSeconds
        if ($span -ge $state.duration_seconds) { $state.status='COMPLETED'; break }
        $nextAt = ([DateTimeOffset]$observation.at).AddSeconds($state.interval_seconds)
        while ([DateTimeOffset]::UtcNow -lt $nextAt) {
            if (Test-Path -LiteralPath (Join-Path $directory 'stop.request')) { break }
            Start-Sleep -Milliseconds 500
        }
    }
} catch {
    $state.status='FAILED'; $state.observer_error=$_.Exception.GetType().Name
    Write-Warning 'Observer failed; only the exception type is retained in the public-safe report.'
} finally {
    $state.ended_at_utc=[DateTime]::UtcNow.ToString('o'); Write-SoakJson $statePath $state
    $null=Write-SoakReport $RunId
    $lock.Dispose()
}
if ($state.status -eq 'FAILED') { exit 1 }
