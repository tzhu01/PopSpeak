# Pre-seed the exact NSIS toolchain expected by Tauri 2.10. This avoids a
# single short, unauthenticated download at the very end of a release build.
[CmdletBinding()]
param(
    [switch]$Force,
    [string]$MirrorPrefix = $env:POPSPEAK_GITHUB_MIRROR
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$nsisUrl = "https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip"
$nsisSha1 = "ef7ff767e5cbd9edd22add3a32c9b8f4500bb10d"
$utilsUrl = "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.5.3/nsis_tauri_utils.dll"
$utilsSha1 = "75197fee3c6a814fe035788d1c34ead39349b860"
$requiredFiles = @(
    "makensis.exe",
    "Bin\makensis.exe",
    "Stubs\lzma-x86-unicode",
    "Stubs\lzma_solid-x86-unicode",
    "Plugins\x86-unicode\additional\nsis_tauri_utils.dll",
    "Include\MUI2.nsh",
    "Include\FileFunc.nsh",
    "Include\x64.nsh",
    "Include\nsDialogs.nsh",
    "Include\WinMessages.nsh",
    "Include\Win\COM.nsh",
    "Include\Win\Propkey.nsh",
    "Include\Win\RestartManager.nsh"
)

$cacheRoot = [System.IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA "tauri"))
$nsisPath = [System.IO.Path]::GetFullPath((Join-Path $cacheRoot "NSIS"))
$expectedNsisPath = [System.IO.Path]::GetFullPath("$env:LOCALAPPDATA\tauri\NSIS")
if (![string]::Equals($nsisPath, $expectedNsisPath, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to modify unexpected NSIS cache path: $nsisPath"
}
$markerPath = Join-Path $nsisPath ".popspeak-toolchain"
$temporaryRoot = Join-Path $env:TEMP ("popspeak-nsis-{0}" -f [guid]::NewGuid())
$stagingPath = Join-Path $cacheRoot ("NSIS.staging-{0}" -f $PID)
$backupPath = Join-Path $cacheRoot ("NSIS.backup-{0}" -f $PID)

function Test-NsisCache {
    if (!(Test-Path -LiteralPath $nsisPath -PathType Container)) { return $false }
    foreach ($relative in $requiredFiles) {
        if (!(Test-Path -LiteralPath (Join-Path $nsisPath $relative) -PathType Leaf)) { return $false }
    }
    $utils = Join-Path $nsisPath "Plugins\x86-unicode\additional\nsis_tauri_utils.dll"
    if ((Get-FileHash -LiteralPath $utils -Algorithm SHA1).Hash.ToLowerInvariant() -ne $utilsSha1) {
        return $false
    }
    if (!(Test-Path -LiteralPath $markerPath -PathType Leaf)) { return $false }
    return (Get-Content -LiteralPath $markerPath -Raw).Trim() -eq "nsis=$nsisSha1;utils=$utilsSha1"
}

function Get-VerifiedFile {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$ExpectedSha1
    )

    $downloadUrl = if ([string]::IsNullOrWhiteSpace($MirrorPrefix)) {
        $Url
    } else {
        $MirrorPrefix.TrimEnd("/") + "/" + $Url
    }
    $arguments = @(
        "--fail", "--location", "--silent", "--show-error",
        "--retry", "5", "--retry-delay", "2", "--retry-all-errors",
        "--connect-timeout", "30", "--max-time", "600",
        "--user-agent", "PopSpeak-build", "--output", $Destination
    )
    if ($env:GITHUB_TOKEN -and $downloadUrl.StartsWith("https://github.com/")) {
        $arguments += @("--header", "Authorization: Bearer $env:GITHUB_TOKEN")
    }
    $arguments += $downloadUrl
    & curl.exe @arguments
    if ($LASTEXITCODE -ne 0) { throw "Download failed: $Url" }
    $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA1).Hash.ToLowerInvariant()
    if ($actual -ne $ExpectedSha1) {
        throw "SHA-1 mismatch for $Url (expected $ExpectedSha1, got $actual)"
    }
}

if (!$Force -and (Test-NsisCache)) {
    Write-Host "Tauri NSIS 3.11 toolchain is already verified."
    return
}

New-Item -ItemType Directory -Force -Path $cacheRoot, $temporaryRoot | Out-Null
try {
    $archive = Join-Path $temporaryRoot "nsis-3.11.zip"
    $utils = Join-Path $temporaryRoot "nsis_tauri_utils.dll"
    Get-VerifiedFile -Url $nsisUrl -Destination $archive -ExpectedSha1 $nsisSha1
    Get-VerifiedFile -Url $utilsUrl -Destination $utils -ExpectedSha1 $utilsSha1
    Expand-Archive -LiteralPath $archive -DestinationPath $temporaryRoot

    $extracted = Join-Path $temporaryRoot "nsis-3.11"
    if (!(Test-Path -LiteralPath $extracted -PathType Container)) {
        throw "The verified NSIS archive did not contain the expected directory."
    }
    $utilsDestination = Join-Path $extracted "Plugins\x86-unicode\additional"
    New-Item -ItemType Directory -Force -Path $utilsDestination | Out-Null
    Copy-Item -LiteralPath $utils -Destination (Join-Path $utilsDestination "nsis_tauri_utils.dll")
    Set-Content -LiteralPath (Join-Path $extracted ".popspeak-toolchain") `
        -Value "nsis=$nsisSha1;utils=$utilsSha1" -Encoding ASCII -NoNewline

    if (Test-Path -LiteralPath $stagingPath) { Remove-Item -LiteralPath $stagingPath -Recurse -Force }
    Move-Item -LiteralPath $extracted -Destination $stagingPath
    if (Test-Path -LiteralPath $backupPath) { Remove-Item -LiteralPath $backupPath -Recurse -Force }
    if (Test-Path -LiteralPath $nsisPath) { Move-Item -LiteralPath $nsisPath -Destination $backupPath }
    try {
        Move-Item -LiteralPath $stagingPath -Destination $nsisPath
    } catch {
        if (Test-Path -LiteralPath $backupPath) { Move-Item -LiteralPath $backupPath -Destination $nsisPath }
        throw
    }
    if (!(Test-NsisCache)) { throw "NSIS cache validation failed after installation." }
    if (Test-Path -LiteralPath $backupPath) { Remove-Item -LiteralPath $backupPath -Recurse -Force }
    Write-Host "Tauri NSIS 3.11 toolchain is ready and verified."
} finally {
    foreach ($path in @($temporaryRoot, $stagingPath)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
}
