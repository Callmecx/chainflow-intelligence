[CmdletBinding()]
param([string]$RunId, [string]$ExportDirectory)
. (Join-Path $PSScriptRoot 'soak-common.ps1')
$runId = Resolve-SoakId $RunId
$result = Write-SoakReport $runId
$directory = Get-SoakDirectory $runId
if ($ExportDirectory) {
    $destination = [IO.Path]::GetFullPath($ExportDirectory)
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    foreach ($extension in @('json','md')) {
        $target = Join-Path $destination ("soak-$runId.$extension")
        if (Test-Path -LiteralPath $target) { throw 'Export already exists. Choose a new directory; previous evidence will not be overwritten.' }
    }
    foreach ($extension in @('json','md')) {
        $target = Join-Path $destination ("soak-$runId.$extension")
        Copy-Item -LiteralPath (Join-Path $directory "report.$extension") -Destination $target
    }
    Write-Host "Sanitized reports exported for run $runId. Review before publishing."
}
Write-Host (Join-Path $directory 'report.md')
$result.summary | ConvertTo-Json -Depth 10
