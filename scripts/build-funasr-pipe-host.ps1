param(
    [string]$UpstreamRevision = "fcf9a0d4a604d0859fc5927cca04df141e3b09c8",
    [string]$UpstreamSource = "",
    [switch]$SkipGeneric,
    [switch]$SkipAvx2
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "runtime-integrity.ps1")
$integrityManifest = Get-RuntimeIntegrityManifest
$funasrSourceSpec = $integrityManifest.funasrBuildSources.funasr
$llamaSourceSpec = $integrityManifest.funasrBuildSources.llamaCpp
if ($UpstreamRevision -notmatch '^[0-9a-f]{40}$') { throw "UpstreamRevision must be a pinned commit SHA" }
if ($UpstreamRevision -ne [string]$funasrSourceSpec.revision) {
    throw "No reviewed source-archive digest exists for FunASR revision $UpstreamRevision. Add it to runtime-integrity-manifest.json before building."
}
$repoRoot = Split-Path -Parent $PSScriptRoot
$hostSource = Join-Path $repoRoot "native\funasr-pipe-host\funasr-pipe-host.cpp"
$runtimeDir = Join-Path $repoRoot "src-tauri\resources\runtimes\funasr"
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) "popspeak-funasr-pipe-$UpstreamRevision"

function Remove-CachedSource([string]$targetPath) {
    $allowed = [IO.Path]::GetFullPath($temporaryRoot)
    $resolved = [IO.Path]::GetFullPath($targetPath)
    if (!$resolved.StartsWith($allowed + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Build cache target escaped its task directory"
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}

function Get-VerifiedSourceArchive(
    [object]$Spec,
    [string]$ArchivePath,
    [string]$Label
) {
    if (Test-Path -LiteralPath $ArchivePath) {
        # Cached archives are never trusted by path or marker alone. A stale or
        # modified cache fails closed instead of silently becoming build input.
        Assert-PinnedFile `
            $ArchivePath `
            ([string]$Spec.archiveSha256) `
            ([long]$Spec.archiveSize) `
            "$Label cached source archive"
        return
    }

    $partialPath = "$ArchivePath.partial"
    $curlArguments = @(
        "-L", "--fail", "--retry", "4", "--retry-delay", "2",
        "--connect-timeout", "20", "--proto", "=https", "--tlsv1.2",
        "--output", $partialPath, ([string]$Spec.archiveUrl)
    )
    & curl.exe @curlArguments
    if ($LASTEXITCODE -ne 0) { throw "Unable to download $Label from its pinned official source" }
    Assert-PinnedFile `
        $partialPath `
        ([string]$Spec.archiveSha256) `
        ([long]$Spec.archiveSize) `
        "$Label downloaded source archive"
    Move-Item -LiteralPath $partialPath -Destination $ArchivePath
}

function Resolve-VisualStudioTool([string]$relativePath) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (!(Test-Path -LiteralPath $vswhere)) {
        throw "Visual Studio Installer (vswhere.exe) was not found"
    }
    $tool = & $vswhere -latest -products * `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -find $relativePath | Select-Object -First 1
    if (!$tool) { throw "Visual Studio tool was not found: $relativePath" }
    return $tool
}

function Import-VisualStudioEnvironment {
    $vcvars = Resolve-VisualStudioTool "VC\Auxiliary\Build\vcvars64.bat"
    $commandLine = '"' + $vcvars + '" >nul && set'
    $environmentLines = & cmd.exe /d /s /c $commandLine
    if ($LASTEXITCODE -ne 0) { throw "Unable to initialize the Visual Studio x64 environment" }
    foreach ($line in $environmentLines) {
        $separator = $line.IndexOf('=')
        if ($separator -le 0) { continue }
        $name = $line.Substring(0, $separator)
        $value = $line.Substring($separator + 1)
        [System.Environment]::SetEnvironmentVariable($name, $value, 'Process')
    }
}

function Resolve-UpstreamSource {
    if ($UpstreamSource.Trim()) {
        throw "UpstreamSource cannot bypass the integrity-gated build. Use the reviewed pinned archive in runtime-integrity-manifest.json."
    }

    $sourceRoot = Join-Path $temporaryRoot "source"
    $archive = Join-Path $temporaryRoot "FunASR-$UpstreamRevision.zip"
    New-Item -ItemType Directory -Path $temporaryRoot -Force | Out-Null
    Get-VerifiedSourceArchive $funasrSourceSpec $archive "FunASR $UpstreamRevision"

    # Extract from the authenticated archive on every run. A revision marker
    # alone cannot authenticate an already-expanded directory.
    if (Test-Path -LiteralPath $sourceRoot) { Remove-CachedSource $sourceRoot }
    New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $sourceRoot
    $expanded = @(Get-ChildItem -LiteralPath $sourceRoot -Directory)
    if ($expanded.Count -ne 1) {
        throw "Pinned FunASR archive must contain exactly one source directory; found $($expanded.Count)"
    }
    $requiredCmake = Join-Path $expanded[0].FullName "runtime\llama.cpp\CMakeLists.txt"
    if (!(Test-Path -LiteralPath $requiredCmake -PathType Leaf)) {
        throw "Pinned FunASR archive is missing runtime/llama.cpp/CMakeLists.txt"
    }
    return $expanded[0].FullName
}

function Resolve-LlamaSource([string]$upstreamRoot) {
    $funasrCmake = Join-Path $upstreamRoot "runtime\llama.cpp\CMakeLists.txt"
    $cmakeText = Get-Content -LiteralPath $funasrCmake -Raw
    $match = [regex]::Match($cmakeText, 'GIT_TAG\s+([0-9a-f]{40})')
    if (!$match.Success) { throw "Pinned llama.cpp revision was not found in $funasrCmake" }
    $revision = $match.Groups[1].Value
    if ($revision -ne [string]$llamaSourceSpec.revision) {
        throw "FunASR requests llama.cpp $revision, but no matching reviewed source-archive digest exists in runtime-integrity-manifest.json"
    }
    $sourceRoot = Join-Path $temporaryRoot "llama-$revision"
    $archive = Join-Path $temporaryRoot "llama-$revision.zip"
    Get-VerifiedSourceArchive $llamaSourceSpec $archive "llama.cpp $revision"

    if (Test-Path -LiteralPath $sourceRoot) { Remove-CachedSource $sourceRoot }
    New-Item -ItemType Directory -Path $sourceRoot -Force | Out-Null
    Expand-Archive -LiteralPath $archive -DestinationPath $sourceRoot
    $expanded = @(Get-ChildItem -LiteralPath $sourceRoot -Directory)
    if ($expanded.Count -ne 1) {
        throw "Pinned llama.cpp archive must contain exactly one source directory; found $($expanded.Count)"
    }
    if (!(Test-Path -LiteralPath (Join-Path $expanded[0].FullName "CMakeLists.txt") -PathType Leaf)) {
        throw "Pinned llama.cpp archive is missing CMakeLists.txt"
    }
    return $expanded[0].FullName
}

function Build-Host(
    [string]$cmake,
    [string]$ninja,
    [string]$upstreamRoot,
    [string]$llamaSource,
    [string]$variant,
    [bool]$avx2
) {
    $llamaRuntime = Join-Path $upstreamRoot "runtime\llama.cpp"
    $overlay = Join-Path $temporaryRoot "overlay-$variant"
    $build = Join-Path $temporaryRoot "build-$variant"
    # CMake/Ninja rebuild changed host sources incrementally; preserve the
    # pinned third-party build cache rather than deleting it for every patch.
    New-Item -ItemType Directory -Path $overlay -Force | Out-Null

    $cmakeLists = @"
cmake_minimum_required(VERSION 3.16)
project(popspeak-funasr-pipe-host CXX C)
set(FUNASR_RUNTIME "$($llamaRuntime.Replace('\', '/'))")
add_subdirectory(`${FUNASR_RUNTIME} funasr-upstream EXCLUDE_FROM_ALL)
add_executable(llama-funasr-pipe-host "$($hostSource.Replace('\', '/'))")
target_include_directories(llama-funasr-pipe-host PRIVATE
    "`$`{FUNASR_RUNTIME`}/fun-asr-nano/funasr-cli"
    "`$`{FUNASR_RUNTIME`}/funasr-common")
target_link_libraries(llama-funasr-pipe-host PRIVATE llama ggml)
target_compile_features(llama-funasr-pipe-host PRIVATE cxx_std_17)
if(MSVC)
    target_compile_definitions(llama-funasr-pipe-host PRIVATE NOMINMAX _USE_MATH_DEFINES)
    target_compile_options(llama-funasr-pipe-host PRIVATE /utf-8)
endif()
"@
    Set-Content -LiteralPath (Join-Path $overlay "CMakeLists.txt") -Value $cmakeLists

    $configure = @(
        "-S", $overlay,
        "-B", $build,
        "-G", "Ninja",
        "-DCMAKE_MAKE_PROGRAM=$ninja",
        "-DCMAKE_BUILD_TYPE=Release",
        "-DFETCHCONTENT_SOURCE_DIR_LLAMA=$llamaSource",
        "-DGGML_NATIVE=OFF",
        "-DGGML_AVX=ON",
        "-DGGML_AVX2=$($avx2.ToString().ToUpperInvariant())",
        "-DGGML_FMA=$($avx2.ToString().ToUpperInvariant())",
        "-DGGML_F16C=$($avx2.ToString().ToUpperInvariant())",
        "-DGGML_BMI2=$($avx2.ToString().ToUpperInvariant())"
    )
    & $cmake @configure
    if ($LASTEXITCODE -ne 0) { throw "CMake configure failed for $variant" }
    & $cmake --build $build --target llama-funasr-pipe-host --parallel 4
    if ($LASTEXITCODE -ne 0) { throw "CMake build failed for $variant" }

    $binary = Join-Path $build "llama-funasr-pipe-host.exe"
    if (!(Test-Path -LiteralPath $binary)) {
        $binary = Get-ChildItem -LiteralPath $build -Filter "llama-funasr-pipe-host.exe" -Recurse |
            Select-Object -First 1 -ExpandProperty FullName
    }
    if (!$binary) { throw "Built host binary was not found for $variant" }
    New-Item -ItemType Directory -Path $runtimeDir -Force | Out-Null
    $destination = if ($avx2) {
        Join-Path $runtimeDir "llama-funasr-pipe-host-avx2.exe"
    } else {
        Join-Path $runtimeDir "llama-funasr-pipe-host.exe"
    }
    Copy-Item -LiteralPath $binary -Destination $destination -Force
    Write-Host "Installed $variant host: $destination"
}

$cmake = Resolve-VisualStudioTool "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
$ninja = Resolve-VisualStudioTool "Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe"
Import-VisualStudioEnvironment
$upstreamRoot = Resolve-UpstreamSource
$llamaSource = Resolve-LlamaSource $upstreamRoot

if (!$SkipGeneric) { Build-Host $cmake $ninja $upstreamRoot $llamaSource "generic" $false }
if (!$SkipAvx2) { Build-Host $cmake $ninja $upstreamRoot $llamaSource "avx2" $true }
