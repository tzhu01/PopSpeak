# 自定义云端语音识别接入与验收

更新：2026-09-07。对应代码：`src-tauri/src/stt/custom_cloud.rs`、`src-tauri/src/stt/seedasr.rs`。

## 本次交付范围

“自定义云端接口”不再等同于一个通用文件上传地址，而是包含 6 个明确区分协议的适配器。音频统一为 16 kHz、16 位、单声道 PCM；用户自己的账号直接连接官方服务。语音、凭证不经过 PopSpeak 开发者服务器，不读取网页 Cookie，不调用网页体验私有接口。

| 软件中的厂商选项 | 本次实现的具体 API | 交互方式 | 本次客户端上限 |
| --- | --- | --- | --- |
| 字节火山引擎 · 豆包语音 | 大模型双向流式优化版 | 二进制 WebSocket，复用豆包适配器 | 单段最多 600 秒；资源授权仍以账号为准 |
| 阿里云 · 智能语音交互 | NLS 实时语音识别 / SpeechTranscriber | WebSocket 开始指令、PCM、结束指令 | 单段最多 600 秒 |
| 腾讯云 · 语音识别 | `/asr/v2/{AppID}` 实时语音识别 | HMAC-SHA1 签名 WebSocket | 单段最多 600 秒；Hy-ASR-3.0-preview 单段最多 60 秒 |
| 科大讯飞 · 语音听写 | IAT 语音听写（流式版） | HMAC-SHA256 签名 WebSocket | 单段最多 60 秒 |
| 百度智能云 · 短语音识别 | 标准版 `server_api` | 停止录音后 JSON + Base64 整段请求 | 单段最多 60 秒，**不是实时字幕接口** |
| 通用 · 音频转写兼容接口 | multipart `/audio/transcriptions` | WAV 文件上传，读取 `text` | 本次客户端最多 600 秒；还受所填服务自身限制 |

600 秒是本软件的安全上限，不是对厂商所有型号、套餐和地区的保证。实际 API 最大时长、并发、可用型号、账单和赠送额度以用户账号为准。本次没有接入每个厂商全部语音产品，也不把实时接口说成任意兼容 Whisper 接口。

## 凭证和操作步骤

### 1. 字节 / 火山引擎

在火山引擎语音控制台开通对应流式识别资源，使用语音应用的 `APP ID + Access Token`，或已经获得语音资源授权的新版语音 API Key。资源 ID 默认 `volc.seedasr.sauc.duration`；如果账号开通的是不同资源，请填写控制台明确给出的资源 ID。方舟文本模型 API Key 和网页体验余额不等于语音资源权限。

软件参数：`app_id + access_token`，或 `api_key`；`model` 保存资源 ID。调用的固定地址为 `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async`。

参考：[官方流式协议](https://www.volcengine.com/docs/6561/1354869)、[官方产品动态（含双向流式优化版地址）](https://www.volcengine.com/docs/6561/162929?lang=en)。前一页面本轮抓取间歇失败，固定地址另经产品动态交叉核对；不是把网页体验当 API。

### 2. 阿里云

进入[智能语音交互控制台](https://nls-portal.console.aliyun.com/)，开通服务，建立项目并配置语种/场景，复制项目 **AppKey**。在总览取得临时 Token。软件里填“项目 AppKey”“语音服务 Token”，选择上海、北京或深圳。这里不是百炼 DashScope Key，也不是直接填写阿里云 AccessKey ID。

本次只接收有效 NLS Token。控制台临时 Token 约 24 小时失效，过期后需要重新填写；本次**没有**自动使用 RAM AccessKey 生成、刷新 Token，也未实现开发者统一发 Token 的服务。长期正式运营需另加服务端短期令牌下发，不能宣称临时 Token 永久可用。

参考：[官方接入步骤](https://help.aliyun.com/zh/isi/getting-started/start-here)、[控制台获取临时 Token](https://help.aliyun.com/zh/isi/getting-started/obtain-an-access-token-in-the-console)、[WebSocket 协议](https://help.aliyun.com/zh/isi/developer-reference/websocket)、[官方地域及 API 说明](https://help.aliyun.com/zh/isi/developer-reference/api-reference)。

### 3. 腾讯云

开通腾讯云语音识别服务，在[API 密钥管理](https://console.cloud.tencent.com/cam/capi)取得 AppID、SecretId、SecretKey，并确保密钥所属账号有 ASR 权限。软件中分别填 `app_id`、`api_key`、`api_secret`，型号默认 `16k_zh`。程序按排序后的原始参数签名，再 URL 编码请求；不会把 SecretKey 直接发到识别端。

只有实际开通/付费的型号才能使用，修改型号名称不意味着购买了该能力；8 kHz 型号不适用于本客户端固定 16 kHz 音频。系统时间明显不正确会导致签名验证失败。

参考：[官方实时语音识别 API](https://cloud.tencent.com/document/api/1093/48982)、[官方 Python SDK 签名和结果结构](https://github.com/TencentCloud/tencentcloud-speech-sdk-python/blob/master/asr/speech_recognizer.py)。

### 4. 科大讯飞

在[讯飞开放平台控制台](https://console.xfyun.cn/)创建应用，为该应用开通“语音听写（流式版）”，复制该产品的 APPID、APIKey、APISecret。不要混填其他讯飞产品的密钥。软件使用普通话听写，启用动态修正，单次最长 60 秒；长静音可能触发服务端提前结束。APISecret 只参与本地 HMAC-SHA256 签名。

参考：[官方 IAT 接口、鉴权、首末帧及动态修正说明](https://www.xfyun.cn/doc/asr/voicedictation/API.html)。这不是讯飞所有长时转写/大模型听写产品的通用入口。

### 5. 百度智能云

在语音技术控制台建立具备短语音识别权限的应用。本次支持经典应用 `API Key + Secret Key` 换取 Access Token，或直接输入已取得的 Access Token。程序用随机会话 ID 作为 `cuid`，不采集 MAC、IMEI 等硬件标识。

默认 `dev_pid=1537`（普通话），本次标准版允许 `1537 / 1737 / 1637 / 1837`。停止录音后提交整段 PCM。百度新统一 API Key 的 Bearer 鉴权、极速版和 WebSocket 实时产品**不在本次适配范围**，不要把其参数混填到经典 Key/Secret 表单。

参考：[标准短语音 API](https://ai.baidu.com/ai-doc/SPEECH/Jlbxdezuf)、[官方鉴权说明](https://cloud.baidu.com/doc/SPEECH/s/Em8snejw1)。

### 6. 通用兼容接口

填写**完整音频转写地址**（例如 `https://你的域名/v1/audio/transcriptions`）、该地址对应的 API Key 和模型 ID。请求是 multipart：WAV 字段 `file`、字符串字段 `model`，密钥通过 Bearer Header 传输。返回必须包含 JSON 字符串字段 `text`。不是把任意聊天、文件上传或语音端点填进来就能工作。

远端必须 HTTPS，仅 `localhost / 127.0.0.1 / ::1` 允许 HTTP 调试。地址中禁止嵌入用户名、密码、查询参数、片段；HTTP 自动重定向关闭，避免凭证或录音被转发到意外地址。自建代理如果兼容此协议，应选通用接口并使用代理自己的密钥。

## 录音和识别结果处理

1. 用户保存厂商配置，随后新录音创建一个新任务；录音中的任务不读取中途修改的草稿凭证。
2. 实时接口完成握手；阿里云、腾讯云等待服务端“开始”确认，再发送 PCM。发送按实际音频时长限速，避免握手期间积压的音频被瞬间大量推送。
3. 阿里云按句子 `index` 更新，腾讯云按结果 `index` 更新；同一句中间结果替换旧版本，不重复拼接。
4. 讯飞按 `sn` 存储片段，`pgs=rpl` 时删除 `rg` 指定的闭区间旧片段，再应用修订，避免“张三张珊”这种重复结果。
5. 停止录音会发送厂商对应的结束帧。只有完整结束确认才算实时任务成功；超时/提前断线明确报错，不把中间结果伪装为成功。厂商主动正常结束时，已有最终文字只交付一次，并停止向已结束的任务继续发音频。
6. 最终结果再进入软件已授权的热词/纠错/润色流程；**本次没有把本地词典自动同步成五家云厂商的热词表**。云端词库、自训练模型属于厂商控制台/专属 API 的独立能力。

## 安全和计费边界

- 五家官方适配器采用厂商域名和产品路径精确校验，不能把腾讯密钥贴到阿里地址；切换厂商需要重新填对应凭证。
- 新适配器的异常文本只保留状态码和排查方向，不回显签名 URL、HTTP 错误正文、密钥。配置结构的调试格式自动遮蔽凭证。
- HTTP/WS 建连、写入、最终结果均有超时；新 JSON 响应及豆包二进制/解压响应最多 1 MiB，文本组装最多 128 KiB。
- 凭证是否本机持久保存由主设置模块控制。本次不创建厂商账号，不充值、不升级套餐、不重试收费识别任务。
- “测试接口”会真实发送约 0.2 秒合成静音，可能消耗赠送或付费额度；它验证协议流程，不是麦克风测试，不评估准确率。百度静音可能返回“无有效语音”，这不等于转写效果验收。
- 软件激活只解锁 PopSpeak 功能，**不会给用户购买厂商额度，也不能消除厂商费用和限制**。

## 测试证据与尚需真实验收项

2026-09-07 自动化覆盖：

- HMAC-SHA1 / SHA256 已知向量；腾讯规范排序和编码；讯飞 RFC1123 请求串；阿里开始/停止任务 ID。
- 不同厂商的凭证字段、官方地址校验、TLS、URL 禁止凭证、PCM 对齐、时长上限、服务端错误脱敏。
- 阿里、腾讯重复中间结果合并；讯飞动态修订闭区间；百度只取最优候选而非拼接候选。
- 本机 WebSocket 真实收发：三家 JSON 实时协议的音频、末帧、修订结果、单次最终输出；本机 HTTP 真实 multipart WAV 提交；307 重定向拒绝（没有第二次转发）。这些是模拟服务端协议测试，**不是厂商账号联网联调**。
- 豆包异常压缩包上限与提前断线不得返回成功。

可重复执行：`cargo test --manifest-path src-tauri/Cargo.toml stt:: --lib`。真实云端测试默认忽略，必须单独显式运行 `stt::custom_cloud::tests::real_cloud_smoke_from_environment`，只从 `POPSPEAK_TEST_CLOUD_JSON` 环境变量取测试账号配置，不打印变量、不自动重试。

目前阿里、腾讯、讯飞、百度没有用户提供的有效专属测试凭证，因此只能报告“已实现并通过本地协议测试”；不能承诺已通过这四家真实鉴权、实际计费和声音准确率测试。字节若执行真实烟测，结果应记录在本次版本验收报告，并和协议测试分开陈述。

正式发布前还应由各厂商有效账号使用同一段普通话音频，分别验收首次握手、完整识别、语音结束/长静音、额度不足、断网、到达单段上限、重启后配置保留。阿里长期令牌刷新和各厂商热词管理应独立排期，不冒充已完成。
