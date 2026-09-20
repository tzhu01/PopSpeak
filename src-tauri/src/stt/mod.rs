pub mod assemblyai;
pub mod cloud;
pub mod custom_cloud;
pub mod deepgram;
pub mod funasr_manager;
pub mod funasr_nano;
pub mod funasr_runtime;
pub mod hotword_replacer;
pub mod hotwords;
pub mod local_whisper;
pub mod model_manager;
pub mod native_asr;
pub mod native_asr_manager;
pub mod seedasr;
pub mod sensevoice;
pub mod sensevoice_manager;
pub mod t2s;
pub mod whisper_compat;
pub mod xiaomi_mimo;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use whisper_compat::{WhisperCompatConfig, WhisperCompatProvider};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub api_key: String,
    pub credential_mode: String,
    pub app_id: String,
    pub language: Option<String>,
    pub smart_format: bool,
    pub sample_rate: u32,
    pub vad_enabled: bool,
    pub noise_suppression_enabled: bool,
    /// Optional display-only offline previews. Never used as final transcripts.
    #[serde(default = "default_live_preview_enabled")]
    pub live_preview_enabled: bool,
}

fn default_live_preview_enabled() -> bool {
    true
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            credential_mode: "api-key".to_string(),
            app_id: String::new(),
            language: None,
            smart_format: true,
            sample_rate: 16000,
            vad_enabled: true,
            noise_suppression_enabled: true,
            live_preview_enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TranscriptEvent {
    Partial { text: String },
    Final { text: String, confidence: f32 },
    SpeechStarted,
    SpeechEnded,
    Error { message: String },
}

#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn connect(&mut self, config: &SttConfig) -> Result<()>;
    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()>;
    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>>;
    /// Disconnect and optionally return a final transcript (for file-based providers).
    async fn disconnect(&mut self) -> Result<Option<String>>;
    fn name(&self) -> &str;
}

pub fn requires_api_key(provider_name: &str) -> bool {
    !matches!(
        provider_name,
        "cloud" | "local-whisper" | "sensevoice" | "funasr-nano" | "native-asr" | "custom-whisper"
    )
}

struct UnavailableSttProvider {
    provider_name: &'static str,
    message: String,
}

#[async_trait]
impl SttProvider for UnavailableSttProvider {
    async fn connect(&mut self, _config: &SttConfig) -> Result<()> {
        anyhow::bail!(self.message.clone())
    }

    async fn send_audio(&mut self, _chunk: &[u8]) -> Result<()> {
        anyhow::bail!(self.message.clone())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        Ok(None)
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        Ok(None)
    }

    fn name(&self) -> &str {
        self.provider_name
    }
}

fn unavailable(provider_name: &'static str, message: impl Into<String>) -> Box<dyn SttProvider> {
    Box::new(UnavailableSttProvider {
        provider_name,
        message: message.into(),
    })
}

// Provider construction mirrors the persisted STT settings. The explicit fields
// avoid hiding provider-specific security-sensitive endpoints and model paths.
#[allow(clippy::too_many_arguments)]
pub fn create_provider(
    provider_name: &str,
    client: Option<reqwest::Client>,
    custom_stt_base_url: Option<&str>,
    custom_stt_model: Option<&str>,
    whisper_cli_path: Option<&str>,
    whisper_model_path: Option<&str>,
    whisper_lora_path: Option<&str>,
    app_handle: Option<&tauri::AppHandle>,
    sensevoice_language: Option<&str>,
    sensevoice_num_threads: Option<i32>,
    sensevoice_model_dir: Option<&str>,
    funasr_model_dir: Option<&str>,
    funasr_num_threads: Option<u32>,
    hotwords: Option<Vec<String>>,
    custom_cloud_config: Option<&custom_cloud::CustomCloudConfig>,
    native_asr_config: Option<&native_asr::NativeAsrConfig>,
) -> Box<dyn SttProvider> {
    if provider_name != "native-asr" {
        native_asr::release_native_cache_if_idle();
    }
    let make = |cfg: WhisperCompatConfig| -> Box<dyn SttProvider> {
        match client {
            Some(ref c) => Box::new(WhisperCompatProvider::with_client(cfg, c.clone())),
            None => Box::new(WhisperCompatProvider::new(cfg)),
        }
    };
    match provider_name {
        "native-asr" => match app_handle {
            Some(app) => Box::new(native_asr::NativeAsrProvider::new(
                app.clone(),
                native_asr_config.cloned().unwrap_or_default(),
            )),
            None => unavailable("native-asr", "原生识别无法访问本地资源目录"),
        },
        "cloud" => {
            let api_base_url = crate::api_base_url();
            match client {
                Some(ref c) => Box::new(cloud::CloudSttProvider::with_client(
                    api_base_url,
                    c.clone(),
                )),
                None => Box::new(cloud::CloudSttProvider::new(api_base_url)),
            }
        }
        "assemblyai" => Box::new(assemblyai::AssemblyAiProvider::new()),
        "volcengine-seedasr" => Box::new(seedasr::SeedAsrProvider::new(
            custom_stt_model
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(seedasr::DEFAULT_RESOURCE_ID),
        )),
        "glm-asr" => make(WhisperCompatConfig {
            provider_name: "GLM-ASR".to_string(),
            endpoint: "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions".to_string(),
            model: "glm-asr-2512".to_string(),
            extra_fields: vec![("stream".to_string(), "false".to_string())],
        }),
        "openai-whisper" => make(WhisperCompatConfig {
            provider_name: "OpenAI Whisper".to_string(),
            endpoint: "https://api.openai.com/v1/audio/transcriptions".to_string(),
            model: "whisper-1".to_string(),
            extra_fields: vec![],
        }),
        "groq-whisper" => make(WhisperCompatConfig {
            provider_name: "Groq Whisper".to_string(),
            endpoint: "https://api.groq.com/openai/v1/audio/transcriptions".to_string(),
            model: "whisper-large-v3-turbo".to_string(),
            extra_fields: vec![],
        }),
        "siliconflow" => make(WhisperCompatConfig {
            provider_name: "SiliconFlow".to_string(),
            endpoint: "https://api.siliconflow.cn/v1/audio/transcriptions".to_string(),
            model: "FunAudioLLM/SenseVoiceSmall".to_string(),
            extra_fields: vec![],
        }),
        "custom-whisper" => Box::new(custom_cloud::CustomCloudProvider::new(
            custom_cloud_config.cloned().unwrap_or_default(),
            custom_stt_base_url,
            custom_stt_model,
        )),
        "xiaomi-mimo" => {
            let endpoint = xiaomi_mimo::normalize_xiaomi_endpoint(
                custom_stt_base_url
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or("https://token-plan-cn.xiaomimimo.com/v1"),
            );
            let model = custom_stt_model
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("mimo-v2.5")
                .to_string();
            match client {
                Some(ref c) => Box::new(xiaomi_mimo::XiaomiMimoProvider::with_client(
                    endpoint,
                    model,
                    c.clone(),
                )),
                None => Box::new(xiaomi_mimo::XiaomiMimoProvider::new(endpoint, model)),
            }
        }
        "local-whisper" => {
            let configured_cli = whisper_cli_path.unwrap_or("");
            let configured_model = whisper_model_path.unwrap_or("");
            let (cli, model) = match app_handle {
                Some(app) => (
                    model_manager::resolve_cli_path(app, configured_cli),
                    model_manager::resolve_model_path(app, configured_model),
                ),
                None => (
                    if configured_cli.is_empty() {
                        "whisper-cli".to_string()
                    } else {
                        configured_cli.to_string()
                    },
                    if configured_model.is_empty() {
                        "models/ggml-tiny.bin".to_string()
                    } else {
                        configured_model.to_string()
                    },
                ),
            };
            let lora = whisper_lora_path
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string());
            Box::new(
                local_whisper::LocalWhisperProvider::new(cli, model, lora)
                    .with_hotwords(hotwords.unwrap_or_default()),
            )
        }
        "funasr-nano" => match app_handle {
            Some(app) => {
                let paths = funasr_manager::paths(app, funasr_model_dir).unwrap_or_else(|error| {
                    tracing::error!("Fun-ASR-Nano path resolution failed: {error}");
                    funasr_manager::FunAsrPaths::default()
                });
                let runtime = app.state::<funasr_runtime::FunAsrRuntime>().inner().clone();
                Box::new(
                    funasr_nano::FunAsrNanoProvider::new(
                        paths,
                        runtime,
                        funasr_num_threads.unwrap_or(4),
                    )
                    .with_hotwords(hotwords.unwrap_or_default()),
                )
            }
            None => {
                tracing::error!("Fun-ASR-Nano requires app handle");
                unavailable("Fun-ASR-Nano GGUF", "Fun-ASR-Nano 无法访问本地资源目录")
            }
        },
        "sensevoice" => match app_handle {
            Some(app) => {
                let lang = sensevoice_language.unwrap_or("auto").to_string();
                let threads = sensevoice_num_threads.unwrap_or(2);
                let dir = sensevoice_model_dir.and_then(|s| {
                    if s.trim().is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                });
                // This runtime's optional replacer is not decoder hotword support.
                // Text correction is performed separately by the pipeline.
                let words = Vec::new();
                Box::new(sensevoice::SenseVoiceProvider::new(
                    app.clone(),
                    lang,
                    threads,
                    dir,
                    words,
                ))
            }
            None => {
                tracing::error!("SenseVoice requires app handle");
                unavailable("SenseVoice", "SenseVoice 无法访问本地资源目录")
            }
        },
        _ => unavailable("Unknown STT", format!("未知语音识别引擎：{provider_name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::requires_api_key;

    #[test]
    fn offline_providers_never_require_api_keys() {
        for provider in ["local-whisper", "sensevoice", "funasr-nano", "native-asr"] {
            assert!(!requires_api_key(provider), "{provider}");
        }
        assert!(requires_api_key("deepgram"));
    }
}
