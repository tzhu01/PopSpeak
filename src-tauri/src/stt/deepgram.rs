use anyhow::Result;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::{SttConfig, SttProvider, TranscriptEvent};

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub struct DeepgramProvider {
    ws: Option<WsStream>,
}

impl Default for DeepgramProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DeepgramProvider {
    pub fn new() -> Self {
        Self { ws: None }
    }

    fn build_url(config: &SttConfig) -> String {
        let lang = config.language.as_deref().unwrap_or("multi");
        format!(
            "wss://api.deepgram.com/v1/listen?\
             model=nova-3&\
             smart_format={}&\
             language={}&\
             punctuate=true&\
             utterances=true&\
             interim_results=true&\
             endpointing=150&\
             encoding=linear16&\
             sample_rate={}&\
             channels=1",
            config.smart_format, lang, config.sample_rate
        )
    }
}

#[async_trait]
impl SttProvider for DeepgramProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        let url = Self::build_url(config);
        let request = http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Token {}", config.api_key))
            .header("Host", "api.deepgram.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        let (ws, _) = connect_async(request).await?;
        self.ws = Some(ws);
        tracing::info!("Deepgram WebSocket connected");
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if let Some(ws) = &mut self.ws {
            ws.send(Message::Binary(chunk.to_vec())).await?;
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        let ws = match &mut self.ws {
            Some(ws) => ws,
            None => return Ok(None),
        };

        match ws.next().await {
            Some(Ok(Message::Text(text))) => {
                let v: serde_json::Value = serde_json::from_str(&text)?;

                // Check for error
                if v.get("type").and_then(|t| t.as_str()) == Some("Error") {
                    let msg = v["message"].as_str().unwrap_or("Unknown error").to_string();
                    return Ok(Some(TranscriptEvent::Error { message: msg }));
                }

                // Parse transcript
                let transcript = v["channel"]["alternatives"][0]["transcript"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();

                if transcript.is_empty() {
                    return Ok(None);
                }

                let is_final = v["is_final"].as_bool().unwrap_or(false);
                let speech_final = v["speech_final"].as_bool().unwrap_or(false);

                if is_final {
                    let confidence = v["channel"]["alternatives"][0]["confidence"]
                        .as_f64()
                        .unwrap_or(0.0) as f32;

                    if speech_final {
                        return Ok(Some(TranscriptEvent::SpeechEnded));
                    }

                    Ok(Some(TranscriptEvent::Final {
                        text: transcript,
                        confidence,
                    }))
                } else {
                    Ok(Some(TranscriptEvent::Partial { text: transcript }))
                }
            }
            Some(Ok(Message::Close(_))) => {
                tracing::info!("Deepgram WebSocket closed");
                Ok(None)
            }
            Some(Err(e)) => {
                tracing::error!("Deepgram WebSocket error: {}", e);
                Ok(Some(TranscriptEvent::Error {
                    message: e.to_string(),
                }))
            }
            _ => Ok(None),
        }
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        if let Some(ws) = &mut self.ws {
            let close_msg = serde_json::json!({"type": "CloseStream"});
            let _ = ws.send(Message::Text(close_msg.to_string())).await;
            let _ = ws.close(None).await;
        }
        self.ws = None;
        tracing::info!("Deepgram disconnected");
        Ok(None)
    }

    fn name(&self) -> &str {
        "Deepgram Nova-3"
    }
}
