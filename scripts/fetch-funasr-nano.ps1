# Build the resident generic/AVX2 Windows hosts and fetch the pinned
# Fun-ASR-Nano Q4_K_M model pack from ModelScope's domestic CDN.
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$modelDir = Join-Path $repoRoot "src-tauri\resources\models\funasr-nano"

function Install-VerifiedFile {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$Sha256
    )
    if (Test-Path -LiteralPath $Destination) {
        $current = (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($current -eq $Sha256) {
            Write-Host "Already verified: $Destination"
            return
        }
    }
    $partial = "$Destination.partial"
    & curl.exe -fL --retry 12 --retry-delay 3 --retry-all-errors --continue-at - --output $partial $Url
    if ($LASTEXITCODE -ne 0) { throw "Download failed: $Url" }
    $actual = (Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Sha256) {
        throw "SHA-256 mismatch for $Url. Expected $Sha256, got $actual"
    }
    Move-Item -LiteralPath $partial -Destination $Destination -Force
    Write-Host "Installed: $Destination"
}

New-Item -ItemType Directory -Force -Path $modelDir | Out-Null
& (Join-Path $PSScriptRoot "build-funasr-pipe-host.ps1")
if ($LASTEXITCODE -ne 0) { throw "Resident FunASR host build failed" }

$encoderRevision = "51dcf4922439c10e0c2e59bc99be8a343d2fe71f"
$vadRevision = "f04fc3013641c8d59c156e2cbf171c1ad596f74d"
$modelBase = "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/$encoderRevision"
Install-VerifiedFile "$modelBase/funasr-encoder-f16.gguf" (Join-Path $modelDir "funasr-encoder-f16.gguf") "f92f91d01a24fbed6c863495b2ee8c6a6788144a02858b75743f0946668de8a2"
Install-VerifiedFile "$modelBase/qwen3-0.6b-q4km.gguf" (Join-Path $modelDir "qwen3-0.6b-q4km.gguf") "cc5057552aa9dddedcda73ea8889854e8a257eb07d0a561b7234465c1e856f22"
Install-VerifiedFile "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/$vadRevision/fsmn-vad.gguf" (Join-Path $modelDir "fsmn-vad.gguf") "1270f2559c495f4e7b6e739541151027d360761a3fda43fc147034f5719f5479"

Write-Host "Fun-ASR-Nano Q4_K_M model and resident pipe hosts are ready."
