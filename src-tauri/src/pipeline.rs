use anyhow::Result;
#[cfg(not(target_os = "macos"))]
use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri::Manager;
use tokio::sync::Notify;

use crate::activation::{ActivationService, RecordingPermit};
use crate::app_detector;
use crate::audio::{AudioCaptureHandle, AudioConfig};
use crate::llm::{self, LlmConfig, PolishRequest};
use crate::output::{self, OutputMode};
use crate::storage;
use crate::stt::{self, SttConfig, SttProvider, TranscriptEvent};
use crate::{LocalLlmServerState, SessionTokenStore};

// ─── Timing constants ───

/// On macOS, verify whether the process has been granted Accessibility (Assistive Access)
/// permission. enigo uses CGEventPost under the hood, which requires this permission;
/// without it all synthesised key events are silently dropped by the OS.
/// Returns true on all non-macOS platforms (no permission needed).
pub fn is_accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }
        unsafe { AXIsProcessTrusted() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// On macOS, open System Settings → Accessibility so the user can grant
/// permission. The low-level AXIsProcessTrustedWithOptions API with the
/// prompt flag causes the system to kill unsigned apps, so we use `open`
/// instead and return false (permission is not yet granted).
pub fn request_accessibility_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Open System Settings → Privacy & Security → Accessibility
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
        if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
            tracing::error!("Failed to open Accessibility settings: {}", e);
        }
        false
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Delay before capturing selected text to ensure hotkey modifiers are released.
const SELECTED_TEXT_CAPTURE_DELAY_MS: u64 = 60;
/// Delay after simulating Ctrl+C to let the clipboard update.
const CLIPBOARD_COPY_SETTLE_MS: u64 = 100;
/// Interval for polling audio volume during recording.
const VOLUME_POLL_INTERVAL_MS: u64 = 50;
/// Timeout for STT finalization after recording stops.
const STT_FINALIZE_TIMEOUT_SECS: u64 = 120;
/// The display-only lane receives the same PCM in 100 ms transport chunks.
/// Recognition itself uses a longer rolling window; decoding a standalone
/// 100 ms fragment would be both inaccurate and wasteful.
const LIVE_PREVIEW_CHUNK_MS: u32 = 100;

fn stt_finalize_timeout_seconds(provider: &str, audio_ms: u64) -> u64 {
    if provider == "native-asr" {
        // Larger CPU models are not realtime. Bound the wait, but allow cold
        // loading plus six seconds of work per recorded second for long clips.
        audio_ms
            .saturating_div(1000)
            .saturating_mul(6)
            .saturating_add(60)
            .clamp(120, 1800)
    } else {
        STT_FINALIZE_TIMEOUT_SECS
    }
}

fn apply_trial_restrictions(config: &mut storage::AppConfig, activated: bool) {
    if !activated {
        config.polish_enabled = false;
        config.translate_enabled = false;
        config.selected_text_enabled = false;
    }
}

fn maximum_recording_milliseconds(config: &storage::AppConfig) -> u64 {
    let user_limit = config.max_recording_seconds.clamp(5, 600);
    let seconds = if config.stt_provider == "custom-whisper" {
        user_limit.min(stt::custom_cloud::max_recording_seconds(
            &config.custom_cloud,
        ))
    } else {
        user_limit
    };
    seconds as u64 * 1000
}

/// Error state belongs to one recording, independently of its transcript.
/// Preserve the first actionable provider failure rather than later fallout.
#[derive(Default)]
struct SessionSttError {
    session: u64,
    message: Option<String>,
}

impl SessionSttError {
    fn reset(&mut self, session: u64) {
        self.session = session;
        self.message = None;
    }

    fn record(&mut self, session: u64, message: String) -> bool {
        if self.session != session || self.message.is_some() {
            return false;
        }
        self.message = Some(message);
        true
    }

    fn message_for(&self, session: u64) -> Option<&str> {
        (self.session == session)
            .then_some(self.message.as_deref())
            .flatten()
    }
}

fn empty_transcript_error_message(
    provider_error: Option<&str>,
    captured_audio_chunks: u64,
    captured_audio_peak: f32,
) -> Option<&'static str> {
    // report_stt_error already emitted the actual failure. Do not replace a
    // checksum, model-load or network error with unrelated microphone advice.
    if provider_error.is_some() {
        None
    } else if captured_audio_chunks == 0 {
        Some("未收到麦克风音频，请检查 Windows 麦克风权限和输入设备。")
    } else if captured_audio_peak < 0.002 {
        Some("麦克风有连接但没有声音信号，请检查静音状态或在设置中选择正确的麦克风。")
    } else {
        Some("已收到麦克风声音，但未识别出语音。请靠近麦克风后重试。")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Idle,
    Recording,
    Transcribing,
    Polishing,
    Outputting,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PipelineStateEvent {
    session_id: u64,
    revision: u64,
    state: PipelineState,
}

#[derive(Debug, Clone, serde::Serialize)]
struct PipelineMessageEvent {
    session_id: u64,
    revision: u64,
    message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct SessionTextEvent {
    session_id: u64,
    revision: u64,
    text: String,
}

fn emit_session_text(
    app: &tauri::AppHandle,
    event: &str,
    session_id: u64,
    revision: u64,
    text: impl Into<String>,
) {
    let _ = app.emit(
        event,
        SessionTextEvent {
            session_id,
            revision,
            text: text.into(),
        },
    );
}

/// Run a local, display-only approximate recognizer alongside the selected
/// final provider. The input channel is independent and lossy by design: if a
/// slow CPU cannot keep up, only stale preview chunks are discarded. No text
/// from this lane is ever appended to the authoritative transcript.
async fn run_local_preview_lane(
    app: tauri::AppHandle,
    audio_rx: flume::Receiver<Vec<u8>>,
    recording_session: u64,
    session_counter: Arc<AtomicU64>,
    num_threads: i32,
    mut config: SttConfig,
) {
    config.live_preview_enabled = true;
    let mut provider =
        stt::sensevoice::SenseVoiceProvider::new_preview(app.clone(), num_threads.clamp(1, 4));
    if let Err(error) = provider.connect(&config).await {
        // Preview is a best-effort local UI feature. A missing optional model
        // must never prevent the selected provider from recording or decoding.
        tracing::warn!("Local live preview unavailable: {error:#}");
        return;
    }

    let mut revision = 0u64;
    loop {
        if session_counter.load(Ordering::SeqCst) != recording_session {
            break;
        }
        tokio::select! {
            chunk = audio_rx.recv_async() => {
                let Ok(chunk) = chunk else { break };
                if session_counter.load(Ordering::SeqCst) != recording_session {
                    break;
                }
                if let Err(error) = provider.send_audio(&chunk).await {
                    tracing::warn!("Local live preview skipped after audio error: {error:#}");
                    break;
                }
            }
            transcript = provider.recv_transcript() => {
                match transcript {
                    Ok(Some(TranscriptEvent::Partial { text })) if !text.trim().is_empty() => {
                        let pipeline = app.state::<PipelineHandle>();
                        if session_counter.load(Ordering::SeqCst) != recording_session
                            || pipeline.current_state() != PipelineState::Recording
                        {
                            continue;
                        }
                        revision = revision.saturating_add(1);
                        emit_session_text(
                            &app,
                            "stt:preview",
                            recording_session,
                            revision,
                            text,
                        );
                    }
                    Ok(Some(TranscriptEvent::Error { message })) => {
                        tracing::warn!("Local live preview stopped: {message}");
                        break;
                    }
                    Ok(Some(_)) | Ok(None) => {}
                    Err(error) => {
                        tracing::warn!("Local live preview stopped: {error:#}");
                        break;
                    }
                }
            }
        }
    }
}

impl PipelineState {
    fn as_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Recording => 1,
            Self::Transcribing => 2,
            Self::Polishing => 3,
            Self::Outputting => 4,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Recording,
            2 => Self::Transcribing,
            3 => Self::Polishing,
            4 => Self::Outputting,
            _ => Self::Idle,
        }
    }
}

pub struct PipelineHandle {
    app_handle: tauri::AppHandle,
    state: Arc<AtomicU8>,
    state_revision: AtomicU64,
    message_revision: AtomicU64,
    audio_handle: Arc<Mutex<Option<AudioCaptureHandle>>>,
    audio_volume: Arc<Mutex<f32>>,
    accumulated_text: Mutex<Arc<Mutex<String>>>,
    stt_error: Mutex<SessionSttError>,
    stt_done: Mutex<Arc<Notify>>,
    abort_notify: Mutex<Arc<Notify>>,
    abort_flag: Arc<AtomicBool>,
    preloaded_config: Arc<Mutex<Option<storage::AppConfig>>>,
    preloaded_app_ctx: Arc<Mutex<Option<app_detector::AppContext>>>,
    preloaded_dictionary: Arc<Mutex<Option<Vec<storage::DictionaryEntry>>>>,
    preloaded_selected_text: Arc<Mutex<Option<String>>>,
    recording_start: Arc<Mutex<Option<std::time::Instant>>>,
    recording_session: Arc<AtomicU64>,
    capture_lock: Mutex<()>,
    recording_permit: Mutex<Option<RecordingPermit>>,
    shared_client: reqwest::Client,
    /// Serializes start()/stop() so that stop() waits for start() to finish
    /// its setup before reading shared state (preloaded_config, audio_handle, etc.).
    /// Without this, a quick press-release in hold mode causes stop() to run
    /// while start() is still connecting to STT, finding empty fields.
    pipeline_lock: tokio::sync::Mutex<()>,
    /// Ring buffer of the last 3 polished sentences for context continuity.
    context_store: Arc<Mutex<VecDeque<String>>>,
}

impl PipelineHandle {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            state: Arc::new(AtomicU8::new(PipelineState::Idle.as_u8())),
            state_revision: AtomicU64::new(0),
            message_revision: AtomicU64::new(0),
            audio_handle: Arc::new(Mutex::new(None)),
            audio_volume: Arc::new(Mutex::new(0.0)),
            accumulated_text: Mutex::new(Arc::new(Mutex::new(String::new()))),
            stt_error: Mutex::new(SessionSttError::default()),
            stt_done: Mutex::new(Arc::new(Notify::new())),
            abort_notify: Mutex::new(Arc::new(Notify::new())),
            abort_flag: Arc::new(AtomicBool::new(false)),
            preloaded_config: Arc::new(Mutex::new(None)),
            preloaded_app_ctx: Arc::new(Mutex::new(None)),
            preloaded_dictionary: Arc::new(Mutex::new(None)),
            preloaded_selected_text: Arc::new(Mutex::new(None)),
            recording_start: Arc::new(Mutex::new(None)),
            recording_session: Arc::new(AtomicU64::new(0)),
            capture_lock: Mutex::new(()),
            recording_permit: Mutex::new(None),
            shared_client: reqwest::Client::new(),
            pipeline_lock: tokio::sync::Mutex::new(()),
            context_store: Arc::new(Mutex::new(VecDeque::with_capacity(3))),
        }
    }

    fn emit_state_event(&self, recording_session: u64, new_state: PipelineState) {
        let revision = self.state_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.app_handle.emit(
            "pipeline:state",
            PipelineStateEvent {
                session_id: recording_session,
                revision,
                state: new_state,
            },
        );
    }

    fn session_is_active(&self, recording_session: u64) -> bool {
        self.current_recording_session() == recording_session
            && !self.abort_flag.load(Ordering::SeqCst)
    }

    fn emit_message_event(&self, recording_session: u64, event: &str, message: impl Into<String>) {
        let revision = self.message_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.app_handle.emit(
            event,
            PipelineMessageEvent {
                session_id: recording_session,
                revision,
                message: message.into(),
            },
        );
    }

    /// Emit a user-facing session message only while it still belongs to the
    /// current recording. The structured identity lets the WebView reject an
    /// already-queued message even if native delivery crosses a new start.
    fn emit_message_for_session(
        &self,
        recording_session: u64,
        event: &str,
        message: impl Into<String>,
    ) -> bool {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        if !self.session_is_active(recording_session) {
            return false;
        }
        self.emit_message_event(recording_session, event, message);
        true
    }

    /// Guard legacy string events that are not part of the error/notice
    /// protocol (for example target-app and output-fallback notifications).
    fn emit_string_for_session(
        &self,
        recording_session: u64,
        event: &str,
        message: impl Into<String>,
    ) -> bool {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        if !self.session_is_active(recording_session) {
            return false;
        }
        let _ = self.app_handle.emit(event, message.into());
        true
    }

    /// Change state only while `recording_session` is still the current,
    /// non-aborted run. Holding the capture lock makes the check, write and
    /// event atomic with respect to `abort()` advancing the session.
    fn set_state_for_session(&self, recording_session: u64, new_state: PipelineState) -> bool {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        if !self.session_is_active(recording_session) {
            return false;
        }
        self.state.store(new_state.as_u8(), Ordering::SeqCst);
        self.emit_state_event(recording_session, new_state);
        // Tray updates disabled — macOS 26.3 SIGTRAP issue.
        // Tray state is refreshed on UI events (click, focus) instead.
        true
    }

    fn transition_state_for_session(
        &self,
        recording_session: u64,
        from: PipelineState,
        to: PipelineState,
    ) -> bool {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        if !self.session_is_active(recording_session)
            || self
                .state
                .compare_exchange(from.as_u8(), to.as_u8(), Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return false;
        }
        self.emit_state_event(recording_session, to);
        true
    }

    pub fn current_state(&self) -> PipelineState {
        PipelineState::from_u8(self.state.load(Ordering::SeqCst))
    }

    pub fn current_recording_session(&self) -> u64 {
        self.recording_session.load(Ordering::SeqCst)
    }

    fn report_stt_error(&self, recording_session: u64, message: String) {
        // start/abort hold the same lock while advancing the session. Keep it
        // through emit so an obsolete provider cannot report into a new UI run.
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        if !self.session_is_active(recording_session) {
            return;
        }
        if crate::lock_or_recover!(self.stt_error, "stt_error")
            .record(recording_session, message.clone())
        {
            self.emit_message_event(recording_session, "pipeline:error", message);
        }
    }

    fn emit_activation_status(&self) {
        if let Ok(status) = self.app_handle.state::<ActivationService>().status() {
            let _ = self.app_handle.emit("activation:updated", status);
        }
    }

    fn finish_trial(&self, permit: &RecordingPermit, captured_ms: Option<u64>) {
        if let Some(id) = permit.reservation_id.as_deref() {
            let service = self.app_handle.state::<ActivationService>();
            let result = match captured_ms {
                Some(ms) => service.settle_recording(id, ms.min(permit.max_duration_ms)),
                None => service.cancel_before_capture(id),
            };
            if let Err(error) = result {
                tracing::error!("Unable to settle recording trial: {error}");
                // This is intentionally a global string notice rather than a
                // session message: settlement can fail while abort() is already
                // advancing the session, but the persisted quota problem still
                // requires user attention.
                let _ = self.app_handle.emit(
                    "pipeline:notice",
                    "试用记录保存失败，已保留预留额度。请重启后检查激活与权益。",
                );
            }
            self.emit_activation_status();
        }
    }

    /// Stop native callbacks before reading sample totals. This transaction is
    /// also used by abort/setup-error, so a reservation is settled at most once.
    fn stop_capture(&self) -> (u64, f32, u64, u64) {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        self.stop_capture_locked()
    }

    fn stop_capture_locked(&self) -> (u64, f32, u64, u64) {
        let handle = crate::lock_or_recover!(self.audio_handle, "audio_handle").take();
        let permit = crate::lock_or_recover!(self.recording_permit, "recording_permit").take();
        let stats = if let Some(mut handle) = handle {
            handle.stop();
            (
                handle.captured_chunks(),
                handle.peak_volume(),
                handle.dropped_chunks(),
                handle.captured_milliseconds(),
            )
        } else {
            (0, 0.0, 0, 0)
        };
        if let Some(permit) = permit {
            self.finish_trial(&permit, Some(stats.3));
        }
        stats
    }

    /// Immediately abort the pipeline regardless of current state.
    /// Stops audio capture, forces state to Idle, and signals any
    /// ongoing stop() to exit early via abort_flag.
    pub fn abort(&self) {
        let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
        tracing::info!(
            "Pipeline abort requested (current state: {:?})",
            self.current_state()
        );

        // Set abort flag so any running stop() exits early
        self.abort_flag.store(true, Ordering::SeqCst);
        let aborted_session = self.recording_session.fetch_add(1, Ordering::SeqCst);

        // Stop audio capture (closes channel → STT task terminates naturally)
        let (captured_audio_chunks, captured_audio_peak, dropped_audio_chunks, _) =
            self.stop_capture_locked();
        tracing::info!(
            "Audio capture summary: chunks={}, peak={:.5}, dropped={}",
            captured_audio_chunks,
            captured_audio_peak,
            dropped_audio_chunks
        );

        // Unblock stop() if it's waiting on stt_done.notified()
        crate::lock_or_recover!(self.stt_done, "stt_done").notify_one();
        crate::lock_or_recover!(self.abort_notify, "abort_notify").notify_one();

        // Clear accumulated text
        crate::lock_or_recover!(self.accumulated_text, "accumulated_text")
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        *self
            .recording_start
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;

        // The atomic session has already advanced to invalidate every native
        // worker. Emit Idle against the session the frontend actually knows,
        // otherwise it would correctly reject the event as belonging to an
        // unknown future session and remain visually stuck in Recording.
        self.state
            .store(PipelineState::Idle.as_u8(), Ordering::SeqCst);
        self.emit_state_event(aborted_session, PipelineState::Idle);
    }

    /// Capture selected text from the foreground app by simulating Ctrl+C / Cmd+C.
    /// Must be called when no hotkey modifier keys are physically held down.
    /// Called from async context via block_in_place, so std::thread::sleep is acceptable.
    fn capture_selected_text(&self) -> Option<String> {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let backup = output::clipboard::snapshot(&mut clipboard);
        let backup_text = match &backup {
            Some(output::clipboard::ClipboardBackup::Text(text)) => Some(text.clone()),
            _ => None,
        };

        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("osascript")
                .args([
                    "-e",
                    r#"tell application "System Events" to keystroke "c" using command down"#,
                ])
                .status();
        }

        #[cfg(not(target_os = "macos"))]
        {
            if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
                let modifier = Key::Control;
                let pressed = enigo.key(modifier, Direction::Press).is_ok();
                if pressed {
                    let _ = enigo.key(Key::Unicode('c'), Direction::Click);
                    let _ = enigo.key(modifier, Direction::Release);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_COPY_SETTLE_MS));

        let selected = clipboard.get_text().ok();

        // Always restore clipboard
        if let Some(backup) = backup {
            output::clipboard::restore(&mut clipboard, backup);
        }

        tracing::info!(
            "Selected text capture: backup_len={}, selected_len={}",
            backup_text.as_deref().map(|s| s.len()).unwrap_or(0),
            selected.as_deref().map(|s| s.len()).unwrap_or(0)
        );

        // On macOS, if Cmd+C had no effect (e.g., no Accessibility permission),
        // the clipboard is unchanged, so selected == backup — return None to avoid
        // passing stale clipboard content to the LLM as if it were selected text.
        match &selected {
            Some(s) if !s.trim().is_empty() => {
                if backup_text.as_deref() == Some(s.as_str()) {
                    tracing::debug!(
                        "Selected text equals clipboard backup — Cmd+C had no effect, ignoring"
                    );
                    None
                } else {
                    Some(s.clone())
                }
            }
            _ => None,
        }
    }

    async fn load_config(&self) -> storage::AppConfig {
        self.app_handle
            .state::<storage::ConfigManager>()
            .load()
            .await
            .unwrap_or_default()
    }

    pub async fn start(&self) -> Result<()> {
        let _guard = self.pipeline_lock.lock().await;
        let recording_session;
        {
            let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
            if self
                .state
                .compare_exchange(
                    PipelineState::Idle.as_u8(),
                    PipelineState::Recording.as_u8(),
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                )
                .is_err()
            {
                return Ok(());
            }
            self.abort_flag.store(false, Ordering::SeqCst);
            recording_session = self.recording_session.fetch_add(1, Ordering::SeqCst) + 1;
            crate::lock_or_recover!(self.stt_error, "stt_error").reset(recording_session);

            self.emit_state_event(recording_session, PipelineState::Recording);

            // Each recording has its own transcript and completion notification.
            // A delayed provider from an aborted session cannot write into the next.
            *crate::lock_or_recover!(self.accumulated_text, "accumulated_text") =
                Arc::new(Mutex::new(String::new()));
            *crate::lock_or_recover!(self.stt_done, "stt_done") = Arc::new(Notify::new());
            *crate::lock_or_recover!(self.abort_notify, "abort_notify") = Arc::new(Notify::new());
        }

        // P0-2: Load config BEFORE starting audio capture — fail fast on missing API key
        let mut config_data = self.load_config().await;
        if !self.session_is_active(recording_session) {
            return Ok(());
        }
        let permit = match self
            .app_handle
            .state::<ActivationService>()
            .begin_recording(
                &config_data.stt_provider,
                maximum_recording_milliseconds(&config_data),
            ) {
            Ok(permit) => permit,
            Err(error) => {
                let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
                self.emit_activation_status();
                return Err(error);
            }
        };
        self.emit_activation_status();
        apply_trial_restrictions(&mut config_data, permit.activated);
        *self
            .preloaded_config
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(config_data.clone());
        *self
            .preloaded_app_ctx
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(app_detector::detect_current_app());
        let dictionary_entries = if permit.activated {
            self.app_handle
                .state::<storage::DictionaryStore>()
                .list()
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let hotwords = stt::hotwords::select_hotwords(
            &dictionary_entries
                .iter()
                .map(|entry| entry.word.clone())
                .collect::<Vec<_>>(),
        );
        *self
            .preloaded_dictionary
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(dictionary_entries);

        let configured_stt_credential = if config_data.stt_provider == "volcengine-seedasr" {
            config_data.volcengine_credential.clone()
        } else {
            config_data.stt_api_key.clone()
        };

        tracing::debug!(
            "Pipeline using config: stt_provider={}, stt_key_len={}, stt_lang={}",
            config_data.stt_provider,
            configured_stt_credential.len(),
            config_data.stt_language
        );

        // Guard: empty API key — bail before starting audio (skip for cloud and local providers)
        if configured_stt_credential.is_empty() && stt::requires_api_key(&config_data.stt_provider)
        {
            self.finish_trial(&permit, None);
            self.emit_message_for_session(
                recording_session,
                "pipeline:error",
                "STT API key is not configured. Please set it in Settings → Speech Recognition.",
            );
            *self
                .preloaded_config
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            *self
                .preloaded_app_ctx
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            *self
                .preloaded_dictionary
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
            return Ok(());
        }

        // P0-3: Pre-connect STT provider before spawning task
        let stt_api_key = if config_data.stt_provider == "cloud" {
            self.app_handle
                .state::<SessionTokenStore>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        } else {
            configured_stt_credential
        };

        let live_preview_enabled =
            config_data.capsule_enabled && config_data.capsule_preview_enabled;
        let stt_config = SttConfig {
            api_key: stt_api_key,
            credential_mode: config_data.volcengine_auth_mode.clone(),
            app_id: config_data.volcengine_app_id.clone(),
            language: if config_data.stt_language == "multi" {
                None
            } else {
                Some(config_data.stt_language.clone())
            },
            smart_format: true,
            sample_rate: 16000,
            vad_enabled: config_data.vad_enabled,
            noise_suppression_enabled: config_data.noise_suppression_enabled,
            // The selected provider is authoritative. Approximate display text
            // now comes from a separate local lane for every provider, including
            // SenseVoice itself, so the two paths cannot contaminate each other.
            live_preview_enabled: false,
        };

        // Supported decoders receive the vocabulary captured at recording start.
        // Explicit wrong-form correction remains a separate post-ASR stage.

        let mut provider = stt::create_provider(
            &config_data.stt_provider,
            Some(self.shared_client.clone()),
            Some(&config_data.stt_base_url),
            Some(&config_data.stt_model),
            Some(&config_data.whisper_cli_path),
            Some(&config_data.whisper_model_path),
            Some(&config_data.whisper_lora_path),
            Some(&self.app_handle),
            Some(&config_data.sensevoice_language),
            Some(config_data.sensevoice_num_threads),
            Some(if config_data.sensevoice_use_custom_dir {
                &config_data.sensevoice_model_dir
            } else {
                ""
            }),
            Some(if config_data.funasr_use_custom_dir {
                &config_data.funasr_model_dir
            } else {
                ""
            }),
            Some(config_data.funasr_num_threads),
            Some(hotwords),
            Some(&config_data.custom_cloud),
            Some(&config_data.native_asr),
        );
        // Start capturing before a cold STT provider connects. For bounded
        // recordings, the authoritative queue reserves enough lazy channel
        // slots for the configured duration (defensively capped at ten minutes),
        // so model initialization cannot consume the first words of an utterance.
        let audio_config = AudioConfig {
            max_duration_ms: Some(permit.max_duration_ms),
            preview_chunk_duration_ms: live_preview_enabled.then_some(LIVE_PREVIEW_CHUNK_MS),
            device_name: if config_data.audio_device_name.trim().is_empty() {
                None
            } else {
                Some(config_data.audio_device_name.clone())
            },
            ..AudioConfig::default()
        };
        if !self.session_is_active(recording_session) {
            self.finish_trial(&permit, None);
            return Ok(());
        }
        let (mut handle, mut audio_rx, preview_rx) = match AudioCaptureHandle::start(audio_config) {
            Ok(result) => result,
            Err(e) => {
                let startup_audio_ms = e
                    .downcast_ref::<crate::audio::capture::CaptureStartError>()
                    .map(|error| error.captured_ms)
                    .filter(|ms| *ms > 0);
                self.finish_trial(&permit, startup_audio_ms);
                tracing::error!("Audio capture failed: {}", e);
                self.emit_message_for_session(
                    recording_session,
                    "pipeline:error",
                    format!("Audio capture failed: {e}"),
                );
                *self
                    .preloaded_config
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = None;
                *self
                    .preloaded_app_ctx
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = None;
                *self
                    .preloaded_dictionary
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = None;
                let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
                return Ok(());
            }
        };

        // Store the audio handle's volume reference.
        // Check abort_flag first — if abort() was called while we were connecting
        // to STT, don't store the handle (it would be orphaned with nobody to stop it).
        {
            let _capture_guard = crate::lock_or_recover!(self.capture_lock, "capture_lock");
            if self.abort_flag.load(Ordering::SeqCst)
                || self.recording_session.load(Ordering::SeqCst) != recording_session
            {
                tracing::info!("Pipeline aborted during setup, discarding audio capture");
                handle.stop();
                self.finish_trial(&permit, Some(handle.captured_milliseconds()));
                return Ok(());
            }
            let audio_vol = handle.get_volume();
            *crate::lock_or_recover!(self.audio_volume, "audio_volume") = audio_vol;
            *crate::lock_or_recover!(self.audio_handle, "audio_handle") = Some(handle);
            *crate::lock_or_recover!(self.recording_permit, "recording_permit") =
                Some(permit.clone());
            *self
                .recording_start
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
        }

        // Start the best-effort local lane immediately after microphone capture,
        // before the selected provider has finished connecting. This preserves
        // early speech even for a cold cloud or large local final engine.
        if let Some(preview_rx) = preview_rx {
            let preview_app = self.app_handle.clone();
            let preview_session_counter = self.recording_session.clone();
            let preview_config = SttConfig {
                live_preview_enabled: true,
                ..stt_config.clone()
            };
            let preview_threads = config_data.sensevoice_num_threads;
            tokio::spawn(run_local_preview_lane(
                preview_app,
                preview_rx,
                recording_session,
                preview_session_counter,
                preview_threads,
                preview_config,
            ));
        }

        // Volume monitoring task
        let app_handle = self.app_handle.clone();
        let audio_handle_ref = self.audio_handle.clone();
        let state_ref = self.state.clone();
        let volume_session = self.recording_session.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(VOLUME_POLL_INTERVAL_MS)).await;
                let current = PipelineState::from_u8(state_ref.load(Ordering::SeqCst));
                if current != PipelineState::Recording
                    || volume_session.load(Ordering::SeqCst) != recording_session
                {
                    break;
                }
                let vol = audio_handle_ref
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .map(|h| h.get_volume())
                    .unwrap_or(0.0);
                let _ = app_handle.emit("audio:volume", vol);
            }
        });

        // Start timing at microphone start, not after a potentially cold model
        // connection. Native PCM already enforces the same cap independently.
        let maximum_ms = permit.max_duration_ms;
        let timer_handle = self.app_handle.clone();
        let timer_session = self.recording_session.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(maximum_ms)).await;
            if timer_session.load(Ordering::SeqCst) != recording_session {
                return;
            }
            let pipeline = timer_handle.state::<PipelineHandle>();
            if pipeline.current_state() == PipelineState::Recording {
                pipeline.emit_message_for_session(
                    recording_session,
                    "pipeline:notice",
                    "已达到本次可用录音时长，正在自动识别并保留文字。",
                );
                if let Err(error) = pipeline.stop_for_session(recording_session).await {
                    tracing::error!("Automatic recording stop failed: {error}");
                }
            }
        });

        let setup_abort_notify = crate::lock_or_recover!(self.abort_notify, "abort_notify").clone();
        let connect_result = tokio::select! {
            result = provider.connect(&stt_config) => Some(result),
            _ = setup_abort_notify.notified() => None,
        };
        let Some(connect_result) = connect_result else {
            // abort() normally consumed the handle already. This is deliberately
            // idempotent and also covers a notification racing capture setup.
            self.stop_capture();
            return Ok(());
        };
        if let Err(e) = connect_result {
            if !self.session_is_active(recording_session) {
                self.stop_capture();
                return Ok(());
            }
            tracing::error!("STT connect failed: {}", e);
            self.emit_message_for_session(
                recording_session,
                "pipeline:error",
                format!("STT connection failed: {e}"),
            );
            self.stop_capture();
            *self
                .preloaded_config
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            *self
                .preloaded_app_ctx
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            *self
                .preloaded_dictionary
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
            return Ok(());
        }
        if !self.session_is_active(recording_session) {
            self.stop_capture();
            return Ok(());
        }

        // Selected text will be captured in stop() after hotkey is released,
        // so Ctrl+C simulation won't conflict with held keys.
        *self
            .preloaded_selected_text
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;

        // STT task — the provider is ready and receives every queued chunk,
        // including audio captured while a cold model was initializing.
        let app_handle = self.app_handle.clone();
        let accumulated =
            crate::lock_or_recover!(self.accumulated_text, "accumulated_text").clone();
        let stt_done = crate::lock_or_recover!(self.stt_done, "stt_done").clone();
        let stt_session = self.recording_session.clone();

        tokio::spawn(async move {
            // Forward audio to STT and receive transcripts
            let mut failed = false;
            let mut stt_revision = 0u64;
            loop {
                if stt_session.load(Ordering::SeqCst) != recording_session {
                    return;
                }
                tokio::select! {
                    chunk = audio_rx.recv() => {
                        match chunk {
                            Some(data) => {
                                if stt_session.load(Ordering::SeqCst) != recording_session { return; }
                                if let Err(error) = provider.send_audio(&data).await {
                                    if stt_session.load(Ordering::SeqCst) != recording_session { return; }
                                    tracing::error!("STT audio delivery failed: {error}");
                                    app_handle.state::<PipelineHandle>().report_stt_error(
                                        recording_session,
                                        format!("STT audio delivery failed: {error:#}"),
                                    );
                                    failed = true;
                                    break;
                                }
                            }
                            None => {
                                // Audio channel closed — disconnect and capture final transcript
                                tracing::debug!("Audio channel closed, calling provider.disconnect()");
                                match provider.disconnect().await {
                                    Ok(Some(text)) => {
                                        if stt_session.load(Ordering::SeqCst) != recording_session { return; }
                                        tracing::debug!("disconnect() returned Ok(Some(text)), len={}", text.len());
                                        let mut acc = crate::lock_or_recover!(accumulated, "accumulated_text");
                                        acc.push_str(&text);
                                        let current = acc.clone();
                                        drop(acc);
                                        tracing::debug!(
                                            "Emitting stt:final event ({} bytes)",
                                            current.len()
                                        );
                                        stt_revision = stt_revision.saturating_add(1);
                                        emit_session_text(
                                            &app_handle,
                                            "stt:final",
                                            recording_session,
                                            stt_revision,
                                            current,
                                        );
                                    }
                                    Ok(None) => {
                                        tracing::warn!("disconnect() returned Ok(None) - no text recognized");
                                    }
                                    Err(e) => {
                                        if stt_session.load(Ordering::SeqCst) != recording_session { return; }
                                        tracing::error!("STT disconnect error: {}", e);
                                        app_handle.state::<PipelineHandle>().report_stt_error(
                                            recording_session, format!("STT error: {e:#}"),
                                        );
                                    }
                                }
                                tracing::debug!("Breaking out of STT loop");
                                break;
                            }
                        }
                    }
                    transcript = provider.recv_transcript() => {
                        if stt_session.load(Ordering::SeqCst) != recording_session { return; }
                        match transcript {
                            Ok(Some(TranscriptEvent::Partial { text })) => {
                                stt_revision = stt_revision.saturating_add(1);
                                emit_session_text(
                                    &app_handle,
                                    "stt:partial",
                                    recording_session,
                                    stt_revision,
                                    text,
                                );
                            }
                            Ok(Some(TranscriptEvent::Final { text, .. })) => {
                                let mut acc = crate::lock_or_recover!(accumulated, "accumulated_text");
                                acc.push_str(&text);
                                acc.push(' ');
                                let current = acc.clone();
                                drop(acc);
                                stt_revision = stt_revision.saturating_add(1);
                                emit_session_text(
                                    &app_handle,
                                    "stt:final",
                                    recording_session,
                                    stt_revision,
                                    current,
                                );
                            }
                            Ok(Some(TranscriptEvent::Error { message })) => {
                                tracing::error!("STT error: {}", message);
                                app_handle.state::<PipelineHandle>().report_stt_error(
                                    recording_session, format!("STT error: {message}"),
                                );
                                // Break out of the loop — STT has failed, no point
                                // continuing. Without break, the loop keeps running
                                // and the pipeline stays stuck in Recording forever.
                                failed = true;
                                break;
                            }
                            Err(e) => {
                                tracing::error!("STT recv error: {}", e);
                                app_handle.state::<PipelineHandle>().report_stt_error(
                                    recording_session, format!("STT error: {e:#}"),
                                );
                                failed = true;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Signal that STT processing is complete
            stt_done.notify_one();
            if failed {
                drop(provider);
                drop(audio_rx);
                let pipeline = app_handle.state::<PipelineHandle>();
                if stt_session.load(Ordering::SeqCst) == recording_session
                    && pipeline.current_state() == PipelineState::Recording
                {
                    let _ = pipeline.stop_for_session(recording_session).await;
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_for_session(self.recording_session.load(Ordering::SeqCst))
            .await
    }

    async fn stop_for_session(&self, recording_session: u64) -> Result<()> {
        let _guard = self.pipeline_lock.lock().await;
        if self.recording_session.load(Ordering::SeqCst) != recording_session {
            return Ok(());
        }
        let (activated, minimum_ms) =
            crate::lock_or_recover!(self.recording_permit, "recording_permit")
                .as_ref()
                .map(|permit| (permit.activated, permit.max_duration_ms.min(500)))
                .unwrap_or((false, 0));
        let abort_notify = crate::lock_or_recover!(self.abort_notify, "abort_notify").clone();
        // start() and stop() share pipeline_lock, so acquiring it already proves
        // provider setup has finished. Only genuinely short recordings need a
        // delay; adding a fixed debounce here penalizes every dictation.
        // Require at least 500 ms to avoid empty captures.
        let recording_duration = self
            .recording_start
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|start| start.elapsed());

        if let Some(duration) = recording_duration {
            if duration < std::time::Duration::from_millis(minimum_ms) {
                tracing::warn!(
                    "Recording too short ({}ms), waiting...",
                    duration.as_millis()
                );
                tokio::time::sleep(std::time::Duration::from_millis(minimum_ms) - duration).await;
            }
        }
        if !self.transition_state_for_session(
            recording_session,
            PipelineState::Recording,
            PipelineState::Transcribing,
        ) {
            return Ok(());
        }

        let stop_start = std::time::Instant::now();

        let config_data = self
            .preloaded_config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_default();

        // Stop audio at the key-up boundary before doing clipboard/context work.
        // Closing the channel lets local STT start final inference immediately;
        // selected-text capture below then overlaps with that inference.
        let (captured_audio_chunks, captured_audio_peak, dropped_audio_chunks, captured_audio_ms) =
            self.stop_capture();
        tracing::info!(
            "Audio capture summary: chunks={}, peak={:.5}, dropped={}",
            captured_audio_chunks,
            captured_audio_peak,
            dropped_audio_chunks
        );

        let selected_text =
            if activated && config_data.selected_text_enabled && is_accessibility_trusted() {
                tokio::time::sleep(std::time::Duration::from_millis(
                    SELECTED_TEXT_CAPTURE_DELAY_MS,
                ))
                .await;
                tokio::task::block_in_place(|| self.capture_selected_text())
            } else {
                None
            };
        tracing::info!(
            "Selected text result: len={}",
            selected_text.as_deref().map(|s| s.len()).unwrap_or(0)
        );
        *self
            .preloaded_selected_text
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = selected_text;

        // P2-1: Pre-build LLM resources while waiting for STT
        let preloaded_config = self
            .preloaded_config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let mut config = match preloaded_config {
            Some(c) => c,
            None => self.load_config().await,
        };
        apply_trial_restrictions(&mut config, activated);
        let app_ctx = self
            .preloaded_app_ctx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap_or_else(app_detector::detect_current_app);
        let mut dictionary_entries = self
            .preloaded_dictionary
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .unwrap_or_default();
        if !activated {
            dictionary_entries.clear();
        }
        let dictionary_words = dictionary_entries
            .iter()
            .map(|entry| entry.word.clone())
            .collect::<Vec<_>>();
        let selected_text = self
            .preloaded_selected_text
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();

        // Keep the session lock through output. abort() is synchronous and can
        // still stop immediately; a later start waits until this session exits.

        // Always use batch output: keyboard mode uses output_text() after full LLM
        // response arrives. Streaming chunk-by-chunk clipboard paste was unreliable
        // on Windows — each Ctrl+V is async and the next set_text() could overwrite
        // the clipboard before the target app processed the previous paste, producing
        // garbled output that differed from what History recorded.

        // Pre-build LLM provider and Enigo while STT is still processing
        let pre_llm = if config.polish_enabled
            && (!config.llm_api_key.is_empty()
                || config.llm_provider == "cloud"
                || config.llm_provider == "local-llama"
                || config.llm_provider == "ollama")
        {
            let llm_api_key = if config.llm_provider == "cloud" {
                self.app_handle
                    .state::<SessionTokenStore>()
                    .0
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            } else {
                config.llm_api_key.clone()
            };

            let llm_base_url = if config.llm_provider == "local-llama" {
                let local_state = self.app_handle.state::<LocalLlmServerState>();
                let server = local_state
                    .0
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if !server.is_running() {
                    let model_filename = if config.local_llm_model.trim().is_empty() {
                        llm::local_server::DEFAULT_LLM_MODEL
                    } else {
                        config.local_llm_model.as_str()
                    };
                    let custom_dir = (!config.local_llm_model_dir.trim().is_empty())
                        .then_some(config.local_llm_model_dir.as_str());
                    let start_config = llm::local_server::StartConfig {
                        port: config.local_llm_port,
                        num_threads: config.local_llm_threads.clamp(1, 16),
                        ctx_size: config.local_llm_ctx_size.clamp(512, 8192),
                    };
                    if let Err(error) =
                        server.start(&self.app_handle, model_filename, start_config, custom_dir)
                    {
                        tracing::warn!("Unable to start managed local polish service: {error}");
                    }
                }
                // The managed process is the source of truth. Never route a
                // portable installation through a stale saved URL or port.
                server.base_url()
            } else {
                config.llm_base_url.clone()
            };

            let llm_config = LlmConfig {
                api_key: llm_api_key,
                model: config.llm_model.clone(),
                base_url: llm_base_url,
                max_tokens: if config.llm_provider == "local-llama" {
                    512
                } else {
                    4096
                },
                temperature: 0.3,
            };
            let provider =
                llm::create_provider(&config.llm_provider, Some(self.shared_client.clone()));
            Some((llm_config, provider))
        } else {
            None
        };

        let stt_done = crate::lock_or_recover!(self.stt_done, "stt_done").clone();
        let finalize_seconds =
            stt_finalize_timeout_seconds(&config.stt_provider, captured_audio_ms);
        tokio::select! {
            _ = stt_done.notified() => {
                tracing::debug!("STT task completed");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(finalize_seconds)) => {
                tracing::warn!("STT task timed out after {}s", finalize_seconds);
                let message = if config.stt_provider == "native-asr" {
                    "本地大模型处理超时。请缩短录音，或改用 SenseVoice；未输出不完整结果。"
                } else {
                    "语音识别处理超时，未输出不完整结果。请检查服务后重试。"
                };
                self.emit_message_for_session(recording_session, "pipeline:error", message);
                // Invalidate the still-running provider task. Continuing with its
                // current accumulator would make an incomplete partial look like
                // an authoritative final result, and the late task could otherwise
                // emit into an already completed UI session.
                self.abort();
                return Ok(());
            }
        }

        let stt_elapsed = stop_start.elapsed();
        tracing::info!(
            "[Pipeline Timing] STT finalize: {}ms",
            stt_elapsed.as_millis()
        );

        // Check if pipeline was aborted while waiting for STT
        if !self.session_is_active(recording_session) {
            tracing::info!("Pipeline aborted after STT wait, skipping LLM and output");
            return Ok(());
        }

        let accumulated =
            crate::lock_or_recover!(self.accumulated_text, "accumulated_text").clone();
        let raw_text = accumulated
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trim()
            .to_string();

        let dictionary_rules = dictionary_entries
            .iter()
            .map(|entry| crate::stt::hotword_replacer::HotwordRule {
                word: entry.word.clone(),
                pronunciation: entry.pronunciation.clone(),
                correction_from: entry.correction_from.clone(),
            })
            .collect::<Vec<_>>();
        let dictionary_started = std::time::Instant::now();
        let raw_text = if activated {
            crate::stt::hotword_replacer::apply_dictionary(&raw_text, &dictionary_rules)
        } else {
            raw_text
        };
        tracing::info!(
            "[Pipeline Timing] Dictionary correction: {}ms ({} rules)",
            dictionary_started.elapsed().as_millis(),
            dictionary_rules.len()
        );

        if raw_text.is_empty() {
            if !self.session_is_active(recording_session) {
                return Ok(());
            }
            let provider_error = crate::lock_or_recover!(self.stt_error, "stt_error");
            let message = empty_transcript_error_message(
                provider_error.message_for(recording_session),
                captured_audio_chunks,
                captured_audio_peak,
            );
            drop(provider_error);
            tracing::warn!(
                "Empty transcript: chunks={}, peak={:.5}, provider={}",
                captured_audio_chunks,
                captured_audio_peak,
                config.stt_provider
            );
            if let Some(message) = message {
                self.emit_message_for_session(recording_session, "pipeline:error", message);
            }
            let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
            return Ok(());
        }

        let final_text;
        let llm_elapsed;

        // Polish with LLM (resources already pre-built)
        // Check abort before entering LLM polish and output
        if !self.session_is_active(recording_session) {
            tracing::info!("Pipeline aborted before LLM/output");
            return Ok(());
        }

        if let Some((llm_config, provider)) = pre_llm {
            if !self.set_state_for_session(recording_session, PipelineState::Polishing) {
                return Ok(());
            }
            let llm_start = std::time::Instant::now();

            if config.llm_provider == "local-llama" {
                // llama-server accepts the process before the model is fully
                // loaded. Wait briefly for /health so the first dictation after
                // launch is polished instead of silently falling back to raw.
                let health_url = format!(
                    "{}/health",
                    llm_config
                        .base_url
                        .trim_end_matches('/')
                        .trim_end_matches("/v1")
                );
                let ready_deadline =
                    tokio::time::Instant::now() + std::time::Duration::from_secs(10);
                loop {
                    if !self.session_is_active(recording_session) {
                        return Ok(());
                    }
                    let healthy = self
                        .shared_client
                        .get(&health_url)
                        .timeout(std::time::Duration::from_secs(1))
                        .send()
                        .await
                        .is_ok_and(|response| response.status().is_success());
                    if healthy {
                        break;
                    }
                    if tokio::time::Instant::now() >= ready_deadline {
                        tracing::warn!("Managed local polish service was not ready after 10s");
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }

            // on_chunk only drives the UI transcript display; actual output happens
            // in batch after the full response arrives (see output_text below).
            let app_handle = self.app_handle.clone();
            let chunk_session = self.recording_session.clone();
            let chunk_revision = Arc::new(AtomicU64::new(0));
            let on_chunk: llm::ChunkCallback = Box::new(move |chunk: &str| {
                if chunk_session.load(Ordering::SeqCst) != recording_session {
                    return;
                }
                let revision = chunk_revision.fetch_add(1, Ordering::SeqCst) + 1;
                emit_session_text(&app_handle, "llm:chunk", recording_session, revision, chunk);
            });

            let polish_mode = config.polish_mode;
            // CPU-only local inference needs a longer first-token allowance
            // than a network API, especially immediately after model loading.
            let timeout_secs = if config.llm_provider == "local-llama" {
                if polish_mode == crate::llm::PolishMode::Fast {
                    15u64
                } else {
                    30u64
                }
            } else if polish_mode == crate::llm::PolishMode::Fast {
                3u64
            } else {
                12u64
            };

            // Gather recent context sentences for continuity.
            let context_sentences: Vec<String> = self
                .context_store
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .cloned()
                .collect();

            let req = PolishRequest {
                raw_text: raw_text.clone(),
                app_type: app_ctx.app_type,
                dictionary: dictionary_words,
                translate_enabled: config.translate_enabled,
                target_lang: config.target_lang.clone(),
                selected_text,
                polish_mode,
                stt_language: config.stt_language.clone(),
                context: context_sentences,
            };

            let polish_result = tokio::select! {
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    provider.polish(&llm_config, &req, Some(&on_chunk)),
                ) => result,
                _ = abort_notify.notified() => return Ok(()),
            };

            let polished = match polish_result {
                Ok(Ok(response)) if response.polished_text.trim().is_empty() => {
                    tracing::warn!("LLM polish returned empty text, preserving raw transcript");
                    self.emit_message_for_session(
                        recording_session,
                        "pipeline:notice",
                        "本地 AI 润色未返回文字，已保留原始识别结果。",
                    );
                    raw_text.clone()
                }
                Ok(Ok(response)) => response.polished_text,
                Ok(Err(e)) => {
                    tracing::error!("LLM polish failed: {}, outputting raw text", e);
                    self.emit_message_for_session(
                        recording_session,
                        "pipeline:notice",
                        "AI 润色暂时不可用，已保留原始识别结果。",
                    );
                    raw_text.clone()
                }
                Err(_) => {
                    tracing::warn!(
                        "LLM polish timed out after {}s, outputting raw text",
                        timeout_secs
                    );
                    self.emit_message_for_session(
                        recording_session,
                        "pipeline:notice",
                        "AI 润色响应超时，已保留原始识别结果。",
                    );
                    raw_text.clone()
                }
            };

            llm_elapsed = llm_start.elapsed();

            if !self.session_is_active(recording_session) {
                tracing::info!("Pipeline aborted after LLM polish, skipping output");
                return Ok(());
            }

            final_text = polished;

            tracing::info!(
                "[Pipeline Timing] LLM polish: {}ms",
                llm_elapsed.as_millis()
            );
        } else {
            llm_elapsed = std::time::Duration::ZERO;
            final_text = raw_text.clone();
        }

        if !self.session_is_active(recording_session) {
            tracing::info!("Pipeline aborted before resolving final text");
            return Ok(());
        }

        // This is the exact text that will be saved and delivered after local
        // dictionary correction and optional LLM processing. It is the
        // authoritative replacement for every approximate/interim preview.
        emit_session_text(
            &self.app_handle,
            "pipeline:resolved",
            recording_session,
            1,
            final_text.clone(),
        );

        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let pending_duration_ms = Some(captured_audio_ms as i64);
        let pending_entry = storage::HistoryEntry {
            id: 0,
            created_at: now.clone(),
            app_name: app_ctx.app_name.clone(),
            app_type: format!("{:?}", app_ctx.app_type),
            raw_text: raw_text.clone(),
            polished_text: final_text.clone(),
            language: Some(config.stt_language.clone()),
            duration_ms: pending_duration_ms,
        };
        if let Err(error) = storage::save_pending_transcript(&self.app_handle, &pending_entry) {
            tracing::error!("Unable to journal completed transcript: {error}");
        }

        if !self.session_is_active(recording_session) {
            return Ok(());
        }

        if let Err(e) = self
            .output_text(
                recording_session,
                &final_text,
                &app_ctx.app_name,
                &app_ctx.window_title,
                &config,
            )
            .await
        {
            tracing::error!("Output failed: {}", e);
            self.emit_message_for_session(
                recording_session,
                "pipeline:error",
                format!("Output failed: {e}"),
            );
        }

        if !self.session_is_active(recording_session) {
            return Ok(());
        }

        let total_elapsed = stop_start.elapsed();

        // Compute recording duration
        self.recording_start
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let duration_ms = Some(captured_audio_ms as i64);

        tracing::info!(
            "[Pipeline Timing] Total stop(): {}ms (STT: {}ms, LLM: {}ms, Output+Save: {}ms)",
            total_elapsed.as_millis(),
            stt_elapsed.as_millis(),
            llm_elapsed.as_millis(),
            total_elapsed.as_millis() - stt_elapsed.as_millis() - llm_elapsed.as_millis(),
        );

        // Emit timing to frontend
        let _ = self.app_handle.emit(
            "pipeline:timing",
            serde_json::json!({
                "stt_ms": stt_elapsed.as_millis() as u64,
                "llm_ms": llm_elapsed.as_millis() as u64,
                "total_ms": total_elapsed.as_millis() as u64,
                "recording_ms": duration_ms,
            }),
        );

        // Push polished result into context ring buffer (max 3 sentences).
        if !final_text.is_empty() {
            let mut ctx = crate::lock_or_recover!(self.context_store, "context_store");
            if ctx.len() >= 3 {
                ctx.pop_front();
            }
            ctx.push_back(final_text.clone());
        }

        // Save to history
        let entry_app_name = app_ctx.app_name.clone();
        let entry = storage::HistoryEntry {
            id: 0, // auto-increment
            created_at: now,
            app_name: app_ctx.app_name,
            app_type: format!("{:?}", app_ctx.app_type),
            raw_text: raw_text.clone(),
            polished_text: final_text.clone(),
            language: Some(config.stt_language.clone()),
            duration_ms,
        };
        let history_id = match self
            .app_handle
            .state::<storage::HistoryStore>()
            .add(entry)
            .await
        {
            Ok(id) => {
                let _ = self.app_handle.emit("rewards:updated", ());
                if let Err(error) = storage::clear_pending_transcript(&self.app_handle) {
                    tracing::warn!("Unable to clear transcript recovery journal: {error}");
                }
                Some(id)
            }
            Err(e) => {
                tracing::error!("Failed to save history: {}", e);
                None
            }
        };

        if !self.session_is_active(recording_session) {
            return Ok(());
        }

        // If configured to open the editor overlay, emit the payload after the row
        // is persisted so the frontend can save edits back to that specific id.
        if config.output_mode == "editor" {
            if let Some(id) = history_id {
                open_editor_window(&self.app_handle);
                let _ = self.app_handle.emit_to(
                    "editor",
                    "editor:show",
                    serde_json::json!({
                        "id": id,
                        "text": final_text,
                        "raw_text": raw_text,
                        "app_name": entry_app_name,
                        "language": config.stt_language,
                        "auto_hide_enabled": config.editor_auto_hide_enabled,
                        "auto_hide_seconds": config.editor_auto_hide_seconds,
                    }),
                );
            }
        }

        let _ = self.set_state_for_session(recording_session, PipelineState::Idle);
        Ok(())
    }

    async fn output_text(
        &self,
        recording_session: u64,
        text: &str,
        app_name: &str,
        window_title: &str,
        config: &storage::AppConfig,
    ) -> Result<()> {
        if !self.set_state_for_session(recording_session, PipelineState::Outputting) {
            return Ok(());
        }

        // Editor mode: skip auto-typing. The overlay is shown after the
        // history row is persisted, so we can carry the row id with it.
        if config.output_mode == "editor" {
            self.emit_string_for_session(recording_session, "pipeline:target_app", app_name);
            return Ok(());
        }

        // Never type into an application that gained focus while recognition was
        // running. Copying is lossless and avoids leaking dictated text into the
        // wrong window.
        let current_app = app_detector::detect_current_app();
        let application_changed = !app_name.is_empty()
            && !current_app.app_name.is_empty()
            && !current_app.app_name.eq_ignore_ascii_case(app_name);
        let window_changed = !window_title.is_empty()
            && !current_app.window_title.is_empty()
            && current_app.window_title != window_title;
        if application_changed || window_changed {
            if !self.session_is_active(recording_session) {
                return Ok(());
            }
            let safe_text = output::sanitize_output(text);
            output::clipboard::copy_only(&safe_text)?;
            self.emit_string_for_session(
                recording_session,
                "pipeline:output_fallback",
                "焦点已切换，识别结果已复制到剪贴板，未自动粘贴。",
            );
            return Ok(());
        }

        // Try keyboard first if configured, fall back to clipboard if no accessibility
        let effective_mode = if config.output_mode == "keyboard" && is_accessibility_trusted() {
            #[cfg(target_os = "macos")]
            {
                // enigo text input may call HIToolbox APIs that must run on main queue.
                // Use clipboard paste on macOS to avoid sporadic SIGTRAP crashes.
                OutputMode::Clipboard
            }
            #[cfg(not(target_os = "macos"))]
            {
                OutputMode::Keyboard
            }
        } else {
            OutputMode::Clipboard
        };

        let output = output::create_output(effective_mode);
        let safe_text = output::sanitize_output(text);
        if !self.session_is_active(recording_session) {
            return Ok(());
        }
        if let Err(error) = output.type_text(&safe_text).await {
            tracing::warn!("Primary output failed: {error}");
            if !self.session_is_active(recording_session) {
                return Ok(());
            }
            if effective_mode == OutputMode::Keyboard {
                let fallback = output::clipboard::ClipboardOutput::new();
                use crate::output::TextOutput;
                if fallback.type_text(&safe_text).await.is_ok() {
                    self.emit_string_for_session(
                        recording_session,
                        "pipeline:output_fallback",
                        "键盘输入不可用，已自动改用剪贴板粘贴。",
                    );
                } else {
                    output::clipboard::copy_only(&safe_text)?;
                    self.emit_string_for_session(
                        recording_session,
                        "pipeline:output_fallback",
                        "自动粘贴失败，识别结果已复制，请按 Ctrl+V 粘贴。",
                    );
                }
            } else {
                output::clipboard::copy_only(&safe_text)?;
                self.emit_string_for_session(
                    recording_session,
                    "pipeline:output_fallback",
                    "自动粘贴失败，识别结果已复制，请手动粘贴。",
                );
            }
        }

        self.emit_string_for_session(recording_session, "pipeline:target_app", app_name);

        Ok(())
    }

    /// P1-2: Pre-warm HTTP connection pool by issuing a HEAD request to the STT endpoint.
    /// Call once after app startup to avoid cold-start TLS handshake on first recording.
    pub async fn pre_warm(&self) {
        if !self
            .app_handle
            .state::<ActivationService>()
            .status()
            .map(|status| status.activated)
            .unwrap_or(false)
        {
            return;
        }
        let config = self.load_config().await;

        // Pre-warm STT endpoint
        let stt_endpoint = match config.stt_provider.as_str() {
            "cloud" => {
                let base = crate::api_base_url();
                format!("{}/api/proxy/stt", base)
            }
            "glm-asr" => "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions".to_string(),
            "openai-whisper" => "https://api.openai.com/v1/audio/transcriptions".to_string(),
            "groq-whisper" => "https://api.groq.com/openai/v1/audio/transcriptions".to_string(),
            "siliconflow" => "https://api.siliconflow.cn/v1/audio/transcriptions".to_string(),
            "custom-whisper" => {
                if config.stt_base_url.trim().is_empty() {
                    tracing::debug!("custom-whisper endpoint is empty, skipping pre-warm");
                    return;
                }
                config.stt_base_url.clone()
            }
            "xiaomi-mimo" => {
                if config.stt_base_url.trim().is_empty() {
                    "https://token-plan-cn.xiaomimimo.com/v1/chat/completions".to_string()
                } else {
                    stt::xiaomi_mimo::normalize_xiaomi_endpoint(&config.stt_base_url)
                }
            }
            "deepgram" => "https://api.deepgram.com/v1/listen".to_string(),
            "assemblyai" => "https://api.assemblyai.com/v2/transcript".to_string(),
            "volcengine-seedasr" => {
                "https://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".to_string()
            }
            _ => {
                tracing::debug!(
                    "Unknown STT provider '{}', skipping pre-warm",
                    config.stt_provider
                );
                return;
            }
        };
        tracing::debug!("Pre-warming HTTP connection to {}", stt_endpoint);
        let _ = self
            .shared_client
            .head(&stt_endpoint)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;
        tracing::debug!("STT connection pre-warm complete");

        // Pre-warm LLM endpoint if polish is enabled
        if config.polish_enabled {
            let llm_url = if config.llm_provider == "cloud" {
                let base = crate::api_base_url();
                format!("{}/api/proxy/llm", base)
            } else {
                config.llm_base_url.clone()
            };
            tracing::debug!("Pre-warming LLM connection to {}", llm_url);
            let _ = self
                .shared_client
                .head(&llm_url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await;
            tracing::debug!("LLM connection pre-warm complete");
        }
    }
}

#[cfg(test)]
mod activation_pipeline_tests {
    use super::*;

    #[test]
    fn session_message_payload_carries_identity_revision_and_message() {
        let payload = serde_json::to_value(PipelineMessageEvent {
            session_id: 17,
            revision: 23,
            message: "late provider error".into(),
        })
        .unwrap();
        assert_eq!(payload["session_id"], 17);
        assert_eq!(payload["revision"], 23);
        assert_eq!(payload["message"], "late provider error");
    }

    #[test]
    fn stt_error_preserves_first_provider_failure_for_current_session() {
        let mut error = SessionSttError::default();
        error.reset(7);
        assert!(error.record(7, "模型 SHA-256 不匹配，请重新下载".into()));
        assert!(!error.record(7, "后续断开连接失败".into()));
        assert_eq!(
            error.message_for(7),
            Some("模型 SHA-256 不匹配，请重新下载")
        );
    }

    #[test]
    fn stt_error_reset_prevents_previous_recording_from_poisoning_next_session() {
        let mut error = SessionSttError::default();
        error.reset(11);
        assert!(error.record(11, "旧录音模型失败".into()));
        error.reset(13); // abort and the next start each advance the native session id.
        assert_eq!(error.message_for(11), None);
        assert_eq!(error.message_for(13), None);
        assert!(!error.record(11, "上一段迟到的网络错误".into()));
        assert_eq!(error.message_for(13), None);
        assert!(error.record(13, "新录音实际错误".into()));
        assert!(!error.record(11, "旧录音第二次报错".into()));
        assert_eq!(error.message_for(13), Some("新录音实际错误"));
    }

    #[test]
    fn stt_error_retains_actionable_native_source_inside_context_chain() {
        let native_error = anyhow::anyhow!("not enough memory").context("原生 CPU 模型加载失败");
        let mut error = SessionSttError::default();
        error.reset(1);
        assert!(error.record(1, format!("STT error: {native_error:#}")));
        assert_eq!(
            error.message_for(1),
            Some("STT error: 原生 CPU 模型加载失败: not enough memory")
        );
    }

    #[test]
    fn stt_error_suppresses_generic_empty_audio_message_for_every_capture_condition() {
        let mut error = SessionSttError::default();
        error.reset(4);
        error.record(4, "原生 CPU 模型加载失败".into());
        for (chunks, peak) in [(0, 0.0), (25, 0.001), (25, 0.5)] {
            assert_eq!(
                empty_transcript_error_message(error.message_for(4), chunks, peak),
                None
            );
        }
    }

    #[test]
    fn empty_audio_without_provider_failure_still_offers_specific_microphone_help() {
        assert!(empty_transcript_error_message(None, 0, 0.0)
            .unwrap()
            .contains("未收到麦克风音频"));
        assert!(empty_transcript_error_message(None, 10, 0.001)
            .unwrap()
            .contains("没有声音信号"));
        assert!(empty_transcript_error_message(None, 10, 0.5)
            .unwrap()
            .contains("未识别出语音"));
        let mut old_error = SessionSttError::default();
        old_error.reset(1);
        old_error.record(1, "已经结束的录音错误".into());
        assert!(empty_transcript_error_message(old_error.message_for(2), 10, 0.5).is_some());
    }

    #[test]
    fn native_cpu_finalize_budget_is_duration_based_but_bounded() {
        assert_eq!(stt_finalize_timeout_seconds("sensevoice", 600_000), 120);
        assert_eq!(stt_finalize_timeout_seconds("native-asr", 1_000), 120);
        assert_eq!(stt_finalize_timeout_seconds("native-asr", 60_000), 420);
        assert_eq!(stt_finalize_timeout_seconds("native-asr", u64::MAX), 1800);
    }

    #[test]
    fn trial_snapshot_disables_all_llm_enrichment_without_changing_saved_config() {
        let saved = storage::AppConfig {
            polish_enabled: true,
            translate_enabled: true,
            selected_text_enabled: true,
            ..Default::default()
        };
        let mut snapshot = saved.clone();
        apply_trial_restrictions(&mut snapshot, false);
        assert!(!snapshot.polish_enabled);
        assert!(!snapshot.translate_enabled);
        assert!(!snapshot.selected_text_enabled);
        assert!(saved.polish_enabled && saved.translate_enabled && saved.selected_text_enabled);
    }

    #[test]
    fn activation_preserves_explicit_user_polish_preferences() {
        let mut snapshot = storage::AppConfig {
            polish_enabled: true,
            translate_enabled: false,
            selected_text_enabled: true,
            ..Default::default()
        };
        apply_trial_restrictions(&mut snapshot, true);
        assert!(snapshot.polish_enabled);
        assert!(!snapshot.translate_enabled);
        assert!(snapshot.selected_text_enabled);
    }

    #[test]
    fn vendor_duration_cap_is_enforced_before_capture_independently_of_license() {
        let mut config = storage::AppConfig {
            stt_provider: "custom-whisper".into(),
            max_recording_seconds: 600,
            ..Default::default()
        };
        config.custom_cloud.vendor = "baidu".into();
        assert_eq!(maximum_recording_milliseconds(&config), 60_000);
        config.custom_cloud.vendor = "iflytek".into();
        assert_eq!(maximum_recording_milliseconds(&config), 60_000);
        config.max_recording_seconds = 15;
        assert_eq!(maximum_recording_milliseconds(&config), 15_000);
        config.stt_provider = "sensevoice".into();
        config.max_recording_seconds = 600;
        assert_eq!(maximum_recording_milliseconds(&config), 600_000);
    }
}

/// Bring the editor overlay window on screen without stealing keyboard focus
/// from whatever the user was typing into. If the window doesn't exist yet
/// (first-run in editor mode) build it lazily from the same URL as the main
/// bundle, addressed by `#editor` hash.
fn open_editor_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("editor") {
        let _ = win.show();
        let _ = win.set_always_on_top(true);
        return;
    }

    let url = tauri::WebviewUrl::App("index.html#editor".into());
    let builder = tauri::WebviewWindowBuilder::new(app, "editor", url)
        .title("PopSpeak Editor")
        .inner_size(560.0, 220.0)
        .min_inner_size(360.0, 160.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(true);
    if let Err(e) = builder.build() {
        tracing::error!("Failed to build editor window: {}", e);
    }
}
