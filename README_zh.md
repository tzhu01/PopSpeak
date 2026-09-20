# PopSpeak

[English](README.md)

[产品官网](https://tzhu01.github.io/PopSpeak/) · [问题反馈](https://github.com/tzhu01/PopSpeak/issues)

当前版本：**0.4.2**。免注册试用累计 200 次或 20 分钟（任一先到），升级保留已有用量与激活状态；激活后开放全部本地功能。云服务由用户自备厂商账号，费用另计。详见[试用升级规则](docs/TRIAL_QUOTA_200_20_MINUTES_ZH.md)与[当前 0.4.2 PRD 入口](docs/PRD_ZH.md)。公众号入口尚待运营方配置，不内置后台登录链接或 Cookie。

PopSpeak 是一款面向 Windows 的开源语音输入助手：按住全局快捷键说话时，
独立 POP 结果窗可先显示本地近似文字；结束后，再由所选模型生成最终稿。
你可以先校对、排版和复制，也可以选择直接输出到原应用。默认链路全程在本机
CPU 上运行，不需要账号、API Key、独立显卡或网络。

## 产品原则

- **先录音，再准备模型：** 麦克风优先启动，模型初始化期间的声音进入预录缓冲，
  避免快捷键按下后的开头丢字。
- **中文优先：** 默认使用 SenseVoice INT8；可切换 Fun-ASR-Nano GGUF 中文准确模式，
  Whisper tiny 作为本地兜底。
- **只用 CPU：** SenseVoice、Fun-ASR、Whisper 和可选的本地 Qwen 润色均不使用 GPU 推理。
- **隐私默认：** 录音识别、专业词汇、纠错和历史记录都留在电脑中；云服务默认关闭。
- **越用越懂你：** 修改识别结果后，可把“曾识别成 → 正确术语”直接保存到
  「专业词汇」，后续自动纠正。

## 当前支持范围

- Windows 10/11 x64
- 无需 CUDA、DirectML 或独立显卡
- 默认快捷键：按住 `Ctrl+/` 说话，松开完成
- 默认输出：剪贴板粘贴，并在粘贴后恢复用户原剪贴板

当前明确只发布 Windows 版。在其他平台的全局输入、权限和运行库没有完成真实
验证前，不对外宣称跨平台。

## 已实现能力

- SenseVoice INT8 进程级缓存与启动预热，避免每次录音重复约 1.7 秒初始化
- Whisper tiny 离线兜底，可选下载 base 模型
- Fun-ASR-Nano encoder F16 + Qwen3 Q4_K_M 按需下载；自动选择 AVX2 或通用 x64 CPU 常驻进程
- 麦克风选择、连续 16 kHz 重采样、噪声门和带预录的 VAD
- 与最终模型解耦的本地实时预览：100 ms PCM 辅路刷新近似文字，所选模型独立生成最终结果
- 热词与专业词：Fun-ASR-Nano / Local Whisper 支持解码提示，其他引擎支持本地后处理规范化；固定错词与拼音纠错为可选高级项
- 无需登录的本地场景：日常输入、聊天直输和会议记录可一键应用
- 编辑器中的本地纠错飞轮
- 按住/切换录音、最长录音保护、可见错误提示
- 当前窗口变化检测；不确定时只复制，避免把文字打进错误窗口
- 本地 SQLite 历史、搜索、崩溃前待输出文本恢复
- 可选 Qwen 0.5B 本地润色，使用隔离的 llama.cpp CPU 运行时
- 豆包及新增自定义厂商凭证明文保存在本机配置，请勿分享私人 settings.json

## 隐私与联网边界

默认配置下，语音识别不会发出网络请求。只有用户主动下载可选模型、手动检查更新，
或主动配置云端/BYOK 服务时才会联网。本地 AI 润色默认关闭，让普通 CPU 上最重要的
“按下就说、松开出字”保持足够快。

## 开发与验证

需要 Node.js 20+、稳定版 Rust，以及勾选「使用 C++ 的桌面开发」和 Windows SDK 的
Visual Studio 2022 Build Tools。

```powershell
npm ci
./scripts/fetch-whisper.ps1
./scripts/fetch-llama-server.ps1
./scripts/fetch-sensevoice.ps1
./scripts/fetch-funasr-nano.ps1
npm run tauri dev
```

四个准备脚本使用固定上游版本并校验 SHA-256。模型、原生运行时和构建产物不会提交
到 Git。

完整质量门禁：

```powershell
npm test
npm run lint
npm run format:check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

打包说明见 [BUILD_WINDOWS.md](BUILD_WINDOWS.md)，目录和产品链路见
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)，竞品、模型路线与上市指标见
[docs/PRODUCT_STRATEGY_ZH.md](docs/PRODUCT_STRATEGY_ZH.md)，面向新用户的产品需求和完整工作流见
[docs/PRD_ZH.md](docs/PRD_ZH.md)。

## 发布说明

发布工作流会生成 Windows NSIS 草稿 Release 和带 SHA-256 清单的便携 ZIP。正式公开
版本强制要求 Authenticode 代码签名：缺少 CI 证书或任一程序签名无效时任务直接失败；
证书私钥只放 GitHub Secrets，不进入仓库。

模型与第三方运行库遵循各自许可证，详见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
本地数据和联网边界见 [PRIVACY.md](PRIVACY.md)。公开二进制前必须完成
[开源发布清单](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md)并关闭其中列出的阻断项。
只读的[源码快照审计](docs/SOURCE_RELEASE_AUDIT.md)会检查已跟踪及未跟踪、未忽略文件中的
常见误提交内容，但不能替代完整的 Git 历史扫描。

## 许可证

PopSpeak 自有源码采用 [MIT License](LICENSE)，SPDX 标识符为
[`MIT`](https://spdx.org/licenses/MIT.html)。MIT 是
[OSI 批准的开源许可证](https://opensource.org/license/mit)：在保留版权与许可
声明的前提下，允许使用、复制、修改、分发、再许可和销售，包括商业使用。

第三方代码、原生运行库和模型权重仍遵循各自许可证，详见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。源代码许可与 PopSpeak 名称、
Logo 及官方签名二进制的品牌身份相互独立。MIT 允许 fork 和自行构建，
但不应把第三方版本表述为 PopSpeak 官方发行版；详见
[许可与品牌边界](docs/LICENSING.md) 及 [TRADEMARKS.md](TRADEMARKS.md)。
文中对 OSI 批准的描述仅针对许可证，不表示获得 OSI 背书。
