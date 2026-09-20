# 0.4.1 模型源验证

## 新增模型与运行方式

Qwen3-ASR 1.7B、Cohere Transcribe 03-2026、Nemotron 3.5 ASR Streaming 0.6B、
Parakeet Unified EN 0.6B 使用本地 CPU 原生 GGUF 后端，按需下载；不将几 GB 新权重
塞入默认压缩包。原有 SenseVoice / Fun-ASR-Nano / Whisper / Qwen2.5 润色保留。

Parakeet 是英语专用；本版新后端使用整段识别，不将模型原生架构中的 streaming
能力等同于本应用已经实现的流式体验。模型卡不展示未经同机测试的准确率分数或延迟。

Cohere 当前 native 后端不自动检测语言：应用明确以中文作为默认，可另选其支持语言，
不会把空语言默默交给底层的英语默认。Parakeet 选择后设为英语。

## 测速方法与结果（2026-09-18，本机网络）

执行 `node scripts/benchmark-model-sources.mjs`，限定四个新模型，每源两轮顺序 Range GET，
每轮 2 MiB，10 秒上限，包含连接耗时，无环境代理配置。返回 206 并检查 GGUF 文件头。
结果只代表当时本机到这些候选源的片段传输，**不是全国最快或全文件速度保证**。

| 模型 | ModelScope 固定版本，两轮 MiB/s | HF Mirror 两轮 MiB/s | Hugging Face |
| --- | --- | --- | --- |
| Qwen3-ASR 1.7B | 0.923 / 0.887 | 首轮超时（已收约 0.35 MiB）/ 0.507 | 两轮连接超时 |
| Cohere Transcribe | 0.887 / 0.915 | 0.354 / 0.590 | 两轮连接超时 |
| Nemotron 3.5 | 0.905 / 0.914 | 0.515 / 0.546 | 两轮连接超时 |
| Parakeet Unified EN | 0.900 / 0.936 | 0.286 / 0.557 | 两轮连接超时 |

另测 ModelScope `www` 入口，四文件 1 MiB 均 HTTP 206，耗时分别 1.266、1.236、
1.924、1.945 秒。它与主源属于**同一平台**，不是独立灾备。

固定顺序：ModelScope 版本地址 → HF Mirror → ModelScope www 备用入口 → HF 官方。
提供至少两个备用 URL；前两个备用入口本次均收到模型字节，但不保证长期可达。
HF 官方保留为全球回源，明确本机测试失败，不标为“国内已验证可用”。

## 固定文件校验

| 模型 | 固定 ModelScope revision | SHA-256 |
| --- | --- | --- |
| Qwen3-ASR 1.7B Q5_K_M | d7aa4b50af3b672e3a5a2782953a823a9332e5b7 | 034c557fe92ff8fcd9a9c041cbdaad347be0a86a58d3a348f63cf3f0180879d0 |
| Cohere Q5_K_M | 0452067461a8df51e2245dd81f0122739caf424f | 14d02f1ad6dd77b3a60f82639879012c3adb4fe25c50a5a47a2c4c661daf1558 |
| Nemotron Q8_0 | 85c784fe0a42833abb5bd9e44c43980a0db46fe8 | b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c |
| Parakeet Q8_0 | 598c2267a9bae5e6daf3c3237a44d272d11b7880 | 4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795 |

源码中的 native_asr_manager.rs 是下载清单的唯一运行时来源。失败、取消或校验不符
不能覆盖当前已安装模型；下载完成后提示保存应用。模型新版通过更新应用里的固定清单
发布，不追随未经验证的远端 latest。

## 来源

- [Qwen3-ASR 官方代码](https://github.com/QwenLM/Qwen3-ASR)
- [Voconly 的公开实现参考](https://github.com/xinkyle/Voconly)
- [Qwen GGUF 文件来源](https://modelscope.cn/models/voconly/Qwen3-ASR-1.7B-gguf)
- [Cohere GGUF 文件来源](https://modelscope.cn/models/voconly/cohere-transcribe-03-2026-gguf)
- [Nemotron GGUF 文件来源](https://modelscope.cn/models/voconly/nemotron-3.5-asr-streaming-0.6b-gguf)
- [Parakeet GGUF 文件来源](https://modelscope.cn/models/voconly/parakeet-unified-en-0.6b-gguf)

原始本机探测日志在 `.model-validation/source-benchmark/results.json`；不随便携包分发。
