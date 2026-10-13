[CmdletBinding()]
param(
    [ValidateRange(1024, 65535)][int]$Port = 8787,
    [ValidateRange(1024, 65535)][int]$FrontendPort = 5178,
    [ValidateRange(60, 86400)][int]$IntervalSeconds = 120,
    [switch]$SkipBuild
)
. (Join-Path $PSScriptRoot 'common.ps1')
Import-ProjectEnvironment
if ($Port -ne 8787) { throw 'The frontend proxy uses port 8787. Keep the default port or update frontend/vite.config.ts first.' }
$statePath = Join-Path $script:RuntimeRoot 'processes.json'
if (Test-Path -LiteralPath $statePath) {
    $existing = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
    foreach ($entry in $existing.processes) {
        if (Test-ProcessIdentity $entry) { throw 'This project already has a managed process. Run stop.ps1 before restarting.' }
    }
}
Assert-PortAvailable $Port
Assert-PortAvailable $FrontendPort
foreach ($directory in @($script:RuntimeRoot, $script:LogRoot, (Join-Path $script:ProjectRoot 'data'))) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
}
$backend = Join-Path $script:ProjectRoot 'backend\target\debug\chainflow.exe'
$vite = Join-Path $script:ProjectRoot 'frontend\node_modules\vite\bin\vite.js'
if (-not $SkipBuild) {
    Initialize-MsvcEnvironment
    Push-Location $script:ProjectRoot
    try {
        & cargo build --locked --manifest-path backend/Cargo.toml
        if ($LASTEXITCODE -ne 0) { throw 'Rust build failed. Run doctor.ps1 and inspect the compiler output.' }
        Push-Location (Join-Path $script:ProjectRoot 'frontend')
        try {
            & npm.cmd ci
            if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
        } finally { Pop-Location }
    } finally { Pop-Location }
}
if (-not (Test-Path -LiteralPath $backend)) { throw 'Backend binary missing; run without -SkipBuild.' }
if (-not (Test-Path -LiteralPath $vite)) { throw 'Frontend dependencies missing; run without -SkipBuild.' }
$node = (Get-Command node.exe -ErrorAction Stop).Source
$db = Join-Path $script:ProjectRoot 'data\chainflow.sqlite'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$state = [ordered]@{ workspace = $script:ProjectRoot; started_at_utc = [DateTime]::UtcNow.ToString('o'); processes = @() }
$env:CHAINFLOW_FRONTEND_ORIGIN = "http://127.0.0.1:$FrontendPort"
try {
    $apiProcess = Start-Process -FilePath $backend -ArgumentList @('serve', '--db', ('"' + $db + '"'), '--port', "$Port", '--interval-seconds', "$IntervalSeconds") -WorkingDirectory $script:ProjectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $script:LogRoot "backend-$stamp.log") -RedirectStandardError (Join-Path $script:LogRoot "backend-$stamp.error.log")
    $identity = Get-ProcessIdentity -ProcessId $apiProcess.Id -Marker $db
    if (-not $identity) { throw 'Backend exited before its identity could be verified. Inspect logs.' }
    $state.processes += $identity
    Write-RuntimeState $state
    $env:VITE_MODE = 'LOCAL_LIVE'
    $webProcess = Start-Process -FilePath $node -ArgumentList @(('"' + $vite + '"'), '--host', '127.0.0.1', '--port', "$FrontendPort", '--strictPort') -WorkingDirectory (Join-Path $script:ProjectRoot 'frontend') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $script:LogRoot "frontend-$stamp.log") -RedirectStandardError (Join-Path $script:LogRoot "frontend-$stamp.error.log")
    $identity = Get-ProcessIdentity -ProcessId $webProcess.Id -Marker $vite
    if (-not $identity) { throw 'Frontend exited before its identity could be verified. Inspect logs.' }
    $state.processes += $identity
    Write-RuntimeState $state
    $ready = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        foreach ($entry in $state.processes) {
            if (-not (Test-ProcessIdentity $entry)) { throw 'A managed process exited during startup. Inspect logs.' }
        }
        try {
            $null = Invoke-RestMethod "http://127.0.0.1:$Port/api/health" -TimeoutSec 2
            $null = Invoke-WebRequest "http://127.0.0.1:$FrontendPort/chainflow-intelligence/" -UseBasicParsing -TimeoutSec 2
            $ready = $true
            break
        } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ready) { throw 'Startup health check timed out. Inspect logs; run doctor.ps1.' }
    Write-Host "Local dashboard: http://127.0.0.1:$FrontendPort/chainflow-intelligence/"
    Write-Host "API: http://127.0.0.1:$Port/api/health"
    Write-Host "Processes verified. Logs: $script:LogRoot"
    Write-Host 'Health availability does not imply market providers are reachable; check the Sources panel.'
} catch {
    $failure = $_
    & (Join-Path $PSScriptRoot 'stop.ps1')
    throw $failure
}
