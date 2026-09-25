# Reproduce the SenseVoice-only Windows NSIS build from a clean source tree.
# This script never signs, tags, publishes, or uploads an installer.
[CmdletBinding()]
param(
    [string]$VerifyInstalledDir = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "Run this build with PowerShell 7 (pwsh), not Windows PowerShell 5.1."
}
if (-not $IsWindows -or [Environment]::Is64BitOperatingSystem -ne $true) {
    throw "The NSIS installer must be built on 64-bit Windows."
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Push-Location $repoRoot
try {
    $dirty = @(git status --porcelain --untracked-files=normal)
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -gt 0) {
        throw "Use a clean clone of the exact reviewed commit; the source tree is dirty."
    }
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-f]{40}$') {
        throw "Unable to identify the reviewed source commit."
    }

    $config = Get-Content -LiteralPath "src-tauri/tauri.conf.json" -Raw | ConvertFrom-Json
    $version = [string]$config.version
    $package = Get-Content -LiteralPath "package.json" -Raw | ConvertFrom-Json
    if ($version -notmatch '^\d+\.\d+\.\d+$' -or $package.version -ne $version) {
        throw "Tauri and npm release versions do not match."
    }
    & npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed." }
    & (Join-Path $PSScriptRoot "fetch-sensevoice.ps1")
    & (Join-Path $PSScriptRoot "prepare-nsis.ps1")
    & (Join-Path $PSScriptRoot "assert-lean-nsis-assets.ps1") -RepositoryRoot $repoRoot

    & npm.cmd test
    if ($LASTEXITCODE -ne 0) { throw "Frontend tests failed." }
    & npm.cmd run lint
    if ($LASTEXITCODE -ne 0) { throw "Frontend lint failed." }
    & npm.cmd run format:check
    if ($LASTEXITCODE -ne 0) { throw "Frontend formatting failed." }
    & cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
    if ($LASTEXITCODE -ne 0) { throw "Rust formatting failed." }
    & cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Rust clippy failed." }
    & cargo test --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw "Rust tests failed." }
    & (Join-Path $PSScriptRoot "test-sensevoice.ps1")

    $buildStartedUtc = (Get-Date).ToUniversalTime()
    & npm.cmd run tauri -- build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed." }

    $executable = Join-Path $repoRoot "src-tauri/target/release/popspeak.exe"
    & (Join-Path $PSScriptRoot "verify-production-executable.ps1") `
        -Executable $executable -ExpectedVersion $version | Out-Null
    $bundleDir = Join-Path $repoRoot "src-tauri/target/release/bundle/nsis"
    $installers = @(Get-ChildItem -LiteralPath $bundleDir -File -Filter "*$version*setup.exe" |
        Where-Object { $_.LastWriteTimeUtc -ge $buildStartedUtc.AddSeconds(-5) })
    if ($installers.Count -ne 1) {
        throw "Expected exactly one newly built NSIS installer for $version; found $($installers.Count)."
    }
    $installer = $installers[0]
    if ($installer.Length -lt 100MB) {
        throw "Installer is unexpectedly small for the bundled SenseVoice model and offline WebView2."
    }
    if ($VerifyInstalledDir) {
        & (Join-Path $PSScriptRoot "assert-lean-nsis-assets.ps1") `
            -InstalledDir $VerifyInstalledDir -Installer $installer.FullName
    }

    [PSCustomObject][ordered]@{
        Commit = $commit
        Version = $version
        Installer = $installer.FullName
        Bytes = $installer.Length
        Sha256 = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        Signed = $false
        InstalledPayloadChecked = [bool]$VerifyInstalledDir
    }
    Write-Warning "This is an unsigned local build. Do not label it as a signed official release."
    if (-not $VerifyInstalledDir) {
        Write-Warning "Installer contents still need verification after installation on a clean Windows VM."
    }
} finally {
    Pop-Location
}
