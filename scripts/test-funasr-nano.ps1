# End-to-end smoke test for the resident FunASR named-pipe host. The same PID
# serves repeated PCM requests, proving that encoder and Qwen3 are loaded only once.
[CmdletBinding()]
param(
    [ValidateSet('generic', 'avx2')][string]$Variant = 'generic',
    [string]$ModelDirectory = '',
    [string]$RuntimeDirectory = ''
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$modelDir = if ($ModelDirectory) { (Resolve-Path -LiteralPath $ModelDirectory).Path } else { Join-Path $repoRoot "src-tauri\resources\models\funasr-nano" }
$runtimeDir = if ($RuntimeDirectory) { (Resolve-Path -LiteralPath $RuntimeDirectory).Path } else { Join-Path $repoRoot "src-tauri\resources\runtimes\funasr" }
$wavPath = Join-Path ([System.IO.Path]::GetTempPath()) "popspeak-funasr-smoke-$PID.wav"
$pipeLeaf = "popspeak-funasr-smoke-$PID-$([Guid]::NewGuid().ToString('N'))"
$pipePath = "\\.\pipe\$pipeLeaf"

$q4Model = Join-Path $modelDir "qwen3-0.6b-q4km.gguf"
$q5Model = Join-Path $modelDir "qwen3-0.6b-q5km.gguf"
$llmModel = if (Test-Path -LiteralPath $q4Model) { $q4Model } else { $q5Model }
$required = @(
    (Join-Path $runtimeDir "llama-funasr-pipe-host.exe"),
    (Join-Path $runtimeDir "llama-funasr-pipe-host-avx2.exe"),
    (Join-Path $modelDir "funasr-encoder-f16.gguf"),
    $llmModel,
    (Join-Path $modelDir "fsmn-vad.gguf")
)
$missing = @($required | Where-Object { !(Test-Path -LiteralPath $_ -PathType Leaf) })
if ($missing.Count -gt 0) { throw "Run scripts/fetch-funasr-nano.ps1 first. Missing: $($missing -join ', ')" }

function Get-WavPcm16 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ([Text.Encoding]::ASCII.GetString($bytes, 0, 4) -ne "RIFF") { throw "Not a RIFF WAV" }
    $offset = 12
    $formatSeen = $false
    while ($offset + 8 -le $bytes.Length) {
        $chunkId = [Text.Encoding]::ASCII.GetString($bytes, $offset, 4)
        $chunkSize = [BitConverter]::ToUInt32($bytes, $offset + 4)
        $payload = $offset + 8
        if ($chunkId -eq "fmt ") {
            $audioFormat = [BitConverter]::ToUInt16($bytes, $payload)
            $channels = [BitConverter]::ToUInt16($bytes, $payload + 2)
            $sampleRate = [BitConverter]::ToUInt32($bytes, $payload + 4)
            $bits = [BitConverter]::ToUInt16($bytes, $payload + 14)
            if ($audioFormat -ne 1 -or $channels -ne 1 -or $sampleRate -ne 16000 -or $bits -ne 16) {
                throw "Smoke WAV must be mono 16 kHz PCM16"
            }
            $formatSeen = $true
        } elseif ($chunkId -eq "data") {
            if (!$formatSeen) { throw "WAV data appeared before fmt chunk" }
            $pcm = [byte[]]::new($chunkSize)
            [Array]::Copy($bytes, $payload, $pcm, 0, $chunkSize)
            return $pcm
        }
        $offset = $payload + $chunkSize + ($chunkSize % 2)
    }
    throw "WAV has no PCM data chunk"
}

function Invoke-FunAsrPipe {
    param(
        [Parameter(Mandatory = $true)][string]$PipeName,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][byte[]]$Pcm,
        [Parameter(Mandatory = $true)][UInt64]$RequestId,
        [UInt16]$Flags = 0,
        [int]$ConnectTimeoutMs = 90000,
        [string[]]$Hotwords = @()
    )
    $pipe = [System.IO.Pipes.NamedPipeClientStream]::new(
        ".", $PipeName, [System.IO.Pipes.PipeDirection]::InOut,
        [System.IO.Pipes.PipeOptions]::None)
    try {
        $pipe.Connect($ConnectTimeoutMs)
        $writer = [System.IO.BinaryWriter]::new($pipe, [Text.Encoding]::UTF8, $true)
        $reader = [System.IO.BinaryReader]::new($pipe, [Text.Encoding]::UTF8, $true)
        $writer.Write([UInt32]0x41465350)
        $writer.Write([UInt16]2)
        $writer.Write($Flags)
        $writer.Write([UInt32]16000)
        $writer.Write($RequestId)
        $writer.Write([UInt64]$Pcm.Length)
        $hotwordBytes = [Text.Encoding]::UTF8.GetBytes(($Hotwords -join "`n"))
        $writer.Write([UInt32]$hotwordBytes.Length)
        if ($hotwordBytes.Length -gt 0) { $writer.Write($hotwordBytes) }
        if ($Pcm.Length -gt 0) { $writer.Write($Pcm) }
        $writer.Flush()
        if ($reader.ReadUInt32() -ne 0x52465350) { throw "Invalid response magic" }
        if ($reader.ReadUInt16() -ne 2) { throw "Invalid response version" }
        $status = $reader.ReadUInt16()
        $returnedId = $reader.ReadUInt64()
        $textLength = $reader.ReadUInt32()
        $elapsedMs = $reader.ReadUInt32()
        $text = [Text.Encoding]::UTF8.GetString($reader.ReadBytes($textLength))
        if ($returnedId -ne $RequestId) { throw "Response request ID mismatch" }
        if ($status -ne 0) { throw "FunASR host failed: $text" }
        return [PSCustomObject]@{ Text = $text; ElapsedMs = $elapsedMs }
    } finally {
        $pipe.Dispose()
    }
}

Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$process = $null
try {
    $voice = $synth.GetInstalledVoices() |
        Where-Object { $_.VoiceInfo.Culture.Name -eq "zh-CN" } |
        Select-Object -First 1
    if (!$voice) { throw "A zh-CN Windows SAPI voice is required for this smoke test" }
    $synth.SelectVoice($voice.VoiceInfo.Name)
    $phrase = "今天请张三确认项目报价，明天上午给我回复。"
    $format = [System.Speech.AudioFormat.SpeechAudioFormatInfo]::new(
        16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
        [System.Speech.AudioFormat.AudioChannel]::Mono)
    $synth.SetOutputToWaveFile($wavPath, $format)
    $synth.Speak($phrase)
    $synth.SetOutputToNull()
    $pcm = Get-WavPcm16 $wavPath

    $runtimeName = if ($Variant -eq 'avx2') { "llama-funasr-pipe-host-avx2.exe" } else { "llama-funasr-pipe-host.exe" }
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = Join-Path $runtimeDir $runtimeName
    $startInfo.Arguments = @(
        "--enc", ('"{0}"' -f (Join-Path $modelDir "funasr-encoder-f16.gguf")),
        "-m", ('"{0}"' -f $llmModel),
        "--vad", ('"{0}"' -f (Join-Path $modelDir "fsmn-vad.gguf")),
        "--pipe", $pipePath,
        "--threads", "4"
    ) -join " "
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $process = [System.Diagnostics.Process]::Start($startInfo)

    $first = Invoke-FunAsrPipe $pipeLeaf $pcm 1
    $second = Invoke-FunAsrPipe $pipeLeaf $pcm 2 -Hotwords @('张珊')
    $third = Invoke-FunAsrPipe $pipeLeaf $pcm 3 -Hotwords @('张三')
    $cleared = Invoke-FunAsrPipe $pipeLeaf $pcm 4
    if ([string]::IsNullOrWhiteSpace($first.Text) -or [string]::IsNullOrWhiteSpace($second.Text)) {
        throw "FunASR returned an empty transcript"
    }
    if ($process.HasExited) { throw "Resident host exited between requests" }
    Write-Host "Resident PID: $($process.Id)"
    Write-Host "Request 1: $($first.Text) ($($first.ElapsedMs) ms)"
    Write-Host "Request 2: $($second.Text) ($($second.ElapsedMs) ms)"
    Write-Host "Request 3 (edited hotword): $($third.Text) ($($third.ElapsedMs) ms)"
    Write-Host "Request 4 (removed hotword): $($cleared.Text) ($($cleared.ElapsedMs) ms)"
    if ($first.Text -ne $cleared.Text) { throw "Empty vocabulary failed to restore deterministic baseline" }
    if ([string]::IsNullOrWhiteSpace($third.Text)) { throw "Edited vocabulary returned an empty transcript" }
    $invalidRejected = $false
    try { [void](Invoke-FunAsrPipe $pipeLeaf ([byte[]]::new(0)) 5 -Hotwords @('<|system|>')) }
    catch { $invalidRejected = $_.Exception.Message -match 'reserved character' }
    if (!$invalidRejected) { throw "Native host accepted a reserved prompt delimiter" }
    Write-Host "Reserved delimiter rejected; same PID served all requests. Raw outputs above have no text post-processing."

    [void](Invoke-FunAsrPipe $pipeLeaf ([byte[]]::new(0)) 6 0x8000 5000)
    if (!$process.WaitForExit(5000)) { throw "Resident host did not stop after shutdown frame" }
} finally {
    $synth.Dispose()
    if ($process -and !$process.HasExited) { $process.Kill() }
    if (Test-Path -LiteralPath $wavPath) { [System.IO.File]::Delete($wavPath) }
}
