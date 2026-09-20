param([switch]$SkipModel)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "runtime-integrity.ps1")
$integrityManifest = Get-RuntimeIntegrityManifest
$runtimeSpec = $integrityManifest.llama
$repoRoot = Split-Path -Parent $PSScriptRoot
$runtimeDir = Join-Path $repoRoot "src-tauri/resources/runtimes/llama"
$modelsDir = Join-Path $repoRoot "src-tauri/resources/models"
$temporaryDir = Join-Path $env:TEMP "popspeak-llama-$PID"
$modelName = "qwen2.5-0.5b-instruct-q4_k_m.gguf"
$modelHash = "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db"
$modelUrl = "https://modelscope.cn/api/v1/models/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/master/$modelName"

New-Item -ItemType Directory -Force -Path $runtimeDir, $modelsDir, $temporaryDir | Out-Null

try {
    # Reuse is allowed only for the complete reviewed runtime subset. Checking
    # the tiny launcher alone does not authenticate the implementation DLLs it
    # loads, and unexpected DLLs are unsafe under the Windows search rules.
    $runtimeReady = Test-ExactRuntimeFileSet $runtimeDir $runtimeSpec.installedFiles
    if (-not $runtimeReady) {
        $archive = Join-Path $temporaryDir ([string]$runtimeSpec.assetName)
        Invoke-WebRequest -Uri ([string]$runtimeSpec.archiveUrl) -Headers @{ "User-Agent" = "PopSpeak-build" } -OutFile $archive
        Assert-PinnedFile $archive ([string]$runtimeSpec.archiveSha256) ([long]$runtimeSpec.archiveSize) "llama.cpp release archive"
        Expand-Archive -LiteralPath $archive -DestinationPath $temporaryDir

        $sourceExecutables = @(Get-ChildItem -LiteralPath $temporaryDir -Recurse -Filter "llama-server.exe" -File)
        if ($sourceExecutables.Count -ne 1) {
            throw "Expected exactly one llama-server.exe in $($runtimeSpec.assetName), found $($sourceExecutables.Count)"
        }
        $sourceDirectory = $sourceExecutables[0].Directory.FullName
        $stagingDir = Join-Path $temporaryDir "verified-runtime"
        New-Item -ItemType Directory -Path $stagingDir -Force | Out-Null
        foreach ($entry in $runtimeSpec.installedFiles) {
            $sourcePath = Join-Path $sourceDirectory ([string]$entry.name)
            Assert-PinnedFile $sourcePath ([string]$entry.sha256) ([long]$entry.size) "llama.cpp asset file $($entry.name)"
            Copy-Item -LiteralPath $sourcePath -Destination $stagingDir
        }
        Assert-ExactRuntimeFileSet $stagingDir $runtimeSpec.installedFiles

        Get-ChildItem -LiteralPath $runtimeDir -Force -File | Remove-Item -Force
        Get-ChildItem -LiteralPath $stagingDir -File |
            ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $runtimeDir -Force }
        Assert-ExactRuntimeFileSet $runtimeDir $runtimeSpec.installedFiles
    }

    if (-not $SkipModel) {
        $modelDest = Join-Path $modelsDir $modelName
        if (-not (Test-Path -LiteralPath $modelDest)) {
            Invoke-WebRequest -Uri $modelUrl -OutFile $modelDest
        }
        $actualModelHash = (Get-FileHash -LiteralPath $modelDest -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualModelHash -ne $modelHash) { throw "SHA-256 mismatch for $modelName" }
    }

    Write-Host "llama.cpp CPU server and Qwen2.5 polishing model are ready."
}
finally {
    if (Test-Path -LiteralPath $temporaryDir) {
        Remove-Item -LiteralPath $temporaryDir -Recurse -Force
    }
}
