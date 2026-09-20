# Bundled license sources

Verified on 2026-09-20. These licenses apply to their respective components, not
to PopSpeak as a whole. Upstream copyright and license clauses are retained.
Some texts are normalized to plain UTF-8 paragraphs from official documents;
the source links below are authoritative. PopSpeak's own license is LICENSE.txt.

| Included file | Component and source |
| --- | --- |
| Apache-2.0.txt | Complete standard Apache 2.0 text, https://www.apache.org/licenses/LICENSE-2.0.txt. Also applies to the independently licensed FunAudioLLM GGUF model repositories, bundled Qwen2.5-0.5B-Instruct model, and the non-identity mappings in PopSpeak's modified/extracted OpenCC `TSCharacters.txt` subset. PopSpeak-local identity/no-op rows are not attributed to OpenCC. The OpenCC source is pinned to commit `5c764a5a886f46eb365656ed91a0410d7162fac6`: https://github.com/BYVoid/OpenCC/blob/5c764a5a886f46eb365656ed91a0410d7162fac6/data/dictionary/TSCharacters.txt. |
| FunASR-MIT.txt | Copyright (c) 2025 FunASR; https://github.com/modelscope/FunASR/blob/runtime-llamacpp-v0.1.9/LICENSE. The current resident host build uses FunASR commit fcf9a0d4a604d0859fc5927cca04df141e3b09c8 and llama.cpp commit 803b7fcae893e9caaee3921779628fef83ac0965. |
| SenseVoice-MIT.txt | Copyright (c) 2025 FunASR; https://github.com/QwenAudio/SenseVoice/blob/main/LICENSE. Identical license content to the FunASR root license. The converted INT8 files are from the sherpa-onnx SenseVoice 2024-07-17 release. |
| sherpa-onnx-Apache-2.0.txt | Unmodified full license from installed sherpa-onnx 1.13.4 Cargo package; verified against https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/LICENSE. |
| onnxruntime-MIT.txt | Copyright (c) Microsoft Corporation; https://github.com/microsoft/onnxruntime/blob/v1.27.0/LICENSE. Packaged onnxruntime.dll reports 1.27.0. |
| onnxruntime-ThirdPartyNotices.txt | Complete upstream dependency notice file from https://raw.githubusercontent.com/microsoft/onnxruntime/v1.27.0/ThirdPartyNotices.txt, not an excerpt. It describes upstream optional components as well as default dependencies; inclusion does not assert that every optional provider is shipped. |
| whisper-cpp-MIT.txt | Copyright (c) 2023-2024 The ggml authors; pinned runtime v1.7.6, https://github.com/ggml-org/whisper.cpp/blob/v1.7.6/LICENSE. |
| Whisper-MIT.txt | Copyright (c) 2022 OpenAI; model and original code, https://github.com/openai/whisper/blob/main/LICENSE. |
| OpenBLAS-BSD-3-Clause.txt | Copyright (c) 2011-2014 The OpenBLAS Project; https://github.com/OpenMathLib/OpenBLAS/blob/v0.3.29/LICENSE. Complete BSD-3-Clause text. The exact OpenBLAS build version embedded in the third-party Windows runtime was not independently established; no version certification is implied by the license source tag. |
| SDL2-zlib.txt | Unmodified full zlib notice for SDL 2.28.5, copyright 1997-2023 Sam Lantinga; https://github.com/libsdl-org/SDL/blob/release-2.28.5/LICENSE.txt. The bundled `SDL2.dll` matches `SDL2-2.28.5/lib/x64/SDL2.dll` from the official asset https://github.com/libsdl-org/SDL/releases/download/release-2.28.5/SDL2-devel-2.28.5-VC.zip and has SHA-256 `de23db1694a3c7a4a735e7ecd3d214b2023cc2267922c6c35d30c7fc7370d677`. |
| llama-cpp-MIT.txt | Copyright (c) 2023-2026 The ggml authors; https://github.com/ggml-org/llama.cpp/blob/b9959/LICENSE for bundled polisher runtime; same copyright/license at resident FunASR host dependency commit 803b7fcae893e9caaee3921779628fef83ac0965. |
| transcribe-cpp-MIT.txt | Copyright (c) 2026 The transcribe.cpp authors; installed transcribe-cpp-sys 0.1.3 LICENSE, https://github.com/handy-computer/transcribe.cpp. |
| transcribe-cpp-ggml-MIT.txt | Copyright (c) 2023-2026 The ggml authors; installed transcribe-cpp-sys 0.1.3 ggml/LICENSE. |
| transcribe-cpp-miniz-MIT.txt | Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC. Installed transcribe-cpp-sys 0.1.3 src/third_party/miniz/LICENSE. |
| Microsoft-Visual-Cpp-Runtime-License.txt | Full paragraph text from official Microsoft Visual C++ 2015-2022 Runtime license, https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/. EULA ID Cpp_2015-2022_ENU.1033. Original DOCX SHA-256 f1e3d56ceb2ad68aae0711b910375009e651ac5530fa0760f0dea6e81e54fae1. |
| Microsoft-Visual-Studio-Build-Tools-License.txt | Full paragraph text from official Microsoft Visual Studio 2022 Diagnostic/Build Tools license, March 2024, https://visualstudio.microsoft.com/license-terms/vs2022-ga-diagnosticbuildtools/. This is separate from the end-user CRT license. |

## Visual C++ redistribution

The packaging script only copies unmodified, valid Microsoft-signed DLLs from
the official Visual Studio `VC/Redist/MSVC/<version>/x64/Microsoft.VC143.CRT`
directory. No DLL is harvested from Windows/System32, and debug_nonredist is
never used. The 2026-09-18 build machine provides ten DLLs, version 14.44.35211.0,
under the 14.44.35112 Redist directory. All are copied beside PopSpeak.exe.

Microsoft's developer redistribution rights are separate from the end-user
runtime license: https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution.
In particular, the Visual C++ Runtime Files section covers unmodified files
under VisualStudioFolder/VC/redist and excludes debug_nonredist. Distribution
remains subject to the publisher holding the applicable Visual Studio license
and complying with its terms; merely bundling these texts does not grant a new
license or prove the publisher's entitlement. These Microsoft runtime files are
not relabeled as MIT or Apache and are not part of PopSpeak's own source license.

## Excluded LLVM OpenMP binaries

`libomp140.x86_64.dll` is not covered by the Visual C++ redistribution statement
above. Microsoft documents `/openmp:llvm` as unavailable for production use
because the required `libomp` DLLs are currently not redistributable:
https://learn.microsoft.com/en-us/cpp/build/reference/openmp-enable-openmp-2-0-support.
Microsoft also instructs publishers to distribute only files named by the
applicable Redist list:
https://learn.microsoft.com/en-us/cpp/windows/determining-which-dlls-to-redistribute.

The copies currently present in the local Whisper and llama runtime directories
are development inputs only and must not be shipped. Their SHA-256 is
`4a20c1e5c115c29771a12324513eb109badac72180f79481527ad79d996ffb33`.
`scripts/assert-redistributable-runtime.ps1` blocks a release while any file
named `libomp140.x86_64.dll` remains under the runtime tree. The blocker must be
resolved by rebuilding without `/openmp:llvm`, or by compiling LLVM OpenMP from
a pinned source revision and completing a separate license/notice review. A
copy of LLVM's source license alone is not evidence that Microsoft's binary may
be redistributed.

## Scope

Optional models retain their own upstream terms described in
THIRD_PARTY_NOTICES.md. This directory improves the distributed runtime/model
notices; it is not a legal opinion or a claim that every transitive npm/Cargo
dependency has undergone a complete legal audit. Regenerate the dependency
notice inventory and recheck upstream terms when lockfiles or runtimes change.
