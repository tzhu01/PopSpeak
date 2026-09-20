# Runs a real CPU inference through the pinned native runtime and INT8 model.
[CmdletBinding()]
param(
    [ValidateRange(1, 16)]
    [int]$Threads = 2
)

$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$manifest = Join-Path $repoRoot "src-tauri/Cargo.toml"
$binaries = Join-Path $repoRoot "src-tauri/binaries"
$modelDir = Join-Path $repoRoot "src-tauri/resources/sensevoice"
$depsDir = Join-Path $repoRoot "src-tauri/target/debug/deps"
$wavPath = Join-Path $env:TEMP "popspeak-sensevoice-smoke-$PID.wav"

Push-Location $repoRoot
try {
    & (Join-Path $PSScriptRoot "fetch-sensevoice.ps1")

    cargo test --manifest-path $manifest --offline --no-run
    if ($LASTEXITCODE -ne 0) { throw "SenseVoice smoke test compilation failed" }

    # Rust test executables live in target/debug/deps. Placing the pinned DLLs
    # beside them prevents Windows from selecting its system ONNX Runtime.
    @(
        "onnxruntime.dll",
        "onnxruntime_providers_shared.dll",
        "sherpa-onnx-c-api.dll",
        "sherpa-onnx-cxx-api.dll"
    ) | ForEach-Object {
        Copy-Item -LiteralPath (Join-Path $binaries $_) -Destination $depsDir -Force
    }

    Add-Type -AssemblyName System.Speech
    $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
    $voice = $synth.GetInstalledVoices() |
        Where-Object { $_.VoiceInfo.Culture.Name -eq "zh-CN" } |
        Select-Object -First 1
    $language = "auto"
    $phrase = "今天测试开头不会丢字"
    if (-not $voice) {
        $voice = $synth.GetInstalledVoices() |
            Where-Object { $_.VoiceInfo.Culture.Name -eq "en-US" } |
            Select-Object -First 1
        $phrase = "today is an offline voice test"
    }
    if (-not $voice) { throw "No zh-CN or en-US Windows SAPI voice is installed" }

    $synth.SelectVoice($voice.VoiceInfo.Name)
    $format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(
        16000,
        [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
        [System.Speech.AudioFormat.AudioChannel]::Mono
    )
    $synth.SetOutputToWaveFile($wavPath, $format)
    $synth.Speak($phrase)
    $synth.Dispose()

    $env:POPSPEAK_SENSEVOICE_MODEL_DIR = $modelDir
    $env:POPSPEAK_SENSEVOICE_SMOKE_WAV = $wavPath
    $env:POPSPEAK_SENSEVOICE_SMOKE_LANGUAGE = $language
    $env:POPSPEAK_SENSEVOICE_SMOKE_THREADS = $Threads
    cargo test --manifest-path $manifest --offline recognizes_real_wav_on_cpu -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "SenseVoice real CPU inference failed" }
}
finally {
    if (Test-Path -LiteralPath $wavPath) {
        [System.IO.File]::Delete($wavPath)
    }
    Pop-Location
}
