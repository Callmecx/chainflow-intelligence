[CmdletBinding()]
param(
    [ValidateRange(15, 120)][int]$DurationMinutes = 15
)
. (Join-Path $PSScriptRoot 'common.ps1')

function Get-Field($Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-CounterDelta($First, $Last, [bool]$SameRuntime) {
    if (-not $SameRuntime -or $null -eq $First -or $null -eq $Last) { return $null }
    $difference = [long]$Last - [long]$First
    if ($difference -lt 0) { return $null }
    return $difference
}

function Get-CanaryObservation {
    $checkedAt = [DateTime]::UtcNow
    $entry = [ordered]@{
        checked_at_utc = $checkedAt.ToString('o')
        api_available = $false
        health_status = 'UNAVAILABLE'
        runtime_started_at_utc = $null
        generated_at_utc = $null
        snapshot_age_seconds = $null
        samples = $null
        pools = $null
        solana_pools = $null
        bsc_pools = $null
        signals = $null
        visible_risk_signals = $null
        scans = $null
        accepted_samples = $null
        provider_errors_total = $null
        api_requests_total = $null
        last_scan_duration_ms = $null
        memory_working_set_bytes = $null
        memory_measurement = 'UNAVAILABLE'
        database_integrity_check = $null
        database_journal_mode = $null
        sources = @()
        error_kind = $null
    }
    try {
        $health = Invoke-RestMethod 'http://127.0.0.1:8787/api/health' -TimeoutSec 10
        $snapshot = Invoke-RestMethod 'http://127.0.0.1:8787/api/tokens' -TimeoutSec 10
        if ((Get-Field $health 'mode') -ne 'LOCAL_LIVE' -or (Get-Field $snapshot 'mode') -ne 'LOCAL_LIVE') {
            throw 'The endpoint is not a ChainFlow LOCAL_LIVE API.'
        }
        $runtime = Get-Field (Get-Field $snapshot 'metrics') 'runtime'
        $metrics = Get-Field $snapshot 'metrics'
        $database = Get-Field $health 'database'
        $entry.api_available = $true
        $entry.health_status = [string](Get-Field $health 'status')
        $entry.runtime_started_at_utc = Get-Field $runtime 'started_at'
        $entry.generated_at_utc = Get-Field $snapshot 'generated_at_utc'
        if ($entry.generated_at_utc) {
            $entry.snapshot_age_seconds = [Math]::Round(($checkedAt - ([DateTimeOffset]$entry.generated_at_utc).UtcDateTime).TotalSeconds, 2)
        }
        $entry.samples = Get-Field $metrics 'samples'
        $entry.pools = Get-Field $metrics 'pools'
        $items = @(Get-Field $snapshot 'items')
        $entry.solana_pools = @($items | Where-Object { $_.chain -eq 'solana' }).Count
        $entry.bsc_pools = @($items | Where-Object { $_.chain -eq 'bsc' }).Count
        $entry.signals = Get-Field $metrics 'signals'
        $entry.visible_risk_signals = @((Get-Field $snapshot 'signals') | Where-Object { $null -ne $_ -and $_.severity -eq 'RISK' }).Count
        $entry.scans = Get-Field $runtime 'scans'
        $entry.accepted_samples = Get-Field $runtime 'accepted_samples'
        $entry.provider_errors_total = Get-Field $runtime 'provider_errors'
        $entry.api_requests_total = Get-Field $runtime 'api_requests'
        $entry.last_scan_duration_ms = Get-Field $runtime 'last_scan_duration_ms'
        $entry.database_integrity_check = Get-Field $database 'integrity_check'
        $entry.database_journal_mode = Get-Field $database 'journal_mode'
        foreach ($source in @(Get-Field $snapshot 'sources')) {
            $lastSuccess = Get-Field $source 'last_success_at'
            $lag = $null
            if ($lastSuccess) {
                $lag = [Math]::Round(($checkedAt - ([DateTimeOffset]$lastSuccess).UtcDateTime).TotalSeconds, 2)
            }
            $entry.sources += [ordered]@{
                id = Get-Field $source 'id'
                status = Get-Field $source 'status'
                checked_at_utc = Get-Field $source 'checked_at'
                last_success_at_utc = $lastSuccess
                last_success_age_seconds = $lag
                latency_ms = Get-Field $source 'latency_ms'
                requests_total = Get-Field $source 'requests'
                errors_total = Get-Field $source 'errors'
            }
        }
        # Read only our executable's actual process metrics. Never publish PID/path.
        $processIdValue = Get-Field $runtime 'pid'
        if ($processIdValue) {
            try {
                $process = Get-Process -Id ([int]$processIdValue) -ErrorAction Stop
                $expectedPaths = @(
                    (Join-Path $script:ProjectRoot 'backend\target\debug\chainflow.exe'),
                    (Join-Path $script:ProjectRoot 'backend\target\release\chainflow.exe')
                )
                if ($process.Path -in $expectedPaths) {
                    $process.Refresh()
                    $entry.memory_working_set_bytes = $process.WorkingSet64
                    $entry.memory_measurement = 'Windows Get-Process WorkingSet64'
                }
            } catch { $entry.memory_measurement = 'UNAVAILABLE' }
        }
    } catch {
        # Avoid leaking arbitrary upstream errors or local paths into public evidence.
        $entry.error_kind = $_.Exception.GetType().Name
        $entry.api_available = $false
        $entry.health_status = 'SOURCE_ERROR'
    }
    return [pscustomobject]$entry
}

$evidenceDirectory = Join-Path $script:ProjectRoot 'docs\evidence'
New-Item -ItemType Directory -Path $evidenceDirectory -Force | Out-Null
$evidencePath = Join-Path $evidenceDirectory 'canary.json'
$observations = [Collections.Generic.List[object]]::new()
$startedAt = [DateTime]::UtcNow
$timer = [Diagnostics.Stopwatch]::StartNew()
$targetSeconds = $DurationMinutes * 60
$report = [ordered]@{
    kind = 'REAL_LOCAL_CANARY'
    status = 'RUNNING'
    started_at_utc = $startedAt.ToString('o')
    ended_at_utc = $null
    requested_duration_seconds = $targetSeconds
    elapsed_seconds = 0
    minimum_duration_met = $false
    interval_seconds = 60
    observations = @()
    summary = $null
}

Write-Host "Observing the already-running local API for at least $DurationMinutes minutes. No service will be started or stopped."
while ($true) {
    $observation = Get-CanaryObservation
    $observations.Add($observation)
    $report.elapsed_seconds = [Math]::Round($timer.Elapsed.TotalSeconds, 2)
    $report.observations = @($observations.ToArray())
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $evidencePath -Encoding UTF8
    Write-Host ("UTC {0}: health={1}; samples={2}; pools={3}; provider_errors={4}" -f $observation.checked_at_utc, $observation.health_status, $observation.samples, $observation.pools, $observation.provider_errors_total)
    if ($timer.Elapsed.TotalSeconds -ge $targetSeconds) { break }
    $nextTick = [Math]::Min(($observations.Count * 60), $targetSeconds)
    $remaining = $nextTick - $timer.Elapsed.TotalSeconds
    if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining * 1000)) }
}
$timer.Stop()
$valid = @($observations | Where-Object { $_.api_available })
$summary = [ordered]@{
    observations = $observations.Count
    successful_api_observations = $valid.Count
    failed_api_observations = $observations.Count - $valid.Count
    same_runtime = $null
    completed_scans_delta = $null
    accepted_samples_delta = $null
    provider_errors_delta = $null
    signals_delta = $null
    api_requests_delta = $null
    database_integrity_check_final = $null
    memory_working_set_min_bytes = $null
    memory_working_set_max_bytes = $null
    sources = @()
    note = 'Counter deltas are unknown after a restart, absent baseline, or counter reset. Completion only confirms observation duration, not provider health.'
}
if ($valid.Count -gt 0) {
    $first = $valid[0]
    $last = $valid[-1]
    $sameRuntime = @($valid | Select-Object -ExpandProperty runtime_started_at_utc -Unique).Count -eq 1
    $summary.same_runtime = $sameRuntime
    $summary.completed_scans_delta = Get-CounterDelta $first.scans $last.scans $sameRuntime
    $summary.accepted_samples_delta = Get-CounterDelta $first.accepted_samples $last.accepted_samples $sameRuntime
    $summary.provider_errors_delta = Get-CounterDelta $first.provider_errors_total $last.provider_errors_total $sameRuntime
    $summary.signals_delta = Get-CounterDelta $first.signals $last.signals $sameRuntime
    $summary.api_requests_delta = Get-CounterDelta $first.api_requests_total $last.api_requests_total $sameRuntime
    # Only the final observation is evidence of final database health.
    $summary.database_integrity_check_final = $observations[$observations.Count - 1].database_integrity_check
    $memoryValues = @($valid | Where-Object { $null -ne $_.memory_working_set_bytes } | ForEach-Object { $_.memory_working_set_bytes })
    if ($memoryValues.Count -gt 0) {
        $memory = $memoryValues | Measure-Object -Minimum -Maximum
        $summary.memory_working_set_min_bytes = $memory.Minimum
        $summary.memory_working_set_max_bytes = $memory.Maximum
    }
    foreach ($source in $last.sources) {
        $baseline = @($first.sources | Where-Object { $_.id -eq $source.id })
        $firstRequests = $null
        $firstErrors = $null
        if ($baseline.Count -gt 0) { $firstRequests = $baseline[0].requests_total; $firstErrors = $baseline[0].errors_total }
        $summary.sources += [ordered]@{
            id = $source.id
            final_status = $source.status
            requests_total_final = $source.requests_total
            requests_delta = Get-CounterDelta $firstRequests $source.requests_total $sameRuntime
            errors_total_final = $source.errors_total
            errors_delta = Get-CounterDelta $firstErrors $source.errors_total $sameRuntime
            last_success_at_utc = $source.last_success_at_utc
            last_success_age_seconds_final = $source.last_success_age_seconds
        }
    }
}
$report.status = 'COMPLETED'
$report.ended_at_utc = [DateTime]::UtcNow.ToString('o')
$report.elapsed_seconds = [Math]::Round($timer.Elapsed.TotalSeconds, 2)
$report.minimum_duration_met = $timer.Elapsed.TotalSeconds -ge $targetSeconds
$report.summary = $summary
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $evidencePath -Encoding UTF8
Write-Host 'Observation complete. Review docs/evidence/canary.json for actual provider and database results.'
