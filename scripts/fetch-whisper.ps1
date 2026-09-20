param([switch]$SkipModel)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "runtime-integrity.ps1")
$integrityManifest = Get-RuntimeIntegrityManifest
$runtimeSpec = $integrityManifest.whisper
$repoRoot = Split-Path -Parent $PSScriptRoot
$runtimeDir = Join-Path $repoRoot "src-tauri/resources/runtimes/whisper"
$modelsDir = Join-Path $repoRoot "src-tauri/resources/models"
$temporaryDir = Join-Path $env:TEMP "popspeak-whisper-$PID"
$tinyModelHash = "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21"
$baseModelHash = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"

New-Item -ItemType Directory -Force -Path $runtimeDir, $modelsDir, $temporaryDir | Out-Null

try {
    # Reuse is allowed only when the directory is an exact byte-for-byte match
    # for the reviewed release subset. Extra DLLs are rejected because Windows
    # DLL search order makes them part of the executable supply chain.
    $runtimeReady = Test-ExactRuntimeFileSet $runtimeDir $runtimeSpec.installedFiles
    if (-not $runtimeReady) {
        $archive = Join-Path $temporaryDir ([string]$runtimeSpec.assetName)
        Invoke-WebRequest -Uri ([string]$runtimeSpec.archiveUrl) -Headers @{ "User-Agent" = "PopSpeak-build" } -OutFile $archive
        Assert-PinnedFile $archive ([string]$runtimeSpec.archiveSha256) ([long]$runtimeSpec.archiveSize) "Whisper release archive"
        Expand-Archive -LiteralPath $archive -DestinationPath $temporaryDir

        $sourceExecutables = @(Get-ChildItem -LiteralPath $temporaryDir -Recurse -Filter "whisper-cli.exe" -File)
        if ($sourceExecutables.Count -ne 1) {
            throw "Expected exactly one whisper-cli.exe in $($runtimeSpec.assetName), found $($sourceExecutables.Count)"
        }
        $sourceDirectory = $sourceExecutables[0].Directory.FullName
        $stagingDir = Join-Path $temporaryDir "verified-runtime"
        New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null
        foreach ($entry in $runtimeSpec.installedFiles) {
            $sourcePath = Join-Path $sourceDirectory ([string]$entry.name)
            Assert-PinnedFile $sourcePath ([string]$entry.sha256) ([long]$entry.size) "Whisper asset file $($entry.name)"
            Copy-Item -LiteralPath $sourcePath -Destination $stagingDir
        }
        Assert-ExactRuntimeFileSet $stagingDir $runtimeSpec.installedFiles

        Get-ChildItem -LiteralPath $runtimeDir -Force -File | Remove-Item -Force
        Get-ChildItem -LiteralPath $stagingDir -File |
            ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $runtimeDir -Force }
        Assert-ExactRuntimeFileSet $runtimeDir $runtimeSpec.installedFiles
    }

    if (-not $SkipModel) {
        $models = @(
            @{ Name = "ggml-tiny.bin"; Hash = $tinyModelHash },
            @{ Name = "ggml-base.bin"; Hash = $baseModelHash }
        )
        foreach ($model in $models) {
            $modelDest = Join-Path $modelsDir $model.Name
            if (-not (Test-Path -LiteralPath $modelDest)) {
                Invoke-WebRequest -Uri "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$($model.Name)" -OutFile $modelDest
            }
            $actualModelHash = (Get-FileHash -LiteralPath $modelDest -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actualModelHash -ne $model.Hash) { throw "SHA-256 mismatch for $($model.Name)" }
        }
    }
    Write-Host "Whisper CPU sidecar and tiny/base models are ready."
}
finally {
    if (Test-Path -LiteralPath $temporaryDir) {
        Remove-Item -LiteralPath $temporaryDir -Recurse -Force
    }
}
