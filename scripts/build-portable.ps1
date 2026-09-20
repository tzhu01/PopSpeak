# PopSpeak reproducible Windows portable build.
# It never downloads during packaging: every required binary/model must have
# been prepared first, which makes a successful build safe to publish offline.
[CmdletBinding()]
param(
    [string]$OutputDir = ".\dist-portable\PopSpeak",
    [switch]$Build,
    [switch]$CreateZip,
    [switch]$Create7z,
    [string]$SevenZipPath = "",
    [string]$VCRuntimeDir = "",
    [switch]$RequireSignature,
    [switch]$CleanOutput,
    [switch]$CleanWorkspace
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$outputPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDir))
$allowedRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "dist-portable"))

function Resolve-OfficialVCRuntime([string]$ExplicitDirectory) {
    $candidates = @()
    if ($ExplicitDirectory.Trim()) {
        $candidates = @([IO.Path]::GetFullPath($ExplicitDirectory))
    } else {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
        if (!(Test-Path -LiteralPath $vswhere -PathType Leaf)) {
            throw "Visual Studio Installer not found. Specify -VCRuntimeDir pointing to an official VS VC\Redist\MSVC\<version>\x64\Microsoft.VC143.CRT directory."
        }
        $installations = @(& $vswhere -all -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
        foreach ($installation in $installations) {
            $redistRoot = Join-Path $installation "VC\Redist\MSVC"
            if (Test-Path -LiteralPath $redistRoot) {
                $candidates += @(Get-ChildItem -LiteralPath $redistRoot -Directory |
                    Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
                    Sort-Object { [version]$_.Name } -Descending |
                    ForEach-Object { Join-Path $_.FullName "x64\Microsoft.VC143.CRT" })
            }
        }
    }
    foreach ($candidate in $candidates) {
        if (!(Test-Path -LiteralPath $candidate -PathType Container)) { continue }
        $resolved = (Resolve-Path -LiteralPath $candidate).Path.TrimEnd('\', '/')
        if ($resolved -notmatch '(?i)\\VC\\Redist\\MSVC\\[^\\]+\\x64\\Microsoft\.VC143\.CRT$') {
            throw "CRT source must be the official Visual Studio x64 redistributable directory, never System32: $resolved"
        }
        $dlls = @(Get-ChildItem -LiteralPath $resolved -File -Filter '*.dll')
        foreach ($requiredDll in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
            if (!($dlls.Name -contains $requiredDll)) { throw "Incomplete official CRT directory: missing $requiredDll" }
        }
        foreach ($dll in $dlls) {
            if ($dll.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "CRT source must not contain links" }
            $signature = Get-AuthenticodeSignature -LiteralPath $dll.FullName
            if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
                throw "CRT DLL must have a valid Microsoft signature: $($dll.Name) ($($signature.Status))"
            }
        }
        return $dlls
    }
    throw "Official Visual C++ x64 CRT redistributable not found; install the VS C++ workload or supply -VCRuntimeDir."
}

if (!$outputPath.StartsWith($allowedRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputDir must resolve inside $allowedRoot"
}

if ($CleanOutput) {
    if (Test-Path -LiteralPath $outputPath) {
        Remove-Item -LiteralPath $outputPath -Recurse -Force
    }
    Write-Host "Removed portable staging directory: $outputPath"
    return
}

if ($CleanWorkspace) {
    foreach ($relativePath in @("node_modules", "dist", "src-tauri\target", "src-tauri\gen")) {
        $generatedPath = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $relativePath))
        if (!$generatedPath.StartsWith($repoRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Generated path escaped repository root: $generatedPath"
        }
        if (Test-Path -LiteralPath $generatedPath) {
            Remove-Item -LiteralPath $generatedPath -Recurse -Force
        }
    }
    Write-Host "Removed generated build caches from $repoRoot"
    return
}

foreach ($destination in @($outputPath, "$outputPath.zip", "$outputPath.zip.sha256", "$outputPath.7z", "$outputPath.7z.sha256")) {
    if (Test-Path -LiteralPath $destination) {
        throw "A release artifact already exists at $destination. Use a fresh staging directory so existing releases and user files are preserved."
    }
}

Push-Location $repoRoot
try {
    if ($Build) {
        npm run tauri build -- --no-bundle --features custom-protocol
        if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }
    }

    $required = [ordered]@{
        App = "src-tauri\target\release\popspeak.exe"
        Whisper = "src-tauri\resources\runtimes\whisper\whisper-cli.exe"
        LlamaServer = "src-tauri\resources\runtimes\llama\llama-server.exe"
        FunAsr = "src-tauri\resources\runtimes\funasr\llama-funasr-pipe-host.exe"
        FunAsrAvx2 = "src-tauri\resources\runtimes\funasr\llama-funasr-pipe-host-avx2.exe"
        SenseVoiceModel = "src-tauri\resources\sensevoice\model.int8.onnx"
        SenseVoiceTokens = "src-tauri\resources\sensevoice\tokens.txt"
        WhisperTiny = "src-tauri\resources\models\ggml-tiny.bin"
        WhisperBase = "src-tauri\resources\models\ggml-base.bin"
        QwenPolisher = "src-tauri\resources\models\qwen2.5-0.5b-instruct-q4_k_m.gguf"
        ModelCatalog = "src-tauri\resources\model-catalog.json"
        PortableReadme = "src-tauri\resources\PORTABLE_README_ZH.txt"
        FunAsrLicense = "src-tauri\resources\licenses\FunASR-MIT.txt"
        ApacheLicense = "src-tauri\resources\licenses\Apache-2.0.txt"
        OnnxRuntime = "src-tauri\binaries\onnxruntime.dll"
        OnnxProviders = "src-tauri\binaries\onnxruntime_providers_shared.dll"
        SherpaC = "src-tauri\binaries\sherpa-onnx-c-api.dll"
        SherpaCxx = "src-tauri\binaries\sherpa-onnx-cxx-api.dll"
    }

    $missing = @($required.GetEnumerator() | Where-Object { !(Test-Path -LiteralPath $_.Value) })
    if ($missing.Count -gt 0) {
        $detail = ($missing | ForEach-Object { " - $($_.Key): $($_.Value)" }) -join [Environment]::NewLine
        throw "Portable build refused because required files are missing:$([Environment]::NewLine)$detail"
    }

    # A path existing is not proof that it contains a license: reject the old
    # empty/one-byte placeholder failure before creating a distributable.
    $licenseFiles = @(Get-ChildItem -LiteralPath "src-tauri\resources\licenses" -File -Recurse)
    if ($licenseFiles.Count -eq 0) { throw "Third-party license directory is empty" }
    foreach ($licenseFile in $licenseFiles) {
        if ($licenseFile.Length -le 200) {
            throw "Third-party license is empty or truncated: $($licenseFile.FullName) ($($licenseFile.Length) bytes)"
        }
    }
    $vcRuntimeFiles = @(Resolve-OfficialVCRuntime $VCRuntimeDir)

    # Reject stale cargo/development-server executables even when -Build was
    # omitted. The executable itself must prove its frontend is embedded.
    $expectedVersion = (Get-Content -LiteralPath "src-tauri\tauri.conf.json" -Raw | ConvertFrom-Json).version
    $releaseCheck = & (Join-Path $PSScriptRoot "verify-production-executable.ps1") `
        -Executable $required.App -ExpectedVersion $expectedVersion

    $expectedRuntimeHashes = @{
        $required.FunAsr = "33e57bc71d63f90557f8e0bbab3d19e0d9b24b3022ce7585111e16505910e2c7"
        $required.FunAsrAvx2 = "c0d3ab3a416bc2d8bb7a385f3a60382ab8f1c2187a798c19e9d0e1c55c257430"
    }
    foreach ($runtimePath in $expectedRuntimeHashes.Keys) {
        $actualHash = (Get-FileHash -LiteralPath $runtimePath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $expectedRuntimeHashes[$runtimePath]) {
            throw "Fun-ASR runtime SHA-256 mismatch: $runtimePath"
        }
    }

    if ($RequireSignature) {
        & (Join-Path $PSScriptRoot "assert-windows-signature.ps1") -Path @(
            $required.App,
            $required.Whisper,
            $required.LlamaServer
            $required.FunAsr
            $required.FunAsrAvx2
        )
    }

    New-Item -ItemType Directory -Path $outputPath | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "models\sensevoice") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "models\whisper") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "models\llm") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "models\funasr-nano") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "runtimes\whisper") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "runtimes\llama") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "runtimes\funasr") -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $outputPath "licenses") -Force | Out-Null

    Copy-Item -LiteralPath $required.App -Destination (Join-Path $outputPath "PopSpeak.exe")
    foreach ($vcRuntimeFile in $vcRuntimeFiles) {
        Copy-Item -LiteralPath $vcRuntimeFile.FullName -Destination (Join-Path $outputPath $vcRuntimeFile.Name)
    }
    $releaseCheck | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputPath "release-build.json") -Encoding UTF8
    Get-ChildItem -LiteralPath "src-tauri\resources\runtimes\whisper" -File |
        Copy-Item -Destination (Join-Path $outputPath "runtimes\whisper")
    Get-ChildItem -LiteralPath "src-tauri\resources\runtimes\llama" -File |
        Copy-Item -Destination (Join-Path $outputPath "runtimes\llama")
    Copy-Item -LiteralPath $required.FunAsr -Destination (Join-Path $outputPath "runtimes\funasr\llama-funasr-pipe-host.exe")
    Copy-Item -LiteralPath $required.FunAsrAvx2 -Destination (Join-Path $outputPath "runtimes\funasr\llama-funasr-pipe-host-avx2.exe")
    @("onnxruntime.dll", "onnxruntime_providers_shared.dll", "sherpa-onnx-c-api.dll", "sherpa-onnx-cxx-api.dll") |
        ForEach-Object { Copy-Item -LiteralPath (Join-Path "src-tauri\binaries" $_) -Destination $outputPath }

    Copy-Item -LiteralPath $required.SenseVoiceModel -Destination (Join-Path $outputPath "models\sensevoice\model.int8.onnx")
    Copy-Item -LiteralPath $required.SenseVoiceTokens -Destination (Join-Path $outputPath "models\sensevoice\tokens.txt")
    Copy-Item -LiteralPath $required.WhisperTiny -Destination (Join-Path $outputPath "models\whisper\ggml-tiny.bin")
    Copy-Item -LiteralPath $required.WhisperBase -Destination (Join-Path $outputPath "models\whisper\ggml-base.bin")
    Copy-Item -LiteralPath $required.QwenPolisher -Destination (Join-Path $outputPath "models\llm\qwen2.5-0.5b-instruct-q4_k_m.gguf")
    Copy-Item -LiteralPath $required.ModelCatalog -Destination (Join-Path $outputPath "models\catalog.json")

    $catalog = Get-Content -LiteralPath (Join-Path $outputPath "models\catalog.json") -Raw | ConvertFrom-Json
    foreach ($model in $catalog.models) {
        foreach ($artifact in $model.artifacts) {
            $artifactPath = Join-Path (Join-Path $outputPath "models") $artifact.path
            if (!(Test-Path -LiteralPath $artifactPath)) {
                throw "Model catalog artifact is missing: $($artifact.path)"
            }
            $file = Get-Item -LiteralPath $artifactPath
            if ($file.Length -ne [long]$artifact.bytes) {
                throw "Model size mismatch for $($artifact.path): expected $($artifact.bytes), got $($file.Length)"
            }
            $actualHash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actualHash -ne $artifact.sha256.ToLowerInvariant()) {
                throw "Model SHA-256 mismatch for $($artifact.path)"
            }
        }
    }
    Copy-Item -LiteralPath "LICENSE" -Destination (Join-Path $outputPath "LICENSE.txt")
    if (Test-Path -LiteralPath "THIRD_PARTY_NOTICES.md") {
        Copy-Item -LiteralPath "THIRD_PARTY_NOTICES.md" -Destination $outputPath
    }
    Get-ChildItem -LiteralPath "src-tauri\resources\licenses" |
        Copy-Item -Destination (Join-Path $outputPath "licenses") -Recurse -Force
    foreach ($licenseFile in $licenseFiles) {
        $licenseRoot = [IO.Path]::GetFullPath("src-tauri\resources\licenses")
        $relativeLicensePath = $licenseFile.FullName.Substring($licenseRoot.Length).TrimStart('\', '/')
        $copiedLicense = Get-Item -LiteralPath (Join-Path (Join-Path $outputPath "licenses") $relativeLicensePath)
        if ($copiedLicense.Length -le 200 -or
            (Get-FileHash -LiteralPath $copiedLicense.FullName -Algorithm SHA256).Hash -ne
            (Get-FileHash -LiteralPath $licenseFile.FullName -Algorithm SHA256).Hash) {
            throw "Third-party license copy is incomplete: $relativeLicensePath"
        }
    }

    Copy-Item -LiteralPath "src-tauri\resources\PORTABLE_README_ZH.txt" -Destination (Join-Path $outputPath "README.txt")

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

    & (Join-Path $PSScriptRoot "verify-portable.ps1") -PortableDir $outputPath `
        -ExpectedVersion $expectedVersion -ExpectedExe $required.App

    if ($CreateZip) {
        $zipPath = "$outputPath.zip"
        Compress-Archive -Path (Join-Path $outputPath "*") -DestinationPath $zipPath -CompressionLevel Optimal
        $zipExtractRoot = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ("PopSpeak-zip-check-" + [Guid]::NewGuid().ToString("N"))))
        New-Item -ItemType Directory -Path $zipExtractRoot | Out-Null
        Expand-Archive -LiteralPath $zipPath -DestinationPath $zipExtractRoot
        & (Join-Path $PSScriptRoot "verify-portable.ps1") -PortableDir $zipExtractRoot `
            -ExpectedVersion $expectedVersion -ExpectedExe $required.App
        $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/')
        if (!$zipExtractRoot.StartsWith($temporaryRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove unexpected ZIP verification path"
        }
        Remove-Item -LiteralPath $zipExtractRoot -Recurse -Force
        $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$zipPath.sha256" -Value "$zipHash  $([IO.Path]::GetFileName($zipPath))" -Encoding ASCII
        Write-Host "Created $zipPath"
    }

    if ($Create7z) {
        if ([string]::IsNullOrWhiteSpace($SevenZipPath)) {
            $sevenZipCommand = Get-Command "7z.exe" -ErrorAction SilentlyContinue
            if ($null -eq $sevenZipCommand) {
                $sevenZipCommand = Get-Command "7za.exe" -ErrorAction SilentlyContinue
            }
            if ($null -eq $sevenZipCommand) {
                throw "Create7z requires -SevenZipPath or a 7z.exe/7za.exe command on PATH"
            }
            $SevenZipPath = $sevenZipCommand.Source
        }
        $resolvedSevenZip = [System.IO.Path]::GetFullPath($SevenZipPath)
        if (!(Test-Path -LiteralPath $resolvedSevenZip -PathType Leaf)) {
            throw "7-Zip executable does not exist: $resolvedSevenZip"
        }

        $archivePath = "$outputPath.7z"
        $outputParent = Split-Path -Parent $outputPath
        $outputLeaf = Split-Path -Leaf $outputPath
        Push-Location $outputParent
        try {
            # Solid LZMA2 gives the best practical lossless ratio for already
            # quantized GGUF files while remaining directly extractable by 7-Zip.
            & $resolvedSevenZip a $archivePath $outputLeaf `
                -t7z -mx=9 -m0=LZMA2:d=128m:fb=273 -ms=on -mmt=4 -y
            if ($LASTEXITCODE -ne 0) {
                throw "7-Zip compression failed with exit code $LASTEXITCODE"
            }
        } finally {
            Pop-Location
        }
        & (Join-Path $PSScriptRoot "verify-portable-archive.ps1") -ArchivePath $archivePath `
            -SevenZipPath $resolvedSevenZip -ExpectedVersion $expectedVersion -ExpectedExe $required.App
        $archiveHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$archivePath.sha256" -Value "$archiveHash  $([IO.Path]::GetFileName($archivePath))" -Encoding ASCII
        Write-Host "Created and verified $archivePath"
    }

    $bytes = (Get-ChildItem -LiteralPath $outputPath -Recurse -File | Measure-Object Length -Sum).Sum
    Write-Host ("Portable build ready: {0} ({1:N1} MiB)" -f $outputPath, ($bytes / 1MB))
} finally {
    Pop-Location
}
