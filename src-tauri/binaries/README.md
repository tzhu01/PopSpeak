# Native SenseVoice dependencies

This directory is intentionally limited to the four DLLs loaded by the PopSpeak
process itself:

- `onnxruntime.dll`
- `onnxruntime_providers_shared.dll`
- `sherpa-onnx-c-api.dll`
- `sherpa-onnx-cxx-api.dll`

Run `scripts/fetch-sensevoice.ps1` to download and verify them. They are ignored by
Git and flattened next to `PopSpeak.exe` during packaging so the Windows loader can
find them.

Whisper and llama.cpp are not stored here. Their DLL names overlap, so each runtime
is kept in its own `src-tauri/resources/runtimes/<engine>/` directory. Prepare them
with `scripts/fetch-whisper.ps1` and `scripts/fetch-llama-server.ps1`.
