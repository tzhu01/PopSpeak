# Validate only the files selected for the lean NSIS bundle. This deliberately
# does not scan the old, full resources/runtimes directory: those files are not
# part of this installer and must not be copied into it.
[CmdletBinding(DefaultParameterSetName = "Source")]
param(
    [Parameter(ParameterSetName = "Source")]
    [string]$RepositoryRoot = (Join-Path $PSScriptRoot ".."),

    [Parameter(Mandatory = $true, ParameterSetName = "Installed")]
    [string]$InstalledDir,

    [Parameter(ParameterSetName = "Installed")]
    [string]$Installer,

    [Parameter(ParameterSetName = "Installed")]
    [switch]$RequireSignature
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$sourceMode = $PSCmdlet.ParameterSetName -eq "Source"
if ($sourceMode) {
    $repoRoot = (Resolve-Path -LiteralPath $RepositoryRoot).Path
    $assetRoot = $repoRoot
} else {
    $assetRoot = (Resolve-Path -LiteralPath $InstalledDir).Path
}

$assets = @(
    @{ Source = "src-tauri/resources/sensevoice/model.int8.onnx"; Installed = "models/sensevoice/model.int8.onnx"; Sha256 = "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51"; Native = $false },
    @{ Source = "src-tauri/resources/sensevoice/tokens.txt"; Installed = "models/sensevoice/tokens.txt"; Sha256 = "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc"; Native = $false },
    @{ Source = "src-tauri/binaries/onnxruntime.dll"; Installed = "onnxruntime.dll"; Sha256 = "daa77083a45bf525da0dde9e87f85d8eb146f58f9c9aa7124ca84545e1c0f148"; Native = $true },
    @{ Source = "src-tauri/binaries/onnxruntime_providers_shared.dll"; Installed = "onnxruntime_providers_shared.dll"; Sha256 = "190d10767c321f324d3785368a0b752d9c5a9e06cb5d4d97bb176f58bdb652f3"; Native = $true },
    @{ Source = "src-tauri/binaries/sherpa-onnx-c-api.dll"; Installed = "sherpa-onnx-c-api.dll"; Sha256 = "dcfc89cf50fbd0fb77c115a6b53ee9b2739e57d6d1f4158a3aeab31dd7139676"; Native = $true },
    @{ Source = "src-tauri/binaries/sherpa-onnx-cxx-api.dll"; Installed = "sherpa-onnx-cxx-api.dll"; Sha256 = "1df614bd2e55254c5811dcafe11c5ed4ccd9a285c92eea7b603db2b5b9a15688"; Native = $true }
)

if ($sourceMode) {
    $configPath = Join-Path $repoRoot "src-tauri/tauri.conf.json"
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $actualMap = $config.bundle.resources
    $expectedMap = [ordered]@{
        "resources/sensevoice/" = "models/sensevoice/"
        "resources/licenses/" = "licenses/"
        "binaries/onnxruntime.dll" = "onnxruntime.dll"
        "binaries/onnxruntime_providers_shared.dll" = "onnxruntime_providers_shared.dll"
        "binaries/sherpa-onnx-c-api.dll" = "sherpa-onnx-c-api.dll"
        "binaries/sherpa-onnx-cxx-api.dll" = "sherpa-onnx-cxx-api.dll"
    }
    $actualNames = @($actualMap.PSObject.Properties.Name)
    if ($actualNames.Count -ne $expectedMap.Count) {
        throw "Tauri resource list is not the reviewed lean NSIS allowlist."
    }
    foreach ($name in $expectedMap.Keys) {
        $property = $actualMap.PSObject.Properties[$name]
        if ($null -eq $property -or $property.Value -ne $expectedMap[$name]) {
            throw "Unexpected Tauri resource mapping: $name"
        }
    }
    if (@($config.bundle.targets).Count -ne 1 -or $config.bundle.targets[0] -ne "nsis") {
        throw "This profile must build only an NSIS installer."
    }
    if ($config.bundle.windows.webviewInstallMode.type -ne "offlineInstaller") {
        throw "The NSIS profile must include the offline WebView2 installer."
    }
}

foreach ($asset in $assets) {
    $relative = if ($sourceMode) { $asset.Source } else { $asset.Installed }
    $path = Join-Path $assetRoot $relative
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing lean installer asset: $relative"
    }
    if (-not ($RequireSignature -and $asset.Native -and -not $sourceMode)) {
        $actualHash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $asset.Sha256) {
            throw "Pinned asset SHA-256 mismatch: $relative"
        }
    }
}

$modelDir = if ($sourceMode) {
    Join-Path $assetRoot "src-tauri/resources/sensevoice"
} else {
    Join-Path $assetRoot "models/sensevoice"
}
$modelFiles = @(Get-ChildItem -LiteralPath $modelDir -Recurse -File)
if ($modelFiles.Count -ne 2) {
    throw "SenseVoice bundle must contain exactly the pinned model and tokens."
}

$licenseSource = Join-Path $repoRoot "src-tauri/resources/licenses"
$licenseDir = if ($sourceMode) { $licenseSource } else { Join-Path $assetRoot "licenses" }
$requiredLicenses = @(
    "SenseVoice-MIT.txt", "sherpa-onnx-Apache-2.0.txt", "onnxruntime-MIT.txt",
    "onnxruntime-ThirdPartyNotices.txt", "transcribe-cpp-MIT.txt",
    "transcribe-cpp-ggml-MIT.txt", "transcribe-cpp-miniz-MIT.txt"
)
foreach ($name in $requiredLicenses) {
    $licensePath = Join-Path $licenseDir $name
    if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf) -or
        (Get-Item -LiteralPath $licensePath).Length -lt 200) {
        throw "Required license is missing or truncated: $name"
    }
}
if (-not $sourceMode) {
    foreach ($sourceLicense in @(Get-ChildItem -LiteralPath $licenseSource -Recurse -File)) {
        $relative = $sourceLicense.FullName.Substring($licenseSource.Length).TrimStart('\', '/')
        $packagedLicense = Join-Path $licenseDir $relative
        if (-not (Test-Path -LiteralPath $packagedLicense -PathType Leaf) -or
            (Get-FileHash -LiteralPath $packagedLicense -Algorithm SHA256).Hash -ne
            (Get-FileHash -LiteralPath $sourceLicense.FullName -Algorithm SHA256).Hash) {
            throw "Packaged license differs from reviewed source: $relative"
        }
    }
    $forbiddenDirs = @("runtimes", "models/whisper", "models/llm", "models/funasr-nano")
    foreach ($relative in $forbiddenDirs) {
        if (Test-Path -LiteralPath (Join-Path $assetRoot $relative)) {
            throw "Unexpected model or runtime in lean installer: $relative"
        }
    }
    $privateNames = '^(settings\.json|history\.json|credentials\.json|pending-transcript\.json|\.env(?:\..*)?|activation\.sqlite3(?:-wal|-shm)?)$'
    $privateExtensions = @(".db", ".sqlite", ".sqlite3", ".pem", ".pfx", ".p12", ".key")
    $privateFiles = @(Get-ChildItem -LiteralPath $assetRoot -Recurse -File | Where-Object {
        $_.Name -match $privateNames -or $_.Extension -in $privateExtensions
    })
    if ($privateFiles.Count -gt 0) { throw "Private data was found in the installed payload." }
    $exe = Join-Path $assetRoot "PopSpeak.exe"
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "PopSpeak.exe is missing." }
    $version = (Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
    & (Join-Path $PSScriptRoot "verify-production-executable.ps1") -Executable $exe -ExpectedVersion $version | Out-Null
    if ($RequireSignature) {
        if (-not $Installer) { throw "A signed-payload check also requires -Installer." }
        $signedPaths = @($exe, $Installer) + @($assets | Where-Object Native | ForEach-Object { Join-Path $assetRoot $_.Installed })
        & (Join-Path $PSScriptRoot "assert-windows-signature.ps1") -Path $signedPaths
    }
}

$scanRoot = if ($sourceMode) { Join-Path $repoRoot "src-tauri/resources/sensevoice" } else { $assetRoot }
$blocked = @(Get-ChildItem -LiteralPath $scanRoot -Recurse -File | Where-Object { $_.Name -like "libomp*.dll" })
if ($blocked.Count -gt 0) { throw "Blocked libomp DLL found in the lean installer asset set." }
if ($Installer) {
    $installerPath = (Resolve-Path -LiteralPath $Installer).Path
    Write-Host "Installer SHA-256: $((Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant())"
}
Write-Host "Lean NSIS asset integrity passed ($($assets.Count) pinned files)."
