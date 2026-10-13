[CmdletBinding()]
param()
. (Join-Path $PSScriptRoot 'common.ps1')
Write-Host "Workspace: $script:ProjectRoot"
Write-Host 'This diagnostic does not print .env values or authentication tokens.'
foreach ($name in @('git.exe', 'node.exe', 'npm.cmd', 'cargo.exe', 'rustc.exe', 'gh.exe')) {
    $command = Get-Command $name -ErrorAction SilentlyContinue
    if ($command) {
        Write-Host "$name : $($command.Source)"
        & $command.Source --version
    } else { Write-Warning "$name not found. See README.md for official installation links." }
}
$statePath = Join-Path $script:RuntimeRoot 'processes.json'
if (Test-Path -LiteralPath $statePath) {
    $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
    foreach ($entry in $state.processes) { Write-Host "Managed PID $($entry.pid), identity verified: $(Test-ProcessIdentity $entry)" }
} else { Write-Host 'No managed processes recorded.' }
foreach ($port in @(8787, 5178)) {
    $connections = @(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue)
    if ($connections.Count -eq 0) { Write-Host "Port $port : no listener" }
    foreach ($connection in $connections) { Write-Host "Port $port : $($connection.LocalAddress), PID $($connection.OwningProcess)" }
}
try {
    $health = Invoke-RestMethod 'http://127.0.0.1:8787/api/health' -TimeoutSec 5
    Write-Host ('API health: ' + ($health | ConvertTo-Json -Compress -Depth 5))
    $sources = Invoke-RestMethod 'http://127.0.0.1:8787/api/sources' -TimeoutSec 5
    $sources | Select-Object id, status, checked_at, last_success_at, requests, errors | Format-Table -AutoSize
} catch { Write-Warning 'Local API unavailable. Start the service or inspect logs/backend-*.error.log.' }
$backend = Join-Path $script:ProjectRoot 'backend\target\debug\chainflow.exe'
$db = Join-Path $script:ProjectRoot 'data\chainflow.sqlite'
if ((Test-Path -LiteralPath $backend) -and (Test-Path -LiteralPath $db)) {
    & $backend doctor --db $db
    if ($LASTEXITCODE -ne 0) { Write-Warning 'Database doctor reported an error.' }
} else { Write-Host 'Database integrity check skipped: binary or database does not yet exist.' }
Write-Host "Logs: $script:LogRoot"
Write-Host 'GitHub login can be checked with gh auth status. Never paste tokens into reports.'
