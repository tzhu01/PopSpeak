# Third-party notices

PopSpeak's original source code is licensed under the OSI-approved MIT License
(SPDX `MIT`). Distributed builds also contain or can download the following
independently licensed components. The PopSpeak license cannot relicense those
components or model weights. This file is an attribution summary, not a
replacement for the upstream license texts.

## FunASR llama.cpp runtime and Fun-ASR-Nano GGUF

- Runtime: https://github.com/modelscope/FunASR/tree/runtime-llamacpp-v0.1.9/runtime/llama.cpp
- Models: https://huggingface.co/FunAudioLLM/Fun-ASR-Nano-GGUF and
  https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF
- Licenses: FunASR runtime MIT; GGUF model repositories Apache License 2.0.
  Retain the runtime package's upstream dependency notices when redistributing it.
- PopSpeak automatically selects the official `windows-x64-avx2` CPU build when
  supported and falls back to generic `windows-x64`; it never uses CUDA or Vulkan.

## SenseVoiceSmall model

- Project: FunAudioLLM/SenseVoice (FunASR)
- Packaged conversion: k2-fsa/sherpa-onnx SenseVoice INT8 model
- Use: default local speech recognizer
- License: MIT; retain the upstream copyright notice and license text when
  distributing the model and software.
- Sources: https://github.com/QwenAudio/SenseVoice and
  https://github.com/k2-fsa/sherpa-onnx

## sherpa-onnx and ONNX Runtime

- sherpa-onnx: Apache License 2.0, https://github.com/k2-fsa/sherpa-onnx
- ONNX Runtime: MIT License, https://github.com/microsoft/onnxruntime

## Whisper

- whisper.cpp runtime: MIT License, https://github.com/ggml-org/whisper.cpp
- Whisper model/code: MIT License, https://github.com/openai/whisper
- OpenBLAS (in the Windows CPU runtime): BSD 3-Clause License,
  https://github.com/OpenMathLib/OpenBLAS
- SDL 2.28.5 (`SDL2.dll`): zlib License. The bundled x64 DLL has SHA-256
  `de23db1694a3c7a4a735e7ecd3d214b2023cc2267922c6c35d30c7fc7370d677`
  and matches `SDL2-2.28.5/lib/x64/SDL2.dll` from the official
  `SDL2-devel-2.28.5-VC.zip` release asset:
  https://github.com/libsdl-org/SDL/releases/download/release-2.28.5/SDL2-devel-2.28.5-VC.zip.
  The full notice is in
  `src-tauri/resources/licenses/SDL2-zlib.txt`.

## OpenCC-derived Traditional-to-Simplified table

- The non-identity mappings in PopSpeak's embedded single-character table are a
  modified/extracted subset of OpenCC `TSCharacters.txt` at commit
  `5c764a5a886f46eb365656ed91a0410d7162fac6`:
  https://github.com/BYVoid/OpenCC/blob/5c764a5a886f46eb365656ed91a0410d7162fac6/data/dictionary/TSCharacters.txt
- Additional identity rows are PopSpeak-local no-op entries and are not
  attributed to OpenCC.
- License: Apache License 2.0. The complete license is in
  `src-tauri/resources/licenses/Apache-2.0.txt`. PopSpeak's selection and file
  format are modifications; the table is not represented as an unmodified
  OpenCC distribution.

## LLVM OpenMP runtime release blocker

The current local Whisper and llama runtime directories contain Microsoft-signed
`libomp140.x86_64.dll` files. Microsoft documents that the DLL required by
`/openmp:llvm` is currently not redistributable. These files therefore must not
be included in a public installer or portable archive. Adding the upstream LLVM
license text does not grant redistribution rights to Microsoft's copy.

The release workflow intentionally fails while either DLL is present. A release
may proceed only after the dependent runtimes are rebuilt without `/openmp:llvm`,
or after PopSpeak builds LLVM OpenMP itself from a pinned source revision and
includes all license/notice material required by that build. See:
https://learn.microsoft.com/en-us/cpp/build/reference/openmp-enable-openmp-2-0-support

## Bundled optional local text polishing

- llama.cpp runtime: MIT License, https://github.com/ggml-org/llama.cpp
- Qwen2.5-0.5B-Instruct GGUF model: Apache License 2.0,
  https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF

## transcribe.cpp CPU GGUF runtime and optional models

- Runtime and Rust bindings: https://github.com/handy-computer/transcribe.cpp,
  crates `transcribe-cpp = 0.1.3` and `transcribe-cpp-sys = 0.1.3`, MIT.
  Full runtime license: `src-tauri/resources/licenses/transcribe-cpp-MIT.txt`.
  The vendored ggml component remains MIT licensed; its notice is included in
  `src-tauri/resources/licenses/transcribe-cpp-ggml-MIT.txt`.
- PopSpeak embeds the CPU-only runtime. Optional GGUF weights are independently
  licensed and downloaded only on request; they are not part of the default
  portable package. Model IDs, immutable ModelScope revisions, byte sizes and
  SHA-256 checksums are pinned in `src-tauri/src/stt/native_asr_manager.rs`.
- Qwen3-ASR 1.7B: Apache-2.0,
  https://huggingface.co/Qwen/Qwen3-ASR-1.7B.
- Cohere Transcribe 03-2026: Apache-2.0,
  https://huggingface.co/CohereLabs/cohere-transcribe-03-2026.
- Nemotron 3.5 ASR Streaming 0.6B: OpenMDW-1.1. Upstream base model revision
  `24b151a851dd15909e1fc611b11bb2da52b9fc81`; `handy-computer` GGUF conversion
  revision `0221a878b3f4c3efd14e976702058fe998d41573`.
  https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b
- Parakeet Unified EN 0.6B: NVIDIA Open Model License. Upstream base model
  revision `d4ac9928f3bf238223ff0779c06b8149bf8ac4e1`; `handy-computer` GGUF
  conversion revision `779af051aa0df6bcf91c0b17ee0ee30d37d75a1c`. English-only; do not
  describe this license as Apache-2.0, MIT or CC-BY-4.0. Required attribution:
  "Licensed by NVIDIA Corporation under the NVIDIA Open Model License".
  https://huggingface.co/nvidia/parakeet-unified-en-0.6b
- Quantizations are published by `voconly-org` on Hugging Face and `voconly`
  on ModelScope, not by the original model developers. Alternate download
  mirrors must match the pinned SHA-256 before installation. Mirror metadata
  cannot relicense the upstream weights; conflicting mirror license labels are
  not authoritative. The public binary/model-download release remains blocked
  until the full applicable model license texts are bundled and every catalog
  entry is pinned to an audited artifact. This adapter uses offline transcription
  and does not imply native streaming or decoder hotword support for all models.

## JavaScript and Rust dependency inventory

The application also incorporates the packages recorded in `package-lock.json`
and `src-tauri/Cargo.lock`. Their copyright notices and licenses remain with their
respective authors. Release maintainers should regenerate a dependency license
inventory whenever either lockfile changes.
