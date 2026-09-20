use anyhow::Result;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::whisper_compat::WhisperCompatProvider;
use super::{SttConfig, SttProvider, TranscriptEvent};

/// Xiaomi MiMo audio understanding provider.
/// Uses chat/completions with input_audio payload and returns extracted text.
pub struct XiaomiMimoProvider {
    stt_config: Option<SttConfig>,
    audio_buffer: Vec<u8>,
    client: reqwest::Client,
    endpoint: String,
    model: String,
}

/// Max audio buffer: ~24 MB PCM ≈ 12.5 min at 16kHz 16-bit mono.
const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024;

pub fn normalize_xiaomi_endpoint(base_or_endpoint: &str) -> String {
    let base = base_or_endpoint.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{}/chat/completions", base)
    }
}

impl XiaomiMimoProvider {
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            stt_config: None,
            audio_buffer: Vec::new(),
            client: reqwest::Client::new(),
            endpoint,
            model,
        }
    }

    pub fn with_client(endpoint: String, model: String, client: reqwest::Client) -> Self {
        Self {
            stt_config: None,
            audio_buffer: Vec::new(),
            client,
            endpoint,
            model,
        }
    }
}

#[async_trait]
impl SttProvider for XiaomiMimoProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        if config.api_key.is_empty() {
            anyhow::bail!("Xiaomi MiMo API key is empty");
        }
        self.stt_config = Some(config.clone());
        self.audio_buffer.clear();
        tracing::info!("Xiaomi MiMo provider ready (buffering mode)");
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if self.audio_buffer.len() + chunk.len() > MAX_AUDIO_BYTES {
            anyhow::bail!("Xiaomi MiMo: audio exceeds maximum length (~12 min)");
        }
        self.audio_buffer.extend_from_slice(chunk);
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        std::future::pending::<Result<Option<TranscriptEvent>>>().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        let config = match &self.stt_config {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        if self.audio_buffer.is_empty() {
            tracing::info!("Xiaomi MiMo: no audio buffered, skipping");
            return Ok(None);
        }

        let audio_len_secs = self.audio_buffer.len() as f64 / (config.sample_rate as f64 * 2.0);
        let wav_data = WhisperCompatProvider::build_wav(&self.audio_buffer, config.sample_rate);
        self.audio_buffer.clear();

        let data_uri = format!("data:audio/wav;base64,{}", STANDARD.encode(wav_data));
        let lang_hint = match &config.language {
            Some(lang) if !lang.is_empty() && lang != "multi" => {
                format!(
                    "请将以下音频转写为文字，语言为{}。只输出转写文本，不要解释。",
                    lang
                )
            }
            _ => "请将以下音频转写为文字。只输出转写文本，不要解释。".to_string(),
        };

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "input_audio",
                            "input_audio": { "data": data_uri }
                        },
                        {
                            "type": "text",
                            "text": lang_hint
                        }
                    ]
                }
            ],
            "max_completion_tokens": 1024
        });

        tracing::info!(
            "Xiaomi MiMo: sending {:.1}s of audio for transcription",
            audio_len_secs
        );

        let resp = self
            .client
            .post(&self.endpoint)
            .header("api-key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await?;

        let status = resp.status();
        let body = resp.text().await?;

        if !status.is_success() {
            let truncate_at = body
                .char_indices()
                .take_while(|&(i, _)| i < 200)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(body.len());
            let sanitized = &body[..truncate_at];
            tracing::error!("Xiaomi MiMo HTTP {}: {}", status, sanitized);
            anyhow::bail!("Xiaomi MiMo error ({}): {}", status, sanitized);
        }

        let v: serde_json::Value = serde_json::from_str(&body)?;
        let message = &v["choices"][0]["message"];
        let text = message["content"].as_str().unwrap_or("").trim().to_string();
        let final_text = if text.is_empty() {
            message["reasoning_content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string()
        } else {
            text
        };

        tracing::info!("Xiaomi MiMo transcription: {} chars", final_text.len());

        if final_text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(final_text))
        }
    }

    fn name(&self) -> &str {
        "Xiaomi MiMo"
    }
}
