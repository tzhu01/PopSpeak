# PopSpeak 模型选型与当前可用状态

## 先说结论

当前公开包默认使用 **SenseVoice Small INT8 + 本地专业词汇纠错**。Fun-ASR-Nano GGUF
已经作为可切换的“中文准确模式”接入；Whisper tiny/base 是兼容回退；
Qwen2.5-0.5B-Instruct 只做可选文字润色，默认关闭。

## 当前完整离线包里有什么

| 模型 | 角色 | 当前状态 | 默认策略 |
| --- | --- | --- | --- |
| SenseVoice Small INT8 | 音频 → 文字 | 已接入、已随包 | 默认 ASR |
| Fun-ASR-Nano GGUF Q5_K_M | 音频 → 文字 | 已接入、已随包 | 中文准确模式 |
| Whisper tiny | 音频 → 文字 | 已接入、已随包 | 极速回退 |
| Whisper base | 音频 → 文字 | 已接入、已随包 | 准确回退 |
| Qwen2.5-0.5B-Instruct Q4_K_M | 文字 → 润色文字 | 已接入、已随包 | 默认关闭 |
| Qwen3-ASR-0.6B | 音频 → 文字 | 尚未接入 | 方言候选 |

包内 `models/catalog.json` 是机器可读清单，记录模型文件、角色、量化、CPU 参数、大小和
SHA-256。应用自动扫描相对目录，设置页不要求用户选择模型文件夹。

## 三个容易混淆的 Qwen/Fun-ASR 名称

### Qwen2.5-0.5B-Instruct

这是因果语言模型，输入已经是文字。它可以补标点、整理口语或按场景改写，但不能读取
麦克风音频，也不会提高声学识别能力。生成式润色还有改错专有名词或改变原意的风险，
所以专业词应优先使用确定性的本地词典映射。

### Fun-ASR-Nano GGUF

这是 ASR。官方 llama.cpp/GGUF 路径把 SenseVoice SAN-M 编码器、适配器和 Qwen3-0.6B
解码器放进纯 C++ CPU 管线，无需 Python/CUDA。本包采用 encoder F16 + Q5_K_M（官方量化
基准中 CER 最低），权重合计约 1.02 GB，并自动在 AVX2 与通用 x64 CPU 运行时之间选择。它更接近“复杂中文、
上下文和专业场景的准确模式”，代价是包更大、每次批处理需要重新加载模型，因此不作为默认极速模式。

### Qwen3-ASR-0.6B

这也是 ASR，官方说明覆盖 30 种语言和 22 类中文方言，支持离线/流式。当前官方易用
路径仍以 qwen-asr 的 Transformers/vLLM 为主。对 PopSpeak 来说，只有形成可审计、可签名、
无 Python、无 GPU、普通 Windows CPU 达标的原生量化运行时后，才适合进入产品。

## 下一阶段决策门禁

继续在 4 核/8GB、6 核/16GB、主流笔记本与新款桌面 CPU 上测：总识别延迟、实时率、
峰值内存、20 组中文专业词和方言集。正式推广前仍需用固定测试集证明它相对 SenseVoice
的准确率收益足以抵消约 1.02 GB 体积和更高延迟。

Qwen3-ASR 不与 Fun-ASR-Nano 同期全量打包；先用同一测试集胜出后再决定，避免让普通用户
下载数 GB 的重复能力。

官方资料：

- [Fun-ASR-Nano llama.cpp/GGUF CPU 运行时](https://github.com/modelscope/FunASR/blob/main/runtime/llama.cpp/fun-asr-nano/README.md)
- [Qwen3-ASR 官方仓库](https://github.com/QwenLM/Qwen3-ASR)
- [Qwen2.5-0.5B-Instruct GGUF 模型卡](https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF)
- [SenseVoice 官方仓库](https://github.com/QwenAudio/SenseVoice)
