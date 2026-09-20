# Build a self-contained Windows portable release, including the optional
# high-accuracy offline model. The output must be a fresh directory so an
# earlier release is never overwritten accidentally.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ModelDirectory,
    [Parameter(Mandatory = $true)][string]$OutputDir,
    [switch]$Build,
    [ValidateSet(1, 3, 5, 7, 9)][int]$CompressionLevel = 5,
    [string]$SevenZipPath = "C:\Program Files\7-Zip\7z.exe",
    [string]$VCRuntimeDir = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$outputPath = [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDir))
$allowedRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot "dist-portable"))
$archivePath = "$outputPath.7z"
if (!$outputPath.StartsWith($allowedRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputDir must resolve inside $allowedRoot"
}
if ((Test-Path -LiteralPath $outputPath) -or (Test-Path -LiteralPath $archivePath) -or (Test-Path -LiteralPath "$archivePath.sha256")) {
    throw "A release already exists at $outputPath or $archivePath; choose a new OutputDir"
}
if (!(Test-Path -LiteralPath $SevenZipPath -PathType Leaf)) {
    throw "7-Zip executable does not exist: $SevenZipPath"
}

$modelRoot = (Resolve-Path -LiteralPath $ModelDirectory).Path
$modelFiles = [ordered]@{
    "funasr-encoder-f16.gguf" = "f92f91d01a24fbed6c863495b2ee8c6a6788144a02858b75743f0946668de8a2"
    "qwen3-0.6b-q4km.gguf" = "cc5057552aa9dddedcda73ea8889854e8a257eb07d0a561b7234465c1e856f22"
    "fsmn-vad.gguf" = "1270f2559c495f4e7b6e739541151027d360761a3fda43fc147034f5719f5479"
}
foreach ($name in $modelFiles.Keys) {
    $sourcePath = Join-Path $modelRoot $name
    if (!(Test-Path -LiteralPath $sourcePath -PathType Leaf)) { throw "Missing model: $sourcePath" }
    $actualHash = (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $modelFiles[$name]) { throw "Model SHA-256 mismatch: $sourcePath" }
}

& (Join-Path $PSScriptRoot "build-portable.ps1") -OutputDir $OutputDir -Build:$Build -VCRuntimeDir $VCRuntimeDir
if (!(Test-Path -LiteralPath (Join-Path $outputPath "PopSpeak.exe") -PathType Leaf)) {
    throw "Base portable build did not produce PopSpeak.exe"
}

$destination = Join-Path $outputPath "models\funasr-nano"
foreach ($name in $modelFiles.Keys) {
    Copy-Item -LiteralPath (Join-Path $modelRoot $name) -Destination (Join-Path $destination $name)
}
$metadata = [ordered]@{
    version = "2026.06-q4km-r1"
    revision = "51dcf4922439c10e0c2e59bc99be8a343d2fe71f"
    quantization = "Encoder F16 + Qwen3-0.6B Q4_K_M"
    installed_at = (Get-Date).ToUniversalTime().ToString("o")
}
$metadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination ".popspeak-funasr-install.json") -Encoding UTF8
Copy-Item -LiteralPath (Join-Path $repoRoot "src-tauri\resources\COMPLETE_PORTABLE_README_ZH.txt") -Destination (Join-Path $outputPath "README.txt") -Force

$manifestPath = Join-Path $outputPath "manifest.sha256"
$manifestLines = Get-ChildItem -LiteralPath $outputPath -Recurse -File |
    Where-Object { $_.FullName -ne $manifestPath } |
    Sort-Object FullName |
    ForEach-Object {
        $relative = $_.FullName.Substring($outputPath.Length).TrimStart('\', '/').Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $relative"
    }
Set-Content -LiteralPath $manifestPath -Value $manifestLines -Encoding ASCII

$expectedVersion = (Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
$expectedExe = Join-Path $repoRoot "src-tauri\target\release\popspeak.exe"
& (Join-Path $PSScriptRoot "verify-portable.ps1") -PortableDir $outputPath `
    -ExpectedVersion $expectedVersion -ExpectedExe $expectedExe -RequireComplete

$outputParent = Split-Path -Parent $outputPath
$outputLeaf = Split-Path -Leaf $outputPath
Push-Location $outputParent
try {
    & $SevenZipPath a $archivePath $outputLeaf -t7z "-mx=$CompressionLevel" -m0=LZMA2:d=64m -ms=on -mmt=8 -y
    if ($LASTEXITCODE -ne 0) { throw "7-Zip compression failed: $LASTEXITCODE" }
} finally {
    Pop-Location
}

& (Join-Path $PSScriptRoot "verify-portable-archive.ps1") -ArchivePath $archivePath `
    -SevenZipPath $SevenZipPath -ExpectedVersion $expectedVersion -ExpectedExe $expectedExe -RequireComplete

Write-Host "Complete portable release: $archivePath"
$archiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath "$archivePath.sha256" -Value "$archiveHash  $([IO.Path]::GetFileName($archivePath))" -Encoding ASCII
Write-Host "Archive SHA-256: $archiveHash"
