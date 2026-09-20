# PopSpeak

[中文说明](README_zh.md)

[Product website](https://tzhu01.github.io/PopSpeak/) · [Issue tracker](https://github.com/tzhu01/PopSpeak/issues)

Current release: **0.4.2**. The offline trial includes 200 recordings or 20 minutes total, whichever is reached first. Existing usage and installation-bound activation are preserved when upgrading. See [trial migration rules](docs/TRIAL_QUOTA_200_20_MINUTES_ZH.md), the [current 0.4.2 PRD entry point](docs/PRD_ZH.md), and [cloud verification boundaries](docs/CLOUD_STT_INTEGRATION_ZH.md). The consumer package contains only an activation public key; keep the operator signing key outside this repository.

PopSpeak is an open-source voice input assistant for Windows. While you hold the
global hotkey, an independent POP result window can show local approximate text;
after release, the selected engine produces the final transcript. Review, format,
and copy it first—or send it directly to the app you were using. The default path
runs entirely on the local CPU—no account, API key, GPU, or network connection is
required.

## Product principles

- **Immediate capture:** microphone recording starts before the recognizer is
  prepared, and a pre-roll buffer keeps the first words.
- **Chinese first:** SenseVoice INT8 is the default engine; Fun-ASR-Nano GGUF is
  available as a higher-accuracy Chinese mode. Whisper tiny is the local fallback.
- **CPU only:** SenseVoice, Fun-ASR, Whisper and the optional local Qwen polisher are all
  explicitly launched without GPU layers.
- **Private by default:** transcription, professional vocabulary, corrections and
  history stay on the device. Cloud providers are optional and disabled by default.
- **Vocabulary that matches engine capability:** Fun-ASR-Nano and Local Whisper
  receive decoder hints; other recognizers apply local spelling normalization. A
  recurring wrong form or pronunciation is an optional, separate correction rule.

## Current platform

- Windows 10/11 x64
- CPU inference; no CUDA, DirectML or discrete GPU required
- Default hotkey: hold `Ctrl+/`, speak, then release
- Default output: clipboard paste with clipboard restoration

The project is intentionally Windows-only today. Other platforms are not claimed or
packaged until their global input, permissions and runtime behavior are tested.

## Core features

- SenseVoice INT8 recognizer cached and pre-warmed once per process
- Whisper tiny offline fallback and optional base-model upgrade
- Fun-ASR-Nano encoder F16 + Qwen3 Q4_K_M with automatic AVX2/generic x64 selection
- Microphone selection, continuous 16 kHz resampling, noise gate and VAD pre-roll
- Provider-independent local live preview: a non-blocking 100 ms PCM side lane
  refreshes approximate text while the selected engine produces the final result
- Professional vocabulary with decoder/post-processing capability labels, plus
  optional exact-alias and same-pinyin correction
- A result editor with copy automation, persistent layout options, topmost control,
  language display, history saving, and correction learning
- Account-free local scene presets for daily typing, chat, and meeting notes
- Hold/toggle recording modes, maximum-recording guard and visible error states
- Safe output to arbitrary apps, with focus-change detection and copy-only fallback
- Local SQLite history, searchable transcript recovery and crash journal
- Optional local Qwen 0.5B polishing through an isolated CPU llama.cpp runtime
- Dedicated cloud credentials and new custom-vendor credentials persist in local settings in plaintext; do not share personal settings files

## Privacy and networking

With the default configuration, voice recognition never makes a network request.
Network access occurs only when the user explicitly downloads an optional model,
checks for an update, or configures a cloud/BYOK provider. Local AI polishing is off
by default so the primary voice-keyboard path stays fast on ordinary CPUs.

## Development

Prerequisites: Node.js 20+, stable Rust, and Visual Studio 2022 Build Tools with
Desktop development with C++ and a Windows SDK.

```powershell
npm ci
./scripts/fetch-whisper.ps1
./scripts/fetch-llama-server.ps1
./scripts/fetch-sensevoice.ps1
./scripts/fetch-funasr-nano.ps1
npm run tauri dev
```

The preparation scripts use pinned upstream releases and verify SHA-256 digests.
Generated models, native runtimes and build output are intentionally ignored by Git.

Quality gate:

```powershell
npm test
npm run lint
npm run format:check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

See [BUILD_WINDOWS.md](BUILD_WINDOWS.md) for installer/portable packaging and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the directory map and product flow.
The Chinese product/market plan is in
[docs/PRODUCT_STRATEGY_ZH.md](docs/PRODUCT_STRATEGY_ZH.md).

## Distribution notes

The release workflow creates a draft Windows NSIS release and a SHA-256-manifested
portable ZIP. Authenticode is a hard public-release gate: missing CI secrets or any
invalid executable signature fails the job. Private keys are never committed.

Third-party runtimes and models have their own licenses and attribution requirements.
See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
Privacy/storage behavior is documented in [PRIVACY.md](PRIVACY.md). Maintainers
must complete the [open-source release checklist](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md)
and resolve its linked blockers before publishing a binary. The read-only
[source snapshot audit](docs/SOURCE_RELEASE_AUDIT.md) checks common accidental
artifacts in tracked and untracked non-ignored files, but it does not replace a
complete Git-history scan.

## License

PopSpeak's original source code is released under the
[MIT License](LICENSE), SPDX identifier [`MIT`](https://spdx.org/licenses/MIT.html).
MIT is an [OSI-approved open-source license](https://opensource.org/license/mit):
subject to retaining its copyright and permission notice, it permits use, copy,
modification, distribution, sublicensing and sale, including commercial use.

Third-party code, native runtimes and model weights keep their own licenses; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The source-code license is
separate from PopSpeak names, logos and official signed binaries. A fork or
self-built package is allowed by MIT, but must not be represented as an official
PopSpeak release; see [the licensing boundary](docs/LICENSING.md) and
[TRADEMARKS.md](TRADEMARKS.md). References to OSI approval describe the license
only and do not imply endorsement by OSI.
