[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [string]$SevenZipPath = "C:\Program Files\7-Zip\7z.exe",
    [string]$ExpectedVersion = "",
    [string]$ExpectedExe = "",
    [switch]$RequireComplete
)

$ErrorActionPreference = "Stop"
$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
$sevenZip = (Resolve-Path -LiteralPath $SevenZipPath).Path
$extractionRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ("PopSpeak-archive-check-" + [Guid]::NewGuid().ToString("N"))))
New-Item -ItemType Directory -Path $extractionRoot | Out-Null
$verified = $false
try {
    & $sevenZip t $archive
    if ($LASTEXITCODE -ne 0) { throw "7-Zip integrity test failed: $LASTEXITCODE" }
    & $sevenZip x $archive "-o$extractionRoot" -y
    if ($LASTEXITCODE -ne 0) { throw "7-Zip extraction failed: $LASTEXITCODE" }
    $entries = @(Get-ChildItem -LiteralPath $extractionRoot -Force)
    if (Test-Path -LiteralPath (Join-Path $extractionRoot "PopSpeak.exe") -PathType Leaf) {
        $portableRoot = $extractionRoot
    } elseif ($entries.Count -eq 1 -and $entries[0].PSIsContainer) {
        $portableRoot = $entries[0].FullName
    } else {
        throw "Archive must contain exactly one portable application folder"
    }
    & (Join-Path $PSScriptRoot "verify-portable.ps1") -PortableDir $portableRoot `
        -ExpectedVersion $ExpectedVersion -ExpectedExe $ExpectedExe -RequireComplete:$RequireComplete
    $verified = $true
    Write-Host "Archive was extracted and all packaged files passed verification: $archive"
} finally {
    if ($verified) {
        $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/')
        if (!$extractionRoot.StartsWith($temporaryRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
            ![IO.Path]::GetFileName($extractionRoot).StartsWith("PopSpeak-archive-check-")) {
            throw "Refusing to remove an unexpected archive verification path: $extractionRoot"
        }
        Remove-Item -LiteralPath $extractionRoot -Recurse -Force
    } else {
        Write-Warning "Failed extraction was retained for inspection: $extractionRoot"
    }
}
