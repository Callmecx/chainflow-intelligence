# Offline deterministic fixtures only. This does not constitute a natural soak.
[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$checks = [Collections.Generic.List[string]]::new()
function Assert-Fixture([bool]$Condition, [string]$Name) {
    if (-not $Condition) { throw "OFFLINE_FIXTURE failed: $Name" }
    $checks.Add($Name)
}
function New-FixtureRow([int]$Minute, [bool]$Available, [string]$Runtime, [int]$Count, [string]$SourceState) {
    return [pscustomobject]@{
        at=([DateTimeOffset]'2026-01-01T00:00:00Z').AddMinutes($Minute).ToString('o'); api_available=$Available
        runtime_started_at=$Runtime; accepted_samples=$Count; process_cpu_total_seconds=$Count; logical_processors=2
        memory_working_set_bytes=1000000+$Count; database_integrity='ok'; data=[pscustomobject]@{stale_pools=0}
        signal_counts=[pscustomobject]@{WATCH=20+$Count;RISK=1;CONFIRMATION=0;DATA_FAULT=0}
        notification_counts=[pscustomobject]@{SENT=$Count;PENDING=0}
        sources=@([pscustomobject]@{id='FIXTURE_SOURCE';status=$SourceState;requests=$Count;errors=0})
    }
}
$rows=@(
    (New-FixtureRow 0 $true 'A' 10 'OK'),
    (New-FixtureRow 1 $false 'A' 10 'UNAVAILABLE'),
    (New-FixtureRow 2 $true 'A' 12 'SOURCE_ERROR'),
    (New-FixtureRow 3 $true 'A' 15 'OK'),
    (New-FixtureRow 10 $true 'B' 1 'OK'),
    (New-FixtureRow 11 $true 'B' 3 'OK')
)
$state=[pscustomobject]@{duration_seconds=660;max_gap_seconds=150}
$summary=Get-SoakSummary $state $rows
Assert-Fixture ($summary.api_recoveries -eq 1) 'API recovery counted independently'
Assert-Fixture ($summary.provider_recoveries -eq 1) 'Provider recovery counted independently'
Assert-Fixture ($summary.backend_restart_transitions -eq 1) 'Backend restart detected'
Assert-Fixture (@($summary.observer_gaps).Count -eq 1 -and $summary.observer_gaps[0].seconds -eq 420) 'Suspend gap retained'
Assert-Fixture ($summary.observed_connected_span_seconds -eq 120) 'Missing intervals excluded from connected coverage'
Assert-Fixture ($summary.duration_met -and $summary.actual_observation_span_seconds -eq 660) 'Actual timestamp duration computed'
Assert-Fixture ($summary.provider_requests_observed_delta -eq 5 -and -not $summary.counter_coverage_complete) 'Restart counters are partial, not full-window totals'
Assert-Fixture ($summary.signal_counts_delta.RISK -eq 0) 'WATCH activity is not counted as RISK'
Assert-Fixture ($null -eq (Get-SoakDelta 10 1)) 'Counter reset is UNKNOWN'
Assert-Fixture ($null -eq (Get-SoakDelta $null 1)) 'Absent baseline is UNKNOWN'
$resetSummary=Get-SoakSummary $state @((New-FixtureRow 0 $true 'A' 20 'OK'),(New-FixtureRow 1 $true 'A' 2 'OK'))
Assert-Fixture (-not $resetSummary.counter_coverage_complete -and $resetSummary.unknown_counter_values -gt 0) 'Same-process counter rollback cannot claim complete coverage'
$restartOutage=Get-SoakSummary $state @((New-FixtureRow 0 $true 'A' 20 'SOURCE_ERROR'),(New-FixtureRow 1 $false 'A' 20 'UNAVAILABLE'),(New-FixtureRow 2 $true 'B' 2 'OK'))
Assert-Fixture ($restartOutage.backend_restart_transitions -eq 1 -and $restartOutage.provider_recoveries -eq 1) 'Restart and provider recovery remain visible across API outage'
$fractionalRows=@((New-FixtureRow 0 $true 'A' 1 'OK'),(New-FixtureRow 1 $true 'A' 2 'OK'))
$fractionalRows[1].at='2026-01-01T00:01:00.394Z'
$fractional=Get-SoakSummary $state $fractionalRows
Assert-Fixture ($fractional.actual_observation_span_seconds -eq 60.394) 'Actual observation span preserves sub-second timestamps'
$empty=Get-SoakSummary $state @()
Assert-Fixture (-not $empty.duration_met -and $null -eq $empty.database_integrity_final) 'Empty run cannot pass duration or database checks'
Assert-Fixture (-not (Test-ProcessIdentity ([pscustomobject]@{pid=$PID;marker='';started_at_utc='bad';executable='bad'}))) 'Malformed process identity rejected'
Assert-Fixture (-not (Test-ProcessIdentity ([pscustomobject]@{pid=$PID;marker='C:\Windows\not-this-project';started_at_utc='bad';executable='bad'}))) 'Foreign marker rejected'
Assert-Fixture (-not (Test-SoakWorkerIdentity ([pscustomobject]@{pid=$PID;marker=(Join-Path $script:ProjectRoot 'backend\target\debug\chainflow.exe');started_at_utc='bad';executable='bad'}))) 'Observer manager rejects even this project backend as stop target'
$rejected=$false;try{$null=Get-SoakDirectory '..\outside'}catch{$rejected=$true}
Assert-Fixture $rejected 'Run ID path traversal rejected'
$filtered=Copy-SoakCounts ([pscustomobject]@{SENT=1;TELEGRAM_BOT_TOKEN='FIXTURE_SECRET';PENDING=2})
Assert-Fixture ($filtered.SENT -eq 1 -and $null -eq (Get-SoakField $filtered 'TELEGRAM_BOT_TOKEN')) 'Count allowlist omits secret-like strings'

# Mock HTTP in this process; no live API or service is touched.
function Invoke-RestMethod {
    param([string]$Uri,[int]$TimeoutSec)
    if ($Uri.EndsWith('/health')) {
        return [pscustomobject]@{mode='LOCAL_LIVE';status='OK';database=[pscustomobject]@{integrity_check='ok';journal_mode='wal'};telegram=[pscustomobject]@{token='FIXTURE_SECRET'}}
    }
    return [pscustomobject]@{mode='LOCAL_LIVE';generated_at_utc=[DateTime]::UtcNow.ToString('o');items=@();sources=@();metrics=[pscustomobject]@{samples=0;pools=0;runtime=[pscustomobject]@{started_at='A';pid=$null};signal_counts=[pscustomobject]@{WATCH=4;RISK=0};notification_counts=[pscustomobject]@{SENT=0;TOKEN='FIXTURE_SECRET'}}}
}
$observed=Get-SoakObservation
Assert-Fixture $observed.api_available 'Empty initial API payload remains a valid observation'
Assert-Fixture (-not (($observed|ConvertTo-Json -Depth 12) -match 'FIXTURE_SECRET')) 'Raw HTTP secrets absent from observation'
Assert-Fixture ($observed.data.fresh_pools -eq 0 -and $null -eq $observed.memory_working_set_bytes) 'Missing process metrics stay UNKNOWN'

# Fixtures must never replace the default real run chosen by status/stop tools.
$run=New-SoakRun 660 'OFFLINE_FIXTURE' -NoLatest
$directory=Get-SoakDirectory $run.run_id
$run.status='RUNNING';$run.worker=[pscustomobject]@{pid=0;marker='';started_at_utc='bad';executable='bad'}
Write-SoakJson (Join-Path $directory 'state.json') $run
foreach($row in $rows){[IO.File]::AppendAllText((Join-Path $directory 'observations.jsonl'),(($row|ConvertTo-Json -Compress -Depth 12)+[Environment]::NewLine),[Text.UTF8Encoding]::new($false))}
$report=Write-SoakReport $run.run_id
Assert-Fixture ($report.kind -eq 'OFFLINE_FIXTURE' -and $report.status -eq 'INTERRUPTED') 'Dead observer is INTERRUPTED with fixture label'
Assert-Fixture ((Test-Path -LiteralPath (Join-Path $directory 'report.md')) -and (Test-Path -LiteralPath (Join-Path $directory 'report.json'))) 'Markdown and JSON reports generated'
$output=[ordered]@{kind='OFFLINE_SOAK_SCRIPT_TEST';fixture=$true;tested_at_utc=[DateTime]::UtcNow.ToString('o');passed=$checks.Count;failed=0;checks=$checks.ToArray();natural_6h='PENDING';natural_24h='PENDING'}
Write-SoakJson (Join-Path $script:RuntimeRoot 'soak-script-tests.json') $output
$output|ConvertTo-Json -Depth 6
