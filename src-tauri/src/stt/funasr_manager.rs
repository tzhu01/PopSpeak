use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

pub const ENCODER_FILENAME: &str = "funasr-encoder-f16.gguf";
pub const LLM_FILENAME: &str = "qwen3-0.6b-q4km.gguf";
pub const LEGACY_Q5_LLM_FILENAME: &str = "qwen3-0.6b-q5km.gguf";
pub const VAD_FILENAME: &str = "fsmn-vad.gguf";
pub const MODEL_VERSION: &str = "2026.06-q4km-r1";
pub const MODEL_REVISION: &str = "51dcf4922439c10e0c2e59bc99be8a343d2fe71f";
pub const MODEL_BYTES: u64 = 955_271_296;

const INSTALL_METADATA_FILENAME: &str = ".popspeak-funasr-install.json";
const STAGING_DIRNAME: &str = ".popspeak-funasr-download";
const MAX_RETRIES_PER_SOURCE: u32 = 2;

#[cfg(windows)]
pub const HOST_FILENAME: &str = "llama-funasr-pipe-host.exe";
#[cfg(windows)]
pub const AVX2_HOST_FILENAME: &str = "llama-funasr-pipe-host-avx2.exe";
#[cfg(not(windows))]
pub const HOST_FILENAME: &str = "llama-funasr-pipe-host";

const FILES: &[ModelFile] = &[
    ModelFile {
        name: ENCODER_FILENAME,
        bytes: 469_331_008,
        sha256: "f92f91d01a24fbed6c863495b2ee8c6a6788144a02858b75743f0946668de8a2",
        sources: &[
            "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/funasr-encoder-f16.gguf",
            "https://hf-mirror.com/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/funasr-encoder-f16.gguf",
            "https://modelscope.ai/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/funasr-encoder-f16.gguf",
            "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/51dcf4922439c10e0c2e59bc99be8a343d2fe71f/funasr-encoder-f16.gguf",
            "https://huggingface.co/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/funasr-encoder-f16.gguf",
        ],
    },
    ModelFile {
        name: LLM_FILENAME,
        bytes: 484_219_776,
        sha256: "cc5057552aa9dddedcda73ea8889854e8a257eb07d0a561b7234465c1e856f22",
        sources: &[
            "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/qwen3-0.6b-q4km.gguf",
            "https://hf-mirror.com/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/qwen3-0.6b-q4km.gguf",
            "https://modelscope.ai/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/master/qwen3-0.6b-q4km.gguf",
            "https://modelscope.cn/models/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/51dcf4922439c10e0c2e59bc99be8a343d2fe71f/qwen3-0.6b-q4km.gguf",
            "https://huggingface.co/FunAudioLLM/Fun-ASR-Nano-GGUF/resolve/main/qwen3-0.6b-q4km.gguf",
        ],
    },
    ModelFile {
        name: VAD_FILENAME,
        bytes: 1_720_512,
        sha256: "1270f2559c495f4e7b6e739541151027d360761a3fda43fc147034f5719f5479",
        sources: &[
            "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/master/fsmn-vad.gguf",
            "https://hf-mirror.com/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf",
            "https://modelscope.ai/models/FunAudioLLM/fsmn-vad-GGUF/resolve/master/fsmn-vad.gguf",
            "https://modelscope.cn/models/FunAudioLLM/fsmn-vad-GGUF/resolve/f04fc3013641c8d59c156e2cbf171c1ad596f74d/fsmn-vad.gguf",
            "https://huggingface.co/FunAudioLLM/fsmn-vad-GGUF/resolve/main/fsmn-vad.gguf",
        ],
    },
];

const LEGACY_Q5_BYTES: u64 = 551_377_792;
const LEGACY_Q5_SHA256: &str = "dc2e6e195c534cbaea208c030d3ab55be4d3385ae4b726966cff8639a69aa2a2";

static DOWNLOAD_CANCELLED: AtomicBool = AtomicBool::new(false);
static DOWNLOAD_ACTIVE: AtomicBool = AtomicBool::new(false);

struct DownloadGuard;

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOAD_ACTIVE.store(false, Ordering::SeqCst);
    }
}

struct ModelFile {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
    sources: &'static [&'static str],
}

fn download_source_name(source: &str) -> &'static str {
    if source.starts_with("https://hf-mirror.com/") {
        "Hugging Face 国内镜像"
    } else if source.starts_with("https://huggingface.co/") {
        "Hugging Face 官方"
    } else if source.starts_with("https://modelscope.ai/") {
        "ModelScope 亚太 CDN"
    } else {
        "ModelScope 国内 CDN"
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FunAsrPaths {
    pub runtime_path: String,
    pub runtime_variant: String,
    pub model_dir: String,
    pub display_dir: String,
    pub source: String,
    pub encoder_path: String,
    pub llm_path: String,
    pub vad_path: String,
    pub runtime_ready: bool,
    pub encoder_ready: bool,
    pub llm_ready: bool,
    pub vad_ready: bool,
    pub ready: bool,
    pub is_custom: bool,
    pub model_version: String,
    pub revision: String,
    pub quantization: String,
    pub installed_bytes: u64,
    pub expected_bytes: u64,
    pub verified: bool,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunAsrDownloadProgress {
    pub current: u64,
    pub total: u64,
    pub percent: u32,
    pub status: String,
    pub message: String,
    pub file_name: String,
    pub file_current: u64,
    pub file_total: u64,
    pub speed_bytes_per_sec: u64,
    pub average_speed_bytes_per_sec: u64,
    pub eta_seconds: Option<u64>,
    pub attempt: u32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallMetadata {
    version: String,
    revision: String,
    quantization: String,
    installed_at: String,
}

fn custom_model_dir(custom_dir: Option<&str>) -> Option<PathBuf> {
    custom_dir
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

fn executable_relative_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("models").join("funasr-nano"))
}

#[cfg(debug_assertions)]
fn has_any_model_file(dir: &Path) -> bool {
    [
        ENCODER_FILENAME,
        LLM_FILENAME,
        LEGACY_Q5_LLM_FILENAME,
        VAD_FILENAME,
    ]
    .iter()
    .any(|name| dir.join(name).is_file())
}

#[cfg(debug_assertions)]
fn development_model_dir(app: &AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    [
        resource_dir.join("models").join("funasr-nano"),
        resource_dir
            .join("resources")
            .join("models")
            .join("funasr-nano"),
    ]
    .into_iter()
    .find(|dir| has_any_model_file(dir))
}

fn default_model_dir(app: &AppHandle) -> Result<(PathBuf, &'static str, String)> {
    #[cfg(debug_assertions)]
    if let Some(dir) = development_model_dir(app) {
        return Ok((
            dir.clone(),
            "development-resource",
            dir.to_string_lossy().into_owned(),
        ));
    }

    if let Some(dir) = executable_relative_dir() {
        return Ok((dir, "package-relative", r".\models\funasr-nano".to_string()));
    }

    let app_data = app
        .path()
        .app_local_data_dir()
        .context("cannot resolve local app data dir")?;
    let dir = app_data.join("models").join("funasr-nano");
    Ok((dir.clone(), "app-data", dir.to_string_lossy().into_owned()))
}

fn resource_file(app: &AppHandle, relative: &str) -> Result<PathBuf> {
    let executable_relative = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(relative)));
    if let Some(path) = executable_relative.as_ref().filter(|path| path.is_file()) {
        return Ok(path.clone());
    }
    let resource_dir = app
        .path()
        .resource_dir()
        .context("resource_dir unavailable")?;
    for candidate in [
        resource_dir.join(relative),
        resource_dir.join("resources").join(relative),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Ok(executable_relative.unwrap_or_else(|| resource_dir.join(relative)))
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn supports_avx2_runtime() -> bool {
    std::is_x86_feature_detected!("avx2")
        && std::is_x86_feature_detected!("fma")
        && std::is_x86_feature_detected!("f16c")
        && std::is_x86_feature_detected!("bmi2")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn supports_avx2_runtime() -> bool {
    false
}

fn expected_size(path: &Path, bytes: u64) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.len() == bytes)
            .unwrap_or(false)
}

fn installed_metadata(dir: &Path) -> Option<InstallMetadata> {
    let bytes = std::fs::read(dir.join(INSTALL_METADATA_FILENAME)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn paths(app: &AppHandle, custom_dir: Option<&str>) -> Result<FunAsrPaths> {
    let (dir, source, display_dir, is_custom) = if let Some(dir) = custom_model_dir(custom_dir) {
        (
            dir.clone(),
            "custom",
            dir.to_string_lossy().into_owned(),
            true,
        )
    } else {
        let (dir, source, display_dir) = default_model_dir(app)?;
        (dir, source, display_dir, false)
    };

    let generic_runtime = resource_file(app, &format!("runtimes/funasr/{HOST_FILENAME}"))?;
    #[cfg(windows)]
    let avx2_runtime = resource_file(app, &format!("runtimes/funasr/{AVX2_HOST_FILENAME}"))?;
    #[cfg(windows)]
    let use_avx2 = supports_avx2_runtime() && avx2_runtime.is_file();
    #[cfg(not(windows))]
    let use_avx2 = false;
    #[cfg(windows)]
    let runtime = if use_avx2 {
        avx2_runtime
    } else {
        generic_runtime
    };
    #[cfg(not(windows))]
    let runtime = generic_runtime;

    let encoder = dir.join(ENCODER_FILENAME);
    let q4_llm = dir.join(LLM_FILENAME);
    let q5_llm = dir.join(LEGACY_Q5_LLM_FILENAME);
    let vad = dir.join(VAD_FILENAME);
    let q4_ready = expected_size(&q4_llm, FILES[1].bytes);
    let q5_ready = expected_size(&q5_llm, LEGACY_Q5_BYTES);
    let llm = if q4_ready { q4_llm } else { q5_llm };
    let metadata = installed_metadata(&dir);
    let quantization = if q4_ready {
        "Encoder F16 + Qwen3-0.6B Q4_K_M"
    } else if q5_ready {
        "Encoder F16 + Qwen3-0.6B Q5_K_M"
    } else {
        "Encoder F16 + Qwen3-0.6B Q4_K_M"
    };
    let model_version = metadata
        .as_ref()
        .map(|value| value.version.clone())
        .unwrap_or_else(|| {
            if q5_ready {
                "legacy-q5km"
            } else {
                "not-installed"
            }
            .to_string()
        });
    let revision = metadata
        .as_ref()
        .map(|value| value.revision.clone())
        .unwrap_or_default();

    let runtime_ready = runtime.is_file();
    let encoder_ready = expected_size(&encoder, FILES[0].bytes);
    let llm_ready = q4_ready || q5_ready;
    let vad_ready = expected_size(&vad, FILES[2].bytes);
    let ready = runtime_ready && encoder_ready && llm_ready && vad_ready;
    let installed_bytes = [encoder.as_path(), llm.as_path(), vad.as_path()]
        .into_iter()
        .filter_map(|path| std::fs::metadata(path).ok().map(|metadata| metadata.len()))
        .sum();
    let verified = ready
        && q4_ready
        && metadata.as_ref().is_some_and(|value| {
            value.version == MODEL_VERSION && value.revision == MODEL_REVISION
        });

    Ok(FunAsrPaths {
        runtime_path: runtime.to_string_lossy().into_owned(),
        runtime_variant: if use_avx2 { "AVX2" } else { "通用 x64" }.to_string(),
        model_dir: dir.to_string_lossy().into_owned(),
        display_dir,
        source: source.to_string(),
        encoder_path: encoder.to_string_lossy().into_owned(),
        llm_path: llm.to_string_lossy().into_owned(),
        vad_path: vad.to_string_lossy().into_owned(),
        runtime_ready,
        encoder_ready,
        llm_ready,
        vad_ready,
        ready,
        is_custom,
        model_version: model_version.clone(),
        revision,
        quantization: quantization.to_string(),
        installed_bytes,
        expected_bytes: MODEL_BYTES,
        verified,
        update_available: ready && (!q4_ready || model_version != MODEL_VERSION),
    })
}

pub async fn download(app: AppHandle, custom_dir: Option<String>, force: bool) -> Result<()> {
    if DOWNLOAD_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        anyhow::bail!("FunASR 模型已有下载任务正在运行")
    }
    let _guard = DownloadGuard;
    DOWNLOAD_CANCELLED.store(false, Ordering::SeqCst);

    let current = paths(&app, custom_dir.as_deref())?;
    if current.ready && !force {
        emit_progress(
            &app,
            FunAsrDownloadProgress {
                current: current.installed_bytes,
                total: current.expected_bytes,
                percent: 100,
                status: "ready".to_string(),
                message: "FunASR 模型已经安装".to_string(),
                file_name: String::new(),
                file_current: 0,
                file_total: 0,
                speed_bytes_per_sec: 0,
                average_speed_bytes_per_sec: 0,
                eta_seconds: Some(0),
                attempt: 0,
                source: String::new(),
            },
        );
        return Ok(());
    }

    let target_dir = PathBuf::from(&current.model_dir);
    std::fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "无法创建 FunASR 模型目录 {}。如果程序安装在 Program Files，请选择自定义目录",
            target_dir.display()
        )
    })?;
    let staging_dir = target_dir.join(STAGING_DIRNAME).join(MODEL_VERSION);
    std::fs::create_dir_all(&staging_dir)
        .with_context(|| format!("创建 FunASR 下载暂存目录 {}", staging_dir.display()))?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60 * 30))
        .user_agent(format!(
            "PopSpeak/{} FunASR-Model-Manager",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;

    let mut completed = 0u64;
    for file in FILES {
        check_cancelled()?;
        let installed = target_dir.join(file.name);
        if !force && expected_size(&installed, file.bytes) {
            let verify_path = installed.clone();
            let expected_hash = file.sha256;
            let valid = tokio::task::spawn_blocking(move || {
                crate::integrity::verify_sha256(&verify_path, expected_hash)
            })
            .await?
            .is_ok();
            if valid {
                completed += file.bytes;
                continue;
            }
        }
        download_model_file(&client, &app, file, &staging_dir, completed).await?;
        completed += file.bytes;
    }

    check_cancelled()?;
    emit_simple(
        &app,
        MODEL_BYTES,
        "installing",
        "正在安全安装 FunASR 模型...",
    );
    install_downloaded_files(&staging_dir, &target_dir)?;
    let metadata = InstallMetadata {
        version: MODEL_VERSION.to_string(),
        revision: MODEL_REVISION.to_string(),
        quantization: "Encoder F16 + Qwen3-0.6B Q4_K_M".to_string(),
        installed_at: chrono::Utc::now().to_rfc3339(),
    };
    let metadata_bytes = serde_json::to_vec_pretty(&metadata)?;
    std::fs::write(target_dir.join(INSTALL_METADATA_FILENAME), metadata_bytes)
        .context("写入 FunASR 安装信息")?;

    if force {
        let legacy = target_dir.join(LEGACY_Q5_LLM_FILENAME);
        if legacy.is_file() {
            std::fs::remove_file(&legacy)
                .with_context(|| format!("删除已被 Q4_K_M 替换的旧模型 {}", legacy.display()))?;
        }
    }
    emit_simple(&app, MODEL_BYTES, "done", "FunASR 下载、校验和安装已完成");
    Ok(())
}

async fn download_model_file(
    client: &reqwest::Client,
    app: &AppHandle,
    spec: &ModelFile,
    staging_dir: &Path,
    completed_before: u64,
) -> Result<()> {
    let completed_path = staging_dir.join(spec.name);
    if expected_size(&completed_path, spec.bytes) {
        let verify_path = completed_path.clone();
        let hash = spec.sha256;
        let valid = tokio::task::spawn_blocking(move || {
            crate::integrity::verify_sha256(&verify_path, hash)
        })
        .await?
        .is_ok();
        if valid {
            return Ok(());
        }
    }

    let partial_path = staging_dir.join(format!("{}.part", spec.name));
    let mut last_error = None;
    for (source_index, source) in spec.sources.iter().enumerate() {
        for attempt in 1..=MAX_RETRIES_PER_SOURCE {
            check_cancelled()?;
            emit_progress(
                app,
                FunAsrDownloadProgress {
                    current: completed_before + file_len(&partial_path).min(spec.bytes),
                    total: MODEL_BYTES,
                    percent: percent(completed_before + file_len(&partial_path).min(spec.bytes)),
                    status: "connecting".to_string(),
                    message: format!(
                        "正在连接 {}（线路 {}，第 {} 次）",
                        download_source_name(source),
                        source_index + 1,
                        attempt
                    ),
                    file_name: spec.name.to_string(),
                    file_current: file_len(&partial_path).min(spec.bytes),
                    file_total: spec.bytes,
                    speed_bytes_per_sec: 0,
                    average_speed_bytes_per_sec: 0,
                    eta_seconds: None,
                    attempt,
                    source: download_source_name(source).to_string(),
                },
            );
            match download_attempt(
                client,
                app,
                spec,
                source,
                &partial_path,
                completed_before,
                attempt,
            )
            .await
            {
                Ok(()) => {
                    let verify_path = partial_path.clone();
                    let hash = spec.sha256;
                    emit_simple(
                        app,
                        completed_before + spec.bytes,
                        "verifying",
                        &format!("正在校验 {}...", spec.name),
                    );
                    let verification = tokio::task::spawn_blocking(move || {
                        crate::integrity::verify_sha256(&verify_path, hash)
                    })
                    .await?;
                    if let Err(error) = verification {
                        let _ = tokio::fs::remove_file(&partial_path).await;
                        last_error = Some(error.context(format!("{} SHA-256 校验失败", spec.name)));
                        break;
                    }
                    if completed_path.is_file() {
                        tokio::fs::remove_file(&completed_path).await?;
                    }
                    tokio::fs::rename(&partial_path, &completed_path).await?;
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(
                        "FunASR download failed: file={} source={} attempt={}: {error}",
                        spec.name,
                        source_index + 1,
                        attempt
                    );
                    last_error = Some(error);
                    if attempt < MAX_RETRIES_PER_SOURCE {
                        let delay = 1u64 << (attempt - 1).min(4);
                        emit_simple(
                            app,
                            completed_before + file_len(&partial_path).min(spec.bytes),
                            "retrying",
                            &format!("下载中断，{delay} 秒后自动续传..."),
                        );
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("{} 的所有下载线路均失败", spec.name)))
}

async fn download_attempt(
    client: &reqwest::Client,
    app: &AppHandle,
    spec: &ModelFile,
    source: &str,
    partial_path: &Path,
    completed_before: u64,
    attempt: u32,
) -> Result<()> {
    let resume_from = file_len(partial_path).min(spec.bytes);
    if file_len(partial_path) > spec.bytes {
        tokio::fs::remove_file(partial_path).await?;
    }
    let mut request = client.get(source);
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("连接 {} 下载 {}", download_source_name(source), spec.name))?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("{} 返回 HTTP {status}", download_source_name(source))
    }
    let resume_accepted = resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
    let effective_start = if resume_accepted { resume_from } else { 0 };
    let mut file = if resume_accepted {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(partial_path)
            .await?
    } else {
        tokio::fs::File::create(partial_path).await?
    };
    let mut written = effective_start;
    let started = Instant::now();
    let mut sample_started = Instant::now();
    let mut sample_bytes = written;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        check_cancelled()?;
        let chunk = chunk.context("模型下载数据流中断")?;
        file.write_all(&chunk)
            .await
            .context("写入 FunASR 下载分片")?;
        written += chunk.len() as u64;
        if written > spec.bytes {
            anyhow::bail!("{} 下载量超过清单大小", spec.name)
        }
        if last_emit.elapsed() >= Duration::from_millis(250) || written == spec.bytes {
            let sample_elapsed = sample_started.elapsed().as_secs_f64().max(0.001);
            let speed = ((written - sample_bytes) as f64 / sample_elapsed) as u64;
            let average = ((written - effective_start) as f64
                / started.elapsed().as_secs_f64().max(0.001)) as u64;
            let current = completed_before + written;
            emit_progress(
                app,
                FunAsrDownloadProgress {
                    current,
                    total: MODEL_BYTES,
                    percent: percent(current),
                    status: "downloading".to_string(),
                    message: format!("正在下载 {}", spec.name),
                    file_name: spec.name.to_string(),
                    file_current: written,
                    file_total: spec.bytes,
                    speed_bytes_per_sec: speed,
                    average_speed_bytes_per_sec: average,
                    eta_seconds: (speed > 0).then(|| (MODEL_BYTES.saturating_sub(current)) / speed),
                    attempt,
                    source: download_source_name(source).to_string(),
                },
            );
            sample_started = Instant::now();
            sample_bytes = written;
            last_emit = Instant::now();
        }
    }
    file.flush().await?;
    if written != spec.bytes {
        anyhow::bail!(
            "{} 下载不完整：{} / {} bytes",
            spec.name,
            written,
            spec.bytes
        )
    }
    Ok(())
}

fn install_downloaded_files(staging_dir: &Path, target_dir: &Path) -> Result<()> {
    // Recover a previous interrupted installation before beginning a new one.
    // A backup always wins when the live target is absent; a downloaded
    // incoming file is moved back to staging so the verified bytes are reused.
    for spec in FILES {
        let target = target_dir.join(spec.name);
        let backup = target_dir.join(format!("{}.popspeak-backup", spec.name));
        let incoming = target_dir.join(format!("{}.popspeak-download", spec.name));
        if backup.is_file() {
            if target.is_file() {
                std::fs::remove_file(&backup)?;
            } else {
                std::fs::rename(&backup, &target)?;
            }
        }
        if incoming.is_file() {
            let staged = staging_dir.join(spec.name);
            if staged.is_file() {
                std::fs::remove_file(&incoming)?;
            } else {
                std::fs::rename(&incoming, &staged)?;
            }
        }
    }
    let downloaded = FILES
        .iter()
        .filter(|spec| staging_dir.join(spec.name).is_file())
        .collect::<Vec<_>>();
    let mut backups: Vec<&str> = Vec::new();
    let mut installed: Vec<&str> = Vec::new();

    let install_result = (|| -> Result<()> {
        for spec in &downloaded {
            let incoming = target_dir.join(format!("{}.popspeak-download", spec.name));
            std::fs::rename(staging_dir.join(spec.name), &incoming)
                .with_context(|| format!("暂存 {}", spec.name))?;
        }
        for spec in &downloaded {
            let target = target_dir.join(spec.name);
            let backup = target_dir.join(format!("{}.popspeak-backup", spec.name));
            if target.is_file() {
                std::fs::rename(&target, &backup)
                    .with_context(|| format!("备份旧模型 {}", target.display()))?;
                backups.push(spec.name);
            }
        }
        for spec in &downloaded {
            let target = target_dir.join(spec.name);
            let incoming = target_dir.join(format!("{}.popspeak-download", spec.name));
            std::fs::rename(&incoming, &target).with_context(|| format!("安装 {}", spec.name))?;
            installed.push(spec.name);
        }
        Ok(())
    })();
    if let Err(error) = install_result {
        rollback_install(staging_dir, target_dir, &downloaded, &backups, &installed);
        return Err(error);
    }
    for name in backups {
        let _ = std::fs::remove_file(target_dir.join(format!("{name}.popspeak-backup")));
    }
    Ok(())
}

fn rollback_install(
    staging_dir: &Path,
    target_dir: &Path,
    files: &[&ModelFile],
    backups: &[&str],
    installed: &[&str],
) {
    for spec in files {
        let target = target_dir.join(spec.name);
        let incoming = target_dir.join(format!("{}.popspeak-download", spec.name));
        let backup = target_dir.join(format!("{}.popspeak-backup", spec.name));
        let staged = staging_dir.join(spec.name);
        if installed.contains(&spec.name) && target.is_file() && !staged.is_file() {
            let _ = std::fs::rename(&target, &staged);
        } else if incoming.is_file() && !staged.is_file() {
            let _ = std::fs::rename(&incoming, &staged);
        }
        if backups.contains(&spec.name) {
            if target.is_file() {
                let _ = std::fs::remove_file(&target);
            }
            let _ = std::fs::rename(&backup, &target);
        }
    }
}

pub async fn verify(app: AppHandle, custom_dir: Option<String>) -> Result<()> {
    let resolved = paths(&app, custom_dir.as_deref())?;
    if !resolved.ready {
        anyhow::bail!("FunASR 模型组件不完整")
    }
    emit_simple(&app, 0, "verifying", "正在完整校验 FunASR 模型...");
    let dir = PathBuf::from(&resolved.model_dir);
    tokio::task::spawn_blocking(move || -> Result<()> {
        crate::integrity::verify_sha256(&dir.join(ENCODER_FILENAME), FILES[0].sha256)?;
        if dir.join(LLM_FILENAME).is_file() {
            crate::integrity::verify_sha256(&dir.join(LLM_FILENAME), FILES[1].sha256)?;
        } else {
            crate::integrity::verify_sha256(&dir.join(LEGACY_Q5_LLM_FILENAME), LEGACY_Q5_SHA256)?;
        }
        crate::integrity::verify_sha256(&dir.join(VAD_FILENAME), FILES[2].sha256)?;
        Ok(())
    })
    .await??;
    emit_simple(&app, MODEL_BYTES, "verified", "FunASR 模型校验通过");
    Ok(())
}

pub async fn remove(app: AppHandle, custom_dir: Option<String>) -> Result<u64> {
    let resolved = paths(&app, custom_dir.as_deref())?;
    let dir = PathBuf::from(&resolved.model_dir);
    tokio::task::spawn_blocking(move || -> Result<u64> {
        let mut removed = 0u64;
        for name in [
            ENCODER_FILENAME,
            LLM_FILENAME,
            LEGACY_Q5_LLM_FILENAME,
            VAD_FILENAME,
            INSTALL_METADATA_FILENAME,
        ] {
            let path = dir.join(name);
            if path.is_file() {
                removed += std::fs::metadata(&path)
                    .map(|value| value.len())
                    .unwrap_or(0);
                std::fs::remove_file(&path)
                    .with_context(|| format!("删除 FunASR 文件 {}", path.display()))?;
            }
        }
        let staging = dir.join(STAGING_DIRNAME);
        if staging.is_dir() {
            std::fs::remove_dir_all(&staging)
                .with_context(|| format!("删除 FunASR 下载缓存 {}", staging.display()))?;
        }
        Ok(removed)
    })
    .await?
}

pub fn cancel_download() {
    DOWNLOAD_CANCELLED.store(true, Ordering::SeqCst);
}

fn check_cancelled() -> Result<()> {
    if DOWNLOAD_CANCELLED.load(Ordering::SeqCst) {
        anyhow::bail!("FunASR 下载已暂停；再次点击下载可从断点继续")
    }
    Ok(())
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn percent(current: u64) -> u32 {
    ((current.min(MODEL_BYTES) as f64 / MODEL_BYTES as f64) * 100.0).round() as u32
}

fn emit_simple(app: &AppHandle, current: u64, status: &str, message: &str) {
    emit_progress(
        app,
        FunAsrDownloadProgress {
            current,
            total: MODEL_BYTES,
            percent: percent(current),
            status: status.to_string(),
            message: message.to_string(),
            file_name: String::new(),
            file_current: 0,
            file_total: 0,
            speed_bytes_per_sec: 0,
            average_speed_bytes_per_sec: 0,
            eta_seconds: None,
            attempt: 0,
            source: String::new(),
        },
    );
}

fn emit_progress(app: &AppHandle, progress: FunAsrDownloadProgress) {
    let _ = app.emit("funasr:download_progress", progress);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_artifact_has_domestic_primary_and_independent_transport_fallbacks() {
        for file in FILES {
            assert_eq!(file.sources.len(), 5);
            let unique: std::collections::HashSet<_> = file.sources.iter().collect();
            assert_eq!(unique.len(), file.sources.len());
            assert!(file.sources[0].starts_with("https://modelscope.cn/"));
            assert!(file.sources[1].starts_with("https://hf-mirror.com/"));
            assert!(file.sources[2].starts_with("https://modelscope.ai/"));
            assert!(file
                .sources
                .iter()
                .all(|source| source.ends_with(file.name)));
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
        assert_eq!(
            download_source_name(FILES[0].sources[1]),
            "Hugging Face 国内镜像"
        );
        assert_eq!(
            download_source_name(FILES[0].sources[2]),
            "ModelScope 亚太 CDN"
        );
        assert_eq!(
            download_source_name(FILES[0].sources[4]),
            "Hugging Face 官方"
        );
    }

    #[test]
    fn q4_catalog_size_is_sum_of_all_artifacts() {
        assert_eq!(
            FILES.iter().map(|file| file.bytes).sum::<u64>(),
            MODEL_BYTES
        );
    }

    #[test]
    fn only_known_model_files_are_considered_installed() {
        let temporary = tempfile::tempdir().expect("temp dir");
        std::fs::write(temporary.path().join("notes.txt"), b"keep me").expect("note");
        assert!(!has_any_model_file(temporary.path()));
        std::fs::write(temporary.path().join(VAD_FILENAME), b"vad").expect("vad");
        assert!(has_any_model_file(temporary.path()));
    }

    #[test]
    fn verified_staging_files_replace_models_atomically() {
        let temporary = tempfile::tempdir().expect("temp dir");
        let staging = temporary.path().join("staging");
        let target = temporary.path().join("target");
        std::fs::create_dir_all(&staging).expect("staging");
        std::fs::create_dir_all(&target).expect("target");
        for (index, file) in FILES.iter().enumerate() {
            std::fs::write(staging.join(file.name), format!("new-{index}")).expect("staged model");
            std::fs::write(target.join(file.name), format!("old-{index}")).expect("old model");
        }

        install_downloaded_files(&staging, &target).expect("install models");

        for (index, file) in FILES.iter().enumerate() {
            assert_eq!(
                std::fs::read_to_string(target.join(file.name)).expect("installed model"),
                format!("new-{index}")
            );
            assert!(!target
                .join(format!("{}.popspeak-backup", file.name))
                .exists());
        }
    }
}
