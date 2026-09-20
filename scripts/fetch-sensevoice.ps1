$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$binariesDir = Join-Path $repoRoot "src-tauri/binaries"
$resourcesDir = Join-Path $repoRoot "src-tauri/resources/sensevoice"
$temporaryDir = Join-Path $env:TEMP "popspeak-sensevoice-$PID"
$sherpaTag = "v1.13.4"

$expectedFiles = [ordered]@{
    (Join-Path $binariesDir "onnxruntime.dll") = "daa77083a45bf525da0dde9e87f85d8eb146f58f9c9aa7124ca84545e1c0f148"
    (Join-Path $binariesDir "onnxruntime_providers_shared.dll") = "190d10767c321f324d3785368a0b752d9c5a9e06cb5d4d97bb176f58bdb652f3"
    (Join-Path $binariesDir "onnxruntime.lib") = "b9fc3cd678257d88a111b0773ede4bfceaf0fe95daab4379f2b2b37348a68781"
    (Join-Path $binariesDir "sherpa-onnx-c-api.dll") = "dcfc89cf50fbd0fb77c115a6b53ee9b2739e57d6d1f4158a3aeab31dd7139676"
    (Join-Path $binariesDir "sherpa-onnx-c-api.lib") = "806798a9fa6da0027f50ee6d8c0fe94f62f4a3f0947c3f1a96fe42acbee97d84"
    (Join-Path $binariesDir "sherpa-onnx-cxx-api.dll") = "1df614bd2e55254c5811dcafe11c5ed4ccd9a285c92eea7b603db2b5b9a15688"
    (Join-Path $binariesDir "sherpa-onnx-cxx-api.lib") = "9b754db267f88e928f77b39afcc9875985e7d51063d0839162e01fb681dd9faf"
    (Join-Path $resourcesDir "model.int8.onnx") = "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51"
    (Join-Path $resourcesDir "tokens.txt") = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc"
}

function Get-VerifiedReleaseAsset {
    param(
        [string]$Repository,
        [string]$Tag,
        [string]$AssetName,
        [string]$Destination
    )
    $headers = @{ "User-Agent" = "PopSpeak-build" }
    if ($env:GITHUB_TOKEN) { $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN" }
    $releaseUrl = "https://api.github.com/repos/$Repository/releases/tags/$Tag"
    $release = Invoke-RestMethod -Uri $releaseUrl -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -eq $AssetName } | Select-Object -First 1
    if (-not $asset) { throw "Release asset not found: $Repository $Tag $AssetName" }
    if (-not $asset.digest -or -not $asset.digest.StartsWith("sha256:")) {
        throw "GitHub did not provide a SHA-256 digest for $AssetName"
    }
    Invoke-WebRequest -Uri $asset.browser_download_url -Headers $headers -OutFile $Destination
    $actual = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
    $expected = $asset.digest.Substring(7).ToLowerInvariant()
    if ($actual -ne $expected) { throw "SHA-256 mismatch for $AssetName" }
}

New-Item -ItemType Directory -Force -Path $binariesDir, $resourcesDir, $temporaryDir | Out-Null

$allReady = $true
foreach ($entry in $expectedFiles.GetEnumerator()) {
    if (-not (Test-Path -LiteralPath $entry.Key) -or
        (Get-FileHash -LiteralPath $entry.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) {
        $allReady = $false
        break
    }
}
if ($allReady) {
    Write-Host "SenseVoice CPU runtime and model are already verified."
    return
}

$sherpaAsset = "sherpa-onnx-$sherpaTag-win-x64-shared-MT-Release-no-tts.tar.bz2"
$modelAsset = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"
$sherpaArchive = Join-Path $temporaryDir $sherpaAsset
$modelArchive = Join-Path $temporaryDir $modelAsset

try {
    Write-Host "Downloading and verifying sherpa-onnx runtime..."
    Get-VerifiedReleaseAsset "k2-fsa/sherpa-onnx" $sherpaTag $sherpaAsset $sherpaArchive
    Write-Host "Downloading and verifying SenseVoice INT8 model..."
    Get-VerifiedReleaseAsset "k2-fsa/sherpa-onnx" "asr-models" $modelAsset $modelArchive

    tar -xf $sherpaArchive -C $temporaryDir
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract sherpa-onnx runtime" }
    tar -xf $modelArchive -C $temporaryDir
    if ($LASTEXITCODE -ne 0) { throw "Failed to extract SenseVoice model" }

    $sherpaRoot = Get-ChildItem -LiteralPath $temporaryDir -Directory |
        Where-Object { $_.Name -like "sherpa-onnx-$sherpaTag-win-x64-*" } |
        Select-Object -First 1
    $modelRoot = Get-ChildItem -LiteralPath $temporaryDir -Directory |
        Where-Object { $_.Name -like "sherpa-onnx-sense-voice-*" } |
        Select-Object -First 1
    if (-not $sherpaRoot -or -not $modelRoot) { throw "Expected archive layout was not found" }

    # The import libraries and matching DLLs are all in `lib`. Copying only
    # `bin` leaves stale sherpa DLLs beside a newer ONNX Runtime and crashes
    # during the first real recognition call.
    Get-ChildItem -LiteralPath (Join-Path $sherpaRoot.FullName "lib") -File |
        Where-Object { $_.Extension -in @(".dll", ".lib") } |
        ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $binariesDir -Force }
    Copy-Item -LiteralPath (Join-Path $modelRoot.FullName "model.int8.onnx") -Destination $resourcesDir -Force
    Copy-Item -LiteralPath (Join-Path $modelRoot.FullName "tokens.txt") -Destination $resourcesDir -Force

    foreach ($entry in $expectedFiles.GetEnumerator()) {
        if (-not (Test-Path -LiteralPath $entry.Key) -or
            (Get-FileHash -LiteralPath $entry.Key -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.Value) {
            throw "Unexpected hash after extraction: $($entry.Key)"
        }
    }
    Write-Host "SenseVoice CPU runtime and model are ready."
}
finally {
    if (Test-Path -LiteralPath $temporaryDir) {
        Remove-Item -LiteralPath $temporaryDir -Recurse -Force
    }
}
