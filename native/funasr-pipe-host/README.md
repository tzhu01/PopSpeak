# PopSpeak FunASR pipe host

This Windows-only native process keeps Fun-ASR-Nano's encoder and Qwen3 model
loaded once, then accepts 16 kHz mono PCM16 requests through a local named pipe.
It reuses the pinned upstream FunASR llama.cpp implementation in the same C++
translation unit, so the inference graph and decoding behavior remain aligned
with the official runtime.

Build both the generic x64 and AVX2 executables with:

```powershell
.\scripts\build-funasr-pipe-host.ps1
```

The build script downloads the pinned upstream source into the system temporary
directory, builds statically with Visual Studio Build Tools, and copies only the
two resulting executables into `src-tauri/resources/runtimes/funasr`.

Protocol v2 is little-endian and uses one request per pipe connection:

- request: magic `PSFA` (u32), version (u16), flags (u16), sample rate (u32),
  request id (u64), PCM byte count (u64), hotword byte count (u32); followed by
  UTF-8 newline-separated hotwords, then raw PCM16 bytes (32-byte header);
- response: magic `PSFR`, version, status, request id, UTF-8 byte count, elapsed
  milliseconds, followed by UTF-8 transcript or error text.

The pipe name contains a per-process random token and remote clients are
rejected. A zero-length PCM request is the readiness probe. Flag `0x8000` asks
the host to shut down after replying.

Hotwords follow the official FunASR contextual prompt format and are tokenized
for each request, before audio embeddings are decoded. The model remains loaded.
The host rejects malformed UTF-8, prompt delimiters, control characters, and
oversized vocabularies. Empty vocabulary restores the original prompt; no words
carry over from the previous request. This is decoder guidance, not WFST or
post-recognition text replacement. Run `scripts/test-funasr-nano.ps1` with
`-Variant generic` and `-Variant avx2` to test add/edit/remove and pipe validation.
