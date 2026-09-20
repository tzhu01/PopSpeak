# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.4.2] - 2026-09-20

### Added
- A PopSpeak-native result editor with manual/automatic copy, copy-on-close, persistent formatting preferences, topmost control, language display, and optional correction learning.
- Three local scene presets (daily input, chat direct input, and meeting notes) that show and apply their output, polishing, and vocabulary changes without requiring an account.
- Explicit hotword capability labels: Fun-ASR-Nano and Local Whisper use decoder prompts; other engines use local post-processing normalization.

### Changed
- Simplified vocabulary entry so a correct term is sufficient; recurring wrong forms and pronunciation-based correction now live under an optional advanced section.
- Rewrote About and portable help copy around the Windows offline-first privacy boundary and actual cloud behavior.
- Kept generated build, coverage, dependency, archive, runtime, and model artifacts out of source releases through ignore rules and release audits. A developer may still have rebuildable caches in a local working directory; they are not source-release contents.

### Verified
- 303 frontend tests and 177 native tests pass; 4 opt-in tests that require real model/audio or paid cloud credentials remain skipped by design.
- The previously built 0.4.2 Windows x64 portable snapshot passed version, manifest, private-file, fixed-model, custom-protocol, and real extraction verification. Rebuild and repeat those checks after subsequent source changes before publishing a binary.

## [0.2.0] - 2026-05-01

### Added
- **Xiaomi MiMo STT provider** — integrates with [Xiaomi Token Plan API](https://token-plan-cn.xiaomimimo.com) using the chat/completions protocol; supports `mimo-v2.5` model with base64-encoded WAV audio input
- **Custom Whisper-compatible STT provider** — configure any OpenAI Whisper-compatible endpoint with a custom base URL, API key, and model name; useful for self-hosted or third-party Whisper services
- **Xiaomi Preset button** in Settings → STT — one click to auto-fill the Token Plan endpoint and default model, reducing manual configuration errors
- Translation keys for new STT endpoint/model fields in both English and Simplified Chinese

### Fixed
- **macOS crash (SIGTRAP) on recording completion** — `Enigo` keyboard simulation APIs (`CGEventPost`, `TSMGetInputSourceProperty`) must run on the main dispatch queue; calling them from a tokio worker thread triggered a `dispatch_assert_queue` assertion crash. Fixed by:
  - Forcing keyboard output to clipboard mode on macOS (avoids direct key injection from background thread)
  - Replacing Enigo-based Cmd+C for selected-text capture with an `osascript` call on macOS (runs in a separate process context, no main-thread requirement)

## [0.1.0] - 2026-02-26

### Added
- Initial open-source release under MIT license
- Global hotkey voice recording with hold-to-record and toggle modes
- Floating capsule widget — always-on-top, draggable, with recording/transcribing/polishing states
- 6 STT providers: Deepgram Nova-3, AssemblyAI, OpenAI Whisper, Groq Whisper, GLM-ASR, SiliconFlow
- 11 LLM providers: OpenAI, DeepSeek, Zhipu, Claude, Gemini, Moonshot, Qwen, Groq, Ollama, OpenRouter, SiliconFlow
- Real-time streaming keyboard output — text appears character-by-character as the LLM generates it
- Clipboard output mode as alternative to keyboard simulation
- Selected text context — highlight text before recording to give the LLM additional context
- Translation mode — speak in one language, output in another (20+ target languages)
- Custom dictionary for domain-specific terms and proper nouns
- Per-app detection — adapts formatting based on the active application
- Local history with full-text search and date grouping
- Dark / light / system theme with smooth transitions
- Onboarding wizard for first-time setup
- System tray with quick actions (show/hide, start recording, quit)
- Auto-start on login
- Optional Cloud (Pro) subscription for managed STT/LLM quota
- BYOK (Bring Your Own Key) mode — fully functional without any cloud dependency
- Cross-platform support: Windows, macOS, Linux
- CI/CD with automated builds for all three platforms
