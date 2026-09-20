//! Vendor-specific ASR protocols. Credentials are never sent to a different
//! vendor's endpoint and transport errors never include signed URLs or bodies.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Sha256;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{client::IntoClientRequest, protocol::WebSocketConfig, Message},
};
use url::Url;
use uuid::Uuid;

use super::{
    seedasr::SeedAsrProvider, whisper_compat::WhisperCompatProvider, SttConfig, SttProvider,
    TranscriptEvent,
};

const SAMPLE_RATE: u32 = 16_000;
const BYTES_PER_SECOND: usize = 32_000;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const FINAL_TIMEOUT: Duration = Duration::from_secs(15);
const BYTE_ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";
const IFLYTEK_ENDPOINT: &str = "wss://iat-api.xfyun.cn/v2/iat";
const BAIDU_ENDPOINT: &str = "https://vop.baidu.com/server_api";

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CustomCloudConfig {
    pub vendor: String,
    pub app_id: String,
    pub api_key: String,
    pub api_secret: String,
    pub access_token: String,
    pub endpoint: String,
    pub model: String,
    pub region: String,
}

impl Default for CustomCloudConfig {
    fn default() -> Self {
        Self {
            vendor: "whisper".into(),
            app_id: String::new(),
            api_key: String::new(),
            api_secret: String::new(),
            access_token: String::new(),
            endpoint: String::new(),
            model: String::new(),
            region: String::new(),
        }
    }
}

impl std::fmt::Debug for CustomCloudConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomCloudConfig")
            .field("vendor", &self.vendor)
            .field("credentials", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Vendor {
    Whisper,
    ByteDance,
    Aliyun,
    Tencent,
    Iflytek,
    Baidu,
}

impl Vendor {
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "" | "whisper" => Self::Whisper,
            "bytedance" => Self::ByteDance,
            "aliyun" => Self::Aliyun,
            "tencent" => Self::Tencent,
            "iflytek" => Self::Iflytek,
            "baidu" => Self::Baidu,
            _ => bail!("不支持的云端接口类型，请重新选择厂商"),
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Whisper => "自定义云端接口",
            Self::ByteDance => "豆包云端识别",
            Self::Aliyun => "阿里云实时识别",
            Self::Tencent => "腾讯云实时识别",
            Self::Iflytek => "讯飞语音听写",
            Self::Baidu => "百度短语音识别",
        }
    }
}

/// This is a protocol limit, independent of the product's activation quota.
pub fn max_recording_seconds(config: &CustomCloudConfig) -> u32 {
    match config.vendor.as_str() {
        "iflytek" | "baidu" => 60,
        "tencent" if config.model.trim() == "Hy-ASR-3.0-preview" => 60,
        _ => 600,
    }
}

fn model_or<'a>(config: &'a CustomCloudConfig, fallback: &'a str) -> &'a str {
    if config.model.trim().is_empty() {
        fallback
    } else {
        config.model.trim()
    }
}

fn require(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("请填写{label}");
    }
    if value.len() > 4096 || value.chars().any(char::is_control) {
        bail!("{label}格式无效");
    }
    Ok(())
}

/// Reject credentials/query strings in a user-provided URL. Official presets
/// use exact paths, hosts and TLS; they are not arbitrary credential proxies.
fn endpoint_for(config: &CustomCloudConfig, vendor: Vendor) -> Result<Url> {
    let default = match vendor {
        Vendor::Whisper => "https://api.openai.com/v1/audio/transcriptions".into(),
        Vendor::ByteDance => BYTE_ENDPOINT.into(),
        Vendor::Aliyun => match config.region.as_str() {
            "" | "cn-shanghai" => "wss://nls-gateway-cn-shanghai.aliyuncs.com/ws/v1".into(),
            "cn-beijing" => "wss://nls-gateway-cn-beijing.aliyuncs.com/ws/v1".into(),
            "cn-shenzhen" => "wss://nls-gateway-cn-shenzhen.aliyuncs.com/ws/v1".into(),
            _ => bail!("阿里云地域无效，请选择上海、北京或深圳"),
        },
        Vendor::Tencent => {
            require(&config.app_id, "腾讯云 APP ID")?;
            if !config.app_id.bytes().all(|b| b.is_ascii_digit()) {
                bail!("腾讯云 APP ID 应为数字");
            }
            format!("wss://asr.cloud.tencent.com/asr/v2/{}", config.app_id)
        }
        Vendor::Iflytek => IFLYTEK_ENDPOINT.into(),
        Vendor::Baidu => BAIDU_ENDPOINT.into(),
    };
    let raw = if config.endpoint.trim().is_empty() {
        &default
    } else {
        config.endpoint.trim()
    };
    let url = Url::parse(raw).map_err(|_| anyhow!("接口地址格式无效"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("接口地址不得包含登录信息、查询参数或片段；请把密钥填入凭证栏");
    }
    if vendor == Vendor::Whisper {
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            bail!("云端接口必须使用 HTTPS；仅本机地址允许 HTTP");
        }
        if url.host_str().is_none() {
            bail!("接口地址缺少主机名");
        }
    } else if url.as_str() != default {
        bail!("所选厂商只允许对应的官方语音接口地址，不能混用其他厂商或代理地址");
    }
    Ok(url)
}

fn validate(config: &CustomCloudConfig, vendor: Vendor) -> Result<()> {
    endpoint_for(config, vendor)?;
    if config.model.len() > 128 || config.model.chars().any(char::is_control) {
        bail!("模型或资源 ID 格式无效");
    }
    match vendor {
        Vendor::Whisper => require(&config.api_key, "API Key")?,
        Vendor::ByteDance if !config.access_token.is_empty() => {
            require(&config.app_id, "APP ID")?;
            require(&config.access_token, "Access Token")?;
        }
        Vendor::ByteDance => require(
            &config.api_key,
            "语音 API Key，或改用 APP ID 与 Access Token",
        )?,
        Vendor::Aliyun => {
            require(&config.app_id, "阿里云项目 AppKey")?;
            require(&config.access_token, "阿里云 NLS Token（过期后需重新获取）")?;
        }
        Vendor::Tencent | Vendor::Iflytek => {
            require(&config.app_id, "APP ID")?;
            require(&config.api_key, "API Key / SecretId")?;
            require(&config.api_secret, "API Secret / SecretKey")?;
            if vendor == Vendor::Tencent
                && !model_or(config, "16k_zh")
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            {
                bail!("腾讯云引擎型号格式无效");
            }
        }
        Vendor::Baidu => {
            if config.access_token.is_empty() {
                require(&config.api_key, "百度 API Key")?;
                require(&config.api_secret, "百度 Secret Key")?;
            } else {
                require(&config.access_token, "百度 Access Token")?;
            }
            let model = model_or(config, "1537")
                .parse::<u32>()
                .map_err(|_| anyhow!("百度 dev_pid 必须是数字"))?;
            if !matches!(model, 1537 | 1737 | 1637 | 1837) {
                bail!("此适配器仅支持百度标准版 16k 模型：1537、1737、1637、1837");
            }
        }
    }
    Ok(())
}

fn sign_sha1(secret: &str, data: &str) -> String {
    let mut mac =
        Hmac::<Sha1>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(data.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}
fn sign_sha256(secret: &str, data: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(data.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}

fn tencent_url(
    config: &CustomCloudConfig,
    timestamp: i64,
    nonce: u32,
    voice_id: &str,
) -> Result<Url> {
    let mut url = endpoint_for(config, Vendor::Tencent)?;
    let fields = BTreeMap::from([
        ("engine_model_type", model_or(config, "16k_zh").to_string()),
        ("expired", (timestamp + 3600).to_string()),
        ("needvad", "1".into()),
        ("nonce", nonce.max(1).to_string()),
        ("secretid", config.api_key.trim().to_string()),
        ("sub_service_type", "1".into()),
        ("timestamp", timestamp.to_string()),
        ("voice_format", "1".into()),
        ("voice_id", voice_id.into()),
    ]);
    let raw_query = fields
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let canonical = format!("asr.cloud.tencent.com{}?{raw_query}", url.path());
    let signature = sign_sha1(config.api_secret.trim(), &canonical);
    url.query_pairs_mut()
        .extend_pairs(fields.iter())
        .append_pair("signature", &signature);
    Ok(url)
}

fn iflytek_url(config: &CustomCloudConfig, date: &str) -> Result<Url> {
    let mut url = endpoint_for(config, Vendor::Iflytek)?;
    let host = "iat-api.xfyun.cn";
    let canonical = format!("host: {host}\ndate: {date}\nGET /v2/iat HTTP/1.1");
    let signature = sign_sha256(config.api_secret.trim(), &canonical);
    let authorization = STANDARD.encode(format!("api_key=\"{}\", algorithm=\"hmac-sha256\", headers=\"host date request-line\", signature=\"{signature}\"", config.api_key.trim()));
    url.query_pairs_mut()
        .append_pair("authorization", &authorization)
        .append_pair("date", date)
        .append_pair("host", host);
    Ok(url)
}

fn aliyun_command(
    config: &CustomCloudConfig,
    task_id: &str,
    message_id: &str,
    start: bool,
) -> Value {
    let mut command = json!({"header": {"appkey": config.app_id.trim(), "task_id": task_id,
        "message_id": message_id, "namespace": "SpeechTranscriber",
        "name": if start { "StartTranscription" } else { "StopTranscription" } }});
    if start {
        command["payload"] = json!({"format":"pcm", "sample_rate": SAMPLE_RATE,
        "enable_intermediate_result":true,"enable_punctuation_prediction":true,"enable_inverse_text_normalization":true});
    }
    command
}

fn iflytek_audio(config: &CustomCloudConfig, pcm: &[u8], first: bool, last: bool) -> Value {
    let mut message = json!({"data":{"status": if last {2} else if first {0} else {1},
        "format":"audio/L16;rate=16000","encoding":"raw","audio": STANDARD.encode(pcm)}});
    if first {
        message["common"] = json!({"app_id":config.app_id.trim()});
        message["business"] = json!({"language":"zh_cn","domain":"iat","accent":"mandarin", "dwa":"wpgs", "vad_eos":10000});
    }
    message
}

#[derive(Default)]
struct TranscriptAssembly {
    segments: BTreeMap<u64, String>,
    terminal: bool,
    ready: bool,
}

impl TranscriptAssembly {
    fn text(&self) -> String {
        self.segments
            .values()
            .map(String::as_str)
            .collect::<String>()
    }
    fn insert(&mut self, index: u64, text: &str) -> Result<()> {
        if self.segments.len() > 10_000 || text.len() > 128 * 1024 {
            bail!("识别结果超出安全长度限制");
        }
        self.segments.insert(index, text.to_string());
        if self.segments.values().map(String::len).sum::<usize>() > 128 * 1024 {
            bail!("识别结果超出安全长度限制");
        }
        Ok(())
    }
    fn apply(&mut self, vendor: Vendor, body: &Value) -> Result<bool> {
        let mut changed = false;
        match vendor {
            Vendor::Aliyun => {
                let status = body["header"]["status"]
                    .as_i64()
                    .ok_or_else(|| anyhow!("阿里云响应缺少状态码"))?;
                if status != 20_000_000 {
                    bail!("阿里云识别失败（代码 {status}），请检查 Token 有效期、项目权限和额度");
                }
                match body["header"]["name"].as_str().unwrap_or("") {
                    "TranscriptionStarted" => self.ready = true,
                    "TranscriptionCompleted" => self.terminal = true,
                    "SentenceEnd" | "TranscriptionResultChanged" => {
                        let index = body["payload"]["index"]
                            .as_u64()
                            .ok_or_else(|| anyhow!("阿里云响应缺少句子编号"))?;
                        let text = body["payload"]["result"]
                            .as_str()
                            .ok_or_else(|| anyhow!("阿里云响应缺少识别文本"))?;
                        self.insert(index, text)?;
                        changed = true;
                    }
                    _ => {}
                }
            }
            Vendor::Tencent => {
                let code = body["code"]
                    .as_i64()
                    .ok_or_else(|| anyhow!("腾讯云响应缺少状态码"))?;
                if code != 0 {
                    bail!("腾讯云识别失败（代码 {code}），请检查密钥、系统时间、权限和额度");
                }
                self.ready = true;
                if let Some(text) = body["result"]["voice_text_str"].as_str() {
                    let index = body["result"]["index"]
                        .as_u64()
                        .ok_or_else(|| anyhow!("腾讯云响应缺少句子编号"))?;
                    self.insert(index, text)?;
                    changed = true;
                }
                self.terminal |= body["final"].as_u64() == Some(1);
            }
            Vendor::Iflytek => {
                let code = body["code"]
                    .as_i64()
                    .ok_or_else(|| anyhow!("讯飞响应缺少状态码"))?;
                if code != 0 {
                    bail!("讯飞识别失败（代码 {code}），请检查应用听写权限、密钥、系统时间和额度");
                }
                if body["data"]["result"].is_object() {
                    let result = &body["data"]["result"];
                    let index = result["sn"]
                        .as_u64()
                        .ok_or_else(|| anyhow!("讯飞响应缺少序号"))?;
                    let text = result["ws"]
                        .as_array()
                        .ok_or_else(|| anyhow!("讯飞响应缺少词序列"))?
                        .iter()
                        .filter_map(|word| word["cw"][0]["w"].as_str())
                        .collect::<String>();
                    if result["pgs"].as_str() == Some("rpl") {
                        let start = result["rg"][0]
                            .as_u64()
                            .ok_or_else(|| anyhow!("讯飞动态修正范围无效"))?;
                        let end = result["rg"][1]
                            .as_u64()
                            .ok_or_else(|| anyhow!("讯飞动态修正范围无效"))?;
                        if start > end {
                            bail!("讯飞动态修正范围无效");
                        }
                        self.segments
                            .retain(|index, _| *index < start || *index > end);
                    }
                    self.insert(index, &text)?;
                    changed = true;
                }
                self.terminal |= body["data"]["status"].as_u64() == Some(2);
            }
            _ => bail!("此接口不是 JSON 实时协议"),
        }
        Ok(changed)
    }
}

pub struct CustomCloudProvider {
    config: CustomCloudConfig,
    vendor: Option<Vendor>,
    ws: Option<WsStream>,
    seed: Option<SeedAsrProvider>,
    client: Option<reqwest::Client>,
    audio: Vec<u8>,
    total_bytes: usize,
    sent_bytes: usize,
    stream_started: Instant,
    first_frame: bool,
    task_id: String,
    assembly: TranscriptAssembly,
    final_delivered: bool,
    connected: bool,
}

impl CustomCloudProvider {
    pub fn new(
        mut config: CustomCloudConfig,
        legacy_endpoint: Option<&str>,
        legacy_model: Option<&str>,
    ) -> Self {
        if matches!(config.vendor.as_str(), "" | "whisper") {
            if config.endpoint.is_empty() {
                config.endpoint = legacy_endpoint.unwrap_or("").to_string();
            }
            if config.model.is_empty() {
                config.model = legacy_model.unwrap_or("").to_string();
            }
        }
        Self {
            config,
            vendor: None,
            ws: None,
            seed: None,
            client: None,
            audio: Vec::new(),
            total_bytes: 0,
            sent_bytes: 0,
            stream_started: Instant::now(),
            first_frame: true,
            task_id: String::new(),
            assembly: TranscriptAssembly::default(),
            final_delivered: false,
            connected: false,
        }
    }

    async fn send_message(&mut self, message: Message) -> Result<()> {
        let ws = self
            .ws
            .as_mut()
            .ok_or_else(|| anyhow!("云端连接尚未建立"))?;
        tokio::time::timeout(WRITE_TIMEOUT, ws.send(message))
            .await
            .map_err(|_| anyhow!("云端音频发送超时"))?
            .map_err(|_| anyhow!("云端音频发送失败，请检查网络"))
    }

    async fn read_message(&mut self) -> Result<bool> {
        let message = self
            .ws
            .as_mut()
            .ok_or_else(|| anyhow!("云端连接尚未建立"))?
            .next()
            .await;
        match message {
            Some(Ok(Message::Text(text))) => {
                if text.len() > MAX_RESPONSE_BYTES {
                    bail!("云端响应超出安全长度限制");
                }
                let body: Value =
                    serde_json::from_str(&text).map_err(|_| anyhow!("云端返回了无效 JSON"))?;
                self.assembly
                    .apply(self.vendor.ok_or_else(|| anyhow!("厂商未初始化"))?, &body)
            }
            Some(Ok(Message::Ping(payload))) => {
                self.send_message(Message::Pong(payload)).await?;
                Ok(false)
            }
            Some(Ok(Message::Close(_))) | None => {
                if !self.assembly.terminal {
                    bail!("云端提前断开，尚未收到完整转写结果");
                }
                Ok(false)
            }
            Some(Err(_)) => bail!("云端连接中断，请检查网络后重试"),
            Some(Ok(_)) => Ok(false),
        }
    }

    async fn write_packet(&mut self, packet: &[u8], last: bool) -> Result<()> {
        // Apply 1:1 pacing even when microphone frames queued during handshake.
        let target = self.stream_started
            + Duration::from_secs_f64(self.sent_bytes as f64 / BYTES_PER_SECOND as f64);
        if target > Instant::now() {
            tokio::time::sleep_until(tokio::time::Instant::from_std(target)).await;
        }
        if let Some(seed) = self.seed.as_mut() {
            tokio::time::timeout(WRITE_TIMEOUT, seed.send_audio(packet))
                .await
                .map_err(|_| anyhow!("豆包语音发送超时"))?
                .map_err(|_| anyhow!("豆包语音发送失败，请检查网络"))?;
            self.sent_bytes += packet.len();
            return Ok(());
        }
        let vendor = self.vendor.ok_or_else(|| anyhow!("厂商未初始化"))?;
        let message = if vendor == Vendor::Iflytek {
            Message::Text(iflytek_audio(&self.config, packet, self.first_frame, last).to_string())
        } else {
            Message::Binary(packet.to_vec())
        };
        self.send_message(message).await?;
        self.first_frame = false;
        self.sent_bytes += packet.len();
        Ok(())
    }

    async fn baidu_token(&self) -> Result<String> {
        if !self.config.access_token.is_empty() {
            return Ok(self.config.access_token.trim().into());
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow!("HTTP 客户端未初始化"))?;
        let response = client
            .post("https://aip.baidubce.com/oauth/2.0/token")
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.config.api_key.trim()),
                ("client_secret", self.config.api_secret.trim()),
            ])
            .send()
            .await
            .map_err(|_| anyhow!("百度令牌请求失败，请检查网络"))?;
        let body = bounded_json(response).await?;
        body["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("百度令牌获取失败，请检查 API Key 与 Secret Key"))
    }

    async fn http_transcribe(&mut self, vendor: Vendor) -> Result<Option<String>> {
        if self.audio.is_empty() {
            return Ok(None);
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow!("HTTP 客户端未初始化"))?;
        let endpoint = endpoint_for(&self.config, vendor)?;
        let response = if vendor == Vendor::Baidu {
            let token = self.baidu_token().await?;
            client.post(endpoint).json(&json!({"format":"pcm", "rate":SAMPLE_RATE,"channel":1,
                "cuid":self.task_id,"token":token,"dev_pid":model_or(&self.config,"1537").parse::<u32>()?,
                "len":self.audio.len(),"speech":STANDARD.encode(&self.audio)}))
                .send().await.map_err(|_| anyhow!("百度识别请求失败，请检查网络"))?
        } else {
            let wav = WhisperCompatProvider::build_wav(&self.audio, SAMPLE_RATE);
            let file = reqwest::multipart::Part::bytes(wav)
                .file_name("audio.wav")
                .mime_str("audio/wav")?;
            let form = reqwest::multipart::Form::new()
                .text("model", model_or(&self.config, "whisper-1").to_string())
                .part("file", file);
            client
                .post(endpoint)
                .bearer_auth(self.config.api_key.trim())
                .multipart(form)
                .send()
                .await
                .map_err(|_| anyhow!("自定义转写请求失败，请检查网络和接口地址"))?
        };
        self.audio.clear();
        let body = bounded_json(response).await?;
        if vendor == Vendor::Baidu {
            return baidu_result(&body);
        }
        let text = body["text"]
            .as_str()
            .ok_or_else(|| anyhow!("接口响应缺少 text 字段；该地址可能不兼容音频转写协议"))?
            .trim();
        Ok((!text.is_empty()).then(|| text.into()))
    }
}

async fn bounded_json(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    if !status.is_success() {
        bail!(
            "云端请求失败（HTTP {}），请检查凭证、接口权限和额度",
            status.as_u16()
        );
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        bail!("云端响应超出安全长度限制");
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| anyhow!("读取云端响应失败"))?;
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            bail!("云端响应超出安全长度限制");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("云端返回了无效 JSON"))
}

fn baidu_result(body: &Value) -> Result<Option<String>> {
    let code = body["err_no"]
        .as_i64()
        .ok_or_else(|| anyhow!("百度响应缺少 err_no 状态码"))?;
    if code == 3301 {
        return Ok(None);
    } // Audio quality/no-speech, not authentication success by itself.
    if code != 0 {
        bail!("百度识别失败（代码 {code}），请检查令牌、语音服务权限和额度");
    }
    // result is a list of alternatives, not chunks of a running transcript.
    let text = body["result"][0]
        .as_str()
        .ok_or_else(|| anyhow!("百度响应缺少识别结果"))?
        .trim();
    Ok((!text.is_empty()).then(|| text.into()))
}

#[async_trait]
impl SttProvider for CustomCloudProvider {
    async fn connect(&mut self, stt: &SttConfig) -> Result<()> {
        let vendor = Vendor::parse(&self.config.vendor)?;
        if vendor == Vendor::Whisper && self.config.api_key.is_empty() {
            self.config.api_key.clone_from(&stt.api_key);
        }
        validate(&self.config, vendor)?;
        if stt.sample_rate != SAMPLE_RATE {
            bail!("此适配器要求 16kHz、16 位、单声道 PCM");
        }
        self.vendor = Some(vendor);
        self.audio.clear();
        self.total_bytes = 0;
        self.sent_bytes = 0;
        self.first_frame = true;
        self.assembly = TranscriptAssembly::default();
        self.final_delivered = false;
        self.task_id = Uuid::new_v4().simple().to_string();
        self.connected = false;
        if vendor == Vendor::ByteDance {
            let mut seed =
                SeedAsrProvider::new(model_or(&self.config, super::seedasr::DEFAULT_RESOURCE_ID));
            let uses_token = !self.config.access_token.is_empty();
            let mapped = SttConfig {
                api_key: if uses_token {
                    self.config.access_token.clone()
                } else {
                    self.config.api_key.clone()
                },
                app_id: self.config.app_id.clone(),
                credential_mode: if uses_token { "app-token" } else { "api-key" }.into(),
                ..stt.clone()
            };
            tokio::time::timeout(CONNECT_TIMEOUT, seed.connect(&mapped))
                .await
                .map_err(|_| anyhow!("豆包语音连接超时"))?
                .map_err(|_| anyhow!("豆包语音连接失败，请检查应用凭证、语音资源权限和额度"))?;
            self.seed = Some(seed);
            self.stream_started = Instant::now();
            self.connected = true;
            return Ok(());
        }
        if matches!(vendor, Vendor::Whisper | Vendor::Baidu) {
            self.client = Some(
                reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .connect_timeout(CONNECT_TIMEOUT)
                    .timeout(Duration::from_secs(60))
                    .build()
                    .map_err(|_| anyhow!("无法创建云端 HTTP 客户端"))?,
            );
            if vendor == Vendor::Baidu {
                self.config.access_token = self.baidu_token().await?;
            }
            self.connected = true;
            return Ok(());
        }
        let mut url = endpoint_for(&self.config, vendor)?;
        match vendor {
            Vendor::Aliyun => {
                url.query_pairs_mut()
                    .append_pair("token", self.config.access_token.trim());
            }
            Vendor::Tencent => {
                url = tencent_url(
                    &self.config,
                    chrono::Utc::now().timestamp(),
                    Uuid::new_v4().as_u128() as u32,
                    &self.task_id,
                )?;
            }
            Vendor::Iflytek => {
                url = iflytek_url(
                    &self.config,
                    &chrono::Utc::now()
                        .format("%a, %d %b %Y %H:%M:%S GMT")
                        .to_string(),
                )?;
            }
            _ => unreachable!(),
        }
        let request = url
            .as_str()
            .into_client_request()
            .map_err(|_| anyhow!("云端连接参数无效"))?;
        let limits = WebSocketConfig {
            max_message_size: Some(MAX_RESPONSE_BYTES),
            max_frame_size: Some(MAX_RESPONSE_BYTES),
            ..Default::default()
        };
        let (ws, _) = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connect_async_with_config(request, Some(limits), false),
        )
        .await
        .map_err(|_| anyhow!("云端连接超时"))?
        .map_err(|_| anyhow!("云端握手失败，请检查网络、凭证、系统时间和接口权限"))?;
        self.ws = Some(ws);
        if vendor == Vendor::Aliyun {
            self.send_message(Message::Text(
                aliyun_command(
                    &self.config,
                    &self.task_id,
                    &Uuid::new_v4().simple().to_string(),
                    true,
                )
                .to_string(),
            ))
            .await?;
        }
        if matches!(vendor, Vendor::Aliyun | Vendor::Tencent) {
            tokio::time::timeout(CONNECT_TIMEOUT, async {
                while !self.assembly.ready {
                    self.read_message().await?;
                }
                Ok::<(), anyhow::Error>(())
            })
            .await
            .map_err(|_| anyhow!("云端未确认开始识别，请检查服务开通状态"))??;
        }
        self.stream_started = Instant::now();
        self.connected = true;
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if !self.connected {
            bail!("云端连接尚未建立");
        }
        if !chunk.len().is_multiple_of(2) {
            bail!("PCM 数据必须按 16 位采样对齐");
        }
        let limit = max_recording_seconds(&self.config) as usize * BYTES_PER_SECOND;
        if chunk.len() > limit.saturating_sub(self.total_bytes) {
            bail!(
                "本接口单段音频超过 {} 秒限制，请缩短录音",
                max_recording_seconds(&self.config)
            );
        }
        self.total_bytes += chunk.len();
        if self.assembly.terminal {
            // The final event has already preserved the recognized prefix.
            // Stop the pipeline rather than silently discard later speech.
            bail!("云端已结束当前语音，请开始新录音");
        }
        self.audio.extend_from_slice(chunk);
        if matches!(self.vendor, Some(Vendor::Whisper | Vendor::Baidu)) {
            return Ok(());
        }
        let packet_size = if self.vendor == Some(Vendor::Iflytek) {
            1280
        } else {
            6400
        };
        while self.audio.len() >= packet_size {
            let packet: Vec<_> = self.audio.drain(..packet_size).collect();
            self.write_packet(&packet, false).await?;
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        if let Some(seed) = self.seed.as_mut() {
            return match seed.recv_transcript().await {
                Ok(Some(TranscriptEvent::Error { .. })) | Err(_) => {
                    Err(anyhow!("豆包语音识别失败，请检查语音资源权限、额度和网络"))
                }
                result => result,
            };
        }
        if self.ws.is_none() || self.final_delivered {
            return std::future::pending().await;
        }
        let changed = self.read_message().await?;
        if self.assembly.terminal {
            self.final_delivered = true;
            return Ok(Some(TranscriptEvent::Final {
                text: self.assembly.text(),
                confidence: 1.0,
            }));
        }
        Ok(changed.then(|| TranscriptEvent::Partial {
            text: self.assembly.text(),
        }))
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        if !self.connected {
            return Ok(None);
        }
        self.connected = false;
        if self.seed.is_some() {
            let tail = std::mem::take(&mut self.audio);
            if !tail.is_empty() {
                self.write_packet(&tail, false).await?;
            }
            let mut seed = self
                .seed
                .take()
                .ok_or_else(|| anyhow!("豆包语音连接已关闭"))?;
            return tokio::time::timeout(FINAL_TIMEOUT, seed.disconnect())
                .await
                .map_err(|_| anyhow!("豆包语音最终结果超时"))?
                .map_err(|_| anyhow!("豆包语音最终识别失败，请检查权限、额度和网络"));
        }
        let vendor = self.vendor.ok_or_else(|| anyhow!("厂商未初始化"))?;
        if matches!(vendor, Vendor::Whisper | Vendor::Baidu) {
            return self.http_transcribe(vendor).await;
        }
        if !self.assembly.terminal {
            let tail = std::mem::take(&mut self.audio);
            if !tail.is_empty() {
                self.write_packet(&tail, false).await?;
            }
            let stop = match vendor {
                Vendor::Aliyun => aliyun_command(
                    &self.config,
                    &self.task_id,
                    &Uuid::new_v4().simple().to_string(),
                    false,
                ),
                Vendor::Tencent => json!({"type":"end"}),
                Vendor::Iflytek => {
                    if self.first_frame {
                        self.write_packet(&[0; 1280], false).await?;
                    }
                    iflytek_audio(&self.config, &[], false, true)
                }
                _ => unreachable!(),
            };
            self.send_message(Message::Text(stop.to_string())).await?;
            tokio::time::timeout(FINAL_TIMEOUT, async {
                while !self.assembly.terminal {
                    self.read_message().await?;
                }
                Ok::<(), anyhow::Error>(())
            })
            .await
            .map_err(|_| anyhow!("等待云端最终结果超时；未将中间结果伪装成完整转写"))??;
        }
        if let Some(mut ws) = self.ws.take() {
            let _ = tokio::time::timeout(Duration::from_secs(1), ws.close(None)).await;
        }
        let text = self.assembly.text();
        Ok((!self.final_delivered && !text.is_empty()).then_some(text))
    }

    fn name(&self) -> &str {
        self.vendor.map(Vendor::name).unwrap_or("自定义云端接口")
    }
}

/// A real, tiny request, not merely a TCP ping. Silence may legitimately have
/// no words; this tests protocol completion, not microphone/accuracy/latency.
pub async fn benchmark(config: CustomCloudConfig) -> Result<u32> {
    let start = Instant::now();
    let mut provider = CustomCloudProvider::new(config, None, None);
    tokio::time::timeout(Duration::from_secs(90), async {
        provider.connect(&SttConfig::default()).await?;
        provider.send_audio(&[0; 6400]).await?;
        provider.disconnect().await?;
        Ok::<(), anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow!("云端接口测试超时"))??;
    Ok(start.elapsed().as_millis().min(u32::MAX as u128) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(vendor: &str) -> CustomCloudConfig {
        CustomCloudConfig {
            vendor: vendor.into(),
            app_id: "12345".into(),
            api_key: "test-key".into(),
            api_secret: "test-secret".into(),
            access_token: "test-token".into(),
            ..CustomCloudConfig::default()
        }
    }

    #[test]
    fn endpoints_do_not_send_vendor_keys_to_other_hosts() {
        for vendor in ["bytedance", "aliyun", "tencent", "iflytek", "baidu"] {
            let mut cfg = fixture(vendor);
            cfg.endpoint = "https://attacker.example/transcriptions".into();
            assert!(validate(&cfg, Vendor::parse(vendor).unwrap()).is_err());
            cfg.endpoint.clear();
            assert!(validate(&cfg, Vendor::parse(vendor).unwrap()).is_ok());
        }
    }
    #[test]
    fn whisper_requires_tls_except_loopback_and_rejects_url_secrets() {
        let mut cfg = fixture("whisper");
        for endpoint in [
            "http://example.com/v1",
            "https://user:secret@example.com/v1",
            "https://example.com/v1?key=secret",
            "https://example.com/v1#secret",
        ] {
            cfg.endpoint = endpoint.into();
            assert!(validate(&cfg, Vendor::Whisper).is_err());
        }
        for endpoint in [
            "https://example.com/v1/audio/transcriptions",
            "http://127.0.0.1:8080/v1/audio/transcriptions",
            "http://[::1]:8080/v1/audio/transcriptions",
        ] {
            cfg.endpoint = endpoint.into();
            assert!(validate(&cfg, Vendor::Whisper).is_ok());
        }
    }
    #[test]
    fn hmac_known_vectors() {
        assert_eq!(
            sign_sha1("key", "The quick brown fox jumps over the lazy dog"),
            "3nybhbi3iqa8ino29wqQcBydtNk="
        );
        assert_eq!(
            sign_sha256("key", "The quick brown fox jumps over the lazy dog"),
            "97yD9DBThCSxMpjmqm+xQ+9NWaFJRhdZl0edvC0aPNg="
        );
    }
    #[test]
    fn tencent_signs_sorted_raw_query_and_encodes_signature() {
        let cfg = fixture("tencent");
        let url = tencent_url(&cfg, 1_700_000_000, 123, "test-voice").unwrap();
        let fields: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(fields["voice_format"], "1");
        assert_eq!(fields["engine_model_type"], "16k_zh");
        let canonical = "asr.cloud.tencent.com/asr/v2/12345?engine_model_type=16k_zh&expired=1700003600&needvad=1&nonce=123&secretid=test-key&sub_service_type=1&timestamp=1700000000&voice_format=1&voice_id=test-voice";
        assert_eq!(fields["signature"], sign_sha1("test-secret", canonical));
        assert!(!url.as_str().contains("test-secret"));
    }
    #[test]
    fn iflytek_signature_has_exact_rfc1123_and_request_line() {
        let cfg = fixture("iflytek");
        let date = "Tue, 14 May 2024 08:46:48 GMT";
        let url = iflytek_url(&cfg, date).unwrap();
        let fields: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        let decoded =
            String::from_utf8(STANDARD.decode(&fields["authorization"]).unwrap()).unwrap();
        let expected = sign_sha256(
            "test-secret",
            &format!("host: iat-api.xfyun.cn\ndate: {date}\nGET /v2/iat HTTP/1.1"),
        );
        assert!(decoded.contains(&format!("signature=\"{expected}\"")));
        assert_eq!(fields["date"], date);
    }
    #[test]
    fn aliyun_start_stop_share_task_but_not_message_id() {
        let cfg = fixture("aliyun");
        let start = aliyun_command(&cfg, "task", "message1", true);
        let stop = aliyun_command(&cfg, "task", "message2", false);
        assert_eq!(start["header"]["task_id"], stop["header"]["task_id"]);
        assert_ne!(start["header"]["message_id"], stop["header"]["message_id"]);
        assert_eq!(start["payload"]["format"], "pcm");
        assert!(stop.get("payload").is_none());
    }
    #[test]
    fn iflytek_first_middle_last_frames() {
        let cfg = fixture("iflytek");
        let first = iflytek_audio(&cfg, &[1, 2], true, false);
        let middle = iflytek_audio(&cfg, &[3, 4], false, false);
        let last = iflytek_audio(&cfg, &[], false, true);
        assert_eq!(first["data"]["status"], 0);
        assert_eq!(first["data"]["audio"], "AQI=");
        assert_eq!(first["business"]["dwa"], "wpgs");
        assert_eq!(middle["data"]["status"], 1);
        assert!(middle.get("common").is_none());
        assert_eq!(last["data"]["status"], 2);
    }
    #[test]
    fn aliyun_revisions_replace_sentence_not_append_duplicate() {
        let mut state = TranscriptAssembly::default();
        for (index, text) in [(1, "今"), (1, "今天。"), (2, "你好。"), (2, "您好。")] {
            state.apply(Vendor::Aliyun,&json!({"header":{"status":20000000,"name":"TranscriptionResultChanged"},"payload":{"index":index,"result":text}})).unwrap();
        }
        assert_eq!(state.text(), "今天。您好。");
        state
            .apply(
                Vendor::Aliyun,
                &json!({"header":{"status":20000000,"name":"TranscriptionCompleted"}}),
            )
            .unwrap();
        assert!(state.terminal);
    }
    #[test]
    fn tencent_revisions_keep_sentence_order() {
        let mut state = TranscriptAssembly::default();
        for (index, text) in [(0, "你"), (0, "你好"), (1, "世界"), (0, "您好")] {
            state
                .apply(
                    Vendor::Tencent,
                    &json!({"code":0,"result":{"index":index,"voice_text_str":text}}),
                )
                .unwrap();
        }
        assert_eq!(state.text(), "您好世界");
        state
            .apply(Vendor::Tencent, &json!({"code":0,"final":1}))
            .unwrap();
        assert!(state.terminal);
    }
    #[test]
    fn iflytek_dynamic_replacement_removes_inclusive_range() {
        let mut state = TranscriptAssembly::default();
        for (sn, text) in [(0, "请"), (1, "张"), (2, "三"), (3, "确认")] {
            state.apply(Vendor::Iflytek,&json!({"code":0,"data":{"status":1,"result":{"sn":sn,"pgs":"apd","ws":[{"cw":[{"w":text}]}]}}})).unwrap();
        }
        state.apply(Vendor::Iflytek,&json!({"code":0,"data":{"status":2,"result":{"sn":4,"pgs":"rpl","rg":[1,3],"ws":[{"cw":[{"w":"张珊确认"},{"w":"not-another-chunk"}]}]}}})).unwrap();
        assert_eq!(state.text(), "请张珊确认");
        assert!(state.terminal);
    }
    #[test]
    fn server_errors_do_not_echo_untrusted_messages_or_credentials() {
        for (vendor, body) in [
            (
                Vendor::Aliyun,
                json!({"header":{"status":40000001,"status_text":"test-token"}}),
            ),
            (
                Vendor::Tencent,
                json!({"code":4001,"message":"test-secret"}),
            ),
            (Vendor::Iflytek, json!({"code":10005,"message":"test-key"})),
        ] {
            let error = TranscriptAssembly::default()
                .apply(vendor, &body)
                .unwrap_err()
                .to_string();
            assert!(!error.contains("test-"));
        }
        assert!(!format!("{:?}", fixture("tencent")).contains("test-secret"));
    }
    #[test]
    fn baidu_chooses_one_alternative_and_handles_no_speech() {
        assert_eq!(
            baidu_result(&json!({"err_no":0,"result":["你好","您好"]})).unwrap(),
            Some("你好".into())
        );
        assert_eq!(baidu_result(&json!({"err_no":3301})).unwrap(), None);
        assert!(
            baidu_result(&json!({"err_no":3302,"err_msg":"test-secret"}))
                .unwrap_err()
                .to_string()
                .contains("3302")
        );
    }
    #[test]
    fn protocol_limits_and_credential_requirements() {
        for vendor in ["iflytek", "baidu"] {
            assert_eq!(max_recording_seconds(&fixture(vendor)), 60);
        }
        assert_eq!(max_recording_seconds(&fixture("aliyun")), 600);
        let mut cfg = fixture("aliyun");
        cfg.access_token.clear();
        assert!(validate(&cfg, Vendor::Aliyun).is_err());
        let mut cfg = fixture("tencent");
        cfg.api_secret.clear();
        assert!(validate(&cfg, Vendor::Tencent).is_err());
    }
    #[tokio::test]
    async fn oversized_or_unaligned_audio_is_rejected_without_network() {
        let mut provider = CustomCloudProvider::new(fixture("baidu"), None, None);
        provider.vendor = Some(Vendor::Baidu);
        provider.connected = true;
        assert!(provider.send_audio(&[1]).await.is_err());
        assert!(provider
            .send_audio(&vec![0; 60 * BYTES_PER_SECOND + 2])
            .await
            .is_err());
        assert!(provider.audio.is_empty());
    }

    #[tokio::test]
    async fn websocket_audio_stop_and_revised_final_roundtrip() {
        for vendor in [Vendor::Aliyun, Vendor::Tencent, Vendor::Iflytek] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
                let mut audio_bytes = 0usize;
                let mut frame_count = 0usize;
                loop {
                    let message = ws.next().await.unwrap().unwrap();
                    let done = match message {
                        Message::Binary(bytes) => {
                            audio_bytes += bytes.len();
                            false
                        }
                        Message::Text(text) => {
                            let body: Value = serde_json::from_str(&text).unwrap();
                            if vendor == Vendor::Iflytek {
                                if body["data"]["status"] == 2 {
                                    true
                                } else {
                                    if frame_count == 0 {
                                        assert_eq!(body["data"]["status"], 0);
                                        assert_eq!(body["common"]["app_id"], "12345");
                                    } else {
                                        assert_eq!(body["data"]["status"], 1);
                                        assert!(body.get("common").is_none());
                                    }
                                    audio_bytes += STANDARD
                                        .decode(body["data"]["audio"].as_str().unwrap())
                                        .unwrap()
                                        .len();
                                    frame_count += 1;
                                    false
                                }
                            } else if vendor == Vendor::Aliyun {
                                assert_eq!(body["header"]["name"], "StopTranscription");
                                true
                            } else {
                                assert_eq!(body["type"], "end");
                                true
                            }
                        }
                        _ => false,
                    };
                    if done {
                        break;
                    }
                }
                assert_eq!(audio_bytes, 6400);
                let responses = match vendor {
                    Vendor::Aliyun => vec![
                        json!({"header":{"status":20000000,"name":"TranscriptionResultChanged"},"payload":{"index":1,"result":"你好"}}),
                        json!({"header":{"status":20000000,"name":"SentenceEnd"},"payload":{"index":1,"result":"您好。"}}),
                        json!({"header":{"status":20000000,"name":"TranscriptionCompleted"}}),
                    ],
                    Vendor::Tencent => vec![
                        json!({"code":0,"result":{"index":0,"voice_text_str":"你好"}}),
                        json!({"code":0,"result":{"index":0,"voice_text_str":"您好。"}}),
                        json!({"code":0,"final":1}),
                    ],
                    Vendor::Iflytek => vec![
                        json!({"code":0,"data":{"status":1,"result":{"sn":0,"pgs":"apd","ws":[{"cw":[{"w":"你好"}]}]}}}),
                        json!({"code":0,"data":{"status":2,"result":{"sn":1,"pgs":"rpl","rg":[0,0],"ws":[{"cw":[{"w":"您好。"}]}]}}}),
                    ],
                    _ => unreachable!(),
                };
                for body in responses {
                    ws.send(Message::Text(body.to_string())).await.unwrap();
                }
                let _ = ws.next().await;
            });
            let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
                .await
                .unwrap();
            // Only the test transport is replaced. Production official endpoint
            // validation remains intact and never accepts this loopback URL.
            let vendor_id = match vendor {
                Vendor::Aliyun => "aliyun",
                Vendor::Tencent => "tencent",
                _ => "iflytek",
            };
            let mut provider = CustomCloudProvider::new(fixture(vendor_id), None, None);
            provider.vendor = Some(vendor);
            provider.ws = Some(ws);
            provider.connected = true;
            provider.send_audio(&[0; 6400]).await.unwrap();
            let result = tokio::time::timeout(Duration::from_secs(3), provider.disconnect())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(result, Some("您好。".into()));
            assert_eq!(provider.disconnect().await.unwrap(), None);
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn terminal_event_is_emitted_once_and_never_silently_accepts_later_audio() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            ws.send(Message::Text(
                json!({"code":0,"final":1,"result":{"index":0,"voice_text_str":"已完成"}})
                    .to_string(),
            ))
            .await
            .unwrap();
            let _ = ws.next().await;
        });
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let mut provider = CustomCloudProvider::new(fixture("tencent"), None, None);
        provider.vendor = Some(Vendor::Tencent);
        provider.ws = Some(ws);
        provider.connected = true;
        assert!(
            matches!(provider.recv_transcript().await.unwrap(),Some(TranscriptEvent::Final{text,..}) if text == "已完成")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), provider.recv_transcript())
                .await
                .is_err()
        );
        assert!(provider.send_audio(&[0; 100]).await.is_err());
        assert_eq!(provider.disconnect().await.unwrap(), None);
        server.await.unwrap();
    }

    async fn read_mock_http(socket: &mut tokio::net::TcpStream) -> String {
        use tokio::io::AsyncReadExt;
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 8192];
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buffer[..n]);
            assert!(request.len() < 100_000);
            if let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap();
                if request.len() >= header_end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&request).into_owned()
    }

    #[tokio::test]
    async fn whisper_multipart_roundtrip_and_redirect_rejection() {
        use tokio::io::AsyncWriteExt;
        for redirect in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_mock_http(&mut socket).await;
                assert!(request.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
                assert!(request
                    .to_lowercase()
                    .contains("authorization: bearer test-key"));
                assert!(request.contains("filename=\"audio.wav\""));
                assert!(request.contains("RIFF"));
                assert!(request.contains("whisper-1"));
                assert!(!request.contains("test-secret"));
                assert!(!request.contains("test-token"));
                let body = "{\"text\":\"协议测试成功\"}";
                let response = if redirect {
                    format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{address}/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
                };
                socket.write_all(response.as_bytes()).await.unwrap();
                drop(socket);
                if redirect {
                    assert!(
                        tokio::time::timeout(Duration::from_millis(100), listener.accept())
                            .await
                            .is_err()
                    );
                }
            });
            let mut config = fixture("whisper");
            config.endpoint = format!("http://{address}/v1/audio/transcriptions");
            let mut provider = CustomCloudProvider::new(config, None, None);
            provider.connect(&SttConfig::default()).await.unwrap();
            provider.send_audio(&[0; 6400]).await.unwrap();
            let result = tokio::time::timeout(Duration::from_secs(3), provider.disconnect())
                .await
                .unwrap();
            if redirect {
                assert!(result.unwrap_err().to_string().contains("307"));
            } else {
                assert_eq!(result.unwrap(), Some("协议测试成功".into()));
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "Explicit opt-in only: sends 0.2s synthetic silence to the configured official provider and may consume paid quota"]
    async fn real_cloud_smoke_from_environment() {
        let encoded = std::env::var("POPSPEAK_TEST_CLOUD_JSON").expect(
            "Set POPSPEAK_TEST_CLOUD_JSON to a CustomCloudConfig; never print this variable",
        );
        let config: CustomCloudConfig =
            serde_json::from_str(&encoded).expect("POPSPEAK_TEST_CLOUD_JSON has invalid schema");
        let vendor = config.vendor.clone();
        let elapsed = benchmark(config)
            .await
            .expect("Official cloud protocol smoke test failed");
        eprintln!("Official cloud protocol completed: vendor={vendor}, elapsed_ms={elapsed}, synthetic_audio_ms=200");
    }
}
