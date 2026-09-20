[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [string]$ExpectedVersion = ""
)

$ErrorActionPreference = "Stop"
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$probeRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ("PopSpeak-release-check-" + [Guid]::NewGuid().ToString("N"))))
New-Item -ItemType Directory -Path $probeRoot | Out-Null
$probePath = Join-Path $probeRoot "release-check.json"
$process = $null
try {
    # The application handles this before GUI/single-instance initialization.
    # It checks the compiled protocol feature and the actual embedded assets;
    # merely using target/release or finding a localhost string proves neither.
    $process = Start-Process -FilePath $executablePath `
        -ArgumentList @("--release-self-check", ('"' + $probePath + '"')) `
        -WorkingDirectory (Split-Path -Parent $executablePath) `
        -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(30000)) {
        $process.Kill()
        throw "Release self-check timed out; this executable is not approved for portable distribution"
    }
    $process.Refresh()
    if ($process.ExitCode -ne 0 -or !(Test-Path -LiteralPath $probePath -PathType Leaf)) {
        throw "Release self-check failed (exit $($process.ExitCode)). Build with scripts/build-portable.ps1 -Build; do not distribute a development-server executable."
    }
    $report = Get-Content -LiteralPath $probePath -Raw | ConvertFrom-Json
    if ($report.schema_version -ne 1 -or $report.custom_protocol -ne $true -or
        $report.embedded_index_html -ne $true -or [int]$report.embedded_asset_count -lt 2) {
        throw "Executable does not contain a complete production frontend or does not use the packaged custom protocol"
    }
    if ($ExpectedVersion -and $report.version -ne $ExpectedVersion) {
        throw "Release self-check version mismatch: $($report.version) (expected $ExpectedVersion)"
    }
    [PSCustomObject][ordered]@{
        schema_version = 1
        version = $report.version
        custom_protocol = $true
        embedded_index_html = $true
        embedded_asset_count = [int]$report.embedded_asset_count
        exe_sha256 = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
} finally {
    if ($null -ne $process) { $process.Dispose() }
    # Only the exact fresh probe directory is removed, never the temp root.
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/')
    if ($probeRoot.StartsWith($temporaryRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($probeRoot).StartsWith("PopSpeak-release-check-")) {
        Remove-Item -LiteralPath $probeRoot -Recurse -Force
    }
}
