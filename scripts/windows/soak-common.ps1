. (Join-Path $PSScriptRoot 'common.ps1')

function Get-SoakField($Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    if ($Object -is [Collections.IDictionary]) { return $Object[$Name] }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}
function Get-SoakDirectory([string]$RunId) {
    if ($RunId -notmatch '^[a-zA-Z0-9][a-zA-Z0-9-]{0,79}$') { throw 'Invalid run ID.' }
    return Join-Path $script:RuntimeRoot ('soaks\' + $RunId)
}
function Write-SoakJson([string]$Path, $Value) {
    $temporary = "$Path.$PID.tmp"
    [IO.File]::WriteAllText($temporary, ($Value | ConvertTo-Json -Depth 20), [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}
function Read-SoakJson([string]$Path) {
    return Get-Content -Raw -LiteralPath $Path -Encoding UTF8 | ConvertFrom-Json
}
function Test-SoakWorkerIdentity($Identity) {
    try {
        $expected = Join-Path $script:ProjectRoot 'scripts\windows\soak-worker.ps1'
        if ($null -eq $Identity -or [string]$Identity.marker -ne $expected) { return $false }
        return Test-ProcessIdentity $Identity
    } catch { return $false }
}
function Resolve-SoakId([string]$RunId) {
    if ($RunId) { $null = Get-SoakDirectory $RunId; return $RunId }
    $latest = Join-Path $script:RuntimeRoot 'soaks\latest.txt'
    if (-not (Test-Path -LiteralPath $latest)) { throw 'No soak run exists. Run run-soak.ps1 or run-canary.ps1 first.' }
    $resolved = (Get-Content -Raw -LiteralPath $latest).Trim()
    $null = Get-SoakDirectory $resolved
    return $resolved
}
function New-SoakRun([int]$DurationSeconds, [string]$Kind, [switch]$NoLatest) {
    $runId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ') + '-' + [Guid]::NewGuid().ToString('N').Substring(0, 6)
    $directory = Get-SoakDirectory $runId
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $state = [ordered]@{
        schema_version = 1; run_id = $runId; kind = $Kind; status = 'CREATED'; workspace = $script:ProjectRoot
        started_at_utc = [DateTime]::UtcNow.ToString('o'); ended_at_utc = $null; heartbeat_at_utc = $null
        duration_seconds = $DurationSeconds; interval_seconds = 60; max_gap_seconds = 150; worker = $null
        observer_error = $null; resume_count = 0
    }
    Write-SoakJson (Join-Path $directory 'state.json') $state
    if (-not $NoLatest) { Set-Content -LiteralPath (Join-Path $script:RuntimeRoot 'soaks\latest.txt') -Value $runId -Encoding ASCII }
    return $state
}
function Get-SoakObservation {
    $at = [DateTime]::UtcNow
    $row = [ordered]@{
        at = $at.ToString('o'); api_available = $false; http_errors = 0; error_kind = $null
        health = 'UNAVAILABLE'; runtime_started_at = $null; snapshot_at = $null; snapshot_age_seconds = $null
        samples = $null; pools = $null; accepted_samples = $null; scans = $null; provider_errors = $null
        signal_counts = $null; notification_counts = $null; telegram = $null; sources = @(); source_health = @()
        data = [ordered]@{ fresh_pools = 0; stale_pools = 0; unknown_time_pools = 0; histories_with_gap = 0; largest_sample_gap_seconds = $null }
        database_integrity = $null; database_journal_mode = $null
        memory_working_set_bytes = $null; process_cpu_total_seconds = $null; logical_processors = [Environment]::ProcessorCount
    }
    try {
        $health = Invoke-RestMethod 'http://127.0.0.1:8787/api/health' -TimeoutSec 10
        $snapshot = Invoke-RestMethod 'http://127.0.0.1:8787/api/tokens' -TimeoutSec 10
        if ((Get-SoakField $health 'mode') -ne 'LOCAL_LIVE' -or (Get-SoakField $snapshot 'mode') -ne 'LOCAL_LIVE') { throw 'Unexpected API mode.' }
        $metrics = Get-SoakField $snapshot 'metrics'
        $runtime = Get-SoakField $metrics 'runtime'
        $database = Get-SoakField $health 'database'
        $row.api_available = $true; $row.health = Get-SoakField $health 'status'
        $row.runtime_started_at = Get-SoakField $runtime 'started_at'
        $row.snapshot_at = Get-SoakField $snapshot 'generated_at_utc'
        if ($row.snapshot_at) { $row.snapshot_age_seconds = [Math]::Round(($at - ([DateTimeOffset]$row.snapshot_at).UtcDateTime).TotalSeconds, 3) }
        $row.samples = Get-SoakField $metrics 'samples'; $row.pools = Get-SoakField $metrics 'pools'
        $row.accepted_samples = Get-SoakField $runtime 'accepted_samples'; $row.scans = Get-SoakField $runtime 'scans'
        $row.provider_errors = Get-SoakField $runtime 'provider_errors'
        $row.signal_counts = Copy-SoakCounts (Get-SoakField $metrics 'signal_counts')
        $row.notification_counts = Copy-SoakCounts (Get-SoakField $metrics 'notification_counts')
        $telegram = Get-SoakField $health 'telegram'
        if ($telegram) {
            $row.telegram = [ordered]@{ configured=(Get-SoakField $telegram 'configured'); enabled=(Get-SoakField $telegram 'enabled'); verification=(Get-SoakField $telegram 'verification') }
        }
        foreach ($source in @(Get-SoakField $metrics 'source_health')) {
            if ($null -eq $source) { continue }
            $row.source_health += [ordered]@{ id=(Get-SoakField $source 'id'); consecutive_failures=(Get-SoakField $source 'consecutive_failures'); recoveries=(Get-SoakField $source 'recoveries'); last_checked_at=(Get-SoakField $source 'last_checked_at'); last_success_at=(Get-SoakField $source 'last_success_at'); source_age_seconds=(Get-SoakField $source 'source_age_seconds') }
        }
        $row.database_integrity = Get-SoakField $database 'integrity_check'; $row.database_journal_mode = Get-SoakField $database 'journal_mode'
        # Only aggregate counts/status are retained, never tokens, messages, wallet lists, paths or secrets.
        foreach ($source in @(Get-SoakField $snapshot 'sources')) {
            $last = Get-SoakField $source 'last_success_at'; $age = $null
            if ($last) { $age = [Math]::Round(($at - ([DateTimeOffset]$last).UtcDateTime).TotalSeconds, 3) }
            $row.sources += [ordered]@{ id = Get-SoakField $source 'id'; status = Get-SoakField $source 'status'; requests = Get-SoakField $source 'requests'; errors = Get-SoakField $source 'errors'; last_success_at = $last; last_success_age_seconds = $age; latency_ms = Get-SoakField $source 'latency_ms' }
        }
        $maxGap = 0.0
        foreach ($item in @(Get-SoakField $snapshot 'items')) {
            $fetched = Get-SoakField $item 'fetched_at'
            if (-not $fetched) { $row.data.unknown_time_pools++ }
            elseif (($at - ([DateTimeOffset]$fetched).UtcDateTime).TotalSeconds -gt 300) { $row.data.stale_pools++ }
            else { $row.data.fresh_pools++ }
            $history = @(Get-SoakField $item 'history'); $hasGap = $false
            for ($index = 1; $index -lt $history.Count; $index++) {
                $gap = (([DateTimeOffset]$history[$index].at) - ([DateTimeOffset]$history[$index - 1].at)).TotalSeconds
                if ($gap -gt 300 -or $gap -le 0) { $hasGap = $true }
                $maxGap = [Math]::Max($maxGap, $gap)
            }
            if ($hasGap) { $row.data.histories_with_gap++ }
        }
        $row.data.largest_sample_gap_seconds = $maxGap
        $backendPid = Get-SoakField $runtime 'pid'
        if ($backendPid) {
            try {
                $process = Get-Process -Id ([int]$backendPid) -ErrorAction Stop
                $expected = @((Join-Path $script:ProjectRoot 'backend\target\debug\chainflow.exe'), (Join-Path $script:ProjectRoot 'backend\target\release\chainflow.exe'))
                if ($process.Path -in $expected) { $process.Refresh(); $row.memory_working_set_bytes = $process.WorkingSet64; $row.process_cpu_total_seconds = $process.TotalProcessorTime.TotalSeconds }
            } catch { }
        }
    } catch {
        $row.api_available = $false; $row.http_errors = 1; $row.health = 'UNAVAILABLE'
        $row.error_kind = $_.Exception.GetType().Name
    }
    return [pscustomobject]$row
}
function Get-SoakDelta($First, $Last) {
    if ($null -eq $First -or $null -eq $Last -or [double]$Last -lt [double]$First) { return $null }
    return [double]$Last - [double]$First
}
function Copy-SoakCounts($Counts) {
    if ($null -eq $Counts) { return $null }
    $copy = [ordered]@{}
    # Enumerate properties explicitly: an empty PSCustomObject has no .Name member
    # on its empty property collection under PowerShell StrictMode.
    $keys = @(); if ($Counts -is [Collections.IDictionary]) { $keys=@($Counts.Keys) } else { $keys=@($Counts.PSObject.Properties | ForEach-Object { $_.Name }) }
    foreach ($key in $keys) {
        $value=Get-SoakField $Counts $key
        if ($key -cmatch '^[A-Z_]{1,40}$' -and ($value -is [int] -or $value -is [long] -or $value -is [double] -or $value -is [decimal]) -and $value -ge 0) { $copy[$key]=$value }
    }
    return [pscustomobject]$copy
}
function Get-SoakSummary($State, [object[]]$Rows) {
    $good = @($Rows | Where-Object { $_.api_available })
    $gaps = @(); $recoveries = 0; $restarts = 0; $providerRecoveries = 0; $coverage = 0.0; $cpu = @()
    $requestDelta = 0.0; $errorDelta = 0.0; $sampleDelta = 0.0; $counterSegments = 0; $counterUnknown = 0
    # Restart/recovery transitions compare successful observations, even across an API outage.
    for ($i = 1; $i -lt $good.Count; $i++) {
        if ($good[$i - 1].runtime_started_at -and $good[$i].runtime_started_at -and $good[$i - 1].runtime_started_at -ne $good[$i].runtime_started_at) { $restarts++ }
        foreach ($source in $good[$i].sources) {
            $prior = @($good[$i - 1].sources | Where-Object { $_.id -eq $source.id })
            if ($prior.Count -and $prior[0].status -in @('SOURCE_ERROR','UNAVAILABLE','STALE') -and $source.status -eq 'OK') { $providerRecoveries++ }
        }
    }
    for ($i = 1; $i -lt $Rows.Count; $i++) {
        $before = $Rows[$i - 1]; $after = $Rows[$i]
        $seconds = (([DateTimeOffset]$after.at) - ([DateTimeOffset]$before.at)).TotalSeconds
        if ($seconds -gt $State.max_gap_seconds -or $seconds -le 0) { $gaps += [ordered]@{from_utc=$before.at;to_utc=$after.at;seconds=$seconds;reason='HOST_OR_OBSERVER_GAP_OR_CLOCK_CHANGE'} }
        if ($before.api_available -and $after.api_available -and $seconds -gt 0 -and $seconds -le $State.max_gap_seconds) { $coverage += $seconds }
        if (-not $before.api_available -and $after.api_available) { $recoveries++ }
        if ($before.api_available -and $after.api_available) {
            if (-not $before.runtime_started_at -or -not $after.runtime_started_at -or $before.runtime_started_at -ne $after.runtime_started_at) { continue }
            $counterSegments++
            $delta = Get-SoakDelta $before.accepted_samples $after.accepted_samples
            if ($null -ne $delta) { $sampleDelta += $delta } else { $counterUnknown++ }
            $cpuDelta = Get-SoakDelta $before.process_cpu_total_seconds $after.process_cpu_total_seconds
            if ($null -ne $cpuDelta -and $seconds -gt 0 -and $seconds -le $State.max_gap_seconds) { $cpu += $cpuDelta / $seconds / [Math]::Max(1, [int]$after.logical_processors) * 100 }
            foreach ($source in $after.sources) {
                $prior = @($before.sources | Where-Object { $_.id -eq $source.id })
                if ($prior.Count -eq 0) { $counterUnknown++; continue }
                $delta = Get-SoakDelta $prior[0].requests $source.requests
                if ($null -ne $delta) { $requestDelta += $delta } else { $counterUnknown++ }
                $delta = Get-SoakDelta $prior[0].errors $source.errors
                if ($null -ne $delta) { $errorDelta += $delta } else { $counterUnknown++ }
            }
            if (@($before.sources).Count -ne @($after.sources).Count -or @($after.sources).Count -eq 0) { $counterUnknown++ }
        }
    }
    $firstAt = $null; $lastAt = $null; $duration = 0.0
    if ($Rows.Count -gt 0) { $firstAt=$Rows[0].at; $lastAt=$Rows[-1].at; $duration=[Math]::Max(0.0, (([DateTimeOffset]$lastAt)-([DateTimeOffset]$firstAt)).TotalSeconds) }
    $classification = [ordered]@{}
    foreach ($name in @('WATCH','RISK','CONFIRMATION','DATA_FAULT')) {
        $delta = $null
        if ($good.Count -ge 2) { $delta = Get-SoakDelta (Get-SoakField $good[0].signal_counts $name) (Get-SoakField $good[-1].signal_counts $name) }
        $classification[$name] = $delta
    }
    $memory = @($good | Where-Object { $null -ne $_.memory_working_set_bytes } | ForEach-Object { $_.memory_working_set_bytes })
    $integrity = $null; $finalNotifications = $null; $finalSources = @(); $lastData = $null
    if ($Rows.Count -gt 0) { $integrity=$Rows[-1].database_integrity; $lastData=$Rows[-1].data }
    if ($good.Count -gt 0) { $finalNotifications=$good[-1].notification_counts; $finalSources=$good[-1].sources }
    return [ordered]@{
        first_observation_utc=$firstAt; last_observation_utc=$lastAt; actual_observation_span_seconds=[Math]::Round($duration,3)
        requested_duration_seconds=$State.duration_seconds; duration_met=($duration -ge $State.duration_seconds)
        observed_connected_span_seconds=[Math]::Round($coverage,3); observations=$Rows.Count; successful_observations=$good.Count
        http_error_observations=(@($Rows | Where-Object { -not $_.api_available }).Count)
        api_recoveries=$recoveries; provider_recoveries=$providerRecoveries; backend_restart_transitions=$restarts
        observer_gaps=$gaps; provider_requests_observed_delta=$requestDelta; provider_errors_observed_delta=$errorDelta
        accepted_samples_observed_delta=$sampleDelta; comparable_counter_segments=$counterSegments
        counter_coverage_complete=($Rows.Count -ge 2 -and $counterSegments -eq ($Rows.Count - 1) -and $counterUnknown -eq 0)
        unknown_counter_values=$counterUnknown
        signal_counts_delta=$classification; notification_counts_final=$finalNotifications; sources_final=$finalSources
        database_integrity_final=$integrity; data_continuity_final=$lastData
        memory_min_bytes=$(if($memory.Count){($memory|Measure-Object -Minimum).Minimum}else{$null})
        memory_max_bytes=$(if($memory.Count){($memory|Measure-Object -Maximum).Maximum}else{$null})
        cpu_machine_percent_average=$(if($cpu.Count){[Math]::Round(($cpu|Measure-Object -Average).Average,4)}else{$null})
        cpu_machine_percent_max=$(if($cpu.Count){[Math]::Round(($cpu|Measure-Object -Maximum).Maximum,4)}else{$null})
        counter_note='Observed comparable intervals only; restarts/missing baselines are excluded, never assumed zero. WATCH is not RISK. Null means UNKNOWN.'
    }
}
function Get-SoakRows([string]$Directory) {
    $path = Join-Path $Directory 'observations.jsonl'
    if (-not (Test-Path -LiteralPath $path)) { return @() }
    $rows = @()
    foreach ($line in Get-Content -LiteralPath $path -Encoding UTF8) {
        if (-not $line.Trim()) { continue }
        try { $rows += ($line | ConvertFrom-Json) } catch { Write-Warning 'Incomplete observation line skipped; process may have been interrupted during write.' }
    }
    return $rows
}
function Write-SoakReport([string]$RunId) {
    $directory = Get-SoakDirectory $RunId; $state = Read-SoakJson (Join-Path $directory 'state.json')
    $rows = @(Get-SoakRows $directory); $status = [string]$state.status
    if ($status -eq 'RUNNING' -and -not (Test-SoakWorkerIdentity $state.worker)) { $status = 'INTERRUPTED' }
    $summary = Get-SoakSummary $state $rows
    $result = [ordered]@{ schema_version=1;kind=$state.kind;run_id=$RunId;status=$status;started_at_utc=$state.started_at_utc;ended_at_utc=$state.ended_at_utc;reported_at_utc=[DateTime]::UtcNow.ToString('o');resume_count=$state.resume_count;observer_error=$state.observer_error;summary=$summary;observations=$rows }
    Write-SoakJson (Join-Path $directory 'report.json') $result
    $md = @('# ChainFlow 运行观察报告', '', ('- Run: ' + $RunId), ('- 数据类型: ' + $state.kind), ('- 状态: ' + $status), ('- 首次 / 最后实际观测 UTC: ' + $summary.first_observation_utc + ' / ' + $summary.last_observation_utc), ('- 实际观测跨度（秒）: ' + $summary.actual_observation_span_seconds), ('- 目标时长（秒）: ' + $state.duration_seconds), ('- 达到时长: ' + $summary.duration_met), ('- 有连接的观测间隔合计（秒）: ' + $summary.observed_connected_span_seconds), ('- 成功 / HTTP 错误观察次数: ' + $summary.successful_observations + ' / ' + $summary.http_error_observations), ('- API / Provider 恢复次数: ' + $summary.api_recoveries + ' / ' + $summary.provider_recoveries), ('- 后端重启转换 / 观察缺口: ' + $summary.backend_restart_transitions + ' / ' + @($summary.observer_gaps).Count), ('- 可比较区间请求 / 失败 / 新采样: ' + $summary.provider_requests_observed_delta + ' / ' + $summary.provider_errors_observed_delta + ' / ' + $summary.accepted_samples_observed_delta), ('- 数据库最终完整性: ' + $summary.database_integrity_final), ('- 内存最小 / 最大 bytes: ' + $summary.memory_min_bytes + ' / ' + $summary.memory_max_bytes), ('- CPU 平均 / 最大（占全机逻辑核百分比）: ' + $summary.cpu_machine_percent_average + ' / ' + $summary.cpu_machine_percent_max), '', '## 分类信号（普通关注不计为风险）', '', '| 分类 | 观测区间累计差值 |', '| --- | ---: |')
    foreach ($key in $summary.signal_counts_delta.Keys) { $value=$summary.signal_counts_delta[$key]; if($null -eq $value){$value='UNKNOWN'}; $md += '| ' + $key + ' | ' + $value + ' |' }
    $md += @('', '完整来源、通知状态、CPU/内存和缺口证据见同目录 report.json。', '', 'COMPLETED 只代表观察计划结束，不等于健康验收通过。缺口可能来自休眠、断电、网络或观察进程中断，无法单凭采样判定原因。JSON null 为 UNKNOWN；没有真实长跑证据时，6h/24h 验收维持 PENDING。计数只覆盖可比较区间，不用缺失窗口补造数据。')
    [IO.File]::WriteAllText((Join-Path $directory 'report.md'), ($md -join "`r`n"), [Text.UTF8Encoding]::new($false))
    return [pscustomobject]$result
}
