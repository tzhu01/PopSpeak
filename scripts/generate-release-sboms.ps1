# Generate two dependency SBOMs and a separate, hashed installer-asset manifest.
# The dependency SBOMs do NOT describe everything embedded in the NSIS payload.
# Install the pinned Rust generator separately before running without -PayloadOnly:
#   cargo install cargo-cyclonedx --version 0.5.9 --locked
# The pinned npm generator is executed through npm's cache, not added to package.json.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,
    [string]$OutputDir = "",
    [string]$InstalledDir = "",
    [string]$ExtractedNsisDir = "",
    [switch]$PayloadOnly,
    [switch]$AllowDirtySource
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "Run this script with PowerShell 7 (pwsh)."
}
if (-not $IsWindows) {
    throw "This release manifest describes the Windows x64 NSIS build."
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$installer = Get-Item -LiteralPath $InstallerPath -ErrorAction Stop
if ($installer.PSIsContainer) { throw "InstallerPath must be a file." }
if (-not $OutputDir) { $OutputDir = $installer.DirectoryName }
$outputRoot = [IO.Path]::GetFullPath($OutputDir)
$config = Get-Content -LiteralPath (Join-Path $repoRoot "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json
$version = [string]$config.version
if ($version -notmatch '^\d+\.\d+\.\d+$' -or $installer.Name -notmatch [regex]::Escape($version)) {
    throw "Installer name and Tauri version do not agree."
}
if ($config.bundle.windows.webviewInstallMode.type -ne "offlineInstaller") {
    throw "The reviewed release profile must embed the offline WebView2 installer."
}

Push-Location $repoRoot
try {
    $commit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-f]{40}$') {
        throw "A Git source commit is required for release provenance."
    }
    $dirty = @(& git status --porcelain --untracked-files=normal).Count -gt 0
    if ($LASTEXITCODE -ne 0) { throw "Unable to inspect source status." }
    if ($dirty -and -not $AllowDirtySource) {
        throw "Generate release artifacts from a clean checkout. Use -AllowDirtySource only for local testing."
    }
    $sourceDateEpoch = (& git show -s --format=%ct HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $sourceDateEpoch -notmatch '^\d+$') {
        throw "Unable to read source commit timestamp."
    }

    # This checks the explicit lean resource allowlist and pinned upstream hashes.
    if ($InstalledDir) {
        $installedRoot = (Resolve-Path -LiteralPath $InstalledDir).Path
        & (Join-Path $PSScriptRoot "assert-lean-nsis-assets.ps1") -InstalledDir $installedRoot -Installer $installer.FullName | Out-Null
        $assetRoot = $installedRoot
    } else {
        & (Join-Path $PSScriptRoot "assert-lean-nsis-assets.ps1") -RepositoryRoot $repoRoot | Out-Null
        $assetRoot = $repoRoot
    }

    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    $npmSbomPath = Join-Path $outputRoot "PopSpeak-$version-npm.cdx.json"
    $rustSbomPath = Join-Path $outputRoot "PopSpeak-$version-rust-windows.cdx.json"
    $manifestPath = Join-Path $outputRoot "PopSpeak-$version-payload-manifest.json"

    if (-not $PayloadOnly) {
        $generator = Get-Command cargo-cyclonedx -ErrorAction SilentlyContinue
        if (-not $generator) {
            throw "Install pinned cargo-cyclonedx 0.5.9 first: cargo install cargo-cyclonedx --version 0.5.9 --locked"
        }
        $cargoGeneratorVersion = (& $generator.Source cyclonedx --version) -join " "
        if ($LASTEXITCODE -ne 0 -or $cargoGeneratorVersion -notmatch '\b0\.5\.9\b') {
            throw "Expected cargo-cyclonedx 0.5.9, found: $cargoGeneratorVersion"
        }

        & npm.cmd exec --yes --package=@cyclonedx/cyclonedx-npm@6.0.1 -- cyclonedx-npm `
            --package-lock-only --omit dev --spec-version 1.6 `
            --output-reproducible --validate --output-file $npmSbomPath
        if ($LASTEXITCODE -ne 0) { throw "CycloneDX npm generation or validation failed." }

        # The Rust generator emits next to Cargo.toml. Use a unique temporary name
        # there, then move it into the release artifact directory.
        $temporaryRustBase = "popspeak-rust-$PID-$([guid]::NewGuid().ToString('N'))"
        $oldEpoch = $env:SOURCE_DATE_EPOCH
        $env:SOURCE_DATE_EPOCH = $sourceDateEpoch
        try {
            & $generator.Source cyclonedx `
                --manifest-path (Join-Path $repoRoot "src-tauri/Cargo.toml") `
                --format json --spec-version 1.5 `
                --target x86_64-pc-windows-msvc `
                --override-filename $temporaryRustBase
            if ($LASTEXITCODE -ne 0) { throw "CycloneDX Rust generation failed." }
            $generated = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "src-tauri") -File |
                Where-Object { $_.Name -like "$temporaryRustBase*" })
            if ($generated.Count -ne 1) {
                throw "Expected one Rust SBOM from cargo-cyclonedx; found $($generated.Count)."
            }
            Move-Item -LiteralPath $generated[0].FullName -Destination $rustSbomPath -Force
        } finally {
            $env:SOURCE_DATE_EPOCH = $oldEpoch
            Get-ChildItem -LiteralPath (Join-Path $repoRoot "src-tauri") -File |
                Where-Object { $_.Name -like "$temporaryRustBase*" } |
                Remove-Item -Force
        }

        foreach ($spec in @(@{ Path = $npmSbomPath; Version = "1.6" }, @{ Path = $rustSbomPath; Version = "1.5" })) {
            $bom = Get-Content -LiteralPath $spec.Path -Raw | ConvertFrom-Json
            if ($bom.bomFormat -ne "CycloneDX" -or $bom.specVersion -ne $spec.Version -or
                @($bom.components).Count -eq 0 -or -not $bom.metadata.component) {
                throw "Unexpected or empty CycloneDX document: $($spec.Path)"
            }
        }
    }

    function New-AssetEntry {
        param([string]$Kind, [string]$Path, [string]$InstalledPath)
        $file = Get-Item -LiteralPath $Path -ErrorAction Stop
        if ($file.PSIsContainer) { throw "Expected file: $Path" }
        return [ordered]@{
            kind = $Kind
            installedPath = $InstalledPath.Replace('\', '/')
            filename = $file.Name
            bytes = $file.Length
            sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }

    $sourceAssets = [ordered]@{
        "models/sensevoice/model.int8.onnx" = "src-tauri/resources/sensevoice/model.int8.onnx"
        "models/sensevoice/tokens.txt" = "src-tauri/resources/sensevoice/tokens.txt"
        "onnxruntime.dll" = "src-tauri/binaries/onnxruntime.dll"
        "onnxruntime_providers_shared.dll" = "src-tauri/binaries/onnxruntime_providers_shared.dll"
        "sherpa-onnx-c-api.dll" = "src-tauri/binaries/sherpa-onnx-c-api.dll"
        "sherpa-onnx-cxx-api.dll" = "src-tauri/binaries/sherpa-onnx-cxx-api.dll"
    }
    $assets = @()
    foreach ($installedPath in $sourceAssets.Keys) {
        $relative = if ($InstalledDir) { $installedPath } else { $sourceAssets[$installedPath] }
        $kind = if ($installedPath.StartsWith("models/")) { "speech-model" } else { "native-runtime" }
        $assets += New-AssetEntry $kind (Join-Path $assetRoot $relative) $installedPath
    }

    $licenseRoot = if ($InstalledDir) { Join-Path $assetRoot "licenses" } else { Join-Path $repoRoot "src-tauri/resources/licenses" }
    foreach ($licenseFile in @(Get-ChildItem -LiteralPath $licenseRoot -Recurse -File | Sort-Object FullName)) {
        $relative = $licenseFile.FullName.Substring($licenseRoot.Length).TrimStart('\', '/')
        $assets += New-AssetEntry "license-notice" $licenseFile.FullName "licenses/$relative"
    }
    $appExecutable = if ($InstalledDir) {
        Join-Path $assetRoot "PopSpeak.exe"
    } else {
        Join-Path $repoRoot "src-tauri/target/release/popspeak.exe"
    }
    if (Test-Path -LiteralPath $appExecutable -PathType Leaf) {
        $assets += New-AssetEntry "application" $appExecutable "PopSpeak.exe"
    } elseif ($InstalledDir) {
        throw "The installed application executable is missing."
    }

    $webViewFiles = @()
    if ($ExtractedNsisDir) {
        $extractedRoot = (Resolve-Path -LiteralPath $ExtractedNsisDir).Path
        foreach ($file in @(Get-ChildItem -LiteralPath $extractedRoot -Recurse -File |
            Where-Object { $_.Name -match '(?i)webview2.*\.(exe|msi|cab)$' } | Sort-Object FullName)) {
            $relative = $file.FullName.Substring($extractedRoot.Length).TrimStart('\', '/')
            $webViewFiles += New-AssetEntry "offline-webview2-bootstrapper" $file.FullName $relative
        }
    }

    $signatureStatus = (Get-AuthenticodeSignature -LiteralPath $installer.FullName).Status.ToString()
    $manifest = [ordered]@{
        format = "PopSpeak packaged-asset manifest"
        schemaVersion = 1
        product = "PopSpeak"
        version = $version
        target = "x86_64-pc-windows-msvc"
        scope = if ($InstalledDir) { "verified installed files plus installer" } else { "verified build inputs plus installer; not proof of NSIS extraction" }
        source = [ordered]@{
            repository = "https://github.com/tzhu01/PopSpeak"
            commit = $commit
            dirty = [bool]$dirty
            sourceDateEpoch = [long]$sourceDateEpoch
        }
        installer = [ordered]@{
            filename = $installer.Name
            bytes = $installer.Length
            sha256 = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            authenticodeStatus = $signatureStatus
        }
        packagedAssets = $assets
        offlineWebView2 = [ordered]@{
            configuredMode = "offlineInstaller"
            embeddedFileVerified = $webViewFiles.Count -gt 0
            extractedFiles = $webViewFiles
            note = if ($webViewFiles.Count -gt 0) { "Files were found in the supplied NSIS extraction directory." } else { "WebView2 is configured to be embedded; its internal bytes were not independently extracted or hashed." }
        }
        dependencySboms = if ($PayloadOnly) { @() } else { @(
            [ordered]@{ filename = [IO.Path]::GetFileName($npmSbomPath); kind = "npm-production"; sha256 = (Get-FileHash -LiteralPath $npmSbomPath -Algorithm SHA256).Hash.ToLowerInvariant() },
            [ordered]@{ filename = [IO.Path]::GetFileName($rustSbomPath); kind = "rust-windows"; sha256 = (Get-FileHash -LiteralPath $rustSbomPath -Algorithm SHA256).Hash.ToLowerInvariant() }
        ) }
        limitations = @(
            "The dependency SBOMs describe resolved npm and Cargo dependencies, not binary composition of the NSIS archive.",
            "Native DLLs, model files, and license notices are inventoried separately above; WebView2 bytes need NSIS extraction for an independent hash.",
            "A source commit and SHA-256 hashes establish traceability, not a signed build attestation."
        )
    }
    $manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
    [PSCustomObject][ordered]@{
        Installer = $installer.FullName
        NpmSbom = if ($PayloadOnly) { $null } else { $npmSbomPath }
        RustSbom = if ($PayloadOnly) { $null } else { $rustSbomPath }
        PayloadManifest = $manifestPath
        InstalledPayloadChecked = [bool]$InstalledDir
        ExtractedWebView2Files = $webViewFiles.Count
    }
} finally {
    Pop-Location
}
