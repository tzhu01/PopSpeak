use anyhow::Result;
use async_trait::async_trait;

use super::funasr_manager::FunAsrPaths;
use super::funasr_runtime::FunAsrRuntime;
use super::{SttConfig, SttProvider, TranscriptEvent};

/// Five minutes of mono 16 kHz PCM16, plus a conservative margin.
const MAX_AUDIO_BYTES: usize = 10 * 1024 * 1024;

pub struct FunAsrNanoProvider {
    paths: FunAsrPaths,
    runtime: FunAsrRuntime,
    threads: u32,
    hotwords: Vec<String>,
    stt_config: Option<SttConfig>,
    audio_buffer: Vec<u8>,
}

impl FunAsrNanoProvider {
    pub fn new(paths: FunAsrPaths, runtime: FunAsrRuntime, threads: u32) -> Self {
        Self {
            paths,
            runtime,
            threads: threads.clamp(1, 16),
            hotwords: Vec::new(),
            stt_config: None,
            audio_buffer: Vec::new(),
        }
    }

    pub fn with_hotwords(mut self, hotwords: Vec<String>) -> Self {
        self.hotwords = super::hotwords::select_hotwords(&hotwords);
        self
    }

    fn pad_pcm_for_model(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
        // The native encoder is noticeably more reliable on short utterances
        // when speech does not start at sample zero. PopSpeak's own VAD already
        // preserves 300 ms of captured pre-roll, but an immediate hotkey press
        // can legitimately contain less than that. Add deterministic silence
        // without throwing away any captured samples.
        let silence_bytes = (sample_rate as usize * 200 / 1_000) * 2;
        let mut padded = Vec::with_capacity(pcm.len() + silence_bytes * 2);
        padded.resize(silence_bytes, 0);
        padded.extend_from_slice(pcm);
        padded.resize(padded.len() + silence_bytes, 0);
        padded
    }
}

#[async_trait]
impl SttProvider for FunAsrNanoProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        if !self.paths.ready {
            anyhow::bail!(
                "Fun-ASR-Nano 离线组件不完整（运行时={}、编码器={}、Qwen3={}、VAD={}），请使用完整离线包或重新安装",
                self.paths.runtime_ready,
                self.paths.encoder_ready,
                self.paths.llm_ready,
                self.paths.vad_ready
            );
        }
        self.runtime.start(self.paths.clone(), self.threads).await?;
        self.stt_config = Some(config.clone());
        self.audio_buffer.clear();
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        if self.audio_buffer.len() + chunk.len() > MAX_AUDIO_BYTES {
            anyhow::bail!("Fun-ASR-Nano：录音超过五分钟上限");
        }
        self.audio_buffer.extend_from_slice(chunk);
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        std::future::pending::<Result<Option<TranscriptEvent>>>().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        let Some(config) = self.stt_config.clone() else {
            return Ok(None);
        };
        if self.audio_buffer.is_empty() {
            return Ok(None);
        }

        let prepared_audio = crate::audio::preprocess::prepare_pcm_i16(
            &self.audio_buffer,
            config.sample_rate,
            config.vad_enabled,
            config.noise_suppression_enabled,
        );
        self.audio_buffer.clear();
        if prepared_audio.is_empty() {
            return Ok(None);
        }

        let seconds = prepared_audio.len() as f64 / (config.sample_rate as f64 * 2.0);
        let model_audio = Self::pad_pcm_for_model(&prepared_audio, config.sample_rate);
        let text = self
            .runtime
            .transcribe(
                self.paths.clone(),
                model_audio,
                seconds >= 20.0,
                self.threads,
                self.hotwords.clone(),
            )
            .await?;
        let text = super::t2s::to_simplified(text.trim());
        tracing::info!(
            "Fun-ASR-Nano transcribed {:.1}s into {} chars",
            seconds,
            text.len()
        );
        if text.is_empty() {
            tracing::warn!("Fun-ASR-Nano returned no text for {:.1}s audio", seconds);
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }

    fn name(&self) -> &str {
        "Fun-ASR-Nano GGUF"
    }
}

#[cfg(test)]
mod tests {
    use super::{FunAsrNanoProvider, FunAsrPaths};
    use crate::stt::{SttConfig, SttProvider};

    #[test]
    fn model_padding_keeps_all_audio_and_adds_200ms_each_side() {
        let pcm = vec![1u8, 2, 3, 4];
        let padded = FunAsrNanoProvider::pad_pcm_for_model(&pcm, 16_000);
        let silence_bytes = 16_000 / 5 * 2;

        assert_eq!(padded.len(), pcm.len() + silence_bytes * 2);
        assert!(padded[..silence_bytes].iter().all(|byte| *byte == 0));
        assert_eq!(&padded[silence_bytes..silence_bytes + pcm.len()], &pcm);
        assert!(padded[silence_bytes + pcm.len()..]
            .iter()
            .all(|byte| *byte == 0));
    }

    #[tokio::test]
    #[ignore = "requires the packaged Fun-ASR models and a real WAV fixture"]
    async fn packaged_runtime_transcribes_real_wav() {
        let resource_root = std::env::var("POPSPEAK_FUNASR_RESOURCE_ROOT")
            .expect("set POPSPEAK_FUNASR_RESOURCE_ROOT to the portable package root");
        let fixture = std::env::var("POPSPEAK_FUNASR_TEST_WAV")
            .expect("set POPSPEAK_FUNASR_TEST_WAV to a 16 kHz mono PCM WAV");
        let resource_root = std::path::PathBuf::from(resource_root);
        let generic_runtime = resource_root
            .join("runtimes")
            .join("funasr")
            .join("llama-funasr-pipe-host.exe");
        let avx2_runtime = resource_root
            .join("runtimes")
            .join("funasr")
            .join("llama-funasr-pipe-host-avx2.exe");
        let runtime = if avx2_runtime.is_file() {
            avx2_runtime
        } else {
            generic_runtime
        };
        let model_dir = resource_root.join("models").join("funasr-nano");
        let paths = FunAsrPaths {
            runtime_path: runtime.to_string_lossy().into_owned(),
            runtime_variant: "test".to_string(),
            model_dir: model_dir.to_string_lossy().into_owned(),
            encoder_path: model_dir
                .join("funasr-encoder-f16.gguf")
                .to_string_lossy()
                .into_owned(),
            llm_path: model_dir
                .join("qwen3-0.6b-q4km.gguf")
                .to_string_lossy()
                .into_owned(),
            vad_path: model_dir
                .join("fsmn-vad.gguf")
                .to_string_lossy()
                .into_owned(),
            runtime_ready: true,
            encoder_ready: true,
            llm_ready: true,
            vad_ready: true,
            ready: true,
            ..FunAsrPaths::default()
        };

        let mut wav = hound::WavReader::open(fixture).expect("open WAV fixture");
        let spec = wav.spec();
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.channels, 1);
        let pcm = wav
            .samples::<i16>()
            .flat_map(|sample| sample.expect("read PCM sample").to_le_bytes())
            .collect::<Vec<_>>();

        let mut provider = FunAsrNanoProvider::new(
            paths,
            crate::stt::funasr_runtime::FunAsrRuntime::default(),
            4,
        );
        provider
            .connect(&SttConfig {
                vad_enabled: false,
                noise_suppression_enabled: false,
                ..SttConfig::default()
            })
            .await
            .expect("connect provider");
        provider.send_audio(&pcm).await.expect("buffer fixture");
        let transcript = provider
            .disconnect()
            .await
            .expect("run packaged inference")
            .expect("non-empty transcript");

        assert!(!transcript.trim().is_empty(), "transcript was empty");
    }
}
