# Import the release PFX, sign bundled native runtimes, and configure Tauri to
# sign the application and NSIS installer. Intended for an ephemeral CI runner.
[CmdletBinding()]
param(
    [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE)) {
    throw "WINDOWS_CERTIFICATE is required (base64-encoded code-signing PFX)."
}
if ([string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE_PASSWORD)) {
    throw "WINDOWS_CERTIFICATE_PASSWORD is required."
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$pfxPath = Join-Path $temporaryRoot ("popspeak-signing-{0}.pfx" -f [guid]::NewGuid())

try {
    $base64 = $env:WINDOWS_CERTIFICATE -replace '\s', ''
    [System.IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($base64))
    $password = ConvertTo-SecureString -String $env:WINDOWS_CERTIFICATE_PASSWORD -Force -AsPlainText
    $certificate = Import-PfxCertificate `
        -FilePath $pfxPath `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -Password $password

    if (!$certificate -or !$certificate.HasPrivateKey) {
        throw "The imported certificate does not contain an accessible private key."
    }
    if ($certificate.NotAfter -le (Get-Date)) {
        throw "The imported code-signing certificate has expired."
    }
    $codeSigningOid = "1.3.6.1.5.5.7.3.3"
    $hasCodeSigningUsage = @($certificate.EnhancedKeyUsageList | Where-Object {
        $_.ObjectId.Value -eq $codeSigningOid
    }).Count -gt 0
    if (!$hasCodeSigningUsage) {
        throw "The imported certificate is not valid for code signing."
    }

    $signTool = Get-ChildItem -Path "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -File |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
        Select-Object -First 1
    if (!$signTool) {
        throw "signtool.exe was not found in the Windows SDK."
    }

    $nativeFiles = @(
        Get-ChildItem -LiteralPath (Join-Path $repoRoot "src-tauri\resources\runtimes") -Recurse -File |
            Where-Object { $_.Extension -in ".exe", ".dll" }
        Get-ChildItem -LiteralPath (Join-Path $repoRoot "src-tauri\binaries") -File |
            Where-Object { $_.Extension -eq ".dll" }
    )
    if ($nativeFiles.Count -eq 0) {
        throw "No native runtime files were found to sign."
    }
    foreach ($file in $nativeFiles) {
        & $signTool.FullName sign /sha1 $certificate.Thumbprint /fd sha256 /tr $TimestampUrl /td sha256 $file.FullName
        if ($LASTEXITCODE -ne 0) {
            throw "signtool failed for $($file.FullName)"
        }
    }

    $configPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
    $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $windowsSigning = [pscustomobject]@{
        certificateThumbprint = $certificate.Thumbprint
        digestAlgorithm = "sha256"
        timestampUrl = $TimestampUrl
    }
    if ($config.bundle.PSObject.Properties["windows"]) {
        $config.bundle.windows = $windowsSigning
    } else {
        $config.bundle | Add-Member -NotePropertyName windows -NotePropertyValue $windowsSigning
    }
    $config | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath $configPath -Encoding UTF8

    Write-Host "Release signing prepared for $($certificate.Subject); expires $($certificate.NotAfter.ToString('u'))."
} finally {
    if (Test-Path -LiteralPath $pfxPath) {
        Remove-Item -LiteralPath $pfxPath -Force
    }
}
