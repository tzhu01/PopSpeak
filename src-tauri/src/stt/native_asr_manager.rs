//! Verified, opt-in GGUF model delivery for the embedded CPU native ASR runtime.
use anyhow::{Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const RECEIPT: &str = ".popspeak-native-asr.json";
const MAX_ATTEMPTS: u32 = 2;

#[derive(Debug, Clone, Serialize)]
pub struct NativeAsrSource {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeAsrModelInfo {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub bytes: u64,
    pub sha256: String,
    pub revision: String,
    pub license: String,
    pub upstream_url: String,
    pub sources: Vec<NativeAsrSource>,
}

pub fn catalog() -> Vec<NativeAsrModelInfo> {
    [
        ("qwen3-asr-1.7b", "Qwen3-ASR 1.7B", "Qwen3-ASR-1.7B-gguf", "Qwen3-ASR-1.7B-Q5_K_M.gguf", 1_517_290_464,
         "034c557fe92ff8fcd9a9c041cbdaad347be0a86a58d3a348f63cf3f0180879d0", "d7aa4b50af3b672e3a5a2782953a823a9332e5b7", "Apache-2.0", "https://huggingface.co/Qwen/Qwen3-ASR-1.7B"),
        ("cohere-transcribe-03-2026", "Cohere Transcribe 03-2026", "cohere-transcribe-03-2026-gguf", "cohere-transcribe-03-2026-Q5_K_M.gguf", 1_770_270_208,
         "14d02f1ad6dd77b3a60f82639879012c3adb4fe25c50a5a47a2c4c661daf1558", "0452067461a8df51e2245dd81f0122739caf424f", "Apache-2.0", "https://huggingface.co/CohereLabs/cohere-transcribe-03-2026"),
        ("nemotron-3.5-asr-streaming-0.6b", "Nemotron 3.5 ASR Streaming 0.6B", "nemotron-3.5-asr-streaming-0.6b-gguf", "nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf", 751_094_240,
         "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c", "85c784fe0a42833abb5bd9e44c43980a0db46fe8", "OpenMDW-1.1", "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b"),
        ("parakeet-unified-en-0.6b", "Parakeet Unified EN 0.6B", "parakeet-unified-en-0.6b-gguf", "parakeet-unified-en-0.6b-Q8_0.gguf", 731_357_568,
         "4b50b6dd862bf6e346929aaf4f5eaacec003bfa3f56462d6c874b41ef2f38795", "598c2267a9bae5e6daf3c3237a44d272d11b7880", "NVIDIA Open Model License", "https://huggingface.co/nvidia/parakeet-unified-en-0.6b"),
    ]
    .into_iter()
    .map(|(id, name, repo, file_name, bytes, sha256, revision, license, upstream_url)| NativeAsrModelInfo {
        id: id.into(), name: name.into(), file_name: file_name.into(), bytes,
        sha256: sha256.into(), revision: revision.into(), license: license.into(), upstream_url: upstream_url.into(),
        sources: vec![
            NativeAsrSource { name: "ModelScope 国内源".into(), url: format!("https://modelscope.cn/models/voconly/{repo}/resolve/{revision}/{file_name}") },
            NativeAsrSource { name: "HF Mirror 国内镜像".into(), url: format!("https://hf-mirror.com/voconly-org/{repo}/resolve/main/{file_name}") },
            NativeAsrSource { name: "ModelScope 同平台备用入口".into(), url: format!("https://www.modelscope.cn/models/voconly/{repo}/resolve/{revision}/{file_name}") },
            NativeAsrSource { name: "Hugging Face 官方仓库".into(), url: format!("https://huggingface.co/voconly-org/{repo}/resolve/main/{file_name}") },
        ],
    }).collect()
}

pub fn model_info(model_id: &str) -> Result<NativeAsrModelInfo> {
    catalog()
        .into_iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| anyhow::anyhow!("未知原生识别模型：{model_id}"))
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeAsrPaths {
    pub model_id: String,
    pub model_dir: String,
    pub model_path: String,
    pub display_dir: String,
    pub source: String,
    pub is_custom: bool,
    pub ready: bool,
    pub verified: bool,
    pub installed_bytes: u64,
    pub expected_bytes: u64,
    pub model_version: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallReceipt {
    model_id: String,
    sha256: String,
    revision: String,
    bytes: u64,
    modified_ns: u128,
}

fn modified_ns(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos())
}

fn valid_header(path: &Path) -> bool {
    let mut header = [0; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && header == *b"GGUF"
}

fn resolve_dir(
    exe_dir: &Path,
    model_id: &str,
    custom_dir: Option<&str>,
) -> Result<(PathBuf, bool)> {
    model_info(model_id)?; // The catalog allowlist prevents path traversal via model_id.
    let custom = custom_dir.map(str::trim).filter(|s| !s.is_empty());
    let base = match custom {
        Some(dir) if Path::new(dir).is_absolute() => PathBuf::from(dir),
        Some(dir) => exe_dir.join(dir),
        None => exe_dir.join("models").join("native-asr"),
    };
    Ok((base.join(model_id), custom.is_some()))
}

pub fn paths(_app: &AppHandle, model_id: &str, custom_dir: Option<&str>) -> Result<NativeAsrPaths> {
    let exe = std::env::current_exe().context("无法确定程序目录")?;
    let (dir, is_custom) =
        resolve_dir(exe.parent().context("程序目录无效")?, model_id, custom_dir)?;
    paths_in_dir(model_id, &dir, is_custom)
}

pub fn paths_in_dir(model_id: &str, dir: &Path, is_custom: bool) -> Result<NativeAsrPaths> {
    let model = model_info(model_id)?;
    let file = dir.join(&model.file_name);
    let bytes = std::fs::metadata(&file).map_or(0, |m| m.len());
    let ready = bytes == model.bytes && valid_header(&file);
    let receipt = std::fs::read(dir.join(RECEIPT))
        .ok()
        .and_then(|data| serde_json::from_slice::<InstallReceipt>(&data).ok());
    let verified = ready
        && receipt.as_ref().is_some_and(|r| {
            r.model_id == model.id
                && r.sha256 == model.sha256
                && r.bytes == bytes
                && r.modified_ns == modified_ns(&file)
        });
    Ok(NativeAsrPaths {
        model_id: model.id,
        model_dir: dir.to_string_lossy().into_owned(),
        model_path: file.to_string_lossy().into_owned(),
        display_dir: dir.to_string_lossy().into_owned(),
        source: if is_custom { "custom" } else { "portable" }.into(),
        is_custom,
        ready,
        verified,
        installed_bytes: bytes,
        expected_bytes: model.bytes,
        model_version: model.revision.clone(),
        update_available: receipt.is_some_and(|r| r.revision != model.revision),
    })
}

pub fn verify_file(path: &Path, model: &NativeAsrModelInfo) -> Result<()> {
    let mut file = std::fs::File::open(path).context("模型文件尚未安装")?;
    anyhow::ensure!(
        file.metadata()?.len() == model.bytes,
        "模型大小不完整，请重新下载"
    );
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let size = file.read(&mut buf)?;
        if size == 0 {
            break;
        }
        hash.update(&buf[..size]);
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()) == model.sha256,
        "模型 SHA-256 不匹配，请重新下载"
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeAsrDownloadProgress {
    pub model_id: String,
    pub current: u64,
    pub total: u64,
    pub percent: u32,
    pub status: String,
    pub message: String,
    pub speed_bytes_per_sec: u64,
    pub attempt: u32,
    pub source: String,
}

type Downloads = Mutex<HashMap<String, Arc<AtomicBool>>>;
fn downloads() -> &'static Downloads {
    static ACTIVE: OnceLock<Downloads> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}
struct DownloadGuard(String);
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        downloads()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

pub fn cancel_download(model_id: &str) {
    if let Some(flag) = downloads()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(model_id)
    {
        flag.store(true, Ordering::SeqCst);
    }
}

#[allow(clippy::too_many_arguments)]
fn progress(
    app: &AppHandle,
    model: &NativeAsrModelInfo,
    current: u64,
    status: &str,
    message: &str,
    speed: u64,
    attempt: u32,
    source: &str,
) {
    let _ = app.emit(
        "native-asr:download-progress",
        NativeAsrDownloadProgress {
            model_id: model.id.clone(),
            current,
            total: model.bytes,
            percent: (current.saturating_mul(100) / model.bytes.max(1)).min(100) as u32,
            status: status.into(),
            message: message.into(),
            speed_bytes_per_sec: speed,
            attempt,
            source: source.into(),
        },
    );
}

#[allow(clippy::too_many_arguments)]
async fn transfer(
    client: &reqwest::Client,
    url: &str,
    staging: &Path,
    model: &NativeAsrModelInfo,
    cancelled: &AtomicBool,
    app: &AppHandle,
    attempt: u32,
    source: &str,
) -> Result<()> {
    let mut offset = tokio::fs::metadata(staging).await.map_or(0, |m| m.len());
    if offset > model.bytes {
        tokio::fs::remove_file(staging).await?;
        offset = 0;
    }
    if offset == model.bytes {
        return Ok(());
    }
    let mut request = client.get(url);
    if offset > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
    }
    let response = tokio::time::timeout(Duration::from_secs(30), request.send())
        .await
        .context("下载源连接超时")??
        .error_for_status()?;
    if offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let expected = format!("bytes {offset}-");
        anyhow::ensure!(
            response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|h| h.starts_with(&expected)),
            "下载源返回了错误的续传区间"
        );
    } else {
        offset = 0;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(staging)
        .await?;
    let start = Instant::now();
    let mut tick = Instant::now();
    let initial = offset;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(25), stream.next())
        .await
        .context("下载源长时间无响应")?
    {
        anyhow::ensure!(!cancelled.load(Ordering::SeqCst), "下载已取消，进度已保留");
        let chunk = chunk?;
        anyhow::ensure!(
            offset + chunk.len() as u64 <= model.bytes,
            "下载文件超过预期大小"
        );
        file.write_all(&chunk).await?;
        offset += chunk.len() as u64;
        if tick.elapsed() >= Duration::from_millis(200) {
            let speed =
                ((offset - initial) as f64 / start.elapsed().as_secs_f64().max(0.01)) as u64;
            progress(
                app,
                model,
                offset,
                "downloading",
                "正在下载，可取消后继续",
                speed,
                attempt,
                source,
            );
            tick = Instant::now();
        }
    }
    file.flush().await?;
    anyhow::ensure!(offset == model.bytes, "下载提前结束，正在准备续传");
    Ok(())
}

pub async fn download_model(
    app: AppHandle,
    model_id: String,
    custom_dir: Option<String>,
) -> Result<NativeAsrPaths> {
    let model = model_info(&model_id)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = downloads().lock().unwrap_or_else(|e| e.into_inner());
        anyhow::ensure!(!active.contains_key(&model_id), "此模型已经在下载");
        active.insert(model_id.clone(), Arc::clone(&cancelled));
    }
    let _guard = DownloadGuard(model_id.clone());
    let result = download_inner(&app, &model, custom_dir.as_deref(), &cancelled).await;
    if let Err(error) = &result {
        progress(
            &app,
            &model,
            0,
            if cancelled.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "error"
            },
            &error.to_string(),
            0,
            0,
            "",
        );
    }
    result
}

fn write_receipt(dir: &Path, installed: &Path, model: &NativeAsrModelInfo) -> Result<()> {
    let receipt = InstallReceipt {
        model_id: model.id.clone(),
        sha256: model.sha256.clone(),
        revision: model.revision.clone(),
        bytes: model.bytes,
        modified_ns: modified_ns(installed),
    };
    std::fs::write(dir.join(RECEIPT), serde_json::to_vec_pretty(&receipt)?)?;
    Ok(())
}

async fn download_inner(
    app: &AppHandle,
    model: &NativeAsrModelInfo,
    custom_dir: Option<&str>,
    cancelled: &Arc<AtomicBool>,
) -> Result<NativeAsrPaths> {
    let model_id = &model.id;
    let info = paths(app, model_id, custom_dir)?;
    let dir = PathBuf::from(&info.model_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .context("无法创建模型目录，请选择可写目录")?;
    if info.ready {
        progress(
            app,
            model,
            model.bytes,
            "verifying",
            "检查已安装模型的 SHA-256",
            0,
            0,
            "本地模型",
        );
        let existing = PathBuf::from(&info.model_path);
        let spec = model.clone();
        let valid = tokio::task::spawn_blocking(move || verify_file(&existing, &spec)).await?;
        anyhow::ensure!(
            !cancelled.load(Ordering::SeqCst),
            "校验已取消，已安装模型未更改"
        );
        if valid.is_ok() {
            write_receipt(&dir, Path::new(&info.model_path), model)?;
            progress(
                app,
                model,
                model.bytes,
                "completed",
                "已安装模型完整，无需重新下载",
                0,
                0,
                "本地模型",
            );
            return paths(app, model_id, custom_dir);
        }
    }
    let staging = dir.join(format!("{}.partial", model.file_name));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(12))
        .build()?;
    let mut last_error = String::new();
    for (index, source) in model.sources.iter().enumerate() {
        for retry in 0..MAX_ATTEMPTS {
            if cancelled.load(Ordering::SeqCst) {
                progress(
                    app,
                    model,
                    0,
                    "cancelled",
                    "已取消，下次下载继续",
                    0,
                    0,
                    &source.name,
                );
                anyhow::bail!("下载已取消");
            }
            let attempt = index as u32 * MAX_ATTEMPTS + retry + 1;
            progress(
                app,
                model,
                0,
                "connecting",
                "连接下载源（失败时自动切换）",
                0,
                attempt,
                &source.name,
            );
            match transfer(
                &client,
                &source.url,
                &staging,
                model,
                cancelled,
                app,
                attempt,
                &source.name,
            )
            .await
            {
                Ok(()) => {
                    progress(
                        app,
                        model,
                        model.bytes,
                        "verifying",
                        "正在校验 SHA-256",
                        0,
                        attempt,
                        &source.name,
                    );
                    let path = staging.clone();
                    let spec = model.clone();
                    let verification =
                        tokio::task::spawn_blocking(move || verify_file(&path, &spec)).await?;
                    if let Err(error) = verification {
                        last_error = error.to_string();
                        // Only remove this model's own invalid staging file, never the installed copy.
                        tokio::fs::remove_file(&staging).await?;
                        progress(
                            app,
                            model,
                            0,
                            "retrying",
                            &last_error,
                            0,
                            attempt,
                            &source.name,
                        );
                        continue;
                    }
                    anyhow::ensure!(
                        !cancelled.load(Ordering::SeqCst),
                        "下载已取消，完整文件保留待下次安装"
                    );
                    let installed = info.model_path.clone();
                    let stage = staging.clone();
                    let install_dir = dir.clone();
                    let spec = model.clone();
                    let abort = Arc::clone(cancelled);
                    tokio::task::spawn_blocking(move || {
                        super::native_asr::with_model_unloaded(&installed, || {
                            anyhow::ensure!(
                                !abort.load(Ordering::SeqCst),
                                "安装已取消，完整文件保留待下次安装"
                            );
                            let target = Path::new(&installed);
                            let backup = install_dir.join(format!("{}.previous", spec.file_name));
                            if target.exists() {
                                if backup.exists() {
                                    std::fs::remove_file(&backup)?;
                                }
                                std::fs::rename(target, &backup)?;
                            }
                            if let Err(error) = std::fs::rename(&stage, target) {
                                if backup.exists() {
                                    let _ = std::fs::rename(&backup, target);
                                }
                                return Err(error.into());
                            }
                            write_receipt(&install_dir, target, &spec)
                        })
                    })
                    .await??;
                    progress(
                        app,
                        model,
                        model.bytes,
                        "completed",
                        "模型安装完成，可选中后开始识别",
                        0,
                        attempt,
                        &source.name,
                    );
                    return paths(app, model_id, custom_dir);
                }
                Err(error) => {
                    last_error = error.to_string();
                    progress(
                        app,
                        model,
                        0,
                        if cancelled.load(Ordering::SeqCst) {
                            "cancelled"
                        } else {
                            "retrying"
                        },
                        &last_error,
                        0,
                        attempt,
                        &source.name,
                    );
                }
            }
        }
    }
    progress(
        app,
        model,
        0,
        "error",
        &last_error,
        0,
        MAX_ATTEMPTS * model.sources.len() as u32,
        "",
    );
    anyhow::bail!("所有模型下载源均失败：{last_error}")
}

pub async fn delete_model(
    app: &AppHandle,
    model_id: &str,
    custom_dir: Option<&str>,
) -> Result<NativeAsrPaths> {
    let info = paths(app, model_id, custom_dir)?;
    let model = model_info(model_id)?;
    {
        let mut active = downloads().lock().unwrap_or_else(|e| e.into_inner());
        anyhow::ensure!(
            !active.contains_key(model_id),
            "请先取消并等待该模型的下载结束"
        );
        active.insert(model_id.into(), Arc::new(AtomicBool::new(false)));
    }
    let _guard = DownloadGuard(model_id.into());
    let spec = model.clone();
    tokio::task::spawn_blocking(move || {
        super::native_asr::with_model_unloaded(&info.model_path, || {
            let dir = Path::new(&info.model_dir);
            // No recursive removal: a custom directory may contain unrelated user files.
            for name in [
                spec.file_name.clone(),
                format!("{}.partial", spec.file_name),
                format!("{}.previous", spec.file_name),
                RECEIPT.into(),
            ] {
                let file = dir.join(name);
                if file.is_file() {
                    std::fs::remove_file(file)?;
                }
            }
            Ok(())
        })
    })
    .await??;
    progress(
        app,
        &model,
        0,
        "deleted",
        "模型已删除，不影响其他模型",
        0,
        0,
        "",
    );
    paths(app, model_id, custom_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_four_pinned_models_and_three_distinct_source_platforms() {
        let items = catalog();
        assert_eq!(items.len(), 4);
        for item in items {
            assert_eq!(item.sha256.len(), 64);
            assert_eq!(item.revision.len(), 40);
            let hosts = item
                .sources
                .iter()
                .map(|s| {
                    reqwest::Url::parse(&s.url)
                        .unwrap()
                        .host_str()
                        .unwrap()
                        .to_string()
                })
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(hosts.len(), 4);
            assert!(item.bytes > 500_000_000);
        }
    }
    #[test]
    fn default_and_custom_paths_are_isolated_by_model_and_reject_traversal() {
        let root = Path::new("C:/PopSpeak");
        let (path, custom) = resolve_dir(root, "qwen3-asr-1.7b", None).unwrap();
        assert_eq!(path, root.join("models/native-asr/qwen3-asr-1.7b"));
        assert!(!custom);
        let (path, custom) = resolve_dir(root, "qwen3-asr-1.7b", Some("Model")).unwrap();
        assert_eq!(path, root.join("Model/qwen3-asr-1.7b"));
        assert!(custom);
        assert!(resolve_dir(root, "../", None).is_err());
    }

    #[test]
    fn checksum_rejects_modified_and_truncated_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("fixture.gguf");
        let mut spec = catalog().remove(0);
        spec.bytes = 8;
        spec.sha256 = format!("{:x}", Sha256::digest(b"GGUFtest"));
        std::fs::write(&file, b"GGUFtest").unwrap();
        assert!(verify_file(&file, &spec).is_ok());
        std::fs::write(&file, b"GGUFfail").unwrap();
        assert!(verify_file(&file, &spec).is_err());
        std::fs::write(&file, b"GGUF").unwrap();
        assert!(verify_file(&file, &spec).is_err());
    }
}
