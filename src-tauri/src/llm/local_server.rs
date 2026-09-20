use anyhow::{Context, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Global cancellation flag for download operation.
static DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Default local LLM model: Qwen2.5-0.5B-Instruct quantized to Q4_K_M (~400 MB).
/// Balanced for CPU-only inference at 10-20 tok/s on i5-8xxx and up.
pub const DEFAULT_LLM_MODEL: &str = "qwen2.5-0.5b-instruct-q4_k_m.gguf";
const DEFAULT_LLM_MODEL_SHA256: &str =
    "74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db";

/// ModelScope mirror — faster for China users, no auth required for public models.
const MODEL_BASE_URL: &str =
    "https://modelscope.cn/api/v1/models/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/master";

/// Approximate on-disk size for the 0.5B Q4_K_M GGUF.
pub const DEFAULT_LLM_MODEL_BYTES: u64 = 491_400_032;

/// Larger alternative (not bundled, user can choose in UI later).
pub const UPGRADE_LLM_MODEL: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";
pub const UPGRADE_LLM_MODEL_BYTES: u64 = 1_100_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct LocalLlmPaths {
    /// Absolute path to the isolated llama-server CPU runtime.
    pub server_path: String,
    /// Directory where local LLM models live (created if missing).
    pub model_dir: String,
    /// Absolute path of the default model.
    pub default_model_path: String,
    /// Whether the default model file exists on disk right now.
    pub default_model_ready: bool,
    /// Absolute path of the optional upgrade model.
    pub upgrade_model_path: String,
    /// Whether the upgrade model has been downloaded.
    pub upgrade_model_ready: bool,
}

fn bundled_model_dir(resource_dir: &std::path::Path) -> Option<PathBuf> {
    [
        resource_dir.join("models").join("llm"),
        resource_dir.join("resources").join("models").join("llm"),
        // Compatibility with the old flat resources/models layout.
        resource_dir.join("resources").join("models"),
    ]
    .into_iter()
    .find(|directory| directory.join(DEFAULT_LLM_MODEL).is_file())
}

/// Directory where LLM models are stored.
///
/// A complete portable package is always authoritative. Older releases saved
/// an absolute development directory in settings; allowing that stale value to
/// win made an otherwise complete ZIP report that its model was missing.
/// Priority: 1. package-relative models/llm, 2. valid custom directory,
/// 3. AppData fallback.
pub fn model_dir(app: &AppHandle, custom_dir: Option<&str>) -> Result<PathBuf> {
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            if let Some(bundled) = bundled_model_dir(parent) {
                return Ok(bundled);
            }
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        if let Some(bundled) = bundled_model_dir(&resource_dir) {
            return Ok(bundled);
        }
    }

    if let Some(d) = custom_dir {
        let trimmed = d.trim();
        if !trimmed.is_empty() {
            let dir = PathBuf::from(trimmed);
            // Only honour an existing custom model. Do not recreate a stale
            // source-tree path copied from a developer machine.
            if dir.join(DEFAULT_LLM_MODEL).is_file() {
                return Ok(dir);
            }
        }
    }

    // Fallback to AppData
    let base = app
        .path()
        .app_local_data_dir()
        .context("app_local_data_dir unavailable")?;
    let dir = base.join("llm_models");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    }
    Ok(dir)
}

/// Resolve the bundled llama-server CPU runtime. It is isolated from Whisper so
/// incompatible ggml DLL versions cannot overwrite one another.
pub fn server_path(app: &AppHandle) -> Result<PathBuf> {
    let resource_dir = app.path().resource_dir().context("resource_dir")?;

    #[cfg(target_os = "windows")]
    let candidates = [
        "resources/runtimes/llama/llama-server.exe",
        "runtimes/llama/llama-server.exe",
        "llama-server.exe",
        "llama-server-x86_64-pc-windows-msvc.exe",
        "binaries/llama-server.exe",
        "binaries/llama-server-x86_64-pc-windows-msvc.exe",
    ];
    #[cfg(target_os = "macos")]
    let candidates = [
        "resources/runtimes/llama/llama-server",
        "runtimes/llama/llama-server",
        "llama-server",
        "llama-server-aarch64-apple-darwin",
        "llama-server-x86_64-apple-darwin",
        "binaries/llama-server",
    ];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates = [
        "resources/runtimes/llama/llama-server",
        "runtimes/llama/llama-server",
        "llama-server",
        "llama-server-x86_64-unknown-linux-gnu",
        "binaries/llama-server",
    ];

    for name in candidates {
        if let Ok(executable) = std::env::current_exe() {
            if let Some(parent) = executable.parent() {
                let path = parent.join(name);
                if path.is_file() {
                    return Ok(path);
                }
            }
        }
        let p = resource_dir.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    anyhow::bail!(
        "llama-server sidecar not found in {}. Tried: {:?}",
        resource_dir.display(),
        candidates
    )
}

/// Return the full set of local LLM paths (server exe + model dir + model files).
pub fn get_local_llm_paths(app: &AppHandle, custom_dir: Option<&str>) -> Result<LocalLlmPaths> {
    let server_path = server_path(app)?;
    let model_dir = model_dir(app, custom_dir)?;
    let default_model_path = model_dir.join(DEFAULT_LLM_MODEL);
    let upgrade_model_path = model_dir.join(UPGRADE_LLM_MODEL);

    Ok(LocalLlmPaths {
        server_path: server_path.display().to_string(),
        model_dir: model_dir.display().to_string(),
        default_model_path: default_model_path.display().to_string(),
        default_model_ready: default_model_path.exists(),
        upgrade_model_path: upgrade_model_path.display().to_string(),
        upgrade_model_ready: upgrade_model_path.exists(),
    })
}

/// Download the default LLM model with resume support and cancellation.
pub async fn download_default_model(app: AppHandle, custom_dir: Option<String>) -> Result<()> {
    // Reset cancellation flag
    DOWNLOAD_CANCELLED.store(false, Ordering::Relaxed);

    let model_dir = model_dir(&app, custom_dir.as_deref())?;
    let dest_path = model_dir.join(DEFAULT_LLM_MODEL);

    if dest_path.exists() {
        crate::integrity::verify_sha256(&dest_path, DEFAULT_LLM_MODEL_SHA256)?;
        tracing::info!(
            "Local LLM model already downloaded: {}",
            dest_path.display()
        );
        return Ok(());
    }

    let tmp_path = dest_path.with_extension("tmp");

    // Check if partial download exists
    let start_byte = if tmp_path.exists() {
        tokio::fs::metadata(&tmp_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    let url = format!("{}/{}", MODEL_BASE_URL, DEFAULT_LLM_MODEL);
    tracing::info!(
        "Downloading local LLM model from {} (resume from {} bytes)",
        url,
        start_byte
    );

    let client = reqwest::Client::new();
    let mut request = client.get(&url);

    // Add Range header for resume
    if start_byte > 0 {
        request = request.header("Range", format!("bytes={}-", start_byte));
    }

    let resp = request
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;

    if !resp.status().is_success() && resp.status().as_u16() != 206 {
        anyhow::bail!("HTTP {}: {}", resp.status(), url);
    }

    let total_bytes = if resp.status().as_u16() == 206 {
        // Partial content - parse Content-Range header
        resp.headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').nth(1))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(DEFAULT_LLM_MODEL_BYTES)
    } else {
        resp.content_length().unwrap_or(DEFAULT_LLM_MODEL_BYTES)
    };

    // A server that ignores Range returns 200. Restart instead of appending a
    // complete response to the partial file and silently corrupting the model.
    let resume_accepted = start_byte > 0 && resp.status().as_u16() == 206;
    let effective_start = if resume_accepted { start_byte } else { 0 };
    let mut downloaded = effective_start;

    // Open file in append mode if resuming
    use tokio::io::AsyncWriteExt;
    let mut file = if resume_accepted {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp_path)
            .await
            .with_context(|| format!("open for append {}", tmp_path.display()))?
    } else {
        tokio::fs::File::create(&tmp_path)
            .await
            .with_context(|| format!("create {}", tmp_path.display()))?
    };

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        // Check cancellation flag
        if DOWNLOAD_CANCELLED.load(Ordering::Relaxed) {
            tracing::warn!("Download cancelled by user");
            let _ = app.emit("llm:local:download:cancelled", serde_json::json!({}));
            anyhow::bail!("Download cancelled");
        }

        let chunk = chunk.context("stream chunk error")?;
        file.write_all(&chunk).await.context("write chunk")?;
        downloaded += chunk.len() as u64;

        let progress = (downloaded as f64 / total_bytes as f64 * 100.0) as u32;
        let _ = app.emit(
            "llm:local:download:progress",
            serde_json::json!({
                "downloaded": downloaded,
                "total": total_bytes,
                "progress": progress,
            }),
        );
    }

    file.flush().await.context("flush file")?;
    drop(file);

    let verify_path = tmp_path.clone();
    tokio::task::spawn_blocking(move || {
        crate::integrity::verify_sha256(&verify_path, DEFAULT_LLM_MODEL_SHA256)
    })
    .await??;

    tokio::fs::rename(&tmp_path, &dest_path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp_path.display(), dest_path.display()))?;

    tracing::info!("Downloaded local LLM model to {}", dest_path.display());
    let _ = app.emit(
        "llm:local:download:complete",
        serde_json::json!({ "path": dest_path.display().to_string() }),
    );

    Ok(())
}

/// Cancel the ongoing download operation.
pub fn cancel_download() {
    DOWNLOAD_CANCELLED.store(true, Ordering::Relaxed);
}

/// Holds the llama-server subprocess handle and its port.
#[derive(Default)]
pub struct LocalLlmServer {
    process: Arc<Mutex<Option<Child>>>,
    port: Arc<Mutex<u16>>,
}

/// Configuration options for starting llama-server.
#[derive(Debug, Clone)]
pub struct StartConfig {
    pub port: u16,
    pub num_threads: u32,
    pub ctx_size: u32,
}

impl Default for StartConfig {
    fn default() -> Self {
        Self {
            port: 11434,
            num_threads: 4,
            ctx_size: 2048,
        }
    }
}

impl LocalLlmServer {
    pub fn new() -> Self {
        Self {
            process: Arc::new(Mutex::new(None)),
            port: Arc::new(Mutex::new(11434)),
        }
    }

    /// Get the currently configured port.
    pub fn current_port(&self) -> u16 {
        *self.port.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Start llama-server with the given model and config. Returns error if already running or model missing.
    pub fn start(
        &self,
        app: &AppHandle,
        model_filename: &str,
        cfg: StartConfig,
        custom_dir: Option<&str>,
    ) -> Result<()> {
        let mut proc_guard = self
            .process
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(child) = proc_guard.as_mut() {
            match child.try_wait() {
                Ok(None) => anyhow::bail!("llama-server already running"),
                Ok(Some(status)) => {
                    tracing::warn!("Previous llama-server exited with {status}; restarting");
                    *proc_guard = None;
                }
                Err(error) => {
                    tracing::warn!("Unable to inspect previous llama-server process: {error}");
                    *proc_guard = None;
                }
            }
        }

        let server_exe = server_path(app)?;
        let model_dir = model_dir(app, custom_dir)?;
        let model_path = model_dir.join(model_filename);

        if !model_path.exists() {
            anyhow::bail!("Model not found: {}", model_path.display());
        }

        let mut cmd = Command::new(&server_exe);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        // llama-server.exe on Windows depends on ggml*.dll sitting next to it.
        // Add its directory to PATH and use it as the working dir.
        if let Some(parent) = server_exe.parent() {
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
            .arg(&model_path)
            .arg("--port")
            .arg(cfg.port.to_string())
            .arg("--ctx-size")
            .arg(cfg.ctx_size.to_string())
            .arg("--n-gpu-layers")
            .arg("0")
            .arg("--n-predict")
            .arg("512")
            .arg("--threads")
            .arg(cfg.num_threads.to_string())
            .arg("--log-disable")
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .with_context(|| format!("spawn llama-server at {}", server_exe.display()))?;

        tracing::info!(
            "Started llama-server PID {} on port {} with model {} (threads={}, ctx={})",
            child.id(),
            cfg.port,
            model_filename,
            cfg.num_threads,
            cfg.ctx_size,
        );

        // Remember the port so we can query it from health checks and pipeline
        *self.port.lock().unwrap_or_else(|error| error.into_inner()) = cfg.port;

        *proc_guard = Some(child);
        Ok(())
    }

    /// Stop the llama-server subprocess if running.
    pub fn stop(&self) -> Result<()> {
        let mut proc_guard = self
            .process
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(mut child) = proc_guard.take() {
            let pid = child.id();
            let _ = child.kill();
            let _ = child.wait();
            tracing::info!("Stopped llama-server PID {}", pid);
        }
        Ok(())
    }

    /// Check if llama-server process is running (without network check).
    pub fn is_running(&self) -> bool {
        let mut proc_guard = self
            .process
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let running = match proc_guard.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        };
        if !running {
            *proc_guard = None;
        }
        running
    }

    /// Check if llama-server is running and healthy (port listening).
    pub async fn health_check(&self) -> bool {
        if !self.is_running() {
            return false;
        }

        // Try to hit /health endpoint with a short timeout.
        let port = *self.port.lock().unwrap_or_else(|error| error.into_inner());
        let url = format!("http://127.0.0.1:{}/health", port);
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        else {
            return false;
        };

        matches!(
            client.get(&url).send().await,
            Ok(resp) if resp.status().is_success()
        )
    }

    /// Return the base URL for OpenAI-compatible requests (e.g. "http://127.0.0.1:11434/v1").
    pub fn base_url(&self) -> String {
        let port = *self.port.lock().unwrap_or_else(|error| error.into_inner());
        format!("http://127.0.0.1:{}/v1", port)
    }
}

impl Drop for LocalLlmServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::{bundled_model_dir, DEFAULT_LLM_MODEL};

    #[test]
    fn package_relative_model_is_discovered() {
        let temp = tempfile::tempdir().expect("tempdir");
        let model_dir = temp.path().join("models").join("llm");
        std::fs::create_dir_all(&model_dir).expect("create model dir");
        std::fs::write(model_dir.join(DEFAULT_LLM_MODEL), b"gguf").expect("write model");

        assert_eq!(bundled_model_dir(temp.path()), Some(model_dir));
    }

    #[test]
    fn directory_without_model_is_not_treated_as_ready() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("models").join("llm"))
            .expect("create empty model dir");

        assert_eq!(bundled_model_dir(temp.path()), None);
    }
}
