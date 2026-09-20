#[cfg(not(windows))]
use anyhow::Result;
use serde::Serialize;

use super::funasr_manager::FunAsrPaths;

#[derive(Debug, Clone, Default, Serialize)]
pub struct FunAsrRuntimeStatus {
    pub running: bool,
    pub ready: bool,
    pub pid: Option<u32>,
    pub runtime_variant: String,
    pub last_error: String,
}

#[cfg(windows)]
mod platform {
    use super::{FunAsrPaths, FunAsrRuntimeStatus};
    use anyhow::{Context, Result};
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use std::os::windows::process::CommandExt;

    const REQUEST_MAGIC: u32 = 0x4146_5350;
    const RESPONSE_MAGIC: u32 = 0x5246_5350;
    const PROTOCOL_VERSION: u16 = 2;
    const FLAG_USE_VAD: u16 = 0x0001;
    const FLAG_SHUTDOWN: u16 = 0x8000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
    static PIPE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn request_header(request_id: u64, flags: u16, pcm_bytes: u64, hotword_bytes: u32) -> Vec<u8> {
        let mut header = Vec::with_capacity(32);
        header.extend_from_slice(&REQUEST_MAGIC.to_le_bytes());
        header.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        header.extend_from_slice(&flags.to_le_bytes());
        header.extend_from_slice(&16_000u32.to_le_bytes());
        header.extend_from_slice(&request_id.to_le_bytes());
        header.extend_from_slice(&pcm_bytes.to_le_bytes());
        header.extend_from_slice(&hotword_bytes.to_le_bytes());
        header
    }

    #[derive(Default)]
    struct RuntimeProcess {
        child: Option<Child>,
        pipe_name: String,
        fingerprint: String,
        runtime_variant: String,
        last_error: String,
    }

    #[derive(Clone, Default)]
    pub struct FunAsrRuntime {
        process: Arc<Mutex<RuntimeProcess>>,
        operation: Arc<tokio::sync::Mutex<()>>,
    }

    impl FunAsrRuntime {
        pub fn status(&self) -> FunAsrRuntimeStatus {
            let mut process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
            let running = process
                .child
                .as_mut()
                .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
            if !running {
                process.child = None;
            }
            FunAsrRuntimeStatus {
                running,
                ready: running && !process.pipe_name.is_empty(),
                pid: process.child.as_ref().map(Child::id),
                runtime_variant: process.runtime_variant.clone(),
                last_error: process.last_error.clone(),
            }
        }

        pub async fn start(&self, paths: FunAsrPaths, threads: u32) -> Result<()> {
            let _operation = self.operation.lock().await;
            let runtime = self.clone();
            tokio::time::timeout(
                Duration::from_secs(90),
                tokio::task::spawn_blocking(move || runtime.ensure_started_sync(&paths, threads)),
            )
            .await
            .context("FunASR 原生进程启动超时")?
            .context("FunASR 原生进程启动任务异常退出")?
        }

        pub async fn stop(&self) -> Result<()> {
            let _operation = self.operation.lock().await;
            let runtime = self.clone();
            tokio::task::spawn_blocking(move || runtime.stop_sync())
                .await
                .context("停止 FunASR 原生进程任务异常退出")?
        }

        pub async fn transcribe(
            &self,
            paths: FunAsrPaths,
            pcm: Vec<u8>,
            use_runtime_vad: bool,
            threads: u32,
            hotwords: Vec<String>,
        ) -> Result<String> {
            let _operation = self.operation.lock().await;
            let runtime = self.clone();
            let job = tokio::task::spawn_blocking(move || {
                runtime.ensure_started_sync(&paths, threads)?;
                let request_id = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                match runtime.request_sync(request_id, &pcm, use_runtime_vad, false, &hotwords) {
                    Ok(text) => Ok(text),
                    Err(first_error) => {
                        tracing::warn!(
                            "FunASR pipe request failed; restarting resident host once: {first_error}"
                        );
                        runtime.stop_sync()?;
                        runtime.ensure_started_sync(&paths, threads)?;
                        runtime
                            .request_sync(request_id, &pcm, use_runtime_vad, false, &hotwords)
                            .context(first_error)
                    }
                }
            });

            match tokio::time::timeout(Duration::from_secs(180), job).await {
                Ok(result) => result.context("FunASR 管道推理任务异常退出")?,
                Err(_) => {
                    let runtime = self.clone();
                    let _ = tokio::task::spawn_blocking(move || runtime.stop_sync()).await;
                    anyhow::bail!("FunASR 管道推理超过 180 秒，原生进程已重置")
                }
            }
        }

        fn ensure_started_sync(&self, paths: &FunAsrPaths, threads: u32) -> Result<()> {
            if !paths.ready {
                anyhow::bail!("Fun-ASR-Nano 模型或原生运行时尚未安装完整")
            }
            let fingerprint = format!(
                "{}|{}|{}|{}",
                paths.runtime_path, paths.encoder_path, paths.llm_path, paths.vad_path
            );
            {
                let mut process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
                let running = process
                    .child
                    .as_mut()
                    .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
                if running && process.fingerprint == fingerprint {
                    return Ok(());
                }
            }
            self.stop_sync()?;

            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = PIPE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let pipe_name = format!(
                r"\\.\pipe\popspeak-funasr-{}-{nonce:x}-{sequence:x}",
                std::process::id()
            );

            let mut command = Command::new(&paths.runtime_path);
            command
                .creation_flags(CREATE_NO_WINDOW)
                .arg("--enc")
                .arg(&paths.encoder_path)
                .arg("-m")
                .arg(&paths.llm_path)
                .arg("--vad")
                .arg(&paths.vad_path)
                .arg("--pipe")
                .arg(&pipe_name)
                .arg("--threads")
                .arg(threads.clamp(1, 16).to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            if let Some(parent) = std::path::Path::new(&paths.runtime_path).parent() {
                command.current_dir(parent);
            }

            let mut child = command
                .spawn()
                .with_context(|| format!("无法启动 FunASR 常驻原生进程：{}", paths.runtime_path))?;
            let pid = child.id();
            if let Some(stderr) = child.stderr.take() {
                std::thread::Builder::new()
                    .name("funasr-native-log".to_string())
                    .spawn(move || {
                        for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                            if line.starts_with("[fatal]") || line.starts_with("[pipe]") {
                                tracing::warn!("FunASR native[{pid}]: {line}");
                            } else if line.starts_with("[ready]") {
                                tracing::info!("FunASR native[{pid}]: {line}");
                            } else {
                                tracing::debug!("FunASR native[{pid}]: {line}");
                            }
                        }
                    })
                    .context("无法创建 FunASR 日志线程")?;
            }
            {
                let mut process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
                process.child = Some(child);
                process.pipe_name = pipe_name.clone();
                process.fingerprint = fingerprint;
                process.runtime_variant = paths.runtime_variant.clone();
                process.last_error.clear();
            }

            let started = Instant::now();
            loop {
                match self.request_sync(0, &[], false, false, &[]) {
                    Ok(_) => {
                        tracing::info!(
                            "FunASR resident host PID {pid} ready in {}ms ({})",
                            started.elapsed().as_millis(),
                            paths.runtime_variant
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        let exited = {
                            let mut process =
                                crate::lock_or_recover!(self.process, "funasr_runtime_process");
                            process
                                .child
                                .as_mut()
                                .and_then(|child| child.try_wait().ok())
                                .flatten()
                        };
                        if let Some(status) = exited {
                            self.record_error(format!(
                                "FunASR 常驻进程在模型加载期间退出：{status}；{error}"
                            ));
                            self.stop_sync()?;
                            anyhow::bail!("FunASR 常驻进程在模型加载期间退出：{status}")
                        }
                        if started.elapsed() >= Duration::from_secs(85) {
                            self.record_error(format!("FunASR 模型加载超时：{error}"));
                            self.stop_sync()?;
                            anyhow::bail!("FunASR 模型加载超时：{error}")
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }

        fn request_sync(
            &self,
            request_id: u64,
            pcm: &[u8],
            use_runtime_vad: bool,
            shutdown: bool,
            hotwords: &[String],
        ) -> Result<String> {
            let pipe_name = {
                let process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
                process.pipe_name.clone()
            };
            if pipe_name.is_empty() {
                anyhow::bail!("FunASR 管道尚未建立")
            }

            let connect_started = Instant::now();
            let mut pipe = loop {
                match OpenOptions::new().read(true).write(true).open(&pipe_name) {
                    Ok(pipe) => break pipe,
                    Err(error) if connect_started.elapsed() < Duration::from_secs(2) => {
                        let _ = error;
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("连接 FunASR 命名管道 {pipe_name}"));
                    }
                }
            };

            let mut flags = if use_runtime_vad { FLAG_USE_VAD } else { 0 };
            if shutdown {
                flags |= FLAG_SHUTDOWN;
            }
            let hotword_payload = crate::stt::hotwords::select_hotwords(hotwords).join("\n");
            let header = request_header(
                request_id,
                flags,
                pcm.len() as u64,
                hotword_payload.len() as u32,
            );
            pipe.write_all(&header).context("写入 FunASR 管道请求头")?;
            pipe.write_all(hotword_payload.as_bytes())
                .context("写入识别热词")?;
            pipe.write_all(pcm).context("写入 FunASR PCM")?;
            pipe.flush().context("刷新 FunASR 管道")?;

            let mut response = [0u8; 24];
            pipe.read_exact(&mut response)
                .context("读取 FunASR 管道响应头")?;
            let magic = u32::from_le_bytes(response[0..4].try_into().unwrap());
            let version = u16::from_le_bytes(response[4..6].try_into().unwrap());
            let status = u16::from_le_bytes(response[6..8].try_into().unwrap());
            let returned_id = u64::from_le_bytes(response[8..16].try_into().unwrap());
            let text_bytes = u32::from_le_bytes(response[16..20].try_into().unwrap()) as usize;
            let elapsed_ms = u32::from_le_bytes(response[20..24].try_into().unwrap());
            if magic != RESPONSE_MAGIC || version != PROTOCOL_VERSION || returned_id != request_id {
                anyhow::bail!("FunASR 管道返回了无效协议帧")
            }
            if text_bytes > MAX_RESPONSE_BYTES {
                anyhow::bail!("FunASR 管道响应异常过大：{text_bytes} bytes")
            }
            let mut text = vec![0u8; text_bytes];
            pipe.read_exact(&mut text).context("读取 FunASR 管道文本")?;
            let text = String::from_utf8(text).context("FunASR 返回的文本不是 UTF-8")?;
            if status != 0 {
                anyhow::bail!("FunASR 原生推理失败：{text}")
            }
            if request_id != 0 {
                tracing::info!("FunASR native inference completed in {elapsed_ms}ms");
            }
            Ok(text)
        }

        fn stop_sync(&self) -> Result<()> {
            let (mut child, pipe_name) = {
                let mut process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
                (process.child.take(), std::mem::take(&mut process.pipe_name))
            };
            let Some(mut child) = child.take() else {
                return Ok(());
            };

            if !pipe_name.is_empty() {
                let _ = self.request_sync_with_name(&pipe_name, FLAG_SHUTDOWN);
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if child.try_wait()?.is_some() {
                    tracing::info!("Stopped FunASR resident host PID {}", child.id());
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            child.kill().context("终止 FunASR 常驻进程")?;
            let _ = child.wait();
            tracing::info!(
                "Killed unresponsive FunASR resident host PID {}",
                child.id()
            );
            Ok(())
        }

        fn request_sync_with_name(&self, pipe_name: &str, flags: u16) -> Result<()> {
            let mut pipe = OpenOptions::new().read(true).write(true).open(pipe_name)?;
            let request_id = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let header = request_header(request_id, flags, 0, 0);
            pipe.write_all(&header)?;
            pipe.flush()?;
            let mut response = [0u8; 24];
            pipe.read_exact(&mut response)?;
            Ok(())
        }

        fn record_error(&self, message: String) {
            let mut process = crate::lock_or_recover!(self.process, "funasr_runtime_process");
            process.last_error = message;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn protocol_v2_header_uses_utf8_byte_length_and_fixed_offsets() {
            let payload = "章珊\nPopSpeak";
            let header = request_header(42, FLAG_USE_VAD, 64_000, payload.len() as u32);
            assert_eq!(header.len(), 32);
            assert_eq!(&header[0..4], b"PSFA");
            assert_eq!(u16::from_le_bytes(header[4..6].try_into().unwrap()), 2);
            assert_eq!(u64::from_le_bytes(header[12..20].try_into().unwrap()), 42);
            assert_eq!(
                u64::from_le_bytes(header[20..28].try_into().unwrap()),
                64_000
            );
            assert_eq!(u32::from_le_bytes(header[28..32].try_into().unwrap()), 15);
            assert_eq!(&request_header(43, FLAG_SHUTDOWN, 0, 0)[20..32], &[0u8; 12]);
        }
    }

    impl Drop for FunAsrRuntime {
        fn drop(&mut self) {
            if Arc::strong_count(&self.process) == 1 {
                let _ = self.stop_sync();
            }
        }
    }
}

#[cfg(windows)]
pub use platform::FunAsrRuntime;

#[cfg(not(windows))]
#[derive(Clone, Default)]
pub struct FunAsrRuntime;

#[cfg(not(windows))]
impl FunAsrRuntime {
    pub fn status(&self) -> FunAsrRuntimeStatus {
        FunAsrRuntimeStatus {
            last_error: "FunASR 常驻命名管道目前仅支持 Windows".to_string(),
            ..FunAsrRuntimeStatus::default()
        }
    }

    pub async fn start(&self, _paths: FunAsrPaths, _threads: u32) -> Result<()> {
        anyhow::bail!("FunASR 常驻命名管道目前仅支持 Windows")
    }

    pub async fn stop(&self) -> Result<()> {
        Ok(())
    }

    pub async fn transcribe(
        &self,
        _paths: FunAsrPaths,
        _pcm: Vec<u8>,
        _use_runtime_vad: bool,
        _threads: u32,
        _hotwords: Vec<String>,
    ) -> Result<String> {
        anyhow::bail!("FunASR 常驻命名管道目前仅支持 Windows")
    }
}
