[CmdletBinding()]
param(
    [string]$RuntimeRoot = (Join-Path $PSScriptRoot "..\src-tauri\resources\runtimes"),
    [string]$LicenseRoot = (Join-Path $PSScriptRoot "..\src-tauri\resources\licenses")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$runtimePath = [IO.Path]::GetFullPath($RuntimeRoot)
$licensePath = [IO.Path]::GetFullPath($LicenseRoot)
$gateErrors = [System.Collections.Generic.List[string]]::new()

if (-not (Test-Path -LiteralPath $runtimePath -PathType Container)) {
    $gateErrors.Add("Runtime directory does not exist: $runtimePath")
}

if (-not (Test-Path -LiteralPath $licensePath -PathType Container)) {
    $gateErrors.Add("License directory does not exist: $licensePath")
}

if ($gateErrors.Count -eq 0) {
    # Microsoft documents /openmp:llvm as unavailable for production because
    # the required libomp DLLs are currently not redistributable. Block every
    # libomp DLL until the runtime is rebuilt without it, or a separately built
    # and audited LLVM artifact is pinned here deliberately.
    # https://learn.microsoft.com/en-us/cpp/build/reference/openmp-enable-openmp-2-0-support
    $openMpDlls = @(
        Get-ChildItem -LiteralPath $runtimePath -Recurse -File |
            Where-Object { $_.Name -like "libomp*.dll" }
    )

    foreach ($dll in $openMpDlls) {
        $relativePath = [IO.Path]::GetRelativePath($repoRoot, $dll.FullName).Replace("\", "/")
        $sha256 = (Get-FileHash -LiteralPath $dll.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $gateErrors.Add(
            "Non-redistributable or unaudited LLVM OpenMP runtime: $relativePath (SHA-256 $sha256)"
        )
    }

    $sdlDlls = @(
        Get-ChildItem -LiteralPath $runtimePath -Recurse -File -Filter "SDL2.dll"
    )
    if ($sdlDlls.Count -gt 0) {
        $sdlLicense = Join-Path $licensePath "SDL2-zlib.txt"
        if (-not (Test-Path -LiteralPath $sdlLicense -PathType Leaf)) {
            $gateErrors.Add("SDL2.dll is present but licenses/SDL2-zlib.txt is missing.")
        }

        $auditedSdlSha256 = "de23db1694a3c7a4a735e7ecd3d214b2023cc2267922c6c35d30c7fc7370d677"
        foreach ($dll in $sdlDlls) {
            $sha256 = (Get-FileHash -LiteralPath $dll.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($sha256 -ne $auditedSdlSha256) {
                $relativePath = [IO.Path]::GetRelativePath($repoRoot, $dll.FullName).Replace("\", "/")
                $gateErrors.Add(
                    "Unaudited SDL2.dll: $relativePath (SHA-256 $sha256; expected $auditedSdlSha256)."
                )
            }
        }
    }
}

if ($gateErrors.Count -gt 0) {
    Write-Host "Runtime redistribution gate FAILED:" -ForegroundColor Red
    foreach ($message in $gateErrors) {
        Write-Host "  - $message" -ForegroundColor Red
    }
    Write-Host "See docs/OPEN_SOURCE_BLOCKERS.md before publishing binaries." -ForegroundColor Yellow
    throw "Runtime redistribution gate failed with $($gateErrors.Count) blocking issue(s)."
}

Write-Host "Runtime redistribution gate passed." -ForegroundColor Green
Write-Host "No blocked libomp DLL was found, and every bundled SDL2.dll matches the audited SDL 2.28.5 artifact."
