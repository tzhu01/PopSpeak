# Refuse to publish files that do not carry a valid Authenticode signature.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path
)

$ErrorActionPreference = "Stop"
$files = foreach ($item in $Path) {
    @(Get-ChildItem -Path $item -File -ErrorAction SilentlyContinue)
}
if ($files.Count -eq 0) {
    throw "No files matched the signature verification paths."
}

foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Invalid Authenticode signature ($($signature.Status)): $($file.FullName)"
    }
    Write-Host "Valid signature: $($file.FullName) [$($signature.SignerCertificate.Subject)]"
}
