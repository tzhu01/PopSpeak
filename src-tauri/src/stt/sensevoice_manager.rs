use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// SenseVoice INT8 模型文件名
pub const SENSEVOICE_MODEL_FILENAME: &str = "model.int8.onnx";
pub const SENSEVOICE_TOKENS_FILENAME: &str = "tokens.txt";

/// Official release archive, retained as the final transport fallback.
pub const SENSEVOICE_TAR_BYTES: u64 = 163_002_883;
const SENSEVOICE_TAR_SHA256: &str =
    "7d1efa2138a65b0b488df37f8b89e3d91a60676e416f515b952358d83dfd347e";

struct ModelFile {
    name: &'static str,
    bytes: u64,
    sha256: &'static str,
}

const FILES: &[ModelFile] = &[
    ModelFile {
        name: SENSEVOICE_MODEL_FILENAME,
        bytes: 239_233_841,
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    },
    ModelFile {
        name: SENSEVOICE_TOKENS_FILENAME,
        bytes: 315_894,
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    },
];

#[derive(Clone, Copy)]
enum SourceTransport {
    Files(&'static str),
    Archive(&'static str),
}

/// Primary and mirror files were checked against upstream SHA-256. Two
/// ModelScope routes share one service: they are NOT independent CDN providers.
const MIRROR_SOURCES: &[MirrorSource] = &[
    MirrorSource {
        name: "ModelScope 国内 CDN",
        transport: SourceTransport::Files("https://modelscope.cn/models/fengge2024/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/master"),
    },
    MirrorSource {
        name: "Hugging Face 国内镜像",
        transport: SourceTransport::Files("https://hf-mirror.com/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main"),
    },
    MirrorSource {
        name: "ModelScope 固定版本备用",
        transport: SourceTransport::Files("https://modelscope.cn/models/fengge2024/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/9bd9398d89294cbf1964af126ff13e6890394cbc"),
    },
    MirrorSource {
        name: "Hugging Face 官方（可用性取决于网络）",
        transport: SourceTransport::Files("https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/main"),
    },
    MirrorSource {
        name: "GitHub 官方",
        transport: SourceTransport::Archive("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"),
    },
];

struct MirrorSource {
    name: &'static str,
    transport: SourceTransport,
}

static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize)]
pub struct SenseVoicePaths {
    pub model_dir: String,
    pub display_dir: String,
    pub source: String,
    pub model_path: String,
    pub tokens_path: String,
    pub ready: bool,
    pub is_custom: bool,
}

fn custom_model_dir(custom_dir: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = custom_dir {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    None
}

fn executable_relative_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join("models").join("sensevoice"))
}

fn default_model_dir(app: &AppHandle) -> Result<(PathBuf, &'static str, String)> {
    // A portable release owns the `models` directory beside PopSpeak.exe. Do
    // this check explicitly instead of depending on Tauri's resource_dir
    // semantics, which differ between development and packaged applications.
    if let Some(dir) = executable_relative_dir() {
        if dir.is_dir() {
            return Ok((dir, "package-relative", r".\models\sensevoice".to_string()));
        }
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        for bundled_dir in [
            resource_dir.join("models").join("sensevoice"),
            resource_dir
                .join("resources")
                .join("models")
                .join("sensevoice"),
            resource_dir.join("resources").join("sensevoice"),
            resource_dir.join("sensevoice"),
        ] {
            if bundled_dir.is_dir() {
                return Ok((
                    bundled_dir.clone(),
                    "development-resource",
                    bundled_dir.to_string_lossy().into_owned(),
                ));
            }
        }
    }

    let app_data = app
        .path()
        .app_data_dir()
        .context("cannot resolve app data dir")?;
    let dir = app_data.join("models").join("sensevoice");
    std::fs::create_dir_all(&dir).with_context(|| format!("create dir {}", dir.display()))?;
    Ok((dir.clone(), "app-data", dir.to_string_lossy().into_owned()))
}

fn model_ready(dir: &Path) -> bool {
    let model_path = dir.join(SENSEVOICE_MODEL_FILENAME);
    let tokens_path = dir.join(SENSEVOICE_TOKENS_FILENAME);
    model_path.is_file()
        && std::fs::metadata(model_path)
            .map(|metadata| metadata.len() > 100_000_000)
            .unwrap_or(false)
        && tokens_path.is_file()
}

pub fn paths(app: &AppHandle, custom_dir: Option<&str>) -> Result<SenseVoicePaths> {
    let (dir, source, display_dir, is_custom) = if let Some(dir) = custom_model_dir(custom_dir) {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("create custom dir {}", dir.display()))?;
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
    let model_path = dir.join(SENSEVOICE_MODEL_FILENAME);
    let tokens_path = dir.join(SENSEVOICE_TOKENS_FILENAME);

    Ok(SenseVoicePaths {
        model_dir: dir.to_string_lossy().into_owned(),
        display_dir,
        source: source.to_string(),
        model_path: model_path.to_string_lossy().into_owned(),
        tokens_path: tokens_path.to_string_lossy().into_owned(),
        ready: model_ready(&dir),
        is_custom,
    })
}

/// Download and verify both model files before atomically replacing the pair.
pub async fn download_sensevoice(
    app: AppHandle,
    custom_dir: Option<String>,
    force: bool,
) -> Result<()> {
    let _guard = DOWNLOAD_LOCK
        .try_lock()
        .map_err(|_| anyhow::anyhow!("SenseVoice 模型正在下载，请等待当前下载完成"))?;
    let existing_paths = paths(&app, custom_dir.as_deref())?;
    if existing_paths.ready && !force {
        emit_progress(&app, 100, 100, "ready", "模型已存在");
        return Ok(());
    }
    let dir = PathBuf::from(&existing_paths.model_dir);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create SenseVoice model dir {}", dir.display()))?;

    emit_progress(
        &app,
        0,
        100,
        "preparing",
        if force {
            "准备重新下载并校验 SenseVoice 模型..."
        } else {
            "准备下载 SenseVoice 模型..."
        },
    );

    let mut last_error = None;
    for (idx, mirror) in MIRROR_SOURCES.iter().enumerate() {
        tracing::info!(
            "尝试镜像源 {}/{}: {}",
            idx + 1,
            MIRROR_SOURCES.len(),
            mirror.name
        );
        emit_progress(
            &app,
            0,
            100,
            "connecting",
            &format!("尝试镜像源: {}", mirror.name),
        );

        let parent = dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        let staging = tempfile::Builder::new()
            .prefix("popspeak-sensevoice-")
            .tempdir_in(&parent)
            .with_context(|| format!("create download staging dir in {}", parent.display()))?;

        match download_source(&app, mirror, staging.path()).await {
            Ok(_) => {
                if !model_ready(staging.path()) {
                    last_error = Some(anyhow::anyhow!(
                        "下载包解压后缺少有效的 {} 或 {}",
                        SENSEVOICE_MODEL_FILENAME,
                        SENSEVOICE_TOKENS_FILENAME
                    ));
                    continue;
                }
                emit_progress(&app, 98, 100, "installing", "正在安全替换模型文件...");
                install_model_files(staging.path(), &dir)?;
                emit_progress(&app, 100, 100, "done", "下载完成");
                tracing::info!("SenseVoice 模型下载成功: {}", mirror.name);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("镜像 {} 失败: {}", mirror.name, e);
                last_error = Some(e);
            }
        }
    }

    let error = last_error.unwrap_or_else(|| anyhow::anyhow!("所有镜像源均失败"));
    emit_progress(&app, 0, 100, "error", &format!("下载失败，可重试：{error}"));
    Err(error)
}

/// Copy both files into the destination before touching the current model, then
/// swap them with rollback support. A failed download can therefore never leave
/// a previously working offline model half-overwritten.
fn install_model_files(staging_dir: &Path, target_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("create target dir {}", target_dir.display()))?;

    let filenames = [SENSEVOICE_MODEL_FILENAME, SENSEVOICE_TOKENS_FILENAME];
    for filename in filenames {
        let source = staging_dir.join(filename);
        let incoming = target_dir.join(format!("{filename}.popspeak-download"));
        let _ = std::fs::remove_file(&incoming);
        std::fs::copy(&source, &incoming)
            .with_context(|| format!("stage {} into {}", source.display(), incoming.display()))?;
    }

    let mut moved_backups = Vec::new();
    for filename in filenames {
        let target = target_dir.join(filename);
        let backup = target_dir.join(format!("{filename}.popspeak-backup"));
        let _ = std::fs::remove_file(&backup);
        if target.is_file() {
            if let Err(error) = std::fs::rename(&target, &backup) {
                rollback_model_install(target_dir, &filenames, &moved_backups);
                return Err(error).with_context(|| format!("backup {}", target.display()));
            }
            moved_backups.push(filename);
        }
    }

    for filename in filenames {
        let target = target_dir.join(filename);
        let incoming = target_dir.join(format!("{filename}.popspeak-download"));
        if let Err(error) = std::fs::rename(&incoming, &target) {
            rollback_model_install(target_dir, &filenames, &moved_backups);
            return Err(error).with_context(|| format!("install {}", target.display()));
        }
    }

    for filename in moved_backups {
        let _ = std::fs::remove_file(target_dir.join(format!("{filename}.popspeak-backup")));
    }
    Ok(())
}

fn rollback_model_install(target_dir: &Path, filenames: &[&str], moved_backups: &[&str]) {
    for filename in filenames {
        let target = target_dir.join(filename);
        let incoming = target_dir.join(format!("{filename}.popspeak-download"));
        let _ = std::fs::remove_file(&incoming);
        if moved_backups.contains(filename) {
            let backup = target_dir.join(format!("{filename}.popspeak-backup"));
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::rename(backup, target);
        }
    }
}

async fn download_source(app: &AppHandle, mirror: &MirrorSource, target_dir: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(60 * 20))
        .user_agent("PopSpeak/0.4")
        .build()?;
    match mirror.transport {
        SourceTransport::Files(base) => {
            for (index, spec) in FILES.iter().enumerate() {
                let target = target_dir.join(spec.name);
                let (start, end) = if index == 0 { (0, 94) } else { (94, 97) };
                download_file(
                    &client,
                    app,
                    &format!("{base}/{}", spec.name),
                    &target,
                    start,
                    end,
                    spec.bytes,
                )
                .await?;
                let sha256 = spec.sha256;
                tokio::task::spawn_blocking(move || {
                    crate::integrity::verify_sha256(&target, sha256)
                })
                .await??;
            }
            Ok(())
        }
        SourceTransport::Archive(url) => download_and_extract(&client, app, url, target_dir).await,
    }
}

/// Download the upstream archive with its own fixed checksum as final fallback.
async fn download_and_extract(
    client: &reqwest::Client,
    app: &AppHandle,
    url: &str,
    target_dir: &Path,
) -> Result<()> {
    let tar_path = target_dir.join("sensevoice.tar.bz2");
    let partial_path = target_dir.join("sensevoice.tar.bz2.partial");

    // 清理旧文件
    let _ = std::fs::remove_file(&tar_path);
    let _ = std::fs::remove_file(&partial_path);

    download_file(client, app, url, &partial_path, 0, 80, SENSEVOICE_TAR_BYTES).await?;
    let verify_path = partial_path.clone();
    tokio::task::spawn_blocking(move || {
        crate::integrity::verify_sha256(&verify_path, SENSEVOICE_TAR_SHA256)
    })
    .await??;
    tokio::fs::rename(&partial_path, &tar_path).await?;

    // 解压 tar.bz2 (80-100%)
    emit_progress(app, 85, 100, "extracting", "正在解压模型...");
    extract_tar_bz2(&tar_path, target_dir, app).await?;

    // 清理 tar 包
    let _ = std::fs::remove_file(&tar_path);

    Ok(())
}

/// 下载单个文件
async fn download_file(
    client: &reqwest::Client,
    app: &AppHandle,
    url: &str,
    target: &Path,
    progress_start: u64,
    progress_end: u64,
    expected_bytes: u64,
) -> Result<()> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {}", url))?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}: {}", resp.status(), url);
    }

    if let Some(length) = resp.content_length() {
        anyhow::ensure!(
            length == expected_bytes,
            "下载大小不符：预期 {expected_bytes} bytes，实际 {length} bytes"
        );
    }
    let total_bytes = expected_bytes;
    tracing::info!("下载 {} ({} bytes)", url, total_bytes);

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(target).await?;
    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;
    let mut last_emit: u64 = 0;
    let start_time = std::time::Instant::now();

    while let Some(chunk) = tokio::time::timeout(std::time::Duration::from_secs(45), stream.next())
        .await
        .context("下载无响应，正在切换备用源")?
    {
        let chunk = chunk?;
        anyhow::ensure!(
            written + chunk.len() as u64 <= expected_bytes,
            "下载超过预期大小"
        );
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;

        if written - last_emit > 512 * 1024 || written == total_bytes {
            let file_progress = if total_bytes > 0 {
                (written as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };
            let overall = progress_start
                + ((progress_end - progress_start) as f64 * file_progress / 100.0) as u64;
            let elapsed_s = start_time.elapsed().as_secs_f64().max(0.1);
            let speed_mbps = (written as f64 / (1024.0 * 1024.0)) / elapsed_s;

            emit_progress(
                app,
                overall,
                100,
                "downloading",
                &format!(
                    "{:.1} / {:.1} MB @ {:.2} MB/s",
                    written as f64 / (1024.0 * 1024.0),
                    total_bytes as f64 / (1024.0 * 1024.0),
                    speed_mbps
                ),
            );
            last_emit = written;
        }
    }

    file.flush().await?;
    anyhow::ensure!(
        written == expected_bytes,
        "下载不完整：预期 {expected_bytes} bytes，实际 {written} bytes"
    );
    Ok(())
}

/// 使用 Rust 库解压，避免依赖 Windows 环境中不一定存在的 tar.exe。
async fn extract_tar_bz2(tar_path: &Path, target_dir: &Path, app: &AppHandle) -> Result<()> {
    emit_progress(app, 90, 100, "extracting", "解压中...");
    let archive_path = tar_path.to_path_buf();
    let destination = target_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::path::Component;
        let file = std::fs::File::open(&archive_path)?;
        let decoder = bzip2::read::BzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            let relative = path.components().skip(1).collect::<PathBuf>();
            if relative.as_os_str().is_empty()
                || relative.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                continue;
            }
            let output_path = destination.join(relative);
            let entry_type = entry.header().entry_type();
            if entry_type.is_dir() {
                std::fs::create_dir_all(output_path)?;
            } else if entry_type.is_file() {
                entry.unpack(output_path)?;
            }
        }
        Ok(())
    })
    .await??;
    Ok(())
}

fn emit_progress(app: &AppHandle, current: u64, total: u64, status: &str, message: &str) {
    let _ = app.emit(
        "sensevoice:download_progress",
        serde_json::json!({
            "current": current,
            "total": total,
            "status": status,
            "message": message,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_sources_have_domestic_primary_and_two_distinct_backups() {
        let urls: Vec<_> = MIRROR_SOURCES
            .iter()
            .filter_map(|source| match source.transport {
                SourceTransport::Files(base) => Some(base),
                SourceTransport::Archive(_) => None,
            })
            .collect();
        assert_eq!(urls.len(), 4);
        assert!(urls[0].starts_with("https://modelscope.cn/"));
        assert!(urls[1].starts_with("https://hf-mirror.com/"));
        assert!(urls[2].contains("/9bd9398d89294cbf1964af126ff13e6890394cbc"));
        assert_eq!(
            urls.iter().collect::<std::collections::HashSet<_>>().len(),
            urls.len()
        );
        assert!(urls
            .iter()
            .all(|url| !url.contains("gh.llkk") && !url.contains("moeyy")));
    }

    #[test]
    fn direct_manifest_is_exact_and_cannot_accept_unpinned_files() {
        assert_eq!(FILES.len(), 2);
        assert_eq!(FILES[0].bytes, 239_233_841);
        assert_eq!(FILES[1].bytes, 315_894);
        for file in FILES {
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn installs_both_model_files_and_removes_swap_artifacts() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let staging = temporary.path().join("staging");
        let target = temporary.path().join("target");
        std::fs::create_dir_all(&staging).expect("staging directory");
        std::fs::create_dir_all(&target).expect("target directory");
        std::fs::write(staging.join(SENSEVOICE_MODEL_FILENAME), b"new model")
            .expect("staged model");
        std::fs::write(staging.join(SENSEVOICE_TOKENS_FILENAME), b"new tokens")
            .expect("staged tokens");
        std::fs::write(target.join(SENSEVOICE_MODEL_FILENAME), b"old model").expect("old model");
        std::fs::write(target.join(SENSEVOICE_TOKENS_FILENAME), b"old tokens").expect("old tokens");

        install_model_files(&staging, &target).expect("install model files");

        assert_eq!(
            std::fs::read(target.join(SENSEVOICE_MODEL_FILENAME)).expect("installed model"),
            b"new model"
        );
        assert_eq!(
            std::fs::read(target.join(SENSEVOICE_TOKENS_FILENAME)).expect("installed tokens"),
            b"new tokens"
        );
        for filename in [SENSEVOICE_MODEL_FILENAME, SENSEVOICE_TOKENS_FILENAME] {
            assert!(!target
                .join(format!("{filename}.popspeak-download"))
                .exists());
            assert!(!target.join(format!("{filename}.popspeak-backup")).exists());
        }
    }
}
