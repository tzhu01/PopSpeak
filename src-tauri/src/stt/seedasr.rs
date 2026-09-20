use std::io::{Read, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Error as WebSocketError, Message},
};
use uuid::Uuid;

use super::{SttConfig, SttProvider, TranscriptEvent};

pub const DEFAULT_RESOURCE_ID: &str = "volc.seedasr.sauc.duration";
const ENDPOINT: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";
const HOST: &str = "openspeech.bytedance.com";
// The official documentation recommends 200 ms per packet for the optimized
// bidirectional endpoint: 16 kHz * 16-bit mono * 0.2 seconds.
const AUDIO_PACKET_BYTES: usize = 6_400;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Default)]
struct ParsedServerPacket {
    text: Option<String>,
    terminal: bool,
    error: Option<String>,
}

pub struct SeedAsrProvider {
    ws: Option<WsStream>,
    resource_id: String,
    audio_buffer: Vec<u8>,
    latest_text: String,
    terminal: bool,
    final_delivered: bool,
}

impl SeedAsrProvider {
    pub fn new(resource_id: impl Into<String>) -> Self {
        let resource_id = resource_id.into();
        Self {
            ws: None,
            resource_id: if resource_id.trim().is_empty() {
                DEFAULT_RESOURCE_ID.to_string()
            } else {
                resource_id
            },
            audio_buffer: Vec::with_capacity(AUDIO_PACKET_BYTES * 2),
            latest_text: String::new(),
            terminal: false,
            final_delivered: false,
        }
    }

    fn gzip(payload: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(payload)?;
        Ok(encoder.finish()?)
    }

    fn gunzip(payload: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(payload).take((MAX_RESPONSE_BYTES + 1) as u64);
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded)?;
        if decoded.len() > MAX_RESPONSE_BYTES {
            anyhow::bail!("豆包语音响应解压后超出安全长度限制");
        }
        Ok(decoded)
    }

    fn framed_payload(header: [u8; 4], payload: &[u8]) -> Result<Vec<u8>> {
        let compressed = Self::gzip(payload)?;
        let mut frame = Vec::with_capacity(8 + compressed.len());
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        frame.extend_from_slice(&compressed);
        Ok(frame)
    }

    fn build_full_client_request() -> Result<Vec<u8>> {
        let request = json!({
            "user": {
                // An installation-scoped hardware identifier is unnecessary for
                // dictation. A random UUID avoids collecting device identifiers.
                "uid": Uuid::new_v4().to_string()
            },
            "audio": {
                "format": "pcm",
                "codec": "raw",
                "rate": 16000,
                "bits": 16,
                "channel": 1
            },
            "request": {
                "model_name": "bigmodel",
                "enable_nonstream": true,
                "enable_itn": true,
                "enable_punc": true,
                "enable_ddc": false,
                "show_utterances": true,
                "result_type": "full",
                "end_window_size": 800
            }
        });
        let payload = serde_json::to_vec(&request)?;
        // v1 / 4-byte header, full-client-request, JSON + gzip.
        Self::framed_payload([0x11, 0x10, 0x11, 0x00], &payload)
    }

    fn build_audio_request(payload: &[u8], final_packet: bool) -> Result<Vec<u8>> {
        // Audio-only request with no serialization and gzip compression. A flag
        // value of 0b0010 marks the last packet without adding a sequence field.
        let flags = if final_packet { 0x02 } else { 0x00 };
        Self::framed_payload([0x11, 0x20 | flags, 0x01, 0x00], payload)
    }

    fn parse_server_packet(bytes: &[u8]) -> Result<ParsedServerPacket> {
        if bytes.len() > MAX_RESPONSE_BYTES {
            anyhow::bail!("豆包语音响应超出安全长度限制");
        }
        if bytes.len() < 4 {
            anyhow::bail!("SeedASR returned a truncated protocol header");
        }

        let header_size = usize::from(bytes[0] & 0x0f) * 4;
        if header_size < 4 || bytes.len() < header_size {
            anyhow::bail!("SeedASR returned an invalid protocol header size");
        }

        let message_type = bytes[1] >> 4;
        let flags = bytes[1] & 0x0f;
        let compression = bytes[2] & 0x0f;
        let mut offset = header_size;

        if message_type == 0x0f {
            if bytes.len() < offset + 8 {
                anyhow::bail!("SeedASR returned a truncated error frame");
            }
            let code = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
            offset += 4;
            let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
            offset += 4;
            if bytes.len() < offset + size {
                anyhow::bail!("SeedASR returned a truncated error message");
            }
            return Ok(ParsedServerPacket {
                error: Some(format!("代码 {code}，请检查语音资源权限、凭证和额度")),
                ..ParsedServerPacket::default()
            });
        }

        if message_type != 0x09 {
            return Ok(ParsedServerPacket::default());
        }

        let sequence = if matches!(flags, 0x01 | 0x03) {
            if bytes.len() < offset + 4 {
                anyhow::bail!("SeedASR returned a truncated sequence field");
            }
            let value = i32::from_be_bytes(bytes[offset..offset + 4].try_into()?);
            offset += 4;
            Some(value)
        } else {
            None
        };

        if bytes.len() < offset + 4 {
            anyhow::bail!("SeedASR returned a truncated payload length");
        }
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;
        if bytes.len() < offset + size {
            anyhow::bail!("SeedASR returned a truncated payload");
        }

        let raw = &bytes[offset..offset + size];
        let payload = if compression == 0x01 {
            Self::gunzip(raw)?
        } else {
            raw.to_vec()
        };
        let body: serde_json::Value =
            serde_json::from_slice(&payload).context("SeedASR returned malformed JSON")?;
        let text = body
            .get("result")
            .and_then(|result| result.get("text"))
            .and_then(|text| text.as_str())
            .map(str::to_owned)
            .filter(|text| !text.trim().is_empty());

        Ok(ParsedServerPacket {
            text,
            terminal: flags & 0x02 != 0 || sequence.is_some_and(|value| value < 0),
            error: None,
        })
    }

    async fn send_audio_packet(
        ws: &mut WsStream,
        payload: &[u8],
        final_packet: bool,
    ) -> Result<()> {
        let frame = Self::build_audio_request(payload, final_packet)?;
        tokio::time::timeout(Duration::from_secs(5), ws.send(Message::Binary(frame)))
            .await
            .context("豆包语音发送超时")?
            .map_err(|_| anyhow::anyhow!("豆包语音发送失败"))?;
        Ok(())
    }

    fn apply_packet(&mut self, packet: ParsedServerPacket) -> Option<TranscriptEvent> {
        if let Some(message) = packet.error {
            return Some(TranscriptEvent::Error { message });
        }
        self.terminal |= packet.terminal;
        if let Some(text) = packet.text.as_ref() {
            self.latest_text.clone_from(text);
        }
        if self.terminal {
            self.final_delivered = true;
            return Some(TranscriptEvent::Final {
                text: self.latest_text.clone(),
                confidence: 1.0,
            });
        }
        let text = packet.text?;
        // The API returns a full running transcript. Treat it as a partial here
        // and return exactly one final transcript from disconnect(), otherwise
        // the pipeline would append repeated full-text responses.
        Some(TranscriptEvent::Partial { text })
    }
}

#[async_trait]
impl SttProvider for SeedAsrProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        if config.api_key.trim().is_empty() {
            anyhow::bail!("火山引擎 API Key / Access Token 为空");
        }
        let uses_app_token = config.credential_mode == "app-token";
        if uses_app_token && config.app_id.trim().is_empty() {
            anyhow::bail!("火山引擎 APP ID 为空");
        }

        let connect_id = Uuid::new_v4().to_string();
        let request_id = Uuid::new_v4().to_string();
        let mut request_builder = http::Request::builder()
            .uri(ENDPOINT)
            .header("Host", HOST)
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .header("X-Api-Resource-Id", self.resource_id.trim())
            .header("X-Api-Connect-Id", &connect_id)
            .header("X-Api-Request-Id", &request_id)
            .header("X-Api-Sequence", "-1");

        request_builder = if uses_app_token {
            request_builder
                .header("X-Api-App-Key", config.app_id.trim())
                .header("X-Api-Access-Key", config.api_key.trim())
        } else {
            request_builder.header("X-Api-Key", config.api_key.trim())
        };
        let request = request_builder.body(())?;

        let limits = WebSocketConfig {
            max_message_size: Some(MAX_RESPONSE_BYTES),
            max_frame_size: Some(MAX_RESPONSE_BYTES),
            ..Default::default()
        };
        let (mut ws, response) = match tokio::time::timeout(
            Duration::from_secs(12),
            connect_async_with_config(request, Some(limits), false),
        )
        .await
        .context("豆包语音连接超时")?
        {
            Ok(connected) => connected,
            Err(WebSocketError::Http(response)) => {
                let status = response.status();
                let log_id = response
                    .headers()
                    .get("X-Tt-Logid")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("无");
                anyhow::bail!(
                    "豆包语音握手失败（HTTP {status}，LogID {log_id}），请检查语音资源权限和凭证"
                );
            }
            Err(_) => anyhow::bail!("豆包语音握手失败，请检查网络和系统时间"),
        };
        if let Some(log_id) = response.headers().get("X-Tt-Logid") {
            tracing::info!(
                "SeedASR connected, logid={}",
                log_id.to_str().unwrap_or("invalid")
            );
        } else {
            tracing::info!("SeedASR WebSocket connected");
        }
        tokio::time::timeout(
            Duration::from_secs(5),
            ws.send(Message::Binary(Self::build_full_client_request()?)),
        )
        .await
        .context("豆包语音初始化发送超时")?
        .map_err(|_| anyhow::anyhow!("豆包语音初始化发送失败"))?;

        self.audio_buffer.clear();
        self.latest_text.clear();
        self.terminal = false;
        self.final_delivered = false;
        self.ws = Some(ws);
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if self.ws.is_none() {
            anyhow::bail!("SeedASR WebSocket is not connected");
        }
        if self.terminal {
            anyhow::bail!("豆包语音已结束当前识别，请开始新录音");
        }
        if chunk.len() > 32_000 * 600 {
            anyhow::bail!("豆包语音单次音频缓冲超出安全长度限制");
        }
        self.audio_buffer.extend_from_slice(chunk);
        while self.audio_buffer.len() >= AUDIO_PACKET_BYTES {
            let packet: Vec<u8> = self.audio_buffer.drain(..AUDIO_PACKET_BYTES).collect();
            if let Some(ws) = &mut self.ws {
                Self::send_audio_packet(ws, &packet, false).await?;
            }
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        if self.terminal {
            return std::future::pending().await;
        }
        let message = match self.ws.as_mut() {
            Some(ws) => ws.next().await,
            None => return std::future::pending().await,
        };

        match message {
            Some(Ok(Message::Binary(bytes))) => {
                let packet = Self::parse_server_packet(&bytes)?;
                Ok(self.apply_packet(packet))
            }
            Some(Ok(Message::Ping(payload))) => {
                if let Some(ws) = &mut self.ws {
                    tokio::time::timeout(Duration::from_secs(5), ws.send(Message::Pong(payload)))
                        .await
                        .context("豆包语音心跳发送超时")?
                        .map_err(|_| anyhow::anyhow!("豆包语音心跳发送失败"))?;
                }
                Ok(None)
            }
            Some(Ok(Message::Close(_))) | None => anyhow::bail!("豆包语音提前断开，未收到最终结果"),
            Some(Ok(_)) => Ok(None),
            Some(Err(_)) => Ok(Some(TranscriptEvent::Error {
                message: "豆包语音连接中断，请检查网络".into(),
            })),
        }
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        let Some(mut ws) = self.ws.take() else {
            return Ok(None);
        };

        if !self.terminal {
            let tail = std::mem::take(&mut self.audio_buffer);
            Self::send_audio_packet(&mut ws, &tail, true).await?;
        }

        let deadline = Instant::now() + Duration::from_secs(8);
        while !self.terminal {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let incoming = match tokio::time::timeout(remaining, ws.next()).await {
                Ok(value) => value,
                Err(_) => break,
            };
            match incoming {
                Some(Ok(Message::Binary(bytes))) => {
                    let packet = Self::parse_server_packet(&bytes)?;
                    if let Some(message) = packet.error {
                        anyhow::bail!("SeedASR error: {message}");
                    }
                    if let Some(text) = packet.text {
                        self.latest_text = text;
                    }
                    if packet.terminal {
                        self.terminal = true;
                        break;
                    }
                }
                Some(Ok(Message::Ping(payload))) => {
                    tokio::time::timeout(Duration::from_secs(5), ws.send(Message::Pong(payload)))
                        .await
                        .context("豆包语音心跳发送超时")?
                        .map_err(|_| anyhow::anyhow!("豆包语音心跳发送失败"))?;
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => anyhow::bail!("豆包语音连接中断，未收到最终结果"),
                Some(Ok(_)) => {}
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), ws.close(None)).await;
        if !self.terminal {
            anyhow::bail!("豆包语音未返回最终确认，连接提前关闭或等待超时");
        }

        let text = std::mem::take(&mut self.latest_text);
        Ok((!self.final_delivered && !text.trim().is_empty()).then_some(text))
    }

    fn name(&self) -> &str {
        "火山引擎 SeedASR 2.0"
    }
}

/// Validate a new-console API key and resource grant with the smallest useful
/// streaming request. This consumes roughly 0.2 seconds of the trial quota.
pub async fn benchmark(
    api_key: &str,
    resource_id: &str,
    app_id: &str,
    credential_mode: &str,
) -> Result<u32> {
    let mut provider = SeedAsrProvider::new(resource_id);
    let config = SttConfig {
        api_key: api_key.to_string(),
        app_id: app_id.to_string(),
        credential_mode: credential_mode.to_string(),
        ..SttConfig::default()
    };
    let started = Instant::now();
    provider.connect(&config).await?;
    provider.send_audio(&vec![0u8; AUDIO_PACKET_BYTES]).await?;
    provider.disconnect().await?;
    Ok(started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_request_uses_pcm_and_second_pass() {
        let frame = SeedAsrProvider::build_full_client_request().expect("request frame");
        assert_eq!(&frame[..4], &[0x11, 0x10, 0x11, 0x00]);
        let size = u32::from_be_bytes(frame[4..8].try_into().unwrap()) as usize;
        let payload = SeedAsrProvider::gunzip(&frame[8..8 + size]).expect("payload");
        let body: serde_json::Value = serde_json::from_slice(&payload).expect("json");
        assert_eq!(body["audio"]["format"], "pcm");
        assert_eq!(body["audio"]["rate"], 16000);
        assert_eq!(body["request"]["enable_nonstream"], true);
        assert_eq!(body["request"]["result_type"], "full");
    }

    #[test]
    fn parses_gzipped_terminal_response() {
        let payload = SeedAsrProvider::gzip(
            r#"{"result":{"text":"你好，PopSpeak。"},"audio_info":{"duration":800}}"#.as_bytes(),
        )
        .unwrap();
        let mut frame = vec![0x11, 0x93, 0x11, 0x00];
        frame.extend_from_slice(&(-1i32).to_be_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);

        let parsed = SeedAsrProvider::parse_server_packet(&frame).expect("response");
        assert_eq!(parsed.text.as_deref(), Some("你好，PopSpeak。"));
        assert!(parsed.terminal);
    }

    #[test]
    fn parses_protocol_error_without_leaking_credentials() {
        let message = b"requested grant not found";
        let mut frame = vec![0x11, 0xf0, 0x00, 0x00];
        frame.extend_from_slice(&45000030u32.to_be_bytes());
        frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
        frame.extend_from_slice(message);
        let parsed = SeedAsrProvider::parse_server_packet(&frame).expect("error frame");
        assert_eq!(
            parsed.error.as_deref(),
            Some("代码 45000030，请检查语音资源权限、凭证和额度")
        );
    }

    #[test]
    fn bounded_decompression_and_terminal_flag_without_sequence() {
        let compressed = SeedAsrProvider::gzip(&vec![0; MAX_RESPONSE_BYTES + 1]).unwrap();
        assert!(SeedAsrProvider::gunzip(&compressed).is_err());
        let body = r#"{"result":{"text":"完成"}}"#.as_bytes();
        let mut frame = vec![0x11, 0x92, 0x10, 0x00];
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(body);
        assert!(
            SeedAsrProvider::parse_server_packet(&frame)
                .unwrap()
                .terminal
        );
    }

    #[tokio::test]
    async fn early_close_is_not_a_successful_final_result() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            let _ = ws.next().await;
            ws.close(None).await.unwrap();
        });
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}"))
            .await
            .unwrap();
        let mut provider = SeedAsrProvider::new(DEFAULT_RESOURCE_ID);
        provider.ws = Some(ws);
        provider.latest_text = "未完成的中间结果".into();
        assert!(provider.disconnect().await.is_err());
        server.await.unwrap();
    }
}
