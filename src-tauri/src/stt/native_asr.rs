//! CPU-only GGUF ASR. All models use real native decoding; this adapter deliberately
//! presents final-only offline results, not simulated streaming or text replacement.
use super::{native_asr_manager as manager, SttConfig, SttProvider, TranscriptEvent};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Manager};
use transcribe_cpp::{
    Backend, CancelToken, Model, ModelOptions, RunOptions, SessionOptions, TimestampKind,
};

const SAMPLE_RATE: usize = 16_000;
const MAX_RECORDING_BYTES: usize = SAMPLE_RATE * 2 * 600;
const CHUNK_SECONDS: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NativeAsrConfig {
    pub model_id: String,
    /// Installation root; the manager adds an allowlisted model-id subdirectory.
    pub model_dir: String,
    pub num_threads: u32,
}

impl Default for NativeAsrConfig {
    fn default() -> Self {
        Self {
            model_id: "qwen3-asr-1.7b".into(),
            model_dir: String::new(),
            num_threads: 4,
        }
    }
}

struct CachedModel {
    path: PathBuf,
    modified: Option<SystemTime>,
    bytes: u64,
    model: Model,
}

fn cache() -> &'static Mutex<Option<CachedModel>> {
    static CACHE: OnceLock<Mutex<Option<CachedModel>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Selecting a different provider should not retain a multi-GB unused GGUF.
/// Never block an active/aborting native worker on the caller's UI task.
pub fn release_native_cache_if_idle() {
    match cache().try_lock() {
        Ok(mut guard) => *guard = None,
        Err(TryLockError::Poisoned(error)) => *error.into_inner() = None,
        Err(TryLockError::WouldBlock) => {}
    }
}

/// Downloads/deletes never race a native run or unmap an in-use file.
pub fn invalidate_cache_for(path: &str) -> Result<()> {
    with_model_unloaded(path, || Ok(()))
}

/// Keep the cache lock across the filesystem change. Otherwise a new inference
/// could reopen the model in the gap between invalidation and rename/delete.
pub fn with_model_unloaded<T>(path: &str, change: impl FnOnce() -> Result<T>) -> Result<T> {
    let mut guard = match cache().try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(error)) => error.into_inner(),
        Err(TryLockError::WouldBlock) => anyhow::bail!("模型正在识别，请结束后再更新或删除"),
    };
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    if guard.as_ref().is_some_and(|cached| cached.path == path) {
        *guard = None;
    }
    change()
}

fn usable_language(language: Option<&str>, languages: &[String]) -> Option<String> {
    let language = language?.replace('_', "-");
    let primary = language.split('-').next()?;
    languages
        .iter()
        .find(|known| known.eq_ignore_ascii_case(&language))
        .or_else(|| {
            languages.iter().find(|known| {
                known
                    .split(['-', '_'])
                    .next()
                    .is_some_and(|part| part.eq_ignore_ascii_case(primary))
            })
        })
        .cloned()
}

fn decoder_language(
    model_id: &str,
    language: Option<&str>,
    languages: &[String],
) -> Result<Option<String>> {
    let selected = usable_language(language, languages);
    if model_id == "cohere-transcribe-03-2026" {
        if language.is_some() {
            Ok(Some(selected.context(
                "Cohere 不支持所选语言，请在设置中选择支持的语言",
            )?))
        } else {
            Ok(Some("zh".to_string()))
        }
    } else {
        Ok(selected)
    }
}

/// Prefer a quiet cut near the end of a bounded window without skipping samples.
fn chunk_end(samples: &[f32], start: usize, limit: usize) -> usize {
    let hard_end = (start + limit).min(samples.len());
    if hard_end == samples.len() {
        return hard_end;
    }
    let search_start = hard_end
        .saturating_sub(SAMPLE_RATE * 2)
        .max(start + limit / 2);
    let frame = SAMPLE_RATE / 50;
    (search_start..hard_end.saturating_sub(frame))
        .step_by(frame)
        .map(|offset| {
            (
                offset + frame,
                samples[offset..offset + frame]
                    .iter()
                    .map(|s| s * s)
                    .sum::<f32>(),
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(hard_end, |(end, _)| end)
}

fn recognize(
    path: &Path,
    config: &NativeAsrConfig,
    samples: &[f32],
    language: Option<&str>,
    cancel: &CancelToken,
) -> Result<String> {
    anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
    let spec = manager::model_info(&config.model_id)?;
    let canonical = std::fs::canonicalize(path).context("模型文件不存在，请先下载")?;
    let metadata = std::fs::metadata(&canonical)?;
    let mut guard = cache().lock().unwrap_or_else(|e| e.into_inner());
    anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
    let reuse = guard.as_ref().is_some_and(|cached| {
        cached.path == canonical
            && cached.modified == metadata.modified().ok()
            && cached.bytes == metadata.len()
    });
    if !reuse {
        // Release old weights before loading another multi-GB model.
        *guard = None;
        let start = Instant::now();
        manager::verify_file(&canonical, &spec)?;
        anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
        let model = Model::load_with(
            &canonical,
            &ModelOptions {
                backend: Backend::Cpu,
                gpu_device: 0,
            },
        )
        .context("原生 CPU 模型加载失败")?;
        anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
        tracing::info!(model = %config.model_id, load_ms = start.elapsed().as_millis(), "Native ASR verified and loaded");
        *guard = Some(CachedModel {
            path: canonical,
            modified: metadata.modified().ok(),
            bytes: metadata.len(),
            model,
        });
    }
    let model = &guard.as_ref().context("模型缓存未初始化")?.model;
    let caps = model.capabilities();
    anyhow::ensure!(
        caps.native_sample_rate == SAMPLE_RATE as i32,
        "模型要求非 16kHz 音频，当前适配器不支持"
    );
    let mut session = model.session_with(&SessionOptions {
        n_threads: config.num_threads.clamp(1, 16) as i32,
        ..SessionOptions::default()
    })?;
    session.set_cancel_token(cancel);
    // This Cohere native backend defaults to English when language is absent;
    // it does NOT implement language detection. PopSpeak explicitly defaults
    // the Chinese UI to Chinese and rejects unsupported explicit language IDs.
    let selected_language = decoder_language(&config.model_id, language, &caps.languages)?;
    let options = RunOptions {
        timestamps: TimestampKind::None,
        language: selected_language,
        ..RunOptions::default()
    };
    let model_limit = if caps.max_audio_ms > 0 {
        (caps.max_audio_ms as usize * SAMPLE_RATE / 1000).max(1)
    } else {
        usize::MAX
    };
    let limit = (SAMPLE_RATE * CHUNK_SECONDS).min(model_limit);
    let mut texts = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
        let end = chunk_end(samples, start, limit);
        let result = session
            .run(&samples[start..end], &options)
            .context("原生 CPU 语音识别失败")?;
        anyhow::ensure!(!cancel.is_cancelled(), "识别已取消");
        anyhow::ensure!(
            !session.was_truncated(),
            "模型输出已达到长度上限，请缩短录音后重试"
        );
        if !result.text.trim().is_empty() {
            texts.push(result.text.trim().to_string());
        }
        start = end;
    }
    Ok(super::t2s::to_simplified(&texts.join(" ")))
}

pub struct NativeAsrProvider {
    app: AppHandle,
    config: NativeAsrConfig,
    stt_config: SttConfig,
    model_path: PathBuf,
    audio: Vec<u8>,
    cancel: CancelToken,
    recording_session: Option<u64>,
}

impl NativeAsrProvider {
    pub fn new(app: AppHandle, config: NativeAsrConfig) -> Self {
        Self {
            app,
            config,
            stt_config: SttConfig::default(),
            model_path: PathBuf::new(),
            audio: Vec::new(),
            cancel: CancelToken::new(),
            recording_session: None,
        }
    }
}

impl Drop for NativeAsrProvider {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[async_trait]
impl SttProvider for NativeAsrProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        anyhow::ensure!(
            config.sample_rate == SAMPLE_RATE as u32,
            "原生识别只接受 16kHz 单声道 PCM"
        );
        let paths = manager::paths(
            &self.app,
            &self.config.model_id,
            Some(&self.config.model_dir),
        )?;
        anyhow::ensure!(
            paths.ready,
            "请先下载完整的 {} 模型",
            manager::model_info(&self.config.model_id)?.name
        );
        // Cold verification/loading runs after recording, so first words cannot
        // overflow the capture queue while a large model initializes.
        self.model_path = paths.model_path.into();
        self.stt_config = config.clone();
        self.audio.clear();
        self.cancel = CancelToken::new();
        self.recording_session = self
            .app
            .try_state::<crate::pipeline::PipelineHandle>()
            .map(|p| p.current_recording_session());
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        anyhow::ensure!(
            self.audio.len().saturating_add(chunk.len()) <= MAX_RECORDING_BYTES,
            "录音超过 10 分钟上限"
        );
        self.audio.extend_from_slice(chunk);
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        std::future::pending().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        if self.audio.is_empty() {
            return Ok(None);
        }
        let audio = std::mem::take(&mut self.audio);
        let prepared = crate::audio::preprocess::prepare_pcm_i16(
            &audio,
            SAMPLE_RATE as u32,
            self.stt_config.vad_enabled,
            self.stt_config.noise_suppression_enabled,
        );
        if prepared.is_empty() {
            return Ok(None);
        }
        let samples = prepared
            .chunks_exact(2)
            .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0)
            .collect::<Vec<_>>();
        let path = self.model_path.clone();
        let config = self.config.clone();
        let language = self.stt_config.language.clone();
        let cancel = self.cancel.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            recognize(&path, &config, &samples, language.as_deref(), &cancel)
        });
        loop {
            tokio::select! {
                result = &mut task => {
                    let text = result.context("原生识别任务异常退出")??;
                    return Ok((!text.trim().is_empty()).then_some(text));
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.app.try_state::<crate::pipeline::PipelineHandle>()
                        .is_some_and(|p| p.current_state() != crate::pipeline::PipelineState::Transcribing
                            || self.recording_session.is_some_and(|id| p.current_recording_session() != id)) {
                        // Native callback checks the token between compute steps.
                        // Model load itself is not forcibly interruptible.
                        self.cancel.cancel();
                    }
                }
            }
        }
    }

    fn name(&self) -> &str {
        "native-asr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohere_default_is_explicit_chinese_not_implicit_english() {
        let languages = vec!["zh".into(), "en".into()];
        assert_eq!(
            decoder_language("cohere-transcribe-03-2026", None, &languages).unwrap(),
            Some("zh".into())
        );
        assert_eq!(
            decoder_language("cohere-transcribe-03-2026", Some("en-US"), &languages).unwrap(),
            Some("en".into())
        );
        assert!(decoder_language("cohere-transcribe-03-2026", Some("yue"), &languages).is_err());
        assert_eq!(
            decoder_language("qwen3-asr-1.7b", None, &languages).unwrap(),
            None
        );
    }

    #[test]
    fn chunking_is_bounded_and_does_not_drop_audio() {
        let samples = vec![0.1; SAMPLE_RATE * 95];
        let mut start = 0;
        let mut count = 0;
        while start < samples.len() {
            let end = chunk_end(&samples, start, SAMPLE_RATE * 20);
            assert!(end > start && end - start <= SAMPLE_RATE * 20);
            count += end - start;
            start = end;
        }
        assert_eq!(count, samples.len());
    }

    #[test]
    fn unsupported_language_does_not_force_chinese_on_english_model() {
        let languages = vec!["en".to_string()];
        assert_eq!(usable_language(Some("zh-CN"), &languages), None);
        assert_eq!(
            usable_language(Some("en-US"), &languages),
            Some("en".to_string())
        );
    }

    #[test]
    fn language_hints_keep_supported_bcp47_locale_and_prefer_exact_match() {
        let languages = vec!["en-US".into(), "en-GB".into(), "zh-CN".into()];
        assert_eq!(
            usable_language(Some("en_gb"), &languages),
            Some("en-GB".into())
        );
        assert_eq!(
            usable_language(Some("zh"), &languages),
            Some("zh-CN".into())
        );
        assert_eq!(
            usable_language(Some("zh_CN"), &languages),
            Some("zh-CN".into())
        );
        assert_eq!(usable_language(Some("fr"), &languages), None);
    }

    /// Set POPSPEAK_NATIVE_ASR_MODEL_ID, POPSPEAK_NATIVE_ASR_MODEL_DIR (the
    /// model-id directory itself), POPSPEAK_NATIVE_ASR_WAV and optionally THREADS.
    #[test]
    #[ignore = "requires an explicitly downloaded multi-GB GGUF and real WAV fixture"]
    fn real_native_model_smoke() -> Result<()> {
        let id = std::env::var("POPSPEAK_NATIVE_ASR_MODEL_ID").context("set model ID")?;
        let dir = std::env::var("POPSPEAK_NATIVE_ASR_MODEL_DIR").context("set model directory")?;
        let wav = std::env::var("POPSPEAK_NATIVE_ASR_WAV").context("set WAV")?;
        let threads = std::env::var("POPSPEAK_NATIVE_ASR_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        let spec = manager::model_info(&id)?;
        let mut reader = hound::WavReader::open(wav)?;
        assert_eq!(reader.spec().sample_rate, 16000);
        assert_eq!(reader.spec().channels, 1);
        let samples = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|v| v as f32 / 32768.0)
            .collect::<Vec<_>>();
        let config = NativeAsrConfig {
            model_id: id,
            model_dir: dir.clone(),
            num_threads: threads,
        };
        let start = Instant::now();
        let result = recognize(
            &Path::new(&dir).join(spec.file_name),
            &config,
            &samples,
            None,
            &CancelToken::new(),
        )?;
        eprintln!(
            "NATIVE_SMOKE model={} threads={} audio_s={:.3} cold_total_ms={} text={:?}",
            config.model_id,
            threads,
            samples.len() as f64 / 16000.0,
            start.elapsed().as_millis(),
            result
        );
        assert!(!result.trim().is_empty());
        Ok(())
    }
}
