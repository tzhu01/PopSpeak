# Windows 构建与发版

目前用于 Windows 安装包的配置是 **SenseVoice 精简版**，不是历史上的全模型便携包。
安装包只内置 SenseVoice Small INT8、本地 CPU 推理所需的 sherpa-onnx / ONNX Runtime
DLL、许可文件和离线 WebView2 安装组件；不会把 Whisper、Fun-ASR-Nano、llama.cpp、
Qwen 文字润色模型或任何云端凭证塞进安装包。源码中存在的其他模型适配器不等于
首个安装包已经提供这些模型。四个原生 GGUF 模型下载入口（Qwen3-ASR、Cohere、
Nemotron、Parakeet）在本版**禁用**；适配源码保留，待完整适用许可文本和模型目录
[第三方许可审查](THIRD_PARTY_NOTICES.md)通过后才考虑开放。

## 环境

- Windows 10/11 x64
- PowerShell 7（`pwsh`；不要用系统自带的 Windows PowerShell 5.1 运行实机识别脚本）
- 与 `.node-version` 一致的 Node.js
- 与 `rust-toolchain.toml` 一致的 Rust MSVC 工具链
- Visual Studio 2022 Build Tools：使用 C++ 的桌面开发 + Windows SDK
- CMake（Visual Studio 的 C++ CMake tools 也可）。新增 GGUF ASR 后端静态编译，
  不需要用户安装 Python/CMake。构建前将 CMake 的 bin 目录加入当前进程 PATH。

```powershell
node --version
npm --version
rustc --version
cargo --version
```

## 从干净提交构建精简安装包

先将**已审查的确切提交**克隆到新的目录，检查 `git status --short` 为空，
然后在仓库根目录运行：

```powershell
pwsh -NoProfile -File ./scripts/build-lean-nsis.ps1
```

脚本执行 `npm ci`、从固定上游资产下载并校验 SenseVoice 模型和四个 DLL、
准备经哈希校验的 NSIS 工具、运行前后端测试 / lint / 格式检查 / 实际 CPU
识别测试，最后编译 NSIS 安装包并输出 SHA-256 与源码提交。构建前的网络下载
不代表安装或后续离线识别需要联网。`tauri.conf.json` 的
`webviewInstallMode: offlineInstaller` 把 WebView2 安装组件也放入安装包，
以便没有 WebView2 的目标机离线安装；它不是语音识别云服务。

可单独检查实际参与封包的资产及资源白名单：

```powershell
pwsh -NoProfile -File ./scripts/assert-lean-nsis-assets.ps1
```

这个检查只看 `tauri.conf.json` 明确列入的精简资源，不把机器上可能残留的
`src-tauri/resources/runtimes/` 当作包内容，也不会从旧版便携包复制文件。

## 质量门禁与产物

上述脚本的任一检查失败就不会把结果视为可发布。安装包在：

```text
src-tauri/target/release/bundle/nsis/
```

本地构建默认**不签名**，只供开发验收。脚本输出安装包 SHA-256，但这只证明
本地文件完整性，不等于包内文件已验收、已签名或已获准公开发版。

正式签名版需先将 `.github/workflows/release.yml` 与本精简资源配置对齐，
完成[发布清单](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md)后由受信任的 Authenticode
证书签名并核验：

1. 在 GitHub Actions Secrets 中配置 `WINDOWS_CERTIFICATE`（代码签名 PFX 的
   base64 内容）和 `WINDOWS_CERTIFICATE_PASSWORD`。
2. 工作流必须验证证书用途和有效期，给主程序、安装包和内置原生 DLL 签名并加时间戳。
3. 任一密钥缺失、证书无效或最终 Authenticode 状态不是 `Valid`，发布任务都会失败；
   生成的 Release 保持草稿，不能作为公开版本使用。

发布者需要从受信任 CA 购买 Windows 代码签名证书；仓库不会也不应包含私钥。

## 旧版全模型便携包（不能直接公开）

`scripts/build-portable.ps1` 和全模型准备脚本属于另一条尚未通过再分发门禁的
开发路径。旧 Whisper / llama.cpp 运行时包含不可直接再分发的 `libomp` DLL；
不能把旧 `dist-portable`、旧 `resources/runtimes` 或其压缩包改名上传 GitHub。
精简安装包不依赖这些目录。

## 人工冒烟测试

1. 在干净的 Windows 10 / 11 x64 虚拟机上安装，记录**实际安装目录**；最好先断网，
   验证 NSIS 内置 WebView2 的安装路径。
2. 在源码仓库运行下列命令，比对包内模型、DLL、许可文件与源码固定哈希，
   并排查旧运行时、私有文件及 `libomp`：

   ```powershell
   pwsh -NoProfile -File ./scripts/assert-lean-nsis-assets.ps1 `
     -InstalledDir 'C:\实际安装目录' `
     -Installer 'C:\安装包路径\PopSpeak_...-setup.exe'
   ```
3. 完成引导，确认 SenseVoice 和麦克风显示就绪。
4. 打开记事本，按住 `Ctrl+/` 后立刻说“今天测试开头不会丢字”，松开。
5. 确认开头完整、文字进入记事本、原剪贴板内容已恢复。
6. 编辑专业词，下一次再说出并验证当前引擎说明的本地纠错方式。
7. 切换麦克风、按住/切换模式，测试静音、最长录音时长和焦点切换。
8. 保持断网重复识别，确认默认本地路径无需登录或联网；云端功能须手动配置。

干净系统安装后验收不能用编译机原有安装目录代替；升级安装可能残留旧文件，
无法证明新包实际包含或不包含什么。

## 公开发布前的外部条件

- 配置受信任的 Authenticode 代码签名证书（私钥只放 GitHub Secrets）；
  若所有者明确选择未签名预发布版，应标成 **unsigned prerelease**，提示
  SmartScreen 警告，绝不能称作已签名正式版。
- 在干净的 Windows 10 与 Windows 11 非开发机上各跑一次上述冒烟测试。
- 核对 `THIRD_PARTY_NOTICES.md`、随包许可文本和实际包内组件一致。
- 记录提交、安装包 SHA-256、包内固定资产校验结果；扫描安装包并制作 SBOM。
- 四个原生 GGUF 模型下载入口在此安装包中必须保持禁用；完整适用许可文本和
  模型目录审计未完之前，不能把未审核模型下载当作正式发行承诺。

依赖 SBOM 与包内资产清单可从干净源码树生成（两类清单的覆盖范围不同）：

```powershell
cargo install cargo-cyclonedx --version 0.5.9 --locked
pwsh -NoProfile -File ./scripts/generate-release-sboms.ps1 `
  -InstallerPath './src-tauri/target/release/bundle/nsis/PopSpeak_0.4.4_x64-setup.exe'
```

产出包含 npm 生产依赖、Windows Rust 依赖的 CycloneDX JSON，以及安装包、模型、
DLL 和许可证的 SHA-256 清单。仅生成依赖 SBOM 不能证明 NSIS 内部文件组成；
须另行解包或在干净系统安装后核对。
