# PopSpeak

> 先说出来，看清再发送。PopSpeak 是一款开源、离线优先的 Windows 语音输入助手。

[产品官网](https://tzhu01.github.io/PopSpeak/) · [使用与构建](BUILD_WINDOWS.md) · [反馈问题](https://github.com/tzhu01/PopSpeak/issues) · [English](README_EN.md)

**当前状态：源码已公开，Windows 安装包和便携包尚未正式发布。** [Releases 页面](https://github.com/tzhu01/PopSpeak/releases)中标为“源码版”的版本仅提供源码归档，不是可直接运行的 Windows 程序。正式二进制发布前还需完成 [签名、依赖许可和实机验收](docs/OPEN_SOURCE_RELEASE_CHECKLIST.md)。请勿把其他渠道的旧压缩包当作官方发行版。

![PopSpeak 录音时的 POP 文字预览窗](site/assets/popspeak-preview.png)

*实机录音中的 POP 文字预览。预览允许修订；结束后由所选识别模型生成最终文本。*

## 为什么做 PopSpeak

- **先预览，后落笔。** 独立 POP 结果窗支持检查、编辑、排版、复制和置顶；在聊天框、文档或代码窗口中，不必为了改一句话反复回退。
- **边说边看。** 本地辅路持续给出近似文字，所选本地或云端模型在录音结束后提供最终稿。`100 ms` 指预览音频分流粒度，**不是**首次出字或最终识别延迟，也不表示所有模型原生流式。
- **离线优先。** 默认识别在本机 CPU 运行，音频、专业词和历史记录留在设备上；云端识别是用户主动配置的可选项。
- **专业词有能力说明。** 根据当前引擎，词汇可作为解码提示或用于本地识别后规范化；界面会说明具体方式，不把文字替换冒充模型热词。
- **结果找得回。** 历史记录支持搜索、复制、编辑和逐条删除；多语种、方言效果取决于所选模型与录音环境。
- **图片也能转文字。** 选择或拖入图片，由本机 Windows OCR 提取文字，再校对、排版和复制；可用语言取决于系统已安装的 OCR 语言包，图片和结果不会上传到 PopSpeak 服务。

## 三步使用

1. 在 **Windows 10/11 x64** 上按 [构建说明](BUILD_WINDOWS.md)从源码运行；正式安装包仍在准备中。无需独立显卡，初次准备模型与运行时需要下载。
2. 默认按住 `Ctrl+/` 说话，POP 窗先显示近似预览，松开后等待最终结果。
3. 在结果窗校对、编辑并复制，或选用直接输出到原应用。离线功能可免登录试用累计 **200 次或 20 分钟**（先到为准）；继续使用与高级本地功能需要激活。

本地可选 SenseVoice Small、Fun-ASR-Nano 和 Whisper 系列模型；豆包与自定义云端接口需要你自己的服务商账号与凭证，相关费用由服务商决定。模型语言、体积与热词能力见 [模型说明](docs/MODEL_SELECTION_ZH.md)。默认离线识别不会上传音频；更多数据边界见 [隐私说明](PRIVACY.md)。

## 参与开源

PopSpeak 自有源码采用 [MIT 许可证](LICENSE)，可使用、修改和分发，包括商业使用。第三方模型、运行时和品牌各有独立边界，见 [第三方声明](THIRD_PARTY_NOTICES.md)与[许可说明](docs/LICENSING.md)。

发现问题请提交 [Issue](https://github.com/tzhu01/PopSpeak/issues)；开发者可阅读 [贡献指南](CONTRIBUTING.md)和[分支与发布流程](docs/RELEASE_AND_BRANCHES_ZH.md)。当前 `main` 是唯一长期分支，网站也由它自动部署，不需要另建 `gh-pages` 分支。
