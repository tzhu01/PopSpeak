[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PortableDir,
    [string]$ExpectedVersion = "",
    [string]$ExpectedExe = "",
    [switch]$RequireComplete
)

$ErrorActionPreference = "Stop"
$portableRoot = (Resolve-Path -LiteralPath $PortableDir).Path.TrimEnd('\', '/')
$manifestPath = Join-Path $portableRoot "manifest.sha256"
if (!(Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw "Missing portable manifest" }
$requiredFiles = @(
    "PopSpeak.exe", "release-build.json", "README.txt", "LICENSE.txt", "THIRD_PARTY_NOTICES.md",
    "models/catalog.json", "models/sensevoice/model.int8.onnx", "models/sensevoice/tokens.txt",
    "models/whisper/ggml-tiny.bin", "models/whisper/ggml-base.bin",
    "models/llm/qwen2.5-0.5b-instruct-q4_k_m.gguf",
    "runtimes/whisper/whisper-cli.exe", "runtimes/llama/llama-server.exe",
    "runtimes/funasr/llama-funasr-pipe-host.exe", "runtimes/funasr/llama-funasr-pipe-host-avx2.exe",
    "onnxruntime.dll", "onnxruntime_providers_shared.dll", "sherpa-onnx-c-api.dll", "sherpa-onnx-cxx-api.dll",
    "msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll",
    "licenses/FunASR-MIT.txt", "licenses/Apache-2.0.txt",
    "licenses/Microsoft-Visual-Cpp-Runtime-License.txt"
)
foreach ($required in $requiredFiles) {
    if (!(Test-Path -LiteralPath (Join-Path $portableRoot $required) -PathType Leaf)) {
        throw "Missing required portable file: $required"
    }
}
foreach ($license in Get-ChildItem -LiteralPath (Join-Path $portableRoot 'licenses') -File -Recurse) {
    if ($license.Length -le 200) { throw "Packaged license is empty or truncated: $($license.Name)" }
}
$allEntries = @(Get-ChildItem -LiteralPath $portableRoot -Recurse -Force)
if (@($allEntries | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count -gt 0) {
    throw "Portable distribution must not contain links to external files or directories"
}
$manifestFiles = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$verifiedCount = 0
foreach ($entry in Get-Content -LiteralPath $manifestPath) {
    if ([string]::IsNullOrWhiteSpace($entry)) { continue }
    if ($entry -notmatch '^([a-fA-F0-9]{64})  (.+)$') { throw "Malformed portable manifest entry" }
    $expectedHash = $Matches[1]
    $relative = $Matches[2]
    if ([IO.Path]::IsPathRooted($relative) -or $relative.Contains(':')) { throw "Manifest contains an absolute path or alternate data stream" }
    $target = [IO.Path]::GetFullPath((Join-Path $portableRoot $relative))
    if (!$target.StartsWith($portableRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Manifest path escapes portable directory"
    }
    if (!$manifestFiles.Add($target)) { throw "Duplicate portable manifest entry: $relative" }
    if ($target -eq $manifestPath) { throw "The manifest must not list itself" }
    if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Missing packaged file: $relative" }
    if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $expectedHash) {
        throw "SHA-256 mismatch: $relative"
    }
    $verifiedCount++
}
foreach ($file in @($allEntries | Where-Object { !$_.PSIsContainer -and $_.FullName -ne $manifestPath })) {
    if (!$manifestFiles.Contains($file.FullName)) { throw "Packaged file is absent from the manifest: $($file.Name)" }
}
$executable = Join-Path $portableRoot "PopSpeak.exe"
$fileVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
if ($ExpectedVersion -and $fileVersion -ne $ExpectedVersion) {
    throw "Wrong executable version: $fileVersion (expected $ExpectedVersion)"
}
if ($ExpectedExe) {
    if ((Get-FileHash -LiteralPath $executable).Hash -ne (Get-FileHash -LiteralPath $ExpectedExe).Hash) {
        throw "Extracted executable is not the newly built executable"
    }
}
$privateFiles = @(Get-ChildItem -LiteralPath $portableRoot -Recurse -File | Where-Object {
    $_.Name -match '^(settings\.json|pending-transcript\.json|history\.json|credentials\.json|\.env(?:\..*)?|activation\.sqlite3(?:-wal|-shm)?|.*signing-private.*)$' -or $_.Extension -in @('.db', '.sqlite', '.sqlite3', '.pem', '.pfx', '.p12', '.key')
})
if ($privateFiles.Count -gt 0) { throw "Portable directory contains private configuration/database files; do not publish" }
$catalog = Get-Content -LiteralPath (Join-Path $portableRoot "models/catalog.json") -Raw | ConvertFrom-Json
foreach ($model in $catalog.models) {
    foreach ($artifact in $model.artifacts) {
        $artifactPath = [IO.Path]::GetFullPath((Join-Path (Join-Path $portableRoot "models") $artifact.path))
        if (!$artifactPath.StartsWith($portableRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Model catalog artifact escapes portable directory"
        }
        if (!(Test-Path -LiteralPath $artifactPath -PathType Leaf) -or
            (Get-Item -LiteralPath $artifactPath).Length -ne [long]$artifact.bytes -or
            (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash -ne $artifact.sha256) {
            throw "Bundled model does not match the catalog: $($artifact.path)"
        }
    }
}
$completeModelHashes = [ordered]@{
    "funasr-encoder-f16.gguf" = "f92f91d01a24fbed6c863495b2ee8c6a6788144a02858b75743f0946668de8a2"
    "qwen3-0.6b-q4km.gguf" = "cc5057552aa9dddedcda73ea8889854e8a257eb07d0a561b7234465c1e856f22"
    "fsmn-vad.gguf" = "1270f2559c495f4e7b6e739541151027d360761a3fda43fc147034f5719f5479"
}
$completeModelRoot = Join-Path $portableRoot "models/funasr-nano"
$hasCompleteModel = @($completeModelHashes.Keys | Where-Object { Test-Path -LiteralPath (Join-Path $completeModelRoot $_) }).Count -gt 0
if ($RequireComplete -or $hasCompleteModel) {
    foreach ($name in $completeModelHashes.Keys) {
        $modelPath = Join-Path $completeModelRoot $name
        if (!(Test-Path -LiteralPath $modelPath -PathType Leaf) -or
            (Get-FileHash -LiteralPath $modelPath -Algorithm SHA256).Hash -ne $completeModelHashes[$name]) {
            throw "Complete offline model is missing or has the wrong SHA-256: $name"
        }
    }
}
$releaseCheck = & (Join-Path $PSScriptRoot "verify-production-executable.ps1") -Executable $executable -ExpectedVersion $ExpectedVersion
$releaseReceipt = Get-Content -LiteralPath (Join-Path $portableRoot "release-build.json") -Raw | ConvertFrom-Json
if ($releaseReceipt.exe_sha256 -ne $releaseCheck.exe_sha256 -or
    $releaseReceipt.custom_protocol -ne $true -or $releaseReceipt.embedded_index_html -ne $true -or
    $releaseReceipt.embedded_asset_count -ne $releaseCheck.embedded_asset_count -or
    $releaseReceipt.version -ne $releaseCheck.version) {
    throw "Production build receipt does not match the packaged executable"
}
[PSCustomObject]@{
    Directory = $portableRoot
    Version = $fileVersion
    VerifiedFiles = $verifiedCount
    ExeSha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    PrivateDataFiles = 0
    CustomProtocol = $true
    EmbeddedAssets = $releaseCheck.embedded_asset_count
    CompleteOfflineModels = $hasCompleteModel
}
