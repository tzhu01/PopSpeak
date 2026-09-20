use anyhow::Result;
use async_trait::async_trait;
use std::io::Write;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use super::whisper_compat::WhisperCompatProvider;
use super::{SttConfig, SttProvider, TranscriptEvent};

/// Covers the UI's five-minute limit at 16 kHz mono, with a small margin.
const MAX_AUDIO_BYTES: usize = 10 * 1024 * 1024;

/// Local whisper.cpp provider. Calls the whisper-cli binary bundled with the app.
/// GPU inference is explicitly disabled so behavior is predictable on Windows.
/// Supports LoRA adapters for dialect fine-tuning.
pub struct LocalWhisperProvider {
    cli_path: String,
    model_path: String,
    lora_path: Option<String>,
    hotwords: Vec<String>,
    stt_config: Option<SttConfig>,
    audio_buffer: Vec<u8>,
}

impl LocalWhisperProvider {
    pub fn new(cli_path: String, model_path: String, lora_path: Option<String>) -> Self {
        Self {
            cli_path,
            model_path,
            lora_path,
            hotwords: Vec::new(),
            stt_config: None,
            audio_buffer: Vec::new(),
        }
    }

    pub fn with_hotwords(mut self, hotwords: Vec<String>) -> Self {
        self.hotwords = super::hotwords::select_hotwords(&hotwords);
        self
    }

    fn initial_prompt(lang: &str, hotwords: &[String]) -> String {
        let words = super::hotwords::select_hotwords(hotwords);
        let mut prompt = if matches!(lang, "auto" | "zh" | "multi") {
            "以下是普通话简体中文对话内容，可能包含少量英文技术词汇。".to_string()
        } else {
            String::new()
        };
        if !words.is_empty() {
            prompt.push_str(" Vocabulary: ");
            prompt.push_str(&words.join(", "));
            prompt.push('.');
        }
        prompt
    }
}

#[async_trait]
impl SttProvider for LocalWhisperProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        self.stt_config = Some(config.clone());
        self.audio_buffer.clear();
        tracing::info!(
            "LocalWhisper model: {}, cli: {}",
            self.model_path,
            self.cli_path
        );
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if self.audio_buffer.len() + chunk.len() > MAX_AUDIO_BYTES {
            anyhow::bail!("LocalWhisper: audio exceeds maximum length (~5 min)");
        }
        self.audio_buffer.extend_from_slice(chunk);
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        // Batch providers must stay pending here; returning `None` immediately
        // would make the pipeline select loop consume an entire CPU core.
        std::future::pending::<Result<Option<TranscriptEvent>>>().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        let config = match &self.stt_config {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        if self.audio_buffer.is_empty() {
            tracing::info!("LocalWhisper: no audio buffered, skipping");
            return Ok(None);
        }

        let prepared_audio = crate::audio::preprocess::prepare_pcm_i16(
            &self.audio_buffer,
            config.sample_rate,
            config.vad_enabled,
            config.noise_suppression_enabled,
        );
        if prepared_audio.is_empty() {
            self.audio_buffer.clear();
            return Ok(None);
        }
        let audio_len_secs = prepared_audio.len() as f64 / (config.sample_rate as f64 * 2.0);
        tracing::info!("LocalWhisper: transcribing {:.1}s of audio", audio_len_secs);

        let wav_data = WhisperCompatProvider::build_wav(&prepared_audio, config.sample_rate);
        self.audio_buffer.clear();

        let mut temp_file = tempfile::NamedTempFile::with_suffix(".wav")?;
        temp_file.write_all(&wav_data)?;
        let temp_path = temp_file.into_temp_path();

        let lang = match &config.language {
            Some(l) if l == "multi" => "auto",
            Some(l) => l.as_str(),
            None => "auto",
        };
        let cpu_threads = std::thread::available_parallelism()
            .map(|parallelism| (parallelism.get() / 2).clamp(2, 4))
            .unwrap_or(2);

        let mut cmd = Command::new(&self.cli_path);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                                        // whisper-cli.exe on Windows depends on ggml.dll / whisper.dll / libopenblas.dll
                                        // sitting next to it. We add the exe's directory to PATH *and* use it as the
                                        // working dir so DLL resolution works regardless of where the exe lives (dev
                                        // bin, Tauri sidecar location, or a bundled resources subfolder).
        if let Some(parent) = std::path::Path::new(&self.cli_path).parent() {
            cmd.current_dir(parent);
            let existing = std::env::var_os("PATH").unwrap_or_default();
            let mut extra = std::ffi::OsString::new();
            extra.push(parent);
            if !existing.is_empty() {
                #[cfg(windows)]
                extra.push(";");
                #[cfg(not(windows))]
                extra.push(":");
                extra.push(&existing);
            }
            cmd.env("PATH", extra);
        }
        cmd.arg("-m")
            .arg(&self.model_path)
            .arg("-f")
            .arg(&temp_path)
            .arg("-l")
            .arg(lang)
            .arg("--threads")
            .arg(cpu_threads.to_string())
            .arg("--no-gpu")
            .arg("--no-timestamps")
            .arg("--no-fallback");

        // Whisper's `initial_prompt` biases the decoder toward matching writing style.
        // Feeding it simplified-Chinese sample text steers it away from the traditional
        // characters it tends to emit for zh/auto by default. Only apply this when the
        // user hasn't picked a specific non-Chinese language.
        let initial_prompt = Self::initial_prompt(lang, &self.hotwords);
        if !initial_prompt.is_empty() {
            cmd.arg("--prompt").arg(initial_prompt);
        }

        if let Some(ref lora) = self.lora_path {
            if !lora.is_empty() && std::path::Path::new(lora).exists() {
                cmd.arg("--lora").arg(lora);
                tracing::info!("LocalWhisper: using LoRA adapter: {}", lora);
            }
        }

        let output = cmd.output().map_err(|e| {
            // Temp file is cleaned up automatically when temp_path is dropped
            anyhow::anyhow!("whisper-cli not found or failed to start: {}", e)
        })?;

        // Explicitly keep the temp file alive until after the subprocess finishes,
        // then let it be deleted automatically on drop.
        drop(temp_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("whisper-cli failed: {}", stderr);
            anyhow::bail!("whisper-cli error: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let text: String = stdout
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();

        // Post-process: Traditional → Simplified. Whisper models trained on
        // zh/multi often leak 5-10% Traditional characters despite the prompt.
        let text = super::t2s::to_simplified(&text);

        tracing::info!("LocalWhisper transcription: {} chars", text.len());

        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }

    fn name(&self) -> &str {
        "LocalWhisper"
    }
}

#[cfg(test)]
mod tests {
    use super::LocalWhisperProvider;

    #[test]
    fn vocabulary_enters_initial_decoder_prompt_not_transcript_replacement() {
        let terms = vec!["PopSpeak".into(), "张珊".into(), "<|system|>".into()];
        let prompt = LocalWhisperProvider::initial_prompt("zh", &terms);
        assert!(prompt.contains("简体中文"));
        assert!(prompt.contains("PopSpeak, 张珊"));
        assert!(!prompt.contains("<|system|>"));
        assert!(!LocalWhisperProvider::initial_prompt("en", &terms).contains("简体中文"));
        assert!(LocalWhisperProvider::initial_prompt("en", &[]).is_empty());
    }
}
