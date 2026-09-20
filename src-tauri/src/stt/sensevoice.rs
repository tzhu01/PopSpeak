use anyhow::{Context, Result};
use async_trait::async_trait;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

use super::sensevoice_manager;
use super::{SttConfig, SttProvider, TranscriptEvent};

pub struct SenseVoiceRecognizer {
    recognizer: OfflineRecognizer,
}

type SharedRecognizer = Arc<Mutex<Option<SenseVoiceRecognizer>>>;

/// Process-wide CPU recognizer cache. Initializing the ONNX session used to
/// happen on every hotkey press and cost roughly 1.7 seconds. Providers now
/// share the same initialized recognizer for an identical model configuration.
#[derive(Default)]
pub struct SenseVoiceRecognizerCache {
    entries: Mutex<HashMap<String, SharedRecognizer>>,
}

impl SenseVoiceRecognizerCache {
    fn handle(
        &self,
        language: &str,
        num_threads: i32,
        custom_model_dir: Option<&str>,
    ) -> SharedRecognizer {
        let key = format!(
            "{}|{}|{}",
            language,
            num_threads,
            custom_model_dir.unwrap_or_default().trim()
        );
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        Arc::clone(
            entries
                .entry(key)
                .or_insert_with(|| Arc::new(Mutex::new(None))),
        )
    }

    fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
}

fn shared_recognizer(
    app: &AppHandle,
    language: &str,
    num_threads: i32,
    custom_model_dir: Option<&str>,
    cache_namespace: &str,
) -> SharedRecognizer {
    app.state::<SenseVoiceRecognizerCache>().handle(
        &format!("{cache_namespace}:{language}"),
        num_threads,
        custom_model_dir,
    )
}

/// Drop cached sessions after model files are replaced so the next dictation
/// definitely opens the newly verified files instead of retaining old weights.
pub fn invalidate_cache(app: &AppHandle) {
    app.state::<SenseVoiceRecognizerCache>().clear();
}

/// Load the CPU recognizer before the first hotkey press. Safe to call more
/// than once; all callers converge on the process-wide cached instance.
pub async fn prewarm(
    app: AppHandle,
    language: String,
    num_threads: i32,
    custom_model_dir: Option<String>,
) -> Result<()> {
    prewarm_namespace(app, language, num_threads, custom_model_dir, "final").await
}

/// Preload the display-only recognizer used by the universal live-preview lane.
/// It intentionally has a separate cache entry so a slow preview decode can
/// never hold the final recognizer's mutex after recording stops.
pub async fn prewarm_preview(app: AppHandle, num_threads: i32) -> Result<()> {
    prewarm_namespace(app, "auto".to_string(), num_threads, None, "preview").await
}

async fn prewarm_namespace(
    app: AppHandle,
    language: String,
    num_threads: i32,
    custom_model_dir: Option<String>,
    cache_namespace: &'static str,
) -> Result<()> {
    let recognizer = shared_recognizer(
        &app,
        &language,
        num_threads,
        custom_model_dir.as_deref(),
        cache_namespace,
    );
    tokio::task::spawn_blocking(move || {
        let mut guard = recognizer.lock().unwrap_or_else(|error| error.into_inner());
        if guard.is_none() {
            *guard = Some(SenseVoiceRecognizer::new(
                &app,
                &language,
                num_threads,
                custom_model_dir.as_deref(),
            )?);
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("SenseVoice prewarm task panicked")??;
    Ok(())
}

impl SenseVoiceRecognizer {
    /// custom_model_dir: 空表示使用默认位置（AppData）
    pub fn new(
        app: &AppHandle,
        language: &str,
        num_threads: i32,
        custom_model_dir: Option<&str>,
    ) -> Result<Self> {
        // 优先尝试新的下载位置（sensevoice_manager）
        let (model_path, tokens_path) = resolve_model_paths(app, custom_model_dir)?;

        if !model_path.exists() {
            anyhow::bail!(
                "SenseVoice 模型未找到: {}. 请在设置中先下载模型。",
                model_path.display()
            );
        }
        if !tokens_path.exists() {
            anyhow::bail!(
                "SenseVoice tokens 未找到: {}. 请在设置中先下载模型。",
                tokens_path.display()
            );
        }

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model_path.to_string_lossy().to_string()),
            language: Some(language.to_string()),
            use_itn: true,
        };
        config.model_config.tokens = Some(tokens_path.to_string_lossy().to_string());
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.debug = false;
        config.model_config.num_threads = num_threads;

        tracing::info!(
            "Initializing SenseVoice: model={}, lang={}, threads={}",
            model_path.display(),
            language,
            num_threads
        );

        let recognizer = OfflineRecognizer::create(&config)
            .ok_or_else(|| anyhow::anyhow!("Failed to create SenseVoice recognizer"))?;

        Ok(Self { recognizer })
    }

    pub fn recognize(&self, samples: &[f32], sample_rate: u32) -> Result<String> {
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(sample_rate as i32, samples);
        self.recognizer.decode(&stream);
        let result = stream
            .get_result()
            .ok_or_else(|| anyhow::anyhow!("SenseVoice returned empty result"))?;
        Ok(result.text)
    }
}

/// Resolve exactly the same model directory that the settings panel reports.
/// In particular, an incomplete custom directory must not silently fall back
/// to the bundled model because that makes configuration errors invisible.
fn resolve_model_paths(app: &AppHandle, custom_dir: Option<&str>) -> Result<(PathBuf, PathBuf)> {
    let resolved = sensevoice_manager::paths(app, custom_dir)
        .context("failed to resolve SenseVoice model paths")?;
    tracing::info!(
        "Resolved SenseVoice model ({}) at {}",
        resolved.source,
        resolved.model_dir
    );
    Ok((
        PathBuf::from(resolved.model_path),
        PathBuf::from(resolved.tokens_path),
    ))
}

/// Convert raw 16-bit PCM little-endian bytes to f32 samples normalized to [-1.0, 1.0].
/// The audio pipeline delivers mono 16 kHz PCM as raw bytes (no WAV header).
fn pcm_i16_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|pair| {
            let sample = i16::from_le_bytes([pair[0], pair[1]]);
            sample as f32 / 32768.0
        })
        .collect()
}

const PCM_BYTES_PER_SECOND: usize = 16_000 * 2;
// The microphone still feeds the preview lane in 100 ms transport chunks, but
// offline ASR needs substantially more context than one chunk. Keep a bounded
// four-second rolling sentence window so refreshes remain useful and cheap.
const PREVIEW_WINDOW_BYTES: usize = PCM_BYTES_PER_SECOND * 4;
const PREVIEW_PREROLL_BYTES: usize = PCM_BYTES_PER_SECOND * 3 / 10;
const PREVIEW_MIN_BYTES: usize = PCM_BYTES_PER_SECOND * 4 / 5;
const PREVIEW_SILENCE_BYTES: usize = PCM_BYTES_PER_SECOND / 2;

/// SenseVoice is an offline recognizer, not a native streaming model. This
/// planner requests bounded, revisable previews of the current sentence and
/// freezes only the display prefix at a pause or the four-second boundary.
/// None of that display text is used to construct the final transcript.
#[derive(Default)]
struct PreviewPlanner {
    segment_start: usize,
    observed_end: usize,
    last_voice_end: usize,
    voiced_bytes: usize,
    requested_end: usize,
    committed_text: String,
}

#[derive(Debug)]
struct PreviewPlan {
    start: usize,
    end: usize,
    commit: bool,
}

impl PreviewPlanner {
    fn observe(&mut self, audio: &[u8]) {
        let end = (audio.len() / 2 * 2).min(self.segment_start + PREVIEW_WINDOW_BYTES);
        // Twenty-millisecond energy frames avoid treating one loud sample as
        // an entire voiced audio callback. This is only a preview trigger;
        // final recognition continues to use the existing audio preparation.
        for (index, frame) in audio[self.observed_end..end].chunks(640).enumerate() {
            let count = frame.len() / 2;
            let energy = frame
                .chunks_exact(2)
                .map(|pair| {
                    let sample = i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32_768.0;
                    sample * sample
                })
                .sum::<f32>();
            if count > 0 && (energy / count as f32).sqrt() >= 0.004 {
                self.voiced_bytes += frame.len();
                self.last_voice_end = self.observed_end + index * 640 + frame.len();
            }
        }
        self.observed_end = end;
        if self.voiced_bytes == 0 {
            self.segment_start = self
                .segment_start
                .max(end.saturating_sub(PREVIEW_PREROLL_BYTES));
        }
    }

    fn next_plan(&mut self, audio: &[u8]) -> Option<PreviewPlan> {
        let available_end = audio.len() / 2 * 2;
        let end = available_end.min(self.segment_start + PREVIEW_WINDOW_BYTES);
        if end.saturating_sub(self.segment_start) < PREVIEW_MIN_BYTES
            || self.voiced_bytes < PCM_BYTES_PER_SECOND / 8
            || end <= self.requested_end
        {
            return None;
        }
        let commit = end - self.segment_start >= PREVIEW_WINDOW_BYTES
            || (end == available_end
                && end.saturating_sub(self.last_voice_end) >= PREVIEW_SILENCE_BYTES);
        let plan = PreviewPlan {
            start: self.segment_start,
            end,
            commit,
        };
        self.requested_end = end;
        if commit {
            // Advance before the task runs: future audio is observed separately
            // and no growing full-recording snapshots are ever submitted.
            self.segment_start = end;
            self.observed_end = end;
            self.last_voice_end = end;
            self.voiced_bytes = 0;
            self.observe(audio);
        }
        Some(plan)
    }

    fn complete(&mut self, text: &str, commit: bool) -> String {
        let mut display = self.committed_text.clone();
        append_preview_text(&mut display, text.trim());
        if commit {
            self.committed_text.clone_from(&display);
        }
        display
    }
}

fn append_preview_text(prefix: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    if prefix.chars().last().is_some_and(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':')
    }) && text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
    {
        prefix.push(' ');
    }
    prefix.push_str(text);
}

struct PreviewResult {
    text: String,
    commit: bool,
    decode_duration: Duration,
}

fn preview_may_start(
    enabled: bool,
    task_running: bool,
    cancelled: bool,
    now: Instant,
    not_before: Instant,
    recording: bool,
) -> bool {
    enabled && !task_running && !cancelled && recording && now >= not_before
}

fn preview_cooldown(num_threads: i32, decode_duration: Duration) -> Duration {
    // Decode completion already provides natural backpressure (only one task
    // may run). A short adaptive pause prevents a slow CPU from spinning while
    // allowing fast machines to refresh at a visibly live cadence.
    let minimum_ms = if num_threads <= 2 { 500 } else { 350 };
    let adaptive_ms = (decode_duration.as_millis() as u64 / 2).min(800);
    Duration::from_millis(minimum_ms + adaptive_ms)
}

/// A stop can close capture before its queued PCM callbacks are drained. Do not
/// start extra preview work while those final callbacks reach this provider.
fn recording_is_active(app: &AppHandle) -> bool {
    app.try_state::<crate::pipeline::PipelineHandle>()
        .map(|pipeline| pipeline.current_state() == crate::pipeline::PipelineState::Recording)
        .unwrap_or(true)
}

/// Provider wrapping SenseVoice for use with the SttProvider trait.
/// Buffers final audio, with optional bounded offline previews during capture.
pub struct SenseVoiceProvider {
    recognizer: SharedRecognizer,
    app: AppHandle,
    language: String,
    num_threads: i32,
    custom_model_dir: Option<String>,
    audio_buffer: Vec<u8>,
    hotwords: Vec<String>,
    stt_config: Option<SttConfig>,
    preview: PreviewPlanner,
    preview_task: Option<tokio::task::JoinHandle<Result<Option<PreviewResult>>>>,
    preview_cancelled: Arc<AtomicBool>,
    preview_not_before: Instant,
    last_preview_text: String,
}

impl SenseVoiceProvider {
    pub fn new(
        app: AppHandle,
        language: String,
        num_threads: i32,
        custom_model_dir: Option<String>,
        hotwords: Vec<String>,
    ) -> Self {
        Self::new_in_namespace(
            app,
            language,
            num_threads,
            custom_model_dir,
            hotwords,
            "final",
        )
    }

    /// Build the local approximate recognizer used alongside every selectable
    /// final STT engine. The bundled model and an independent cache namespace
    /// keep preview failures and in-flight decodes isolated from final output.
    pub fn new_preview(app: AppHandle, num_threads: i32) -> Self {
        Self::new_in_namespace(
            app,
            "auto".to_string(),
            num_threads,
            None,
            Vec::new(),
            "preview",
        )
    }

    fn new_in_namespace(
        app: AppHandle,
        language: String,
        num_threads: i32,
        custom_model_dir: Option<String>,
        hotwords: Vec<String>,
        cache_namespace: &'static str,
    ) -> Self {
        let recognizer = shared_recognizer(
            &app,
            &language,
            num_threads,
            custom_model_dir.as_deref(),
            cache_namespace,
        );
        Self {
            recognizer,
            app,
            language,
            num_threads,
            custom_model_dir,
            audio_buffer: Vec::new(),
            hotwords,
            stt_config: None,
            preview: PreviewPlanner::default(),
            preview_task: None,
            preview_cancelled: Arc::new(AtomicBool::new(false)),
            preview_not_before: Instant::now(),
            last_preview_text: String::new(),
        }
    }

    fn stop_preview(&mut self) {
        self.preview_cancelled.store(true, Ordering::SeqCst);
        if let Some(task) = self.preview_task.take() {
            // Queued blocking work can be aborted. ONNX already in decode must
            // finish, but its late display result is discarded and no more
            // preview tasks are queued behind it.
            task.abort();
        }
    }

    fn maybe_start_preview(&mut self) {
        let Some(config) = self.stt_config.as_ref() else {
            return;
        };
        if !preview_may_start(
            config.live_preview_enabled,
            self.preview_task.is_some(),
            self.preview_cancelled.load(Ordering::SeqCst),
            Instant::now(),
            self.preview_not_before,
            recording_is_active(&self.app),
        ) {
            return;
        }
        let Some(plan) = self.preview.next_plan(&self.audio_buffer) else {
            return;
        };
        let pcm = self.audio_buffer[plan.start..plan.end].to_vec();
        let config = config.clone();
        let recognizer = Arc::clone(&self.recognizer);
        let cancelled = Arc::clone(&self.preview_cancelled);
        let app = self.app.clone();
        self.preview_task = Some(tokio::task::spawn_blocking(move || {
            if cancelled.load(Ordering::SeqCst) || !recording_is_active(&app) {
                return Ok(None);
            }
            let prepared = crate::audio::preprocess::prepare_pcm_i16(
                &pcm,
                16_000,
                true,
                config.noise_suppression_enabled,
            );
            if prepared.is_empty() {
                return Ok(Some(PreviewResult {
                    text: String::new(),
                    commit: plan.commit,
                    decode_duration: Duration::ZERO,
                }));
            }
            let mut samples = vec![0.0; 3_200];
            samples.extend(pcm_i16_to_f32(&prepared));
            let guard = recognizer.lock().unwrap_or_else(|error| error.into_inner());
            if cancelled.load(Ordering::SeqCst) || !recording_is_active(&app) {
                return Ok(None);
            }
            let rec = guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("SenseVoice preview recognizer not initialized"))?;
            let started = Instant::now();
            let text = rec.recognize(&samples, 16_000)?;
            Ok(Some(PreviewResult {
                text,
                commit: plan.commit,
                decode_duration: started.elapsed(),
            }))
        }));
    }
}

impl Drop for SenseVoiceProvider {
    fn drop(&mut self) {
        self.stop_preview();
    }
}

#[async_trait]
impl SttProvider for SenseVoiceProvider {
    async fn connect(&mut self, config: &SttConfig) -> Result<()> {
        self.stop_preview();
        self.preview_cancelled = Arc::new(AtomicBool::new(false));
        self.preview = PreviewPlanner::default();
        self.preview_not_before = Instant::now();
        self.last_preview_text.clear();
        self.stt_config = Some(config.clone());
        let recognizer = Arc::clone(&self.recognizer);
        let app = self.app.clone();
        let language = self.language.clone();
        let num_threads = self.num_threads;
        let custom_model_dir = self.custom_model_dir.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = recognizer.lock().unwrap_or_else(|error| error.into_inner());
            if guard.is_none() {
                *guard = Some(SenseVoiceRecognizer::new(
                    &app,
                    &language,
                    num_threads,
                    custom_model_dir.as_deref(),
                )?);
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("SenseVoice initialization task panicked")??;
        self.audio_buffer.clear();
        Ok(())
    }

    async fn send_audio(&mut self, chunk: &[u8]) -> Result<()> {
        tracing::debug!(
            "SenseVoice send_audio: received {} bytes, buffer now {} bytes",
            chunk.len(),
            self.audio_buffer.len() + chunk.len()
        );
        self.audio_buffer.extend_from_slice(chunk);
        if self
            .stt_config
            .as_ref()
            .is_some_and(|config| config.live_preview_enabled)
        {
            self.preview.observe(&self.audio_buffer);
            self.maybe_start_preview();
        }
        Ok(())
    }

    async fn recv_transcript(&mut self) -> Result<Option<TranscriptEvent>> {
        if let Some(task) = self.preview_task.as_mut() {
            // Await the stored handle, not a newly spawned future: tokio::select!
            // repeatedly cancels this receive whenever another audio chunk wins.
            let result = task.await;
            self.preview_task = None;
            self.preview_not_before =
                Instant::now() + preview_cooldown(self.num_threads, Duration::ZERO);
            if !self.preview_cancelled.load(Ordering::SeqCst) && recording_is_active(&self.app) {
                match result {
                    Ok(Ok(Some(result))) => {
                        // Slow CPUs get extra breathing room. There is only one
                        // preview task, so slow inference never creates a backlog.
                        self.preview_not_before = Instant::now()
                            + preview_cooldown(self.num_threads, result.decode_duration);
                        let text = self.preview.complete(&result.text, result.commit);
                        if !text.is_empty() && text != self.last_preview_text {
                            self.last_preview_text.clone_from(&text);
                            return Ok(Some(TranscriptEvent::Partial { text }));
                        }
                    }
                    Ok(Err(error)) => tracing::warn!("SenseVoice preview skipped: {error}"),
                    Err(error) if !error.is_cancelled() => {
                        tracing::warn!("SenseVoice preview task failed: {error}")
                    }
                    _ => {}
                }
            }
        }
        std::future::pending::<Result<Option<TranscriptEvent>>>().await
    }

    async fn disconnect(&mut self) -> Result<Option<String>> {
        self.stop_preview();
        tracing::debug!("SenseVoice disconnect() called");

        if self.audio_buffer.is_empty() {
            tracing::debug!("SenseVoice disconnect: audio buffer empty, returning None");
            return Ok(None);
        }

        let config = self.stt_config.clone().unwrap_or_default();
        let prepared_audio = crate::audio::preprocess::prepare_pcm_i16(
            &self.audio_buffer,
            16_000,
            config.vad_enabled,
            config.noise_suppression_enabled,
        );
        if prepared_audio.is_empty() {
            self.audio_buffer.clear();
            return Ok(None);
        }

        // Keep 200 ms model padding after VAD. The real beginning is already
        // protected by the 300 ms VAD pre-roll.
        let silence_samples = (16000.0 * 0.2) as usize; // 200ms at 16kHz
        let silence_bytes: Vec<u8> = vec![0u8; silence_samples * 2]; // i16 = 2 bytes

        let mut padded_buffer = silence_bytes;
        padded_buffer.extend_from_slice(&prepared_audio);

        // Audio pipeline delivers raw 16-bit PCM LE mono at 16 kHz.
        let samples = pcm_i16_to_f32(&padded_buffer);
        let sample_rate = 16000u32;
        let duration_s = samples.len() as f32 / sample_rate as f32;
        tracing::info!(
            "SenseVoice: transcribing {:.2}s of audio ({} samples, with 200ms silence padding)",
            duration_s,
            samples.len()
        );
        self.audio_buffer.clear();

        tracing::debug!("SenseVoice: spawning blocking task for recognition");
        let recognizer = Arc::clone(&self.recognizer);
        let text = tokio::task::spawn_blocking(move || -> Result<String> {
            tracing::debug!("SenseVoice: blocking task started, acquiring lock");
            let guard = recognizer.lock().unwrap_or_else(|error| error.into_inner());
            let rec = guard
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("SenseVoice recognizer not initialized"))?;
            tracing::debug!("SenseVoice: calling recognize()");
            rec.recognize(&samples, sample_rate)
        })
        .await
        .context("SenseVoice task panicked")??;

        tracing::info!("SenseVoice recognized {} bytes", text.len());

        // 应用热词后处理
        let final_text = if !self.hotwords.is_empty() {
            let replaced = super::hotword_replacer::apply_hotwords(&text, &self.hotwords);
            if replaced != text {
                tracing::info!("Applied local vocabulary correction");
            }
            replaced
        } else {
            text
        };

        tracing::debug!(
            "SenseVoice disconnect: returning Some(text), len={}",
            final_text.len()
        );
        Ok(Some(final_text))
    }

    fn name(&self) -> &str {
        "sensevoice"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voiced_pcm(seconds: usize) -> Vec<u8> {
        (0..seconds * 16_000)
            .flat_map(|index| (if index % 2 == 0 { 4_000i16 } else { -4_000i16 }).to_le_bytes())
            .collect()
    }

    #[test]
    fn preview_requires_voice_and_useful_context() {
        let mut planner = PreviewPlanner::default();
        let silence = vec![0; PREVIEW_WINDOW_BYTES];
        planner.observe(&silence);
        assert!(planner.next_plan(&silence).is_none());
        assert_eq!(planner.segment_start, silence.len() - PREVIEW_PREROLL_BYTES);

        let mut planner = PreviewPlanner::default();
        let voice = voiced_pcm(1);
        planner.observe(&voice[..voice.len() / 2]);
        assert!(planner.next_plan(&voice[..voice.len() / 2]).is_none());
        planner.observe(&voice);
        let plan = planner.next_plan(&voice).expect("first voice preview");
        assert_eq!(plan.start, 0);
        assert_eq!(plan.end, PCM_BYTES_PER_SECOND);
        assert!(!plan.commit);
        assert!(
            planner.next_plan(&voice).is_none(),
            "do not decode identical audio twice"
        );
    }

    #[test]
    fn preview_commits_display_prefix_at_silence_without_repeating_it() {
        let mut planner = PreviewPlanner::default();
        let mut audio = voiced_pcm(1);
        planner.observe(&audio);
        let first = planner.next_plan(&audio).unwrap();
        assert_eq!(planner.complete("你好", first.commit), "你好");
        assert!(
            planner.committed_text.is_empty(),
            "tentative text is revisable"
        );
        audio.extend(vec![0; PREVIEW_SILENCE_BYTES]);
        planner.observe(&audio);
        let finished = planner.next_plan(&audio).unwrap();
        assert!(finished.commit);
        assert_eq!(planner.complete("你好。", finished.commit), "你好。");
        assert_eq!(planner.segment_start, audio.len());

        audio.extend(voiced_pcm(1));
        planner.observe(&audio);
        let second = planner.next_plan(&audio).unwrap();
        assert_eq!(planner.complete("明天", second.commit), "你好。明天");
        assert_eq!(planner.complete("明天见", false), "你好。明天见");
        assert_eq!(
            planner.committed_text, "你好。",
            "preview never appends tentative revisions"
        );
    }

    #[test]
    fn long_recordings_only_submit_bounded_nonoverlapping_windows() {
        let mut planner = PreviewPlanner::default();
        let audio = voiced_pcm(80);
        planner.observe(&audio);
        let window_count = audio.len() / PREVIEW_WINDOW_BYTES;
        for index in 0..window_count {
            let plan = planner.next_plan(&audio).unwrap();
            assert_eq!(plan.start, index * PREVIEW_WINDOW_BYTES);
            assert_eq!(plan.end - plan.start, PREVIEW_WINDOW_BYTES);
            assert!(plan.commit);
            assert!(planner.observed_end - planner.segment_start <= PREVIEW_WINDOW_BYTES);
        }
        assert!(planner.next_plan(&audio).is_none());
    }

    #[test]
    fn preview_preserves_pcm_alignment_across_odd_sized_callbacks() {
        let mut planner = PreviewPlanner::default();
        let audio = voiced_pcm(1);
        for length in (1..audio.len()).step_by(319) {
            planner.observe(&audio[..length]);
            assert_eq!(planner.observed_end % 2, 0);
        }
        planner.observe(&audio);
        let plan = planner.next_plan(&audio).unwrap();
        assert_eq!(plan.start % 2, 0);
        assert_eq!(plan.end % 2, 0);
    }

    #[test]
    fn preview_backpressure_cancel_stop_and_cooldown_prevent_new_work() {
        let now = Instant::now();
        assert!(preview_may_start(true, false, false, now, now, true));
        assert!(!preview_may_start(false, false, false, now, now, true));
        assert!(!preview_may_start(true, true, false, now, now, true));
        assert!(!preview_may_start(true, false, true, now, now, true));
        assert!(!preview_may_start(true, false, false, now, now, false));
        assert!(!preview_may_start(
            true,
            false,
            false,
            now,
            now + Duration::from_millis(1),
            true
        ));
        assert_eq!(
            preview_cooldown(2, Duration::from_millis(400)),
            Duration::from_millis(700)
        );
        assert_eq!(
            preview_cooldown(4, Duration::from_millis(400)),
            Duration::from_millis(550)
        );
        assert_eq!(
            preview_cooldown(2, Duration::from_secs(20)),
            Duration::from_millis(1_300)
        );
    }

    #[test]
    fn preview_keeps_word_spacing_without_inserting_spaces_between_chinese() {
        let mut planner = PreviewPlanner::default();
        assert_eq!(planner.complete("hello", true), "hello");
        assert_eq!(planner.complete("world", false), "hello world");
        let mut planner = PreviewPlanner::default();
        assert_eq!(planner.complete("Hello.", true), "Hello.");
        assert_eq!(planner.complete("World", false), "Hello. World");
        let mut planner = PreviewPlanner::default();
        assert_eq!(planner.complete("你好", true), "你好");
        assert_eq!(planner.complete("世界", false), "你好世界");
    }

    /// Real native-model smoke test for release machines. It is ignored during
    /// normal unit tests because the model and spoken WAV are generated assets.
    ///
    /// Required environment variables:
    /// - POPSPEAK_SENSEVOICE_MODEL_DIR
    /// - POPSPEAK_SENSEVOICE_SMOKE_WAV (16-bit mono WAV)
    #[test]
    #[ignore = "requires the SenseVoice model and a spoken WAV fixture"]
    fn recognizes_real_wav_on_cpu() {
        let model_dir = std::env::var("POPSPEAK_SENSEVOICE_MODEL_DIR")
            .expect("POPSPEAK_SENSEVOICE_MODEL_DIR is required");
        let wav_path = std::env::var("POPSPEAK_SENSEVOICE_SMOKE_WAV")
            .expect("POPSPEAK_SENSEVOICE_SMOKE_WAV is required");
        let language = std::env::var("POPSPEAK_SENSEVOICE_SMOKE_LANGUAGE")
            .unwrap_or_else(|_| "zh".to_string());
        let num_threads = std::env::var("POPSPEAK_SENSEVOICE_SMOKE_THREADS")
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(2)
            .clamp(1, 16);

        let mut reader = hound::WavReader::open(wav_path).expect("open smoke WAV");
        let spec = reader.spec();
        assert_eq!(spec.channels, 1, "smoke WAV must be mono");
        assert_eq!(spec.bits_per_sample, 16, "smoke WAV must be 16-bit");
        let samples = reader
            .samples::<i16>()
            .map(|sample| sample.expect("valid WAV sample") as f32 / 32768.0)
            .collect::<Vec<_>>();

        let model_dir = std::path::Path::new(&model_dir);
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(
                model_dir
                    .join("model.int8.onnx")
                    .to_string_lossy()
                    .into_owned(),
            ),
            language: Some(language),
            use_itn: true,
        };
        config.model_config.tokens =
            Some(model_dir.join("tokens.txt").to_string_lossy().into_owned());
        config.model_config.provider = Some("cpu".to_string());
        config.model_config.num_threads = num_threads;
        config.model_config.debug = false;

        let started = std::time::Instant::now();
        let recognizer = OfflineRecognizer::create(&config).expect("create CPU recognizer");
        let init_ms = started.elapsed().as_millis();
        let first_decode_started = std::time::Instant::now();
        let stream = recognizer.create_stream();
        stream.accept_waveform(spec.sample_rate as i32, &samples);
        recognizer.decode(&stream);
        let text = stream.get_result().expect("SenseVoice result").text;
        let first_decode_ms = first_decode_started.elapsed().as_millis();

        let warm_decode_started = std::time::Instant::now();
        let warm_stream = recognizer.create_stream();
        warm_stream.accept_waveform(spec.sample_rate as i32, &samples);
        recognizer.decode(&warm_stream);
        let warm_text = warm_stream
            .get_result()
            .expect("warm SenseVoice result")
            .text;
        let warm_decode_ms = warm_decode_started.elapsed().as_millis();
        println!(
            "SenseVoice smoke: threads={num_threads}, init={init_ms} ms, first={first_decode_ms} ms, warm={warm_decode_ms} ms; transcript: {text}"
        );
        assert!(!text.trim().is_empty(), "SenseVoice transcript is empty");
        assert!(
            !warm_text.trim().is_empty(),
            "warm SenseVoice transcript is empty"
        );

        // Exercise the actual planner and native decoder as audio arrives. The
        // display may revise earlier words; the independent final result above
        // remains the only transcript suitable for history/output/points.
        assert_eq!(spec.sample_rate, 16_000, "preview WAV must be 16 kHz");
        let pcm = samples
            .iter()
            .flat_map(|sample| ((*sample * 32_768.0) as i16).to_le_bytes())
            .collect::<Vec<_>>();
        let mut planner = PreviewPlanner::default();
        let mut buffer = Vec::new();
        let mut nonempty_previews = 0;
        for chunk in pcm.chunks(PCM_BYTES_PER_SECOND) {
            buffer.extend_from_slice(chunk);
            planner.observe(&buffer);
            if let Some(plan) = planner.next_plan(&buffer) {
                assert!(plan.end - plan.start <= PREVIEW_WINDOW_BYTES);
                let prepared = crate::audio::preprocess::prepare_pcm_i16(
                    &buffer[plan.start..plan.end],
                    16_000,
                    true,
                    true,
                );
                if prepared.is_empty() {
                    continue;
                }
                let mut preview_samples = vec![0.0; 3_200];
                preview_samples.extend(pcm_i16_to_f32(&prepared));
                let stream = recognizer.create_stream();
                stream.accept_waveform(16_000, &preview_samples);
                let started = Instant::now();
                recognizer.decode(&stream);
                let text = stream.get_result().expect("native preview result").text;
                let display = planner.complete(&text, plan.commit);
                if !display.trim().is_empty() {
                    nonempty_previews += 1;
                }
                println!(
                    "SenseVoice preview: threads={num_threads}, input={:.2}s, decode={}ms, commit={}; display: {display}",
                    (plan.end - plan.start) as f32 / PCM_BYTES_PER_SECOND as f32,
                    started.elapsed().as_millis(), plan.commit,
                );
            }
        }
        assert!(
            nonempty_previews > 0,
            "no real native preview text was produced"
        );
        let final_stream = recognizer.create_stream();
        final_stream.accept_waveform(spec.sample_rate as i32, &samples);
        recognizer.decode(&final_stream);
        let after_preview_text = final_stream
            .get_result()
            .expect("independent final result")
            .text;
        assert_eq!(
            after_preview_text, warm_text,
            "display-only previews must not mutate subsequent full-audio recognition"
        );
    }
}
