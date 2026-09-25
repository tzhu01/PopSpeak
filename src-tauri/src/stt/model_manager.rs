use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// Ships-with-app default model name. Small enough to bundle (~77 MB) and CPU-friendly.
/// Runs on Intel Core i5 (Sandy Bridge and up) at near-realtime.
pub const DEFAULT_MODEL_FILENAME: &str = "ggml-tiny.bin";

/// Optional upgrade model — better accuracy, still CPU-only. Not bundled; fetched on demand.
pub const UPGRADE_MODEL_FILENAME: &str = "ggml-base.bin";
/// Larger open-weight multilingual models stay optional and are never added to the installer.
pub const SMALL_MODEL_FILENAME: &str = "ggml-small-q5_1.bin";
pub const TURBO_MODEL_FILENAME: &str = "ggml-large-v3-turbo-q5_0.bin";
const DEFAULT_MODEL_SHA256: &str =
    "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21";
const UPGRADE_MODEL_SHA256: &str =
    "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe";
const SMALL_MODEL_SHA256: &str = "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb";
const TURBO_MODEL_SHA256: &str = "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2";

/// HuggingFace hosts the ggml weights maintained by whisper.cpp upstream.
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
/// Domestic mirror is only a transport. The official upstream SHA-256 is still mandatory.
const MODEL_MIRROR_BASE_URL: &str = "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main";
/// Domestic transports must match the exact upstream SHA-256 above.
const MODEL_DOMESTIC_BASE_URL: &str =
    "https://modelscope.cn/models/cjc1887415157/whisper.cpp/resolve/master";
const MODEL_DOMESTIC_PINNED_BASE_URL: &str =
    "https://modelscope.cn/models/cjc1887415157/whisper.cpp/resolve/ac12dbec310c2fd6e67398e808c40d80210ce4d0";
const TURBO_MODEL_DOMESTIC_BASE_URL: &str =
    "https://modelscope.cn/models/timeless/whispercpp/resolve/master";
const TURBO_MODEL_DOMESTIC_PINNED_BASE_URL: &str =
    "https://modelscope.cn/models/timeless/whispercpp/resolve/f93b8669080e40afe671b275d8cd67fd2060c956";

fn download_source_bases(filename: &str) -> [&'static str; 4] {
    let (domestic, pinned) = if filename == TURBO_MODEL_FILENAME {
        (
            TURBO_MODEL_DOMESTIC_BASE_URL,
            TURBO_MODEL_DOMESTIC_PINNED_BASE_URL,
        )
    } else {
        (MODEL_DOMESTIC_BASE_URL, MODEL_DOMESTIC_PINNED_BASE_URL)
    };
    // The two ModelScope URLs are version-route fallbacks on the same service,
    // not independent CDNs. HF direct is optional; the first three were reachable
    // in the local no-proxy probe, and all transfers still require full SHA-256.
    [domestic, MODEL_MIRROR_BASE_URL, pinned, MODEL_BASE_URL]
}

/// Approximate on-disk size for the base model — surfaced in the UI before download.
pub const UPGRADE_MODEL_BYTES: u64 = 147_951_465;
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize)]
pub struct LocalModelPaths {
    /// Absolute path to the bundled whisper-cli executable.
    pub cli_path: String,
    /// Directory where models live (created if missing).
    pub model_dir: String,
    /// Absolute path of the default (bundled or downloaded) model.
    pub default_model_path: String,
    /// Whether the default model file exists on disk right now.
    pub default_model_ready: bool,
    /// Absolute path of the optional upgrade model.
    pub upgrade_model_path: String,
    /// Whether the upgrade model has already been downloaded.
    pub upgrade_model_ready: bool,
    pub small_model_path: String,
    pub small_model_ready: bool,
    pub turbo_model_path: String,
    pub turbo_model_ready: bool,
}

fn legacy_model_dir(app: &AppHandle) -> Result<PathBuf> {
    let base = app
        .path()
        .app_local_data_dir()
        .context("app_local_data_dir unavailable")?;
    let dir = base.join("models");
    Ok(dir)
}

fn ensure_writable_directory(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("创建模型目录 {}", dir.display()))?;
    // A real write probe also catches protected Program Files installations.
    tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("模型目录无法写入：{}", dir.display()))?;
    Ok(())
}

fn download_directory(app: &AppHandle, custom_dir: Option<&str>) -> Result<PathBuf> {
    if let Some(custom) = custom_dir.map(str::trim).filter(|value| !value.is_empty()) {
        let dir = PathBuf::from(custom);
        anyhow::ensure!(dir.is_absolute(), "请选择完整的模型目录路径");
        ensure_writable_directory(&dir)?;
        return Ok(dir);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| {
        exe.parent()
            .map(|parent| parent.join("models").join("whisper"))
    }) {
        if ensure_writable_directory(&dir).is_ok() {
            return Ok(dir);
        }
    }
    let fallback = legacy_model_dir(app)?;
    ensure_writable_directory(&fallback)?;
    Ok(fallback)
}

fn bundled_model_path(app: &AppHandle, filename: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("models").join("whisper").join(filename));
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.extend([
            resource_dir.join("models").join("whisper").join(filename),
            resource_dir
                .join("resources")
                .join("models")
                .join("whisper")
                .join(filename),
            // Compatibility with PopSpeak 0.2.0 and older developer builds.
            resource_dir.join("resources").join("models").join(filename),
            resource_dir.join("models").join(filename),
            resource_dir.join(filename),
        ]);
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn locate_model(app: &AppHandle, filename: &str) -> Result<PathBuf> {
    if let Some(path) = bundled_model_path(app, filename) {
        return Ok(path);
    }
    let legacy = legacy_model_dir(app)?.join(filename);
    if legacy.is_file() {
        return Ok(legacy);
    }
    Ok(download_directory(app, None)?.join(filename))
}

/// New downloads prefer `models/whisper` beside the executable. Read-only
/// installations use AppData, and the actual destination is always shown in UI.
pub fn model_dir(app: &AppHandle) -> Result<PathBuf> {
    download_directory(app, None)
}

/// Resolve the bundled whisper-cli CPU runtime.
/// Whisper and llama.cpp intentionally live in separate resource directories because
/// their versioned ggml DLLs have overlapping names and must never overwrite each other.
pub fn cli_path(app: &AppHandle) -> Result<PathBuf> {
    let resource_dir = app.path().resource_dir().context("resource_dir")?;

    #[cfg(target_os = "windows")]
    let candidates = [
        "resources/runtimes/whisper/whisper-cli.exe",
        "runtimes/whisper/whisper-cli.exe",
        "whisper-cli.exe",
        "whisper-cli-x86_64-pc-windows-msvc.exe",
        "binaries/whisper-cli.exe",
        "binaries/whisper-cli-x86_64-pc-windows-msvc.exe",
    ];
    #[cfg(target_os = "macos")]
    let candidates = [
        "resources/runtimes/whisper/whisper-cli",
        "runtimes/whisper/whisper-cli",
        "whisper-cli",
        "whisper-cli-aarch64-apple-darwin",
        "whisper-cli-x86_64-apple-darwin",
        "binaries/whisper-cli",
    ];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates = [
        "resources/runtimes/whisper/whisper-cli",
        "runtimes/whisper/whisper-cli",
        "whisper-cli",
        "whisper-cli-x86_64-unknown-linux-gnu",
        "binaries/whisper-cli",
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

    // Fallback: sibling of the current executable (useful for `cargo run`).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in candidates {
                let p = dir.join(name);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }

    anyhow::bail!(
        "whisper-cli sidecar not found in resource dir {}",
        resource_dir.display()
    )
}

/// Snapshot of on-disk state, used by the frontend to decide what to show.
pub fn paths(app: &AppHandle, custom_dir: Option<&str>) -> Result<LocalModelPaths> {
    let dir = download_directory(app, custom_dir)?;
    let locate = |filename: &str| -> Result<PathBuf> {
        if custom_dir
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            Ok(dir.join(filename))
        } else {
            locate_model(app, filename)
        }
    };
    let default_path = locate(DEFAULT_MODEL_FILENAME)?;
    let upgrade_path = locate(UPGRADE_MODEL_FILENAME)?;
    let small_path = locate(SMALL_MODEL_FILENAME)?;
    let turbo_path = locate(TURBO_MODEL_FILENAME)?;
    let cli = cli_path(app).ok();

    Ok(LocalModelPaths {
        cli_path: cli
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        model_dir: dir.to_string_lossy().into_owned(),
        default_model_path: default_path.to_string_lossy().into_owned(),
        default_model_ready: default_path.exists(),
        upgrade_model_path: upgrade_path.to_string_lossy().into_owned(),
        upgrade_model_ready: upgrade_path.exists(),
        small_model_path: small_path.to_string_lossy().into_owned(),
        small_model_ready: small_path.exists(),
        turbo_model_path: turbo_path.to_string_lossy().into_owned(),
        turbo_model_ready: turbo_path.exists(),
    })
}

/// Resolve the model path the pipeline should actually pass to whisper-cli.
///
/// Priority:
/// 1. A non-empty absolute user override from settings (if the file exists).
/// 2. The bundled/downloaded default in the app data dir.
/// 3. The legacy `models/ggml-tiny.bin` relative path, for backward compatibility.
pub fn resolve_model_path(app: &AppHandle, configured: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() && std::path::Path::new(configured).is_file() {
        return configured.to_string();
    }
    for filename in [
        UPGRADE_MODEL_FILENAME,
        SMALL_MODEL_FILENAME,
        TURBO_MODEL_FILENAME,
    ] {
        if configured.to_ascii_lowercase().ends_with(filename) {
            if let Ok(model) = locate_model(app, filename) {
                if model.exists() {
                    return model.to_string_lossy().into_owned();
                }
            }
            // A missing explicit choice must never silently run a smaller model.
            return configured.to_string();
        }
    }
    if !configured.is_empty()
        && !configured
            .to_ascii_lowercase()
            .ends_with(DEFAULT_MODEL_FILENAME)
    {
        return configured.to_string();
    }
    if let Ok(default) = locate_model(app, DEFAULT_MODEL_FILENAME) {
        if default.exists() {
            return default.to_string_lossy().into_owned();
        }
    }
    configured.to_string()
}

/// Resolve the whisper-cli path used at runtime.
pub fn resolve_cli_path(app: &AppHandle, configured: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() && std::path::Path::new(configured).is_file() {
        return configured.to_string();
    }
    match cli_path(app) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => configured.to_string(),
    }
}

/// Download a specific ggml model file, streaming progress to the frontend
/// via the `model:progress` event. The download is atomic — data streams into
/// `<name>.partial`, then gets renamed on success.
pub async fn download_model(
    app: AppHandle,
    filename: String,
    custom_dir: Option<String>,
) -> Result<PathBuf> {
    let expected_hash = match filename.as_str() {
        DEFAULT_MODEL_FILENAME => DEFAULT_MODEL_SHA256,
        UPGRADE_MODEL_FILENAME => UPGRADE_MODEL_SHA256,
        SMALL_MODEL_FILENAME => SMALL_MODEL_SHA256,
        TURBO_MODEL_FILENAME => TURBO_MODEL_SHA256,
        _ => anyhow::bail!("unsupported model file: {}", filename),
    };

    let _download_guard = DOWNLOAD_LOCK
        .try_lock()
        .context("已有模型正在下载，请等待当前下载完成")?;
    let dir = download_directory(&app, custom_dir.as_deref())?;
    let target = dir.join(&filename);
    if target.is_file() {
        emit_progress(&app, &filename, 0, 0, "verifying", 0.0, 0);
        let existing = target.clone();
        if tokio::task::spawn_blocking(move || {
            crate::integrity::verify_sha256(&existing, expected_hash)
        })
        .await?
        .is_ok()
        {
            emit_progress(&app, &filename, 1, 1, "ready", 0.0, 0);
            return Ok(target);
        }
    }
    emit_progress(&app, &filename, 0, 0, "starting", 0.0, 0);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60 * 30))
        .build()?;
    let mut failures = Vec::new();
    // Retry the entire transfer, including stream and checksum failures, and
    // alternate sources. A mirror that responds but stalls must not trap users.
    let sources = download_source_bases(&filename);
    for attempt in 1..=5 {
        let base_url = sources[(attempt as usize - 1) % sources.len()];
        if attempt > 1 {
            emit_progress(&app, &filename, 0, 0, "retrying", 0.0, attempt);
        }
        match download_attempt(
            &app,
            &client,
            &filename,
            expected_hash,
            &target,
            base_url,
            attempt,
        )
        .await
        {
            Ok(()) => return Ok(target),
            Err(error) => {
                tracing::warn!("Model download attempt {} failed: {:#}", attempt, error);
                failures.push(format!("第 {} 次：{:#}", attempt, error));
            }
        }
    }
    emit_progress(&app, &filename, 0, 0, "error", 0.0, 5);
    anyhow::bail!("模型下载失败，可点击重试。{}", failures.join("；"))
}

async fn download_attempt(
    app: &AppHandle,
    client: &reqwest::Client,
    filename: &str,
    expected_hash: &'static str,
    target: &Path,
    base_url: &str,
    attempt: u32,
) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let url = if base_url.starts_with("https://modelscope.cn/") {
        format!("{}/{}", base_url, filename)
    } else {
        format!("{}/{}?download=true", base_url, filename)
    };
    let resp = tokio::time::timeout(std::time::Duration::from_secs(45), client.get(url).send())
        .await
        .context("下载源连接 45 秒无响应")??
        .error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    emit_progress(app, filename, 0, total, "downloading", 0.0, attempt);
    // TempPath cleans up after every failed attempt. Persist replaces a corrupt
    // installed model only after the complete new file passes the official hash.
    let partial = tempfile::Builder::new()
        .prefix(".popspeak-model-")
        .suffix(".partial")
        .tempfile_in(target.parent().context("模型目录缺失")?)?
        .into_temp_path();
    let mut file = tokio::fs::File::create(&partial).await?;
    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;
    let started = std::time::Instant::now();
    let mut last_emit = started;
    while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(45), stream.next())
        .await
        .context("下载源 45 秒无响应")?
    {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
        if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            let speed = written as f64 / started.elapsed().as_secs_f64().max(0.001);
            emit_progress(app, filename, written, total, "downloading", speed, attempt);
            last_emit = std::time::Instant::now();
        }
    }
    file.flush().await?;
    drop(file);

    anyhow::ensure!(
        total == 0 || written == total,
        "下载文件长度不完整：{written}/{total}"
    );
    emit_progress(
        app,
        filename,
        written,
        total.max(written),
        "verifying",
        0.0,
        attempt,
    );
    let verify_path = partial.to_path_buf();
    tokio::task::spawn_blocking(move || {
        crate::integrity::verify_sha256(&verify_path, expected_hash)
    })
    .await??;

    partial.persist(target).context("安装模型文件失败")?;
    emit_progress(
        app,
        filename,
        written,
        total.max(written),
        "done",
        0.0,
        attempt,
    );
    tracing::info!(
        "Downloaded {} ({} bytes) -> {}",
        filename,
        written,
        target.display()
    );
    Ok(())
}

/// Index bundled models without copying hundreds of megabytes into AppData.
/// Release packaging verifies SHA-256 once; hashing every model again on every
/// application launch would delay the first hotkey on slower Windows disks.
pub fn install_bundled_models(app: &AppHandle) {
    for name in [DEFAULT_MODEL_FILENAME, UPGRADE_MODEL_FILENAME] {
        if let Some(candidate) = bundled_model_path(app, name) {
            tracing::info!("Indexed bundled Whisper model: {}", candidate.display());
        }
    }
}

fn emit_progress(
    app: &AppHandle,
    filename: &str,
    downloaded: u64,
    total: u64,
    phase: &str,
    speed: f64,
    attempt: u32,
) {
    let _ = app.emit(
        "model:progress",
        serde_json::json!({
            "filename": filename,
            "downloaded": downloaded,
            "total": total,
            "phase": phase,
            "speed_bytes_per_sec": speed,
            "attempt": attempt,
        }),
    );
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn every_whisper_model_has_domestic_primary_and_distinct_fallback_routes() {
        for filename in [
            DEFAULT_MODEL_FILENAME,
            UPGRADE_MODEL_FILENAME,
            SMALL_MODEL_FILENAME,
            TURBO_MODEL_FILENAME,
        ] {
            let sources = download_source_bases(filename);
            assert!(sources[0].starts_with("https://modelscope.cn/"));
            assert!(sources[1].starts_with("https://hf-mirror.com/"));
            assert_eq!(
                sources
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                4
            );
            assert!(sources.iter().all(|source| source.starts_with("https://")));
        }
        assert!(download_source_bases(TURBO_MODEL_FILENAME)[0].contains("timeless/whispercpp"));
        assert!(!download_source_bases(TURBO_MODEL_FILENAME)[0].contains("cjc1887415157"));
    }
}
