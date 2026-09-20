# PopSpeak 架构与产品链路

## 产品链路

```text
全局快捷键按下
  → 立即启动麦克风；主队列覆盖本次有上限的完整录音（最高 10 分钟）
  → 16 kHz 单声道重采样、噪声门、带 300 ms 预录的 VAD
  → 同一份 PCM 分流到本地近似预览与所选主识别器
  → 主识别器给出权威的完整识别结果
  → 专业词汇/已学习错词纠正
  → 可选本地或云端润色
  → 焦点安全检查
  → 粘贴、只复制或编辑器确认
  → 写入本地历史并清除崩溃恢复日志
```

最重要的时序约束是“采集先于模型初始化”。SenseVoice 识别器按语言、线程数和模型路径
做进程级缓存，并在应用启动/设置变化后预热；即使冷启动，初始化期间的音频也不会丢失。

## 双路录音预览

开启 POP 录音预览时，采集层会把同一份 16 kHz 单声道 PCM 同时送入两条互不阻塞的链路：

- **本地近似预览链路**：采集层每 100 ms 发送一个传输块，独立的本地 SenseVoice
  预览识别器再按滚动语音上下文聚合解码。100 ms 是传输粒度，并不表示把每个 100 ms
  片段单独识别。预览队列有界；预览处理过慢时允许丢弃预览块，不能反压或中断主识别。
- **权威主识别链路**：用户选择的本地或云端模型照常处理主音频流。结束录音后，主模型的
  完整结果会替换近似预览，并继续进入词典纠正、可选润色、输出和历史记录链路。

预览识别器使用独立的模型缓存命名空间，不能占用主 SenseVoice 识别器的互斥锁。预览文字
只用于显示，不写入历史、积分或最终输出。后端的 `pipeline:state`、`stt:preview`、
`stt:partial`、`stt:final`、`llm:chunk` 与 `pipeline:resolved` 事件携带录音
`session_id`；状态与文本事件另外携带单调递增的 `revision`。前端仅接受当前会话中更新的修订，
并在收到主识别结果后拒绝迟到的预览，从而避免取消、快速重录或异步任务导致文字串线。
会话级 `pipeline:error` / `pipeline:notice` 同样携带 `session_id`，并使用独立递增的消息
`revision`；前端会丢弃旧会话或旧修订的提示。只有不再从属于某段录音的全局通知可以继续
使用字符串载荷。

## 根目录

- `src/`：React/TypeScript 界面。
- `src-tauri/`：Rust/Tauri 主程序、Windows 权限与打包配置。
- `scripts/`：经过哈希校验的模型/运行时准备和便携包脚本。
- `.github/workflows/`：Windows CI 与草稿 Release。
- `docs/`：架构、产品、发布与运营资料。

## 前端 `src/`

- `App.tsx`：按窗口 hash 分发主界面、胶囊和编辑器；启动时加载配置、历史和词典。
- `components/Onboarding/`：离线模式选择、真实设备/模型就绪检查和快捷键实测。
- `components/Capsule/`：录音、识别、润色、完成、错误等轻量状态反馈。
- `components/Editor/`：输出前编辑；比较原文与修改结果并生成纠错候选。
- `components/Settings/GeneralPane.tsx`：快捷键、录音模式、麦克风、VAD、最长时长和输出方式。
- `components/Settings/SttPane.tsx`：SenseVoice/Fun-ASR/Whisper 及可选提供商配置和状态检查。
- `components/Settings/LlmPane.tsx`：默认关闭的润色配置、本地模型下载和生命周期控制。
- `components/Settings/DictionaryPane.tsx`：专业词汇、发音和“曾识别成”错词映射。
- `components/History/`：本地历史搜索、复制和再次编辑。
- `hooks/useTauriEvents.ts`：把 Rust 事件同步为 UI 状态和可见提示。
- `stores/appStore.ts`：Zustand 单一前端状态源及 CPU/offline 默认配置。
- `lib/tauri.ts`：所有 Tauri command 的类型安全包装。
- `lib/correction.ts`：从用户编辑中提取安全、尽量小的纠错跨度。

## 后端 `src-tauri/src/`

- `lib.rs`：Tauri 初始化、托盘/窗口、全局快捷键、命令注册、单实例和开机恢复。
- `pipeline.rs`：录音到输出的核心状态机；并发任务、超时、错误传播和历史落库都在这里。
- `audio/capture.rs`：设备枚举、F32/I16/U16 输入、连续重采样、主链路 20 ms 音频块、
  可选预览链路 100 ms 传输块和启动确认。
- `audio/preprocess.rs`：高通/噪声门、VAD、300 ms pre-roll 与静音拒识。
- `stt/sensevoice.rs`：sherpa-onnx SenseVoice CPU 识别、进程缓存和批量断开转写。
- `stt/sensevoice_manager.rs`：模型路径、哈希校验、下载、进度和安全解包。
- `stt/local_whisper.rs` / `stt/model_manager.rs`：`--no-gpu` Whisper CLI 与本地模型管理。
- `stt/funasr_nano.rs` / `stt/funasr_manager.rs` / `stt/funasr_runtime.rs`：Fun-ASR-Nano GGUF 按需模型管理；原生进程常驻，PCM16 通过 Windows 命名管道逐段送入
  识别、AVX2/通用运行时自动选择及相对模型索引。
- `stt/hotword_replacer.rs`：错词映射、大小写/空格恢复和显式同音词规则。
- `stt/*cloud*`：可选网络识别提供商；默认产品链路不会实例化它们。
- `llm/local_server.rs`：隔离 llama.cpp CPU 进程、模型下载、健康检查和退出回收。
- `llm/prompt.rs`：可选文本润色提示词、词典与场景上下文，含输入隔离规则。
- `output/clipboard.rs`：文本/图片剪贴板快照、粘贴后条件恢复。
- `output/keyboard.rs`：键盘注入；失败必须返回错误，不做静默伪成功。
- `output/mod.rs`：应用焦点复核与 copy-only 安全降级。
- `storage/mod.rs`：SQLite 历史、专业词汇及 `correction_from` 迁移。
- `storage/credentials.rs`：Windows Credential Manager 密钥存取及旧明文迁移。
- `app_detector/`：只在本机根据进程名/窗口标题判断邮件、聊天、文档等场景。
- `integrity.rs`：SHA-256 文件校验。

## 原生资源布局

- `src-tauri/binaries/` 只放主进程加载的 sherpa-onnx/ONNX Runtime DLL。
- `src-tauri/resources/runtimes/whisper/` 放 Whisper 及其 DLL。
- `src-tauri/resources/runtimes/funasr/` 只随包携带 Fun-ASR-Nano 的通用与 AVX2 常驻管道宿主；约 911 MiB 模型由用户按需下载。
- `src-tauri/resources/runtimes/llama/` 放 llama.cpp 及其 DLL。
- 构建源中的 `src-tauri/resources/sensevoice/` 与 `resources/models/` 放离线模型；
  安装包和便携 ZIP 统一映射为 `models/sensevoice`、`models/whisper`、`models/llm`。
- `models/catalog.json` 记录每个随包模型的角色、量化、CPU 参数、文件大小和 SHA-256；
  运行时优先自动索引 `PopSpeak.exe` 同级相对目录，不要求用户配置路径。

Whisper 与 llama.cpp 的 `ggml*.dll` 名称重叠，必须保持目录隔离。上述生成资源均被 Git
忽略，由准备脚本和 CI 重建。

## 本地数据

- 配置：Tauri Store；默认 SenseVoice、CPU、本地、润色关闭。
- 历史/专业词汇：应用本地数据目录中的 `popspeak.db`。
- 凭证：通用 STT/LLM API-key 字段使用 Windows Credential Manager；专用 SeedASR
  凭证和自定义云厂商凭证目前会以明文写入本机 `settings.json`，不得分享该文件。
- 待输出恢复：原子写入的临时 JSON；成功写历史后删除。

完整的数据流、凭证边界与删除说明见 [`PRIVACY.md`](../PRIVACY.md)。

启动时会从旧标识 `com.opentypeless.app` / `com.popspeak.app` 迁移配置、模型、
待恢复文本和 SQLite；数据库使用 SQLite 在线备份 API，WAL 中的数据也会保留，且永不覆盖
新目录中已存在的用户文件。
