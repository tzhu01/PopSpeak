use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaptureState {
    Idle,
    Recording,
}

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_duration_ms: u32,
    /// Optional secondary PCM fan-out used by the local live-preview lane.
    ///
    /// The primary receiver keeps `chunk_duration_ms` semantics. When this is
    /// `Some`, the exact same post-resample 16 kHz mono PCM samples are also
    /// grouped into preview chunks of this duration. Preview backpressure is
    /// isolated from the primary STT path.
    pub preview_chunk_duration_ms: Option<u32>,
    pub device_name: Option<String>,
    /// Native PCM hard cap, including audio buffered while STT connects.
    pub max_duration_ms: Option<u64>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
            chunk_duration_ms: 20,
            preview_chunk_duration_ms: None,
            device_name: None,
            max_duration_ms: None,
        }
    }
}

/// Fallback queue used by callers that do not set a native capture limit.
/// Pipeline recordings always set a limit and therefore reserve enough queue
/// slots for the whole bounded recording. The channel allocates storage lazily,
/// so a 10-minute limit does not eagerly consume ~20 MiB.
const DEFAULT_AUDIO_CHANNEL_CAPACITY: usize = 500;
/// Defensive ceiling for a 20 ms primary lane (10 minutes / 30,000 chunks).
const MAX_AUDIO_CHANNEL_CAPACITY: usize = 30_000;
/// One second of 100 ms preview chunks is enough recognition context while
/// keeping display latency bounded. When full, the preview-only queue evicts
/// its oldest chunk; the authoritative STT stream is never touched.
const PREVIEW_AUDIO_CHANNEL_CAPACITY: usize = 10;
const MAX_BUFFER_SAMPLES: usize = 12 * 1024 * 1024;

fn primary_audio_channel_capacity(config: &AudioConfig) -> usize {
    config
        .max_duration_ms
        .map(|duration_ms| {
            duration_ms
                .div_ceil(config.chunk_duration_ms.max(1) as u64)
                .try_into()
                .unwrap_or(MAX_AUDIO_CHANNEL_CAPACITY)
        })
        .unwrap_or(DEFAULT_AUDIO_CHANNEL_CAPACITY)
        .clamp(DEFAULT_AUDIO_CHANNEL_CAPACITY, MAX_AUDIO_CHANNEL_CAPACITY)
}

pub fn list_input_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut names = host
        .input_devices()
        .context("failed to enumerate input devices")?
        .filter_map(|device| device.name().ok())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

pub struct AudioCaptureHandle {
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    volume: Arc<Mutex<f32>>,
    state: Arc<Mutex<CaptureState>>,
    stopped: Arc<AtomicBool>,
    dropped_chunks: Arc<AtomicU64>,
    preview_dropped_chunks: Arc<AtomicU64>,
    captured_chunks: Arc<AtomicU64>,
    peak_volume: Arc<Mutex<f32>>,
    captured_samples: Arc<AtomicU64>,
    accepting: Arc<AtomicBool>,
    capture_gate: Arc<Mutex<()>>,
    sample_rate: u32,
}

/// Capture handle plus the authoritative and optional preview PCM receivers.
pub type AudioCaptureStreams = (
    AudioCaptureHandle,
    mpsc::Receiver<Vec<u8>>,
    Option<flume::Receiver<Vec<u8>>>,
);

#[derive(Clone)]
struct PreviewAudioSender {
    sender: flume::Sender<Vec<u8>>,
    /// A receiver clone lets the single capture producer evict one stale item
    /// before retrying. The actual preview consumer observes the same queue.
    evictor: flume::Receiver<Vec<u8>>,
}

impl PreviewAudioSender {
    fn try_send_latest(&self, bytes: Vec<u8>, dropped: &AtomicU64) {
        match self.sender.try_send(bytes) {
            Ok(()) => {}
            Err(flume::TrySendError::Full(bytes)) => {
                if self.evictor.try_recv().is_ok() {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
                if self.sender.try_send(bytes).is_err() {
                    // The only remaining normal failure is receiver shutdown.
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[derive(Clone)]
struct CaptureTelemetry {
    volume: Arc<Mutex<f32>>,
    state: Arc<Mutex<CaptureState>>,
    dropped_chunks: Arc<AtomicU64>,
    preview_dropped_chunks: Arc<AtomicU64>,
    captured_chunks: Arc<AtomicU64>,
    peak_volume: Arc<Mutex<f32>>,
    captured_samples: Arc<AtomicU64>,
    accepting: Arc<AtomicBool>,
    capture_gate: Arc<Mutex<()>>,
}

#[derive(Debug)]
pub struct CaptureStartError {
    pub captured_ms: u64,
    message: String,
}

impl std::fmt::Display for CaptureStartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CaptureStartError {}

impl AudioCaptureHandle {
    pub fn start(config: AudioConfig) -> Result<AudioCaptureStreams> {
        if config.sample_rate == 0
            || config.chunk_duration_ms == 0
            || config.preview_chunk_duration_ms == Some(0)
        {
            anyhow::bail!("Audio sample rate and chunk duration must be positive");
        }
        let (audio_tx, audio_rx) =
            mpsc::channel::<Vec<u8>>(primary_audio_channel_capacity(&config));
        let (preview_tx, preview_rx) = if config.preview_chunk_duration_ms.is_some() {
            let (sender, receiver) = flume::bounded::<Vec<u8>>(PREVIEW_AUDIO_CHANNEL_CAPACITY);
            (
                Some(PreviewAudioSender {
                    sender,
                    evictor: receiver.clone(),
                }),
                Some(receiver),
            )
        } else {
            (None, None)
        };
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<std::result::Result<(), String>>();
        let volume = Arc::new(Mutex::new(0.0));
        let state = Arc::new(Mutex::new(CaptureState::Recording));
        let stopped = Arc::new(AtomicBool::new(false));
        let dropped_chunks = Arc::new(AtomicU64::new(0));
        let preview_dropped_chunks = Arc::new(AtomicU64::new(0));
        let captured_chunks = Arc::new(AtomicU64::new(0));
        let peak_volume = Arc::new(Mutex::new(0.0));
        let captured_samples = Arc::new(AtomicU64::new(0));
        let accepting = Arc::new(AtomicBool::new(true));
        let capture_gate = Arc::new(Mutex::new(()));
        let sample_rate = config.sample_rate;

        let thread_stopped = Arc::clone(&stopped);
        let telemetry = CaptureTelemetry {
            volume: Arc::clone(&volume),
            state: Arc::clone(&state),
            dropped_chunks: Arc::clone(&dropped_chunks),
            preview_dropped_chunks: Arc::clone(&preview_dropped_chunks),
            captured_chunks: Arc::clone(&captured_chunks),
            peak_volume: Arc::clone(&peak_volume),
            captured_samples: Arc::clone(&captured_samples),
            accepting: Arc::clone(&accepting),
            capture_gate: Arc::clone(&capture_gate),
        };
        std::thread::Builder::new()
            .name("popspeak-audio-capture".to_string())
            .spawn(move || {
                let ready_error = ready_tx.clone();
                if let Err(error) =
                    run_capture(config, audio_tx, preview_tx, stop_rx, telemetry, ready_tx)
                {
                    let _ = ready_error.send(Err(error.to_string()));
                    tracing::error!("Audio capture thread error: {error}");
                }
                thread_stopped.store(true, Ordering::SeqCst);
            })?;

        let startup_error = match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Some("Microphone initialization timed out".to_string())
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Some("Microphone initialization failed".to_string())
            }
        };
        if let Some(message) = startup_error {
            let _gate = crate::lock_or_recover!(capture_gate, "audio_capture_gate");
            accepting.store(false, Ordering::SeqCst);
            let captured_ms = captured_samples
                .load(Ordering::SeqCst)
                .saturating_mul(1000)
                .div_ceil(sample_rate as u64);
            return Err(CaptureStartError {
                captured_ms,
                message,
            }
            .into());
        }

        Ok((
            Self {
                stop_tx: Some(stop_tx),
                volume,
                state,
                stopped,
                dropped_chunks,
                preview_dropped_chunks,
                captured_chunks,
                peak_volume,
                captured_samples,
                accepting,
                capture_gate,
                sample_rate,
            },
            audio_rx,
            preview_rx,
        ))
    }

    pub fn stop(&mut self) {
        // Serialize with the native callback, so the sample counter is final
        // when stop() returns even though stream teardown runs on its thread.
        let _gate = crate::lock_or_recover!(self.capture_gate, "audio_capture_gate");
        self.accepting.store(false, Ordering::SeqCst);
        self.stop_tx = None;
        *crate::lock_or_recover!(self.volume, "audio_volume") = 0.0;
        *crate::lock_or_recover!(self.state, "audio_state") = CaptureState::Idle;
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn get_volume(&self) -> f32 {
        *crate::lock_or_recover!(self.volume, "audio_volume")
    }

    pub fn state(&self) -> CaptureState {
        *crate::lock_or_recover!(self.state, "audio_state")
    }

    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    pub fn preview_dropped_chunks(&self) -> u64 {
        self.preview_dropped_chunks.load(Ordering::Relaxed)
    }

    pub fn captured_chunks(&self) -> u64 {
        self.captured_chunks.load(Ordering::Relaxed)
    }

    pub fn peak_volume(&self) -> f32 {
        *crate::lock_or_recover!(self.peak_volume, "audio_peak_volume")
    }

    pub fn captured_milliseconds(&self) -> u64 {
        self.captured_samples
            .load(Ordering::SeqCst)
            .saturating_mul(1000)
            .div_ceil(self.sample_rate.max(1) as u64)
    }
}

impl Drop for AudioCaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

struct CaptureProcessor {
    device_rate: u32,
    device_channels: u16,
    target_rate: u32,
    samples_per_chunk: usize,
    preview_samples_per_chunk: Option<usize>,
    source: VecDeque<f32>,
    phase: f64,
    pcm: VecDeque<i16>,
    preview_pcm: Option<VecDeque<i16>>,
    max_samples: Option<u64>,
    captured_samples: u64,
}

impl CaptureProcessor {
    fn new(
        device_rate: u32,
        device_channels: u16,
        target_rate: u32,
        chunk_ms: u32,
        preview_chunk_ms: Option<u32>,
        max_duration_ms: Option<u64>,
    ) -> Self {
        let preview_samples_per_chunk = preview_chunk_ms
            .map(|duration_ms| ((target_rate as u64 * duration_ms as u64 / 1000) as usize).max(1));
        Self {
            device_rate,
            device_channels,
            target_rate,
            samples_per_chunk: ((target_rate as u64 * chunk_ms as u64 / 1000) as usize).max(1),
            preview_samples_per_chunk,
            source: VecDeque::new(),
            phase: 0.0,
            pcm: VecDeque::new(),
            preview_pcm: preview_samples_per_chunk.map(|_| VecDeque::new()),
            max_samples: max_duration_ms.map(|ms| ms.saturating_mul(target_rate as u64) / 1000),
            captured_samples: 0,
        }
    }

    fn process(
        &mut self,
        samples: &[f32],
        sender: &mpsc::Sender<Vec<u8>>,
        preview_sender: Option<&PreviewAudioSender>,
        dropped: &AtomicU64,
        preview_dropped: &AtomicU64,
    ) -> u64 {
        if self.at_limit() {
            return 0;
        }
        let channels = self.device_channels.max(1) as usize;
        // Windows microphones frequently expose two channels even when only
        // one carries the microphone signal. Averaging halves that signal and
        // can fully cancel channels with opposite phase. Pick the loudest
        // channel for this callback; duplicated stereo remains unchanged.
        let loudest_channel = (0..channels)
            .max_by(|left, right| {
                let left_energy = samples
                    .iter()
                    .skip(*left)
                    .step_by(channels)
                    .map(|sample| sample * sample)
                    .sum::<f32>();
                let right_energy = samples
                    .iter()
                    .skip(*right)
                    .step_by(channels)
                    .map(|sample| sample * sample)
                    .sum::<f32>();
                left_energy.total_cmp(&right_energy)
            })
            .unwrap_or(0);
        for frame in samples.chunks_exact(channels) {
            self.source
                .push_back(frame[loudest_channel].clamp(-1.0, 1.0));
        }

        if self.device_rate == self.target_rate {
            while !self.at_limit() {
                let Some(sample) = self.source.pop_front() else {
                    break;
                };
                self.push_pcm(sample);
            }
        } else {
            let ratio = self.device_rate as f64 / self.target_rate as f64;
            while !self.at_limit() && self.phase + 1.0 < self.source.len() as f64 {
                let index = self.phase.floor() as usize;
                let fraction = (self.phase - index as f64) as f32;
                let left = self.source[index];
                let right = self.source[index + 1];
                self.push_pcm(left + (right - left) * fraction);
                self.phase += ratio;
            }
            let consumed = self.phase.floor() as usize;
            for _ in 0..consumed.min(self.source.len()) {
                self.source.pop_front();
            }
            self.phase -= consumed as f64;
        }

        self.flush(
            sender,
            preview_sender,
            dropped,
            preview_dropped,
            self.at_limit(),
        )
    }

    fn at_limit(&self) -> bool {
        self.max_samples
            .is_some_and(|limit| self.captured_samples >= limit)
    }

    fn flush(
        &mut self,
        sender: &mpsc::Sender<Vec<u8>>,
        preview_sender: Option<&PreviewAudioSender>,
        dropped: &AtomicU64,
        preview_dropped: &AtomicU64,
        include_tail: bool,
    ) -> u64 {
        let sent_chunks = flush_pcm_queue(
            &mut self.pcm,
            self.samples_per_chunk,
            sender,
            dropped,
            include_tail,
        );
        if let (Some(preview_pcm), Some(samples_per_chunk), Some(preview_sender)) = (
            self.preview_pcm.as_mut(),
            self.preview_samples_per_chunk,
            preview_sender,
        ) {
            flush_preview_pcm_queue(
                preview_pcm,
                samples_per_chunk,
                preview_sender,
                preview_dropped,
                include_tail,
            );
        }
        sent_chunks
    }

    fn push_pcm(&mut self, sample: f32) {
        if self.at_limit() {
            return;
        }
        self.captured_samples += 1;
        let pcm_sample = (sample * 32_767.0).clamp(-32_768.0, 32_767.0) as i16;
        if self.pcm.len() < MAX_BUFFER_SAMPLES {
            self.pcm.push_back(pcm_sample);
        }
        if let Some(preview_pcm) = self.preview_pcm.as_mut() {
            if preview_pcm.len() < MAX_BUFFER_SAMPLES {
                preview_pcm.push_back(pcm_sample);
            }
        }
    }
}

fn flush_pcm_queue(
    pcm: &mut VecDeque<i16>,
    samples_per_chunk: usize,
    sender: &mpsc::Sender<Vec<u8>>,
    dropped: &AtomicU64,
    include_tail: bool,
) -> u64 {
    let mut sent_chunks = 0;
    while pcm.len() >= samples_per_chunk || (include_tail && !pcm.is_empty()) {
        let count = samples_per_chunk.min(pcm.len());
        let mut bytes = Vec::with_capacity(count * 2);
        for _ in 0..count {
            if let Some(sample) = pcm.pop_front() {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        match sender.try_send(bytes) {
            Ok(()) => sent_chunks += 1,
            Err(_) => {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    sent_chunks
}

fn flush_preview_pcm_queue(
    pcm: &mut VecDeque<i16>,
    samples_per_chunk: usize,
    sender: &PreviewAudioSender,
    dropped: &AtomicU64,
    include_tail: bool,
) {
    while pcm.len() >= samples_per_chunk || (include_tail && !pcm.is_empty()) {
        let count = samples_per_chunk.min(pcm.len());
        let mut bytes = Vec::with_capacity(count * 2);
        for _ in 0..count {
            if let Some(sample) = pcm.pop_front() {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        sender.try_send_latest(bytes, dropped);
    }
}

fn select_input_device(host: &cpal::Host, requested: Option<&str>) -> Result<cpal::Device> {
    if let Some(name) = requested.map(str::trim).filter(|name| !name.is_empty()) {
        if let Some(device) = host
            .input_devices()
            .context("failed to enumerate input devices")?
            .find(|device| device.name().ok().as_deref() == Some(name))
        {
            return Ok(device);
        }
        tracing::warn!("Configured microphone '{name}' is unavailable; using system default");
    }
    host.default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device available"))
}

fn build_stream<T>(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    processor: Arc<Mutex<CaptureProcessor>>,
    sender: mpsc::Sender<Vec<u8>>,
    preview_sender: Option<PreviewAudioSender>,
    telemetry: CaptureTelemetry,
) -> Result<Stream>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    Ok(device.build_input_stream(
        stream_config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let _gate = crate::lock_or_recover!(telemetry.capture_gate, "audio_capture_gate");
            if !telemetry.accepting.load(Ordering::SeqCst) {
                return;
            }
            if data.is_empty() {
                return;
            }
            let converted = data
                .iter()
                .copied()
                .map(f32::from_sample)
                .collect::<Vec<_>>();
            let rms = (converted.iter().map(|sample| sample * sample).sum::<f32>()
                / converted.len() as f32)
                .sqrt();
            if let Ok(mut current) = telemetry.volume.lock() {
                *current = rms.min(1.0);
            }
            if let Ok(mut maximum) = telemetry.peak_volume.lock() {
                *maximum = maximum.max(rms.min(1.0));
            }
            if let Ok(mut processor) = processor.lock() {
                let sent = processor.process(
                    &converted,
                    &sender,
                    preview_sender.as_ref(),
                    &telemetry.dropped_chunks,
                    &telemetry.preview_dropped_chunks,
                );
                telemetry
                    .captured_samples
                    .store(processor.captured_samples, Ordering::SeqCst);
                telemetry.captured_chunks.fetch_add(sent, Ordering::Relaxed);
            }
        },
        |error| tracing::error!("Audio capture callback error: {error}"),
        None,
    )?)
}

fn run_capture(
    config: AudioConfig,
    sender: mpsc::Sender<Vec<u8>>,
    preview_sender: Option<PreviewAudioSender>,
    stop_rx: std::sync::mpsc::Receiver<()>,
    telemetry: CaptureTelemetry,
    ready: std::sync::mpsc::Sender<std::result::Result<(), String>>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = select_input_device(&host, config.device_name.as_deref())?;
    let device_name = device
        .name()
        .unwrap_or_else(|_| "Unknown microphone".to_string());
    let supported = device.default_input_config()?;
    let device_rate = supported.sample_rate().0;
    let device_channels = supported.channels();
    let sample_format = supported.sample_format();
    let stream_config: cpal::StreamConfig = supported.into();
    let processor = Arc::new(Mutex::new(CaptureProcessor::new(
        device_rate,
        device_channels,
        config.sample_rate,
        config.chunk_duration_ms,
        config.preview_chunk_duration_ms,
        config.max_duration_ms,
    )));

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(
            &device,
            &stream_config,
            Arc::clone(&processor),
            sender.clone(),
            preview_sender.clone(),
            telemetry.clone(),
        )?,
        SampleFormat::I16 => build_stream::<i16>(
            &device,
            &stream_config,
            Arc::clone(&processor),
            sender.clone(),
            preview_sender.clone(),
            telemetry.clone(),
        )?,
        SampleFormat::U16 => build_stream::<u16>(
            &device,
            &stream_config,
            Arc::clone(&processor),
            sender.clone(),
            preview_sender.clone(),
            telemetry.clone(),
        )?,
        other => anyhow::bail!("Unsupported microphone sample format: {other:?}"),
    };

    stream.play()?;
    let _ = ready.send(Ok(()));
    *crate::lock_or_recover!(telemetry.state, "audio_state") = CaptureState::Recording;
    tracing::info!(
        "Audio capture started: '{}' {}Hz {}ch {:?} -> {}Hz mono",
        device_name,
        device_rate,
        device_channels,
        sample_format,
        config.sample_rate
    );

    // Stop the actual device when the hard limit is reached, including during
    // a long/cold provider.connect(). The pipeline timer will finalize STT.
    loop {
        match stop_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !telemetry.accepting.load(Ordering::SeqCst)
                    || processor.lock().map(|p| p.at_limit()).unwrap_or(true)
                {
                    break;
                }
            }
        }
    }
    drop(stream);
    if let Ok(mut processor) = processor.lock() {
        let sent = processor.flush(
            &sender,
            preview_sender.as_ref(),
            &telemetry.dropped_chunks,
            &telemetry.preview_dropped_chunks,
            true,
        );
        telemetry.captured_chunks.fetch_add(sent, Ordering::Relaxed);
    }
    *crate::lock_or_recover!(telemetry.state, "audio_state") = CaptureState::Idle;
    let dropped_count = telemetry.dropped_chunks.load(Ordering::Relaxed);
    if dropped_count > 0 {
        tracing::warn!(
            "Audio capture dropped {dropped_count} chunks because the STT queue was full"
        );
    }
    let preview_dropped_count = telemetry.preview_dropped_chunks.load(Ordering::Relaxed);
    if preview_dropped_count > 0 {
        tracing::debug!(
            "Audio capture dropped {preview_dropped_count} preview chunks because the preview queue was full"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_opt_in_and_allocates_no_secondary_pcm_buffer_by_default() {
        assert_eq!(AudioConfig::default().preview_chunk_duration_ms, None);
        let processor = CaptureProcessor::new(16_000, 1, 16_000, 20, None, None);
        assert!(processor.preview_samples_per_chunk.is_none());
        assert!(processor.preview_pcm.is_none());
    }

    #[test]
    fn bounded_recordings_queue_the_full_authoritative_lane_during_cold_start() {
        let ten_minutes = AudioConfig {
            max_duration_ms: Some(600_000),
            ..AudioConfig::default()
        };
        assert_eq!(
            primary_audio_channel_capacity(&ten_minutes),
            MAX_AUDIO_CHANNEL_CAPACITY
        );

        let short = AudioConfig {
            max_duration_ms: Some(2_000),
            ..AudioConfig::default()
        };
        assert_eq!(
            primary_audio_channel_capacity(&short),
            DEFAULT_AUDIO_CHANNEL_CAPACITY
        );
    }

    #[test]
    fn streaming_resampler_keeps_callback_boundaries_continuous() {
        let (sender, mut receiver) = mpsc::channel(10);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(48_000, 1, 16_000, 20, None, None);
        processor.process(&vec![0.25; 480], &sender, None, &dropped, &preview_dropped);
        processor.process(&vec![0.25; 480], &sender, None, &dropped, &preview_dropped);
        let chunk = receiver.try_recv().expect("one 20 ms chunk");
        assert_eq!(chunk.len(), 640);
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn stereo_capture_keeps_the_active_channel_instead_of_halving_it() {
        let (sender, mut receiver) = mpsc::channel(10);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 2, 16_000, 20, None, None);
        let stereo = (0..320).flat_map(|_| [0.5f32, 0.0f32]).collect::<Vec<_>>();

        assert_eq!(
            processor.process(&stereo, &sender, None, &dropped, &preview_dropped),
            1
        );
        let chunk = receiver.try_recv().expect("one 20 ms chunk");
        let first = i16::from_le_bytes([chunk[0], chunk[1]]);
        assert!(first > 15_000, "active channel was attenuated: {first}");
    }

    #[test]
    fn trial_boundary_flushes_exact_sub_chunk_tail_and_never_exceeds_limit() {
        let (sender, mut receiver) = mpsc::channel(20);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, None, Some(23));
        processor.process(&vec![0.2; 1600], &sender, None, &dropped, &preview_dropped);
        assert_eq!(processor.captured_samples, 368);
        assert!(processor.at_limit());
        assert_eq!(receiver.try_recv().unwrap().len(), 640);
        assert_eq!(receiver.try_recv().unwrap().len(), 96);
        assert!(receiver.try_recv().is_err());
        for _ in 0..5 {
            processor.process(&vec![0.2; 1600], &sender, None, &dropped, &preview_dropped);
        }
        assert_eq!(processor.captured_samples, 368);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn one_millisecond_trial_and_stereo_resampling_stay_within_pcm_budget() {
        let (sender, mut receiver) = mpsc::channel(20);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(48_000, 2, 16_000, 20, None, Some(1));
        processor.process(&vec![0.2; 4800], &sender, None, &dropped, &preview_dropped);
        assert_eq!(processor.captured_samples, 16);
        assert_eq!(receiver.try_recv().unwrap().len(), 32);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn quota_counts_capture_even_when_cold_connection_queue_is_full() {
        let (sender, _receiver) = mpsc::channel(1);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, None, Some(120));
        for _ in 0..10 {
            processor.process(&vec![0.3; 320], &sender, None, &dropped, &preview_dropped);
        }
        assert_eq!(processor.captured_samples, 1920);
        assert_eq!(dropped.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn manual_stop_flushes_partial_chunk_without_adding_samples() {
        let (sender, mut receiver) = mpsc::channel(20);
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, None, Some(500));
        processor.process(&vec![0.4; 77], &sender, None, &dropped, &preview_dropped);
        assert!(receiver.try_recv().is_err());
        processor.flush(&sender, None, &dropped, &preview_dropped, true);
        assert_eq!(receiver.try_recv().unwrap().len(), 154);
        assert_eq!(processor.captured_samples, 77);
        assert_eq!(
            processor.flush(&sender, None, &dropped, &preview_dropped, true),
            0
        );
    }

    #[test]
    fn preview_fanout_groups_five_primary_chunks_without_changing_pcm_order() {
        let (sender, mut receiver) = mpsc::channel(10);
        let (preview_tx, preview_receiver) = flume::bounded(2);
        let preview_sender = PreviewAudioSender {
            sender: preview_tx,
            evictor: preview_receiver.clone(),
        };
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, Some(100), None);
        let samples = (0..1600)
            .map(|index| (index as f32 / 1600.0) - 0.5)
            .collect::<Vec<_>>();

        assert_eq!(
            processor.process(
                &samples,
                &sender,
                Some(&preview_sender),
                &dropped,
                &preview_dropped,
            ),
            5
        );

        let mut primary_bytes = Vec::new();
        for _ in 0..5 {
            let chunk = receiver.try_recv().expect("one 20 ms primary chunk");
            assert_eq!(chunk.len(), 640);
            primary_bytes.extend_from_slice(&chunk);
        }
        let preview_bytes = preview_receiver
            .try_recv()
            .expect("one 100 ms preview chunk");
        assert_eq!(preview_bytes.len(), 3200);
        assert_eq!(preview_bytes, primary_bytes);
        assert!(receiver.try_recv().is_err());
        assert!(preview_receiver.try_recv().is_err());
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        assert_eq!(preview_dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn stop_flushes_the_same_exact_tail_to_primary_and_preview() {
        let (sender, mut receiver) = mpsc::channel(10);
        let (preview_tx, preview_receiver) = flume::bounded(2);
        let preview_sender = PreviewAudioSender {
            sender: preview_tx,
            evictor: preview_receiver.clone(),
        };
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, Some(100), None);
        let samples = (0..1037)
            .map(|index| (index as f32 / 1037.0) - 0.5)
            .collect::<Vec<_>>();

        processor.process(
            &samples,
            &sender,
            Some(&preview_sender),
            &dropped,
            &preview_dropped,
        );
        processor.flush(
            &sender,
            Some(&preview_sender),
            &dropped,
            &preview_dropped,
            true,
        );

        let mut primary_bytes = Vec::new();
        while let Ok(chunk) = receiver.try_recv() {
            primary_bytes.extend_from_slice(&chunk);
        }
        let preview_bytes = preview_receiver
            .try_recv()
            .expect("one partial preview tail");
        assert_eq!(primary_bytes.len(), 1037 * 2);
        assert_eq!(preview_bytes, primary_bytes);
        assert!(preview_receiver.try_recv().is_err());
        assert_eq!(processor.captured_samples, 1037);
        assert_eq!(
            processor.flush(
                &sender,
                Some(&preview_sender),
                &dropped,
                &preview_dropped,
                true,
            ),
            0
        );
        assert!(receiver.try_recv().is_err());
        assert!(preview_receiver.try_recv().is_err());
    }

    #[test]
    fn preview_backpressure_evicts_oldest_preview_and_preserves_primary_audio() {
        let (sender, mut receiver) = mpsc::channel(20);
        let (preview_tx, preview_receiver) = flume::bounded(1);
        let preview_sender = PreviewAudioSender {
            sender: preview_tx,
            evictor: preview_receiver.clone(),
        };
        let dropped = AtomicU64::new(0);
        let preview_dropped = AtomicU64::new(0);
        let mut processor = CaptureProcessor::new(16_000, 1, 16_000, 20, Some(100), None);
        let samples = (0..3200)
            .map(|index| (index as f32 / 3200.0) - 0.5)
            .collect::<Vec<_>>();

        assert_eq!(
            processor.process(
                &samples,
                &sender,
                Some(&preview_sender),
                &dropped,
                &preview_dropped,
            ),
            10
        );

        let mut primary_bytes = Vec::new();
        for _ in 0..10 {
            primary_bytes.extend_from_slice(&receiver.try_recv().expect("primary chunk"));
        }
        assert_eq!(primary_bytes.len(), 3200 * 2);
        assert_eq!(preview_receiver.try_recv().unwrap(), primary_bytes[3200..]);
        assert!(preview_receiver.try_recv().is_err());
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        assert_eq!(preview_dropped.load(Ordering::Relaxed), 1);
    }
}
