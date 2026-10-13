Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:ProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:RuntimeRoot = Join-Path $script:ProjectRoot '.runtime'
$script:LogRoot = Join-Path $script:ProjectRoot 'logs'

function Initialize-ProjectTools {
    $bins = @(
        (Join-Path $script:ProjectRoot '.tools\cargo\bin'),
        (Join-Path $script:ProjectRoot '.tools\rust\bin'),
        (Join-Path $script:ProjectRoot '.tools\node'),
        (Join-Path $script:ProjectRoot '.tools\gh\bin'),
        (Join-Path $env:USERPROFILE '.cargo\bin')
    )
    $customPath = Join-Path $script:RuntimeRoot 'toolchain-path.txt'
    if (Test-Path -LiteralPath $customPath) {
        foreach ($line in Get-Content -LiteralPath $customPath) {
            $entry = $line.Trim()
            if ($entry -and [IO.Path]::IsPathRooted($entry) -and (Test-Path -LiteralPath $entry -PathType Container)) {
                $bins = @($entry) + $bins
            }
        }
    }
    $bins = @($bins | Where-Object { Test-Path -LiteralPath $_ -PathType Container })
    $env:PATH = ($bins + @($env:PATH)) -join [IO.Path]::PathSeparator
    # Process-only settings: do not change the machine/user environment.
    $env:CARGO_HOME = Join-Path $script:ProjectRoot '.tools\cargo'
}

function Import-ProjectEnvironment {
    $envPath = Join-Path $script:ProjectRoot '.env'
    if (-not (Test-Path -LiteralPath $envPath)) { return }
    foreach ($line in Get-Content -LiteralPath $envPath) {
        $text = $line.Trim()
        if (-not $text -or $text.StartsWith('#')) { continue }
        if ($text -notmatch '^([A-Z][A-Z0-9_]*)=(.*)$') { throw '.env must contain KEY=VALUE lines.' }
        $key = $Matches[1]
        $value = $Matches[2].Trim()
        if ($key -notin @('RUST_LOG', 'CHAINFLOW_RULES_JSON')) {
            throw "Unsupported .env key: $key. See .env.example."
        }
        if ($value.Length -ge 2 -and (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'")))) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        [Environment]::SetEnvironmentVariable($key, $value, 'Process')
    }
}

function Initialize-MsvcEnvironment {
    if (Get-Command link.exe -ErrorAction SilentlyContinue) { return }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { return }
    $install = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $install) { return }
    $devCmd = Join-Path $install 'Common7\Tools\VsDevCmd.bat'
    if (-not (Test-Path -LiteralPath $devCmd)) { return }
    $command = '""' + $devCmd + '" -no_logo -arch=x64 -host_arch=x64 && set"'
    $environmentLines = & $env:ComSpec /d /s /c $command
    if ($LASTEXITCODE -ne 0) { throw 'Visual Studio C++ environment initialization failed.' }
    foreach ($line in $environmentLines) {
        if ($line -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process') }
    }
}

function Get-ProcessIdentity([int]$ProcessId, [string]$Marker) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if (-not $process) { return $null }
    $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $ProcessId" -ErrorAction SilentlyContinue
    if (-not $cim -or -not $cim.ExecutablePath -or -not $cim.CommandLine) { return $null }
    return [ordered]@{
        pid = $ProcessId
        executable = [IO.Path]::GetFullPath($cim.ExecutablePath)
        started_at_utc = $process.StartTime.ToUniversalTime().ToString('o')
        marker = $Marker
    }
}

function Test-ProcessIdentity($Identity) {
    $actual = Get-ProcessIdentity -ProcessId ([int]$Identity.pid) -Marker ([string]$Identity.marker)
    if (-not $actual) { return $false }
    $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $($Identity.pid)" -ErrorAction SilentlyContinue
    # PowerShell 7.5 may deserialize ISO timestamps into DateTime; compare instants.
    $actualTicks = ([DateTimeOffset]$actual.started_at_utc).UtcDateTime.Ticks
    $expectedTicks = ([DateTimeOffset]$Identity.started_at_utc).UtcDateTime.Ticks
    return ($actual.executable -eq $Identity.executable -and
        $actualTicks -eq $expectedTicks -and
        $cim.CommandLine.IndexOf([string]$Identity.marker, [StringComparison]::OrdinalIgnoreCase) -ge 0)
}

function Write-RuntimeState($State) {
    $statePath = Join-Path $script:RuntimeRoot 'processes.json'
    $temporary = "$statePath.tmp"
    $State | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $temporary -Encoding UTF8
    Move-Item -LiteralPath $temporary -Destination $statePath -Force
}

function Assert-PortAvailable([int]$Port) {
    $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $Port)
    try { $listener.Start() }
    catch { throw "127.0.0.1:$Port is occupied. Run doctor.ps1; no existing process will be stopped." }
    finally { $listener.Stop() }
}

Initialize-ProjectTools
