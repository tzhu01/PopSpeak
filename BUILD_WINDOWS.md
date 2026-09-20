# Windows 构建与发版

## 环境

- Windows 10/11 x64
- Node.js 20+
- stable Rust（MSVC toolchain）
- Visual Studio 2022 Build Tools：使用 C++ 的桌面开发 + Windows SDK
- CMake（Visual Studio 的 C++ CMake tools 也可）。新增 GGUF ASR 后端静态编译，
  不需要用户安装 Python/CMake。构建前将 CMake 的 bin 目录加入当前进程 PATH。

```powershell
node --version
npm --version
rustc --version
cargo --version
```

## 准备依赖

在仓库根目录运行：

```powershell
npm ci
./scripts/fetch-whisper.ps1
./scripts/fetch-llama-server.ps1
./scripts/fetch-sensevoice.ps1
./scripts/fetch-funasr-nano.ps1
./scripts/prepare-nsis.ps1
```

这些脚本会准备并校验：

- `resources/runtimes/whisper/`：Whisper CPU 程序和专属 DLL
- `resources/runtimes/funasr/`：Fun-ASR-Nano 通用/AVX2 静态 CPU 程序
- `resources/runtimes/llama/`：llama.cpp CPU 程序和专属 DLL
- `resources/models/ggml-tiny.bin` 与 `ggml-base.bin`：离线 Whisper tiny/base
- `resources/models/qwen2.5-0.5b-instruct-q4_k_m.gguf`：离线文字润色（不是 ASR）
- `resources/models/funasr-nano/`：Fun-ASR-Nano 编码器、Qwen3 Q5_K_M 与 FSMN-VAD
- `resources/sensevoice/`：SenseVoice INT8 模型与 tokens
- `binaries/`：由 PopSpeak 主进程加载的 sherpa-onnx/ONNX Runtime DLL
- `%LOCALAPPDATA%/tauri/NSIS`：哈希校验后的 Tauri NSIS 3.11 封包工具缓存

Whisper 和 llama.cpp 必须在不同目录；两者存在同名但不保证兼容的 `ggml*.dll`。

## 质量门禁

```powershell
npm test
npm run lint
npm run format:check
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

任一命令失败都不应发布。

## 安装包

```powershell
npm run tauri build
```

当前 Tauri 配置生成 NSIS 安装程序，目录为：

```text
src-tauri/target/release/bundle/nsis/
```

本地构建默认不签名，仅用于开发验收。公开发布只能走 `.github/workflows/release.yml`：

1. 在 GitHub Actions Secrets 中配置 `WINDOWS_CERTIFICATE`（代码签名 PFX 的
   base64 内容）和 `WINDOWS_CERTIFICATE_PASSWORD`。
2. 工作流会验证证书用途和有效期，给主程序、安装包和内置原生运行时签名并加时间戳。
3. 任一密钥缺失、证书无效或最终 Authenticode 状态不是 `Valid`，发布任务都会失败；
   生成的 Release 保持草稿，不能作为公开版本使用。

发布者需要从受信任 CA 购买 Windows 代码签名证书；仓库不会也不应包含私钥。

## 便携包

先完成 release 主程序构建，再运行：

```powershell
./scripts/build-portable.ps1 -CreateZip
```

脚本会拒绝缺少任一模型、CPU 运行时或关键 DLL 的不完整构建，并生成
`dist-portable/PopSpeak/manifest.sha256`。便携包运行时目录固定为：

```text
PopSpeak.exe
models/catalog.json
models/sensevoice/
models/whisper/
models/llm/
runtimes/whisper/
runtimes/llama/
```

应用优先自动索引 `PopSpeak.exe` 同级的相对目录；用户无需手填模型路径。

## 人工冒烟测试

1. 在断网状态启动新安装的 PopSpeak。
2. 完成引导，确认 SenseVoice 和麦克风显示就绪。
3. 打开记事本，按住 `Ctrl+/` 后立刻说“今天测试开头不会丢字”，松开。
4. 确认开头完整、文字进入记事本、原剪贴板内容已恢复。
5. 编辑“专业”为“展业”，保存到专业词汇，下一次再次说出并验证自动纠正。
6. 切换麦克风、按住/切换模式，测试静音、超过最长录音和焦点切换。
7. 把网络断开后重复识别，确认没有登录或联网提示。
8. 可选：启用本地润色，确认任务管理器中没有 GPU 使用，并验证 llama-server
   退出后能自动重启。

CI 公开包使用 `-RequireSignature`，未签名的主程序和原生运行时无法进入便携包。

## 公开发布前的外部条件

- 配置受信任的 Authenticode 代码签名证书（私钥只放 GitHub Secrets）。
- 在干净的 Windows 10 与 Windows 11 非开发机上各跑一次上述冒烟测试。
- 核对 `THIRD_PARTY_NOTICES.md` 与实际随包组件一致。
- 用 `manifest.sha256` 校验便携包，并扫描安装包。
