# PopSpeak

> Speak first. Review before it lands. PopSpeak is an open-source, offline-first voice input assistant for Windows.

[Website](https://tzhu01.github.io/PopSpeak/) · [Build from source](BUILD_WINDOWS.md) · [Report an issue](https://github.com/tzhu01/PopSpeak/issues) · [简体中文](README.md)

**Check the actual assets and signing status on the [Releases page](https://github.com/tzhu01/PopSpeak/releases).** A "source only" release is not a Windows installer, and an unsigned prerelease installer is not an officially signed build. Older archives are not substitutes for this build. See the [release checklist](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md).

![PopSpeak's POP text-preview window while recording](site/assets/popspeak-preview.png)

*An actual POP-window recording preview. It can be revised while you speak; the selected recognizer supplies the final transcript afterward.*

## What makes it different

- **Review before insertion.** The independent POP result window lets you edit, format, copy, and pin text before using it in another app.
- **A preview regardless of the final model.** A local side lane revises approximate text while you speak; the selected offline or cloud recognizer produces the final result after recording. `100 ms` is the audio handoff granularity, not a promise of first-text or final-result latency, and not native streaming support in every model.
- **Offline first.** Default recognition runs on the local CPU. Audio, vocabulary, and history stay on the device; cloud recognition requires an explicit opt-in.
- **Honest vocabulary support.** Depending on the engine, terms are passed as decoder hints or applied as local post-recognition normalization; the app labels which path is active.
- **Reusable history.** Search, copy, edit, and delete past results. Language and dialect quality depend on the chosen model and recording conditions.

## Try it

1. On **Windows 10/11 x64**, check [Releases](https://github.com/tzhu01/PopSpeak/releases) for an installer or use the [source-build guide](BUILD_WINDOWS.md). The source build downloads pinned model/runtime assets; normal local recognition does not require a GPU.
2. Hold `Ctrl+/` to speak. Review the approximate preview, then release the shortcut for the final text.
3. Edit and copy the result, or choose direct output to the original app. Account-free local trial use is limited to **200 recordings or 20 minutes**, whichever comes first; continuing and advanced local features require activation.

The first lean Windows installer bundles **SenseVoice Small only**; Whisper and Fun-ASR-Nano runtimes are absent. Downloads for the four native GGUF models (Qwen3-ASR, Cohere, Nemotron, and Parakeet) are **disabled in this release**. Their adapter source remains for a future release after the applicable model licenses and catalog have been audited. Optional Doubao and custom cloud providers require your own credentials and may incur provider charges. See [model selection](docs/MODEL_SELECTION_ZH.md) and [privacy](PRIVACY.md) for details.

## Open source

PopSpeak's original source is [MIT licensed](LICENSE), including commercial use. Third-party models, runtimes, and the PopSpeak brand have separate terms; see [notices](THIRD_PARTY_NOTICES.md) and [licensing boundaries](docs/LICENSING.md). Contributions are welcome via [Issues](https://github.com/tzhu01/PopSpeak/issues) and [pull requests](CONTRIBUTING.md). The [branch and release policy](docs/RELEASE_AND_BRANCHES_ZH.md) keeps `main` as the only long-lived branch; GitHub Pages deploys directly from it.
