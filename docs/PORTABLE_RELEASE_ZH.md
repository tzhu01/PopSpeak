# PopSpeak 完整便携版发布

最终 `dist-portable` 仅保留一个版本的三个交付物：

```text
dist-portable/
  PopSpeak-Final/
    PopSpeak.exe
    models/
    runtimes/
    release-build.json
    manifest.sha256
    ...
  PopSpeak-Final.7z
  PopSpeak-Final.7z.sha256
```

用户需要完整解压目录，再双击 `PopSpeak.exe`；不要单独复制 EXE，也不需要启动 Node、Vite 或 localhost 开发服务器。保持 `models` 与 `runtimes` 的相对路径。

## 构建和验证

从仓库根目录运行，`ModelDirectory` 指向已校验的精确离线模型目录。该目录必须包含 `funasr-encoder-f16.gguf`、`qwen3-0.6b-q4km.gguf` 和 `fsmn-vad.gguf`。构建不自动下载模型。目标必须是全新目录，脚本拒绝覆盖现有版本。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-complete-portable.ps1 `
  -ModelDirectory "D:\已有模型目录\funasr-nano" `
  -OutputDir ".\dist-portable\PopSpeak-Final" `
  -Build
```

发布构建明确使用 `tauri build --no-bundle --features custom-protocol`。只看到 `target/release/popspeak.exe` 不能证明它是可独立运行的版本：没有启用生产协议的 Tauri EXE 仍可能连接 localhost 开发服务器。

因此打包脚本会让 EXE 执行 `--release-self-check <临时 JSON 文件>`，在打开界面、单实例处理和用户数据初始化之前检查实际编译协议、内嵌 `index.html` 和资源数。缺少该自检、超时、依赖开发服务器或资源为空时均拒绝打包。结果和 EXE 的 SHA-256 写入 `release-build.json`。

完整脚本会依次验证暂存目录、压缩包 CRC、实际解压目录；解压后重新核对清单覆盖率、所有文件 SHA-256、模型官方固定哈希、版本、EXE 与本次构建是否一致，并重新运行生产自检。全部成功后才写最终压缩包哈希。临时解压目录在成功后移除；失败时保留并输出路径，以便排查。

需要单独复验时：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-portable-archive.ps1 `
  -ArchivePath ".\dist-portable\PopSpeak-Final.7z" `
  -ExpectedExe ".\src-tauri\target\release\popspeak.exe" `
  -ExpectedVersion "0.4.2" -RequireComplete
```

暂存和临时解压都需要磁盘空间，应按完整模型目录大小预留两份展开容量，另外预留压缩包和编译产物空间。内嵌资源自检不替代实际窗口及语音功能测试；发布前还要在未运行 Vite 的情况下打开解压 EXE，检查首页、模型页、设置、悬浮球和默认离线识别。

## 收敛为一个最终版本

先完成新版本的构建、压缩、解压校验和窗口测试，再归档旧版本。旧版本内可能保存用户自定义模型或其他文件，应完整移到 `dist-portable` 以外的、带时间戳的备份目录，例如 `<工作区>\popspeak-release-archive\<时间戳>`，不要批量删除。

移动前，列出 `dist-portable` 的顶层清单并确认完整绝对路径；逐项使用 PowerShell 的 `Move-Item -LiteralPath`，只移动清单中已确认的旧交付物，排除上面三个最终文件。不要移动整个 `dist-portable` 或工作区根目录。确认没有应用或模型常驻进程占用待归档目录，也要检查本机自定义模型路径是否仍指向旧目录。

以后更新时可先生成 `dist-portable\<全新暂存目录>\PopSpeak-Final` 及同名压缩包，验证通过后归档旧的三个最终交付物，再提升新交付物。压缩包内部始终使用 `PopSpeak-Final` 目录，避免解压到多个版本名。最终重新枚举 `dist-portable`，确认只剩三个交付物，再从最终路径打开 EXE。

包内不得包含个人设置、识别历史、用户词典、云 API 凭证、账号数据库或激活签名私钥。脚本从确定的程序/模型路径取文件，校验器会拒绝常见私有配置与数据库文件、外部链接和未列入清单的附加文件。用户 AppData 和运营私钥目录不属于发布包，也不属于清理对象。
