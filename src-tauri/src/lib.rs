pub mod activation;
pub mod app_detector;
pub mod audio;
mod integrity;
pub mod llm;
pub mod output;
pub mod pipeline;
pub mod storage;
pub mod stt;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;
use tracing_subscriber::EnvFilter;

use std::sync::{Arc, Mutex};

/// Lock a std::sync::Mutex, logging a warning if it was poisoned.
#[macro_export]
macro_rules! lock_or_recover {
    ($mutex:expr, $name:expr) => {
        $mutex.lock().unwrap_or_else(|e| {
            tracing::warn!(
                target: "mutex_poison",
                "Mutex '{}' was poisoned, recovering gracefully",
                $name
            );
            e.into_inner()
        })
    };
}

/// Default cloud API base URL. Override with the `API_BASE_URL` environment variable.
pub const DEFAULT_API_BASE_URL: &str = "https://www.popspeak.com";

/// Read the cloud API base URL from the environment, falling back to the compiled default.
pub fn api_base_url() -> String {
    std::env::var("API_BASE_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string())
}

/// Cached hotkey mode to avoid loading config from disk on every keypress.
/// Updated whenever config is saved.
struct HotkeyModeCache(Arc<Mutex<String>>);

/// Cached close_to_tray setting to avoid blocking I/O in the window close handler.
struct CloseToTrayCache(Arc<Mutex<bool>>);

/// Session token for cloud providers. Set by the frontend after Better Auth login.
/// The Rust pipeline reads this when creating cloud STT/LLM providers.
pub struct SessionTokenStore(pub Arc<Mutex<String>>);

/// Local LLM server subprocess handle (llama-server).
pub struct LocalLlmServerState(pub Arc<Mutex<llm::local_server::LocalLlmServer>>);

/// Managed tray icon handle for dynamic menu/tooltip updates.
pub struct TrayHandle {
    pub tray: Mutex<tauri::tray::TrayIcon>,
}

/// Persisted window position and size.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Build (or rebuild) the system tray menu based on current state.
fn build_tray_menu(
    app: &tauri::AppHandle,
    is_recording: bool,
    window_visible: bool,
) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let show_hide = MenuItem::with_id(
        app,
        "show_hide",
        if window_visible {
            "Hide Window"
        } else {
            "Show Window"
        },
        true,
        None::<&str>,
    )?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let record = MenuItem::with_id(
        app,
        "record",
        if is_recording {
            "Stop Recording"
        } else {
            "Start Recording"
        },
        true,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let show_capsule = MenuItem::with_id(app, "show_capsule", "显示悬浮球", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
    let account = MenuItem::with_id(app, "account", "Account", true, None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let about = MenuItem::with_id(app, "about", "About PopSpeak", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show_hide,
            &show_capsule,
            &sep1,
            &record,
            &sep2,
            &settings,
            &history,
            &account,
            &sep3,
            &about,
            &quit,
        ],
    )?;
    Ok(menu)
}

/// Rebuild the tray menu and update tooltip based on pipeline state.
pub fn refresh_tray(app: &tauri::AppHandle) {
    let is_recording = app
        .try_state::<pipeline::PipelineHandle>()
        .map(|p| p.current_state() == pipeline::PipelineState::Recording)
        .unwrap_or(false);
    let window_visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    if let Some(tray_handle) = app.try_state::<TrayHandle>() {
        if let Ok(tray) = tray_handle.tray.lock() {
            if let Ok(menu) = build_tray_menu(app, is_recording, window_visible) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    }
}

#[tauri::command]
async fn start_recording(state: tauri::State<'_, pipeline::PipelineHandle>) -> Result<(), String> {
    state.start().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_recording(state: tauri::State<'_, pipeline::PipelineHandle>) -> Result<(), String> {
    state.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn abort_recording(state: tauri::State<'_, pipeline::PipelineHandle>) -> Result<(), String> {
    state.abort();
    Ok(())
}

#[tauri::command]
fn check_accessibility_permission() -> bool {
    pipeline::is_accessibility_trusted()
}

#[tauri::command]
fn request_accessibility_permission() -> bool {
    pipeline::request_accessibility_permission()
}

#[tauri::command]
fn list_audio_input_devices() -> Result<Vec<String>, String> {
    audio::list_input_devices().map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    current_version: String,
    latest_version: String,
    available: bool,
    release_url: String,
}

fn version_numbers(version: &str) -> [u64; 3] {
    let mut numbers = version
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0));
    [
        numbers.next().unwrap_or(0),
        numbers.next().unwrap_or(0),
        numbers.next().unwrap_or(0),
    ]
}

/// Network access is opt-in: this command only runs when the user presses
/// "Check for updates". Speech recognition never depends on this endpoint.
#[tauri::command]
async fn check_for_updates() -> Result<UpdateInfo, String> {
    #[derive(serde::Deserialize)]
    struct GithubRelease {
        tag_name: String,
        html_url: String,
    }

    let release = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent(format!("PopSpeak/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?
        .get("https://api.github.com/repos/tzhu01/PopSpeak/releases/latest")
        .send()
        .await
        .map_err(|error| format!("检查更新失败: {error}"))?
        .error_for_status()
        .map_err(|error| format!("更新服务暂不可用: {error}"))?
        .json::<GithubRelease>()
        .await
        .map_err(|error| format!("无法读取版本信息: {error}"))?;

    if !release
        .html_url
        .starts_with("https://github.com/tzhu01/PopSpeak/")
    {
        return Err("更新地址未通过安全校验".to_string());
    }
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(UpdateInfo {
        available: version_numbers(&release.tag_name) > version_numbers(&current),
        current_version: current,
        latest_version: release.tag_name,
        release_url: release.html_url,
    })
}

#[tauri::command]
async fn get_config(
    state: tauri::State<'_, storage::ConfigManager>,
) -> Result<storage::AppConfig, String> {
    state.load().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_activation_status(
    service: tauri::State<'_, activation::ActivationService>,
) -> Result<activation::ActivationStatus, String> {
    service.status().map_err(|e| e.to_string())
}

#[tauri::command]
fn activate_license(
    app: tauri::AppHandle,
    service: tauri::State<'_, activation::ActivationService>,
    code: String,
) -> Result<activation::ActivationStatus, String> {
    let status = service.activate(&code).map_err(|e| e.to_string())?;
    let _ = app.emit("activation:updated", &status);
    Ok(status)
}

fn require_activation(app: &tauri::AppHandle, feature: &str) -> Result<(), String> {
    app.state::<activation::ActivationService>()
        .ensure_feature(feature)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_capsule_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::ConfigManager>,
    enabled: bool,
) -> Result<storage::AppConfig, String> {
    patch_capsule_preferences(
        app,
        state,
        storage::CapsulePreferencesPatch {
            capsule_enabled: Some(enabled),
            ..Default::default()
        },
    )
    .await
}

#[tauri::command]
async fn patch_capsule_preferences(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::ConfigManager>,
    patch: storage::CapsulePreferencesPatch,
) -> Result<storage::AppConfig, String> {
    let config = state
        .patch_capsule_preferences(patch)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("config:updated", &config);
    if let Some(window) = app.get_webview_window("capsule") {
        let _ = window.set_always_on_top(config.capsule_always_on_top);
        if !config.capsule_enabled {
            let _ = window.hide();
        }
    }
    Ok(config)
}

#[tauri::command]
async fn update_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::ConfigManager>,
    cache: tauri::State<'_, HotkeyModeCache>,
    close_tray_cache: tauri::State<'_, CloseToTrayCache>,
    local_server_state: tauri::State<'_, LocalLlmServerState>,
    funasr_runtime: tauri::State<'_, stt::funasr_runtime::FunAsrRuntime>,
    config: storage::AppConfig,
) -> Result<storage::AppConfig, String> {
    let previous = state.load().await.unwrap_or_default();
    if config.stt_provider != "sensevoice" {
        require_activation(&app, "advanced-recognition")?;
    }
    if config.polish_enabled || config.translate_enabled {
        require_activation(&app, "postprocessing")?;
    }
    state
        .save_main_settings(&config)
        .await
        .map_err(|e| e.to_string())?;
    // Saving normalizes managed model paths, providers, and numeric limits.
    // Acknowledge and run the persisted configuration, not the incoming draft.
    let config = state.load().await.map_err(|e| e.to_string())?;
    *crate::lock_or_recover!(cache.0, "hotkey_mode_cache") = config.hotkey_mode.clone();
    *crate::lock_or_recover!(close_tray_cache.0, "close_to_tray_cache") = config.close_to_tray;
    // Keep the main and capsule webviews in sync immediately after saving.
    // Without this event, each webview retains its own stale Zustand config
    // until the application is restarted.
    let _ = app.emit("config:updated", &config);

    if config.stt_provider == "sensevoice"
        && (previous.stt_provider != config.stt_provider
            || previous.sensevoice_language != config.sensevoice_language
            || previous.sensevoice_num_threads != config.sensevoice_num_threads
            || previous.sensevoice_use_custom_dir != config.sensevoice_use_custom_dir
            || previous.sensevoice_model_dir != config.sensevoice_model_dir)
    {
        let prewarm_app = app.clone();
        let language = config.sensevoice_language.clone();
        let threads = config.sensevoice_num_threads.clamp(1, 16);
        let model_dir = (config.sensevoice_use_custom_dir
            && !config.sensevoice_model_dir.trim().is_empty())
        .then(|| config.sensevoice_model_dir.clone());
        tauri::async_runtime::spawn(async move {
            if let Err(error) =
                stt::sensevoice::prewarm(prewarm_app, language, threads, model_dir).await
            {
                tracing::warn!("SenseVoice prewarm after settings update failed: {error}");
            }
        });
    }

    let preview_settings_changed = previous.capsule_enabled != config.capsule_enabled
        || previous.capsule_preview_enabled != config.capsule_preview_enabled
        || previous.sensevoice_num_threads != config.sensevoice_num_threads;
    if config.capsule_enabled && config.capsule_preview_enabled && preview_settings_changed {
        let preview_app = app.clone();
        let preview_threads = config.sensevoice_num_threads.clamp(1, 4);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = stt::sensevoice::prewarm_preview(preview_app, preview_threads).await
            {
                // The selected final engine remains usable when the optional
                // display-only recognizer is unavailable.
                tracing::warn!("Live preview prewarm after settings update failed: {error}");
            }
        });
    }

    let funasr_settings_changed = previous.funasr_use_custom_dir != config.funasr_use_custom_dir
        || previous.funasr_model_dir != config.funasr_model_dir
        || previous.funasr_num_threads != config.funasr_num_threads;
    if require_activation(&app, "advanced-recognition").is_ok()
        && (funasr_settings_changed
            || (config.stt_provider == "funasr-nano"
                && previous.stt_provider != config.stt_provider))
    {
        let runtime = funasr_runtime.inner().clone();
        let funasr_app = app.clone();
        let custom_dir = (config.funasr_use_custom_dir
            && !config.funasr_model_dir.trim().is_empty())
        .then(|| config.funasr_model_dir.clone());
        let threads = config.funasr_num_threads;
        tauri::async_runtime::spawn(async move {
            if funasr_settings_changed {
                let _ = runtime.stop().await;
            }
            match stt::funasr_manager::paths(&funasr_app, custom_dir.as_deref()) {
                Ok(paths) if paths.ready => {
                    if let Err(error) = runtime.start(paths, threads).await {
                        tracing::warn!("FunASR resident host restart failed: {error}");
                    }
                }
                Ok(_) => tracing::info!("FunASR model is not installed; resident host not started"),
                Err(error) => tracing::warn!("FunASR path resolution failed: {error}"),
            }
        });
    }

    let wants_local_llm = config.polish_enabled && config.llm_provider == "local-llama";
    let local_settings_changed = previous.local_llm_model != config.local_llm_model
        || previous.local_llm_model_dir != config.local_llm_model_dir
        || previous.local_llm_port != config.local_llm_port
        || previous.local_llm_threads != config.local_llm_threads
        || previous.local_llm_ctx_size != config.local_llm_ctx_size;
    let server = local_server_state
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !wants_local_llm {
        let _ = server.stop();
        return Ok(config);
    }

    if local_settings_changed && server.is_running() {
        let _ = server.stop();
    }
    if !server.is_running() {
        let start_config = llm::local_server::StartConfig {
            port: config.local_llm_port,
            num_threads: config.local_llm_threads.clamp(1, 16),
            ctx_size: config.local_llm_ctx_size.clamp(512, 8192),
        };
        let custom_dir = if config.local_llm_model_dir.trim().is_empty() {
            None
        } else {
            Some(config.local_llm_model_dir.as_str())
        };
        let model_filename = if config.local_llm_model.trim().is_empty() {
            llm::local_server::DEFAULT_LLM_MODEL
        } else {
            config.local_llm_model.as_str()
        };
        if let Err(error) = server.start(&app, model_filename, start_config, custom_dir) {
            tracing::warn!("Local LLM was not started after settings update: {error}");
            let _ = app.emit(
                "pipeline:notice",
                "本地润色模型尚未就绪；语音识别仍可离线使用，本次将输出原始文本。",
            );
        }
    }
    Ok(config)
}

type WhisperCompatSettings = (String, String, Vec<(String, String)>);

/// Return the endpoint, model, and extra fields for a Whisper-compatible STT provider.
fn whisper_compat_config(
    provider: &str,
    custom_stt_base_url: Option<&str>,
    custom_stt_model: Option<&str>,
) -> Option<WhisperCompatSettings> {
    match provider {
        "glm-asr" => Some((
            "https://open.bigmodel.cn/api/paas/v4/audio/transcriptions".to_string(),
            "glm-asr-2512".to_string(),
            vec![("stream".to_string(), "false".to_string())],
        )),
        "openai-whisper" => Some((
            "https://api.openai.com/v1/audio/transcriptions".to_string(),
            "whisper-1".to_string(),
            vec![],
        )),
        "groq-whisper" => Some((
            "https://api.groq.com/openai/v1/audio/transcriptions".to_string(),
            "whisper-large-v3-turbo".to_string(),
            vec![],
        )),
        "siliconflow" => Some((
            "https://api.siliconflow.cn/v1/audio/transcriptions".to_string(),
            "FunAudioLLM/SenseVoiceSmall".to_string(),
            vec![],
        )),
        "custom-whisper" => {
            let endpoint = custom_stt_base_url?.trim();
            let model = custom_stt_model?.trim();
            if endpoint.is_empty() || model.is_empty() {
                return None;
            }
            Some((endpoint.to_string(), model.to_string(), vec![]))
        }
        _ => None,
    }
}

fn xiaomi_mimo_config(
    custom_stt_base_url: Option<&str>,
    custom_stt_model: Option<&str>,
) -> (String, String) {
    let endpoint = stt::xiaomi_mimo::normalize_xiaomi_endpoint(
        custom_stt_base_url
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("https://token-plan-cn.xiaomimimo.com/v1"),
    );
    let model = custom_stt_model
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("mimo-v2.5")
        .to_string();
    (endpoint, model)
}

#[tauri::command]
// Tauri IPC exposes these values as flat command arguments to keep the existing
// frontend invoke contract stable; grouping them would be a breaking API change.
#[allow(clippy::too_many_arguments)]
async fn test_stt_connection(
    app: tauri::AppHandle,
    api_key: String,
    provider: String,
    stt_base_url: Option<String>,
    stt_model: Option<String>,
    volcengine_app_id: Option<String>,
    volcengine_auth_mode: Option<String>,
    custom_cloud: Option<stt::custom_cloud::CustomCloudConfig>,
    token_store: tauri::State<'_, SessionTokenStore>,
) -> Result<bool, String> {
    require_activation(&app, "cloud-recognition")?;
    if provider == "custom-whisper" {
        return stt::custom_cloud::benchmark(custom_cloud.ok_or("请配置自定义云端厂商与凭证")?)
            .await
            .map(|_| true)
            .map_err(|e| e.to_string());
    }
    if provider.is_empty() {
        return Ok(false);
    }

    // Cloud provider: verify session token + Pro status via API
    if provider == "cloud" {
        let token = crate::lock_or_recover!(token_store.0, "session_token").clone();
        if token.is_empty() {
            return Ok(false);
        }
        let api_base = api_base_url();
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/api/subscription/status", api_base))
            .header("Authorization", format!("Bearer {}", token))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        return Ok(body["plan"].as_str() == Some("pro"));
    }

    if api_key.is_empty() {
        return Ok(false);
    }

    match provider.as_str() {
        "deepgram" => {
            let client = reqwest::Client::new();
            let resp = client
                .get("https://api.deepgram.com/v1/projects")
                .header("Authorization", format!("Token {}", api_key))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(resp.status().is_success())
        }
        "assemblyai" => {
            let client = reqwest::Client::new();
            let resp = client
                .get("https://api.assemblyai.com/v2/transcript?limit=1")
                .header("Authorization", api_key)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(resp.status().is_success())
        }
        "volcengine-seedasr" => {
            let resource_id = stt_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(stt::seedasr::DEFAULT_RESOURCE_ID);
            stt::seedasr::benchmark(
                &api_key,
                resource_id,
                volcengine_app_id.as_deref().unwrap_or_default(),
                volcengine_auth_mode.as_deref().unwrap_or("app-token"),
            )
            .await
            .map(|_| true)
            .map_err(|error| error.to_string())
        }
        "glm-asr" | "openai-whisper" | "groq-whisper" | "siliconflow" | "custom-whisper" => {
            let Some((endpoint, model, extra_fields)) =
                whisper_compat_config(&provider, stt_base_url.as_deref(), stt_model.as_deref())
            else {
                return Err(format!("Unknown STT provider: {}", provider));
            };

            let silent_pcm = vec![0u8; 3200]; // 0.1s at 16kHz 16-bit mono
            let wav = stt::whisper_compat::WhisperCompatProvider::build_wav(&silent_pcm, 16000);

            let file_part = reqwest::multipart::Part::bytes(wav)
                .file_name("test.wav")
                .mime_str("audio/wav")
                .map_err(|e| e.to_string())?;
            let mut form = reqwest::multipart::Form::new()
                .text("model", model)
                .part("file", file_part);
            for (key, value) in extra_fields {
                form = form.text(key, value);
            }

            let client = reqwest::Client::new();
            let resp = client
                .post(&endpoint)
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(resp.status().is_success())
        }
        "xiaomi-mimo" => {
            let (endpoint, model) =
                xiaomi_mimo_config(stt_base_url.as_deref(), stt_model.as_deref());

            let body = serde_json::json!({
                "model": model,
                "messages": [{
                    "role": "user",
                    "content": "hi"
                }],
                "max_completion_tokens": 1
            });

            let client = reqwest::Client::new();
            let resp = client
                .post(&endpoint)
                .header("api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(resp.status().is_success())
        }
        _ => Err(format!("Unknown STT provider: {}", provider)),
    }
}

#[tauri::command]
async fn test_llm_connection(
    app: tauri::AppHandle,
    api_key: String,
    provider: String,
    base_url: String,
    model: String,
    token_store: tauri::State<'_, SessionTokenStore>,
) -> Result<bool, String> {
    require_activation(&app, "postprocessing")?;
    if provider.is_empty() {
        return Ok(false);
    }

    // Cloud provider: verify session token + Pro status via API
    if provider == "cloud" {
        let token = crate::lock_or_recover!(token_store.0, "session_token").clone();
        if token.is_empty() {
            return Ok(false);
        }
        let api_base = api_base_url();
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/api/subscription/status", api_base))
            .header("Authorization", format!("Bearer {}", token))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        return Ok(body["plan"].as_str() == Some("pro"));
    }

    if api_key.is_empty() || base_url.is_empty() {
        return Ok(false);
    }

    // Validate base_url is a proper HTTP(S) URL
    let parsed = url::Url::parse(&base_url).map_err(|e| format!("Invalid base URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Base URL must use http or https scheme".to_string());
    }

    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(resp.status().is_success())
}

#[tauri::command]
async fn fetch_llm_models(api_key: String, base_url: String) -> Result<Vec<String>, String> {
    if base_url.is_empty() {
        return Ok(vec![]);
    }

    // Validate base_url is a proper HTTP(S) URL
    let parsed = url::Url::parse(&base_url).map_err(|e| format!("Invalid base URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Base URL must use http or https scheme".to_string());
    }

    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Ok(vec![]);
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    // OpenAI-compatible: { data: [{ id: "model-name" }] }
    // Ollama-compatible: { models: [{ name: "model-name" }] }
    let mut models: Vec<String> = Vec::new();

    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                models.push(id.to_string());
            }
        }
    } else if let Some(data) = body.get("models").and_then(|d| d.as_array()) {
        for item in data {
            if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                models.push(name.to_string());
            }
        }
    }

    models.sort();
    Ok(models)
}

#[tauri::command]
// Keep parity with `test_stt_connection`: Tauri deserializes the established
// frontend command payload into these flat parameters.
#[allow(clippy::too_many_arguments)]
async fn bench_stt_connection(
    app: tauri::AppHandle,
    api_key: String,
    provider: String,
    stt_base_url: Option<String>,
    stt_model: Option<String>,
    volcengine_app_id: Option<String>,
    volcengine_auth_mode: Option<String>,
    custom_cloud: Option<stt::custom_cloud::CustomCloudConfig>,
    token_store: tauri::State<'_, SessionTokenStore>,
) -> Result<u32, String> {
    require_activation(&app, "cloud-recognition")?;
    if provider == "custom-whisper" {
        return stt::custom_cloud::benchmark(custom_cloud.ok_or("请配置自定义云端厂商与凭证")?)
            .await
            .map_err(|e| e.to_string());
    }
    if provider.is_empty() {
        return Err("No provider specified".to_string());
    }

    if provider == "cloud" {
        let token = crate::lock_or_recover!(token_store.0, "session_token").clone();
        if token.is_empty() {
            return Err("Not signed in".to_string());
        }
        let api_base = api_base_url();
        let client = reqwest::Client::new();
        let t0 = std::time::Instant::now();
        let resp = client
            .get(format!("{}/api/subscription/status", api_base))
            .header("Authorization", format!("Bearer {}", token))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let elapsed = t0.elapsed().as_millis() as u32;
        if !resp.status().is_success() {
            return Err("Request failed".to_string());
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        if body["plan"].as_str() != Some("pro") {
            return Err("Pro plan required".to_string());
        }
        return Ok(elapsed);
    }

    if api_key.is_empty() {
        return Err("API key is empty".to_string());
    }

    match provider.as_str() {
        "deepgram" => {
            let client = reqwest::Client::new();
            let t0 = std::time::Instant::now();
            let resp = client
                .get("https://api.deepgram.com/v1/projects")
                .header("Authorization", format!("Token {}", api_key))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let elapsed = t0.elapsed().as_millis() as u32;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(elapsed)
        }
        "assemblyai" => {
            let client = reqwest::Client::new();
            let t0 = std::time::Instant::now();
            let resp = client
                .get("https://api.assemblyai.com/v2/transcript?limit=1")
                .header("Authorization", api_key)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let elapsed = t0.elapsed().as_millis() as u32;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(elapsed)
        }
        "volcengine-seedasr" => {
            let resource_id = stt_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(stt::seedasr::DEFAULT_RESOURCE_ID);
            stt::seedasr::benchmark(
                &api_key,
                resource_id,
                volcengine_app_id.as_deref().unwrap_or_default(),
                volcengine_auth_mode.as_deref().unwrap_or("app-token"),
            )
            .await
            .map_err(|error| error.to_string())
        }
        "glm-asr" | "openai-whisper" | "groq-whisper" | "siliconflow" | "custom-whisper" => {
            let Some((endpoint, model, extra_fields)) =
                whisper_compat_config(&provider, stt_base_url.as_deref(), stt_model.as_deref())
            else {
                return Err(format!("Unknown STT provider: {}", provider));
            };

            let silent_pcm = vec![0u8; 3200]; // 0.1s at 16kHz 16-bit mono
            let wav = stt::whisper_compat::WhisperCompatProvider::build_wav(&silent_pcm, 16000);

            let file_part = reqwest::multipart::Part::bytes(wav)
                .file_name("test.wav")
                .mime_str("audio/wav")
                .map_err(|e| e.to_string())?;
            let mut form = reqwest::multipart::Form::new()
                .text("model", model)
                .part("file", file_part);
            for (key, value) in extra_fields {
                form = form.text(key, value);
            }

            let client = reqwest::Client::new();
            let t0 = std::time::Instant::now();
            let resp = client
                .post(&endpoint)
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let elapsed = t0.elapsed().as_millis() as u32;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(elapsed)
        }
        "xiaomi-mimo" => {
            let (endpoint, model) =
                xiaomi_mimo_config(stt_base_url.as_deref(), stt_model.as_deref());

            let body = serde_json::json!({
                "model": model,
                "messages": [{
                    "role": "user",
                    "content": "hi"
                }],
                "max_completion_tokens": 1
            });

            let client = reqwest::Client::new();
            let t0 = std::time::Instant::now();
            let resp = client
                .post(&endpoint)
                .header("api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let elapsed = t0.elapsed().as_millis() as u32;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status()));
            }
            Ok(elapsed)
        }
        _ => Err(format!("Unknown STT provider: {}", provider)),
    }
}

#[tauri::command]
async fn bench_llm_connection(
    app: tauri::AppHandle,
    api_key: String,
    provider: String,
    base_url: String,
    model: String,
    token_store: tauri::State<'_, SessionTokenStore>,
) -> Result<u32, String> {
    require_activation(&app, "postprocessing")?;
    if provider.is_empty() {
        return Err("No provider specified".to_string());
    }

    if provider == "cloud" {
        let token = crate::lock_or_recover!(token_store.0, "session_token").clone();
        if token.is_empty() {
            return Err("Not signed in".to_string());
        }
        let api_base = api_base_url();
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}],
            "stream": false
        });
        let t0 = std::time::Instant::now();
        let resp = client
            .post(format!("{}/api/proxy/llm", api_base))
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let elapsed = t0.elapsed().as_millis() as u32;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        return Ok(elapsed);
    }

    if api_key.is_empty() || base_url.is_empty() {
        return Err("API key or base URL is empty".to_string());
    }

    let parsed = url::Url::parse(&base_url).map_err(|e| format!("Invalid base URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Base URL must use http or https scheme".to_string());
    }

    let client = reqwest::Client::new();
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1
    });

    let t0 = std::time::Instant::now();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let elapsed = t0.elapsed().as_millis() as u32;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    Ok(elapsed)
}

#[tauri::command]
async fn get_history(
    state: tauri::State<'_, storage::HistoryStore>,
    limit: u32,
    offset: u32,
) -> Result<Vec<storage::HistoryEntry>, String> {
    state.list(limit, offset).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_reward_summary(
    state: tauri::State<'_, storage::HistoryStore>,
) -> Result<storage::RewardsSummary, String> {
    state.rewards_summary().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_history(state: tauri::State<'_, storage::HistoryStore>) -> Result<(), String> {
    state.clear().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_history_entry(
    id: i64,
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::HistoryStore>,
) -> Result<(), String> {
    state.remove(id).await.map_err(|e| e.to_string())?;
    let _ = app.emit("history:deleted", serde_json::json!({ "id": id }));
    Ok(())
}

#[tauri::command]
async fn get_dictionary(
    state: tauri::State<'_, storage::DictionaryStore>,
) -> Result<Vec<storage::DictionaryEntry>, String> {
    state.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn add_dictionary_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::DictionaryStore>,
    word: String,
    pronunciation: Option<String>,
    correction_from: Option<String>,
) -> Result<(), String> {
    require_activation(&app, "hotwords")?;
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }
    if word.len() > 100 {
        return Err("Word is too long (max 100 characters)".to_string());
    }
    if let Some(ref p) = pronunciation {
        if p.len() > 100 {
            return Err("Pronunciation is too long (max 100 characters)".to_string());
        }
    }
    if let Some(ref source) = correction_from {
        if source.len() > 100 {
            return Err("Correction source is too long (max 100 characters)".to_string());
        }
        if source.trim() == word {
            return Err("Correction source must differ from the target word".to_string());
        }
    }
    state
        .add(&word, pronunciation.as_deref(), correction_from.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("dictionary:updated", ());
    Ok(())
}

#[tauri::command]
async fn update_dictionary_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::DictionaryStore>,
    id: i64,
    word: String,
    pronunciation: Option<String>,
    correction_from: Option<String>,
) -> Result<(), String> {
    require_activation(&app, "hotwords")?;
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }
    if word.len() > 100 {
        return Err("Word is too long (max 100 characters)".to_string());
    }
    if pronunciation
        .as_deref()
        .is_some_and(|value| value.len() > 100)
    {
        return Err("Pronunciation is too long (max 100 characters)".to_string());
    }
    if let Some(ref source) = correction_from {
        if source.len() > 100 {
            return Err("Correction source is too long (max 100 characters)".to_string());
        }
        if source.trim() == word {
            return Err("Correction source must differ from the target word".to_string());
        }
    }
    state
        .update(
            id,
            &word,
            pronunciation.as_deref(),
            correction_from.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit("dictionary:updated", ());
    Ok(())
}

#[tauri::command]
async fn remove_dictionary_entry(
    app: tauri::AppHandle,
    state: tauri::State<'_, storage::DictionaryStore>,
    id: i64,
) -> Result<(), String> {
    state.remove(id).await.map_err(|e| e.to_string())?;
    let _ = app.emit("dictionary:updated", ());
    Ok(())
}

#[tauri::command]
async fn get_local_model_paths(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<stt::model_manager::LocalModelPaths, String> {
    stt::model_manager::paths(&app, custom_dir.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_funasr_paths(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<stt::funasr_manager::FunAsrPaths, String> {
    stt::funasr_manager::paths(&app, custom_dir.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_native_asr_catalog() -> Vec<stt::native_asr_manager::NativeAsrModelInfo> {
    stt::native_asr_manager::catalog()
}

#[tauri::command]
async fn get_native_asr_paths(
    app: tauri::AppHandle,
    model_id: String,
    custom_dir: Option<String>,
) -> Result<stt::native_asr_manager::NativeAsrPaths, String> {
    stt::native_asr_manager::paths(&app, &model_id, custom_dir.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_native_asr_model(
    app: tauri::AppHandle,
    model_id: String,
    custom_dir: Option<String>,
) -> Result<stt::native_asr_manager::NativeAsrPaths, String> {
    stt::native_asr_manager::download_model(app, model_id, custom_dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_native_asr_model(
    app: tauri::AppHandle,
    model_id: String,
    custom_dir: Option<String>,
) -> Result<stt::native_asr_manager::NativeAsrPaths, String> {
    stt::native_asr_manager::delete_model(&app, &model_id, custom_dir.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_native_asr_download(model_id: String) {
    stt::native_asr_manager::cancel_download(&model_id);
}

#[tauri::command]
fn get_funasr_runtime_status(
    runtime: tauri::State<'_, stt::funasr_runtime::FunAsrRuntime>,
) -> stt::funasr_runtime::FunAsrRuntimeStatus {
    runtime.status()
}

#[tauri::command]
async fn download_funasr(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, stt::funasr_runtime::FunAsrRuntime>,
    config_state: tauri::State<'_, storage::ConfigManager>,
    custom_dir: Option<String>,
    force: Option<bool>,
) -> Result<(), String> {
    runtime.stop().await.map_err(|error| error.to_string())?;
    stt::funasr_manager::download(app.clone(), custom_dir.clone(), force.unwrap_or(false))
        .await
        .map_err(|error| error.to_string())?;
    if require_activation(&app, "advanced-recognition").is_err() {
        let _ = app.emit(
            "pipeline:notice",
            "精确离线组件下载完成；激活后可加载使用。",
        );
        return Ok(());
    }
    let paths = stt::funasr_manager::paths(&app, custom_dir.as_deref())
        .map_err(|error| error.to_string())?;
    let threads = config_state
        .load()
        .await
        .unwrap_or_default()
        .funasr_num_threads;
    runtime
        .start(paths, threads)
        .await
        .map_err(|error| error.to_string())?;
    let _ = app.emit(
        "pipeline:notice",
        "精确离线组件已安装并完成常驻预加载；保存识别模式后即可使用。",
    );
    Ok(())
}

#[tauri::command]
fn cancel_funasr_download() {
    stt::funasr_manager::cancel_download();
}

#[tauri::command]
async fn verify_funasr(app: tauri::AppHandle, custom_dir: Option<String>) -> Result<(), String> {
    stt::funasr_manager::verify(app, custom_dir)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_funasr(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, stt::funasr_runtime::FunAsrRuntime>,
    custom_dir: Option<String>,
) -> Result<u64, String> {
    runtime.stop().await.map_err(|error| error.to_string())?;
    stt::funasr_manager::remove(app, custom_dir)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn restart_funasr_runtime(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, stt::funasr_runtime::FunAsrRuntime>,
    config_state: tauri::State<'_, storage::ConfigManager>,
) -> Result<(), String> {
    require_activation(&app, "advanced-recognition")?;
    let config = config_state
        .load()
        .await
        .map_err(|error| error.to_string())?;
    let custom_dir = (config.funasr_use_custom_dir && !config.funasr_model_dir.trim().is_empty())
        .then_some(config.funasr_model_dir.as_str());
    let paths = stt::funasr_manager::paths(&app, custom_dir).map_err(|error| error.to_string())?;
    runtime.stop().await.map_err(|error| error.to_string())?;
    runtime
        .start(paths, config.funasr_num_threads)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_funasr_model_directory(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<(), String> {
    let paths = stt::funasr_manager::paths(&app, custom_dir.as_deref())
        .map_err(|error| error.to_string())?;
    let directory = std::path::PathBuf::from(paths.model_dir);
    if !directory.is_dir() {
        return Err(format!("模型目录不存在：{}", directory.display()));
    }
    open_directory_in_file_manager(&directory).map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_local_model(
    app: tauri::AppHandle,
    filename: String,
    custom_dir: Option<String>,
) -> Result<String, String> {
    stt::model_manager::download_model(app, filename, custom_dir)
        .await
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_local_llm_paths(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<llm::local_server::LocalLlmPaths, String> {
    llm::local_server::get_local_llm_paths(&app, custom_dir.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_local_llm(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<(), String> {
    llm::local_server::download_default_model(app, custom_dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_local_llm_download() {
    llm::local_server::cancel_download();
}

#[tauri::command]
async fn start_local_llm(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, LocalLlmServerState>,
    config_state: tauri::State<'_, storage::ConfigManager>,
) -> Result<(), String> {
    require_activation(&app, "postprocessing")?;
    let app_config = config_state.load().await.map_err(|e| e.to_string())?;

    // Pick model file — user-configured or fallback default.
    let model_filename = if app_config.local_llm_model.trim().is_empty() {
        llm::local_server::DEFAULT_LLM_MODEL.to_string()
    } else {
        app_config.local_llm_model.clone()
    };

    let start_cfg = llm::local_server::StartConfig {
        port: app_config.local_llm_port,
        num_threads: app_config.local_llm_threads,
        ctx_size: app_config.local_llm_ctx_size,
    };

    let custom_dir = if app_config.local_llm_model_dir.trim().is_empty() {
        None
    } else {
        Some(app_config.local_llm_model_dir.clone())
    };

    let server = server_state
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    server
        .start(&app, &model_filename, start_cfg, custom_dir.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_local_llm(server_state: tauri::State<'_, LocalLlmServerState>) -> Result<(), String> {
    let server = server_state
        .0
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    server.stop().map_err(|e| e.to_string())
}

#[tauri::command]
async fn local_llm_health(
    server_state: tauri::State<'_, LocalLlmServerState>,
) -> Result<bool, String> {
    // Get running state and configured port (no lock held across await)
    let (is_running, port) = {
        let server = server_state
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (server.is_running(), server.current_port())
    };

    if !is_running {
        return Ok(false);
    }

    // HTTP health check without holding any locks
    let url = format!("http://127.0.0.1:{}/health", port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => Ok(true),
        _ => Ok(false),
    }
}

#[tauri::command]
async fn update_history_entry(
    id: i64,
    polished_text: String,
    app: tauri::AppHandle,
    history_state: tauri::State<'_, storage::HistoryStore>,
) -> Result<(), String> {
    history_state
        .update_polished(id, &polished_text)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "history:updated",
        serde_json::json!({ "id": id, "polished_text": polished_text }),
    );
    Ok(())
}

#[tauri::command]
async fn hide_editor_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("editor") {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn get_sensevoice_paths(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<stt::sensevoice_manager::SenseVoicePaths, String> {
    stt::sensevoice_manager::paths(&app, custom_dir.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_sensevoice_model_directory(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
) -> Result<(), String> {
    let paths = stt::sensevoice_manager::paths(&app, custom_dir.as_deref())
        .map_err(|error| error.to_string())?;
    let directory = std::path::PathBuf::from(paths.model_dir);
    if !directory.is_dir() {
        return Err(format!("模型目录不存在：{}", directory.display()));
    }
    open_directory_in_file_manager(&directory).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn open_directory_in_file_manager(directory: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    std::process::Command::new("explorer.exe")
        .arg(directory)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_directory_in_file_manager(directory: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("open")
        .arg(directory)
        .spawn()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_directory_in_file_manager(directory: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(directory)
        .spawn()
        .map(|_| ())
}

#[tauri::command]
async fn download_sensevoice(
    app: tauri::AppHandle,
    custom_dir: Option<String>,
    force: Option<bool>,
) -> Result<(), String> {
    stt::sensevoice_manager::download_sensevoice(app.clone(), custom_dir, force.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())?;
    stt::sensevoice::invalidate_cache(&app);
    Ok(())
}

#[tauri::command]
async fn set_session_token(
    state: tauri::State<'_, SessionTokenStore>,
    token: String,
) -> Result<(), String> {
    *crate::lock_or_recover!(state.0, "session_token") = token;
    Ok(())
}

#[tauri::command]
async fn set_auto_start(
    app: tauri::AppHandle,
    config_state: tauri::State<'_, storage::ConfigManager>,
    enabled: bool,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    let mut config = config_state.load().await.map_err(|e| e.to_string())?;
    config.auto_start = enabled;
    config_state
        .save(&config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn update_hotkey(
    app: tauri::AppHandle,
    config_state: tauri::State<'_, storage::ConfigManager>,
    hotkey: String,
) -> Result<(), String> {
    let new_shortcut =
        parse_hotkey(&hotkey).ok_or_else(|| format!("Invalid hotkey: {}", hotkey))?;

    // Unregister all existing shortcuts, then register the new one
    // (the global handler from with_handler is still active)
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())?;
    app.global_shortcut()
        .register(new_shortcut)
        .map_err(|e| e.to_string())?;

    // Save updated hotkey to config
    let mut config = config_state.load().await.map_err(|e| e.to_string())?;
    config.hotkey = hotkey;
    config_state
        .save(&config)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Temporarily unregister all global shortcuts so the webview can capture key events.
#[tauri::command]
fn pause_hotkey(app: tauri::AppHandle) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| e.to_string())
}

/// Re-register the current hotkey from config after recording is done.
#[tauri::command]
async fn resume_hotkey(
    app: tauri::AppHandle,
    config_state: tauri::State<'_, storage::ConfigManager>,
) -> Result<(), String> {
    let config = config_state.load().await.map_err(|e| e.to_string())?;
    let shortcut = parse_hotkey(&config.hotkey).unwrap_or_else(default_shortcut);
    // Ensure clean state, then register
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| e.to_string())
}

// ─── Hotkey parsing ───

fn default_shortcut() -> Shortcut {
    let default_hotkey = storage::AppConfig::default().hotkey;
    let fallback = {
        #[cfg(target_os = "macos")]
        {
            Shortcut::new(Some(Modifiers::ALT), Code::Slash)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Shortcut::new(Some(Modifiers::CONTROL), Code::Slash)
        }
    };
    parse_hotkey(&default_hotkey).unwrap_or(fallback)
}

fn build_shortcut_handler(
    app_handle: tauri::AppHandle,
) -> impl Fn(&tauri::AppHandle, &Shortcut, tauri_plugin_global_shortcut::ShortcutEvent)
       + Send
       + Sync
       + 'static {
    move |_app, _shortcut, event| {
        let handle = app_handle.clone();
        match event.state {
            ShortcutState::Pressed => {
                let hotkey_mode = crate::lock_or_recover!(
                    handle.state::<HotkeyModeCache>().0,
                    "hotkey_mode_cache"
                )
                .clone();
                tauri::async_runtime::spawn(async move {
                    let pipeline = handle.state::<pipeline::PipelineHandle>();

                    if hotkey_mode == "toggle" {
                        if pipeline.current_state() == pipeline::PipelineState::Idle {
                            if let Err(e) = pipeline.start().await {
                                tracing::error!("Failed to start recording: {}", e);
                                let _ = handle.emit("pipeline:error", e.to_string());
                            }
                        } else if let Err(e) = pipeline.stop().await {
                            tracing::error!("Failed to stop recording: {}", e);
                            let _ = handle.emit("pipeline:error", e.to_string());
                        }
                    } else if let Err(e) = pipeline.start().await {
                        tracing::error!("Failed to start recording: {}", e);
                        let _ = handle.emit("pipeline:error", e.to_string());
                    }
                });
            }
            ShortcutState::Released => {
                let hotkey_mode = crate::lock_or_recover!(
                    handle.state::<HotkeyModeCache>().0,
                    "hotkey_mode_cache"
                )
                .clone();
                if hotkey_mode != "toggle" {
                    tauri::async_runtime::spawn(async move {
                        let pipeline = handle.state::<pipeline::PipelineHandle>();
                        if let Err(e) = pipeline.stop().await {
                            tracing::error!("Failed to stop recording: {}", e);
                            let _ = handle.emit("pipeline:error", e.to_string());
                        }
                    });
                }
            }
        }
    }
}

fn parse_hotkey(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let key_str = parts.last()?;

    for &part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "alt" => modifiers |= Modifiers::ALT,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "meta" | "super" | "win" | "cmd" => modifiers |= Modifiers::META,
            _ => return None,
        }
    }

    let code = match key_str.to_lowercase().as_str() {
        "space" => Code::Space,
        "tab" => Code::Tab,
        "enter" | "return" => Code::Enter,
        "backspace" => Code::Backspace,
        "escape" | "esc" => Code::Escape,
        "delete" => Code::Delete,
        "insert" => Code::Insert,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "arrowup" | "up" => Code::ArrowUp,
        "arrowdown" | "down" => Code::ArrowDown,
        "arrowleft" | "left" => Code::ArrowLeft,
        "arrowright" | "right" => Code::ArrowRight,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "/" | "slash" => Code::Slash,
        "\\" | "backslash" => Code::Backslash,
        "." | "period" => Code::Period,
        "," | "comma" => Code::Comma,
        ";" | "semicolon" => Code::Semicolon,
        "'" | "quote" => Code::Quote,
        "`" | "backquote" => Code::Backquote,
        "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal,
        "[" | "bracketleft" => Code::BracketLeft,
        "]" | "bracketright" => Code::BracketRight,
        _ => return None,
    };

    let mods = if modifiers.is_empty() {
        None
    } else {
        Some(modifiers)
    };
    Some(Shortcut::new(mods, code))
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_ctrl_slash() {
        let s = parse_hotkey("Ctrl+/");
        assert!(s.is_some());
        let s = s.unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL);
        assert_eq!(s.key, Code::Slash);
    }

    #[test]
    fn test_parse_hotkey_ctrl_shift_a() {
        let s = parse_hotkey("Ctrl+Shift+A");
        assert!(s.is_some());
        let s = s.unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(s.key, Code::KeyA);
    }

    #[test]
    fn test_parse_hotkey_case_insensitive() {
        let s = parse_hotkey("cTrL+/");
        assert!(s.is_some());
        let s = s.unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL);
        assert_eq!(s.key, Code::Slash);
    }

    #[test]
    fn test_parse_hotkey_f_keys() {
        for (key, expected) in [("F1", Code::F1), ("F12", Code::F12)] {
            let s = parse_hotkey(&format!("Ctrl+{}", key));
            assert!(s.is_some(), "Failed to parse Ctrl+{}", key);
            assert_eq!(s.unwrap().key, expected);
        }
    }

    #[test]
    fn test_parse_hotkey_meta_modifier() {
        for name in ["Meta", "Super", "Win", "Cmd"] {
            let s = parse_hotkey(&format!("{}+A", name));
            assert!(s.is_some(), "Failed to parse {}+A", name);
            assert_eq!(s.unwrap().mods, Modifiers::SUPER);
        }
    }

    #[test]
    fn test_parse_hotkey_no_modifier() {
        let s = parse_hotkey("A");
        assert!(s.is_some());
        assert_eq!(s.unwrap().mods, Modifiers::empty());
    }

    #[test]
    fn test_parse_hotkey_invalid_key() {
        let s = parse_hotkey("Alt+InvalidKey");
        assert!(s.is_none());
    }

    #[test]
    fn test_parse_hotkey_empty_string() {
        let s = parse_hotkey("");
        assert!(s.is_none());
    }

    #[test]
    fn test_parse_hotkey_digits() {
        let s = parse_hotkey("Ctrl+0");
        assert!(s.is_some());
        assert_eq!(s.unwrap().key, Code::Digit0);

        let s = parse_hotkey("Ctrl+9");
        assert!(s.is_some());
        assert_eq!(s.unwrap().key, Code::Digit9);
    }

    #[test]
    fn test_parse_hotkey_navigation_keys() {
        for (key, expected) in [
            ("Enter", Code::Enter),
            ("Tab", Code::Tab),
            ("Escape", Code::Escape),
            ("Backspace", Code::Backspace),
            ("Delete", Code::Delete),
            ("Up", Code::ArrowUp),
            ("Down", Code::ArrowDown),
        ] {
            let s = parse_hotkey(&format!("Alt+{}", key));
            assert!(s.is_some(), "Failed to parse Alt+{}", key);
            assert_eq!(s.unwrap().key, expected);
        }
    }
}
fn app_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// Inspect the same assets and protocol configuration used by the real app.
pub fn release_self_check() -> serde_json::Value {
    let context = app_context();
    let embedded_index_html = context
        .assets()
        .get(&"index.html".into())
        .is_some_and(|bytes| !bytes.is_empty());
    serde_json::json!({
        "schema_version": 1,
        "version": env!("CARGO_PKG_VERSION"),
        "custom_protocol": !tauri::is_dev(),
        "embedded_index_html": embedded_index_html,
        "embedded_asset_count": context.assets().iter().count(),
    })
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("popspeak=info".parse().expect("static directive is valid")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Deep-link URL forwarding is handled automatically by the
            // "deep-link" feature of single-instance plugin.
            // Just focus the main window so the user sees the result.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            // Open devtools only when the "devtools" feature is explicitly enabled
            #[cfg(feature = "devtools")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
                if let Some(window) = app.get_webview_window("capsule") {
                    window.open_devtools();
                }
            }

            let app_handle = app.handle().clone();

            // Initialize data directory and database
            if let Err(error) = storage::migrate_legacy_install_data(&app_handle) {
                tracing::warn!("Legacy PopSpeak data migration was skipped: {error}");
            }
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let activation_service = match activation::ActivationService::open(
                &data_dir,
                Some(include_str!("../resources/activation-public-key.txt").trim()),
            ) {
                Ok(service) => service,
                Err(error) => {
                    tracing::warn!(
                        "Activation service unavailable; history remains accessible: {error}"
                    );
                    activation::ActivationService::unavailable(error.to_string())
                }
            };
            let activation_status = activation_service.status().ok();
            let is_activated = activation_status.as_ref().is_some_and(|s| s.activated);
            let trial_available = activation_status.as_ref().is_some_and(|s| !s.activated);
            app.manage(activation_service);
            let legacy_db_path = data_dir.join("opentypeless.db");
            let db_path = data_dir.join("popspeak.db");
            if !db_path.exists() && legacy_db_path.exists() {
                std::fs::copy(&legacy_db_path, &db_path)?;
            }

            // Initialize stores
            let config_manager = storage::ConfigManager::new(app_handle.clone());
            let history_store = storage::HistoryStore::new(db_path.clone())
                .map_err(|e| anyhow::anyhow!("Failed to init history store: {}", e))?;
            match storage::load_pending_transcript(&app_handle) {
                Ok(Some(entry)) => match tauri::async_runtime::block_on(history_store.add(entry)) {
                    Ok(_) => {
                        let _ = storage::clear_pending_transcript(&app_handle);
                        tracing::info!("Recovered an interrupted transcript into History");
                    }
                    Err(error) => {
                        tracing::error!("Failed to recover interrupted transcript: {error}");
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!("Ignored invalid transcript recovery journal: {error}");
                }
            }
            let dictionary_store = storage::DictionaryStore::new(db_path)
                .map_err(|e| anyhow::anyhow!("Failed to init dictionary store: {}", e))?;
            let pipeline_handle = pipeline::PipelineHandle::new(app_handle.clone());

            // Load initial config to get hotkey
            let mut initial_config =
                tauri::async_runtime::block_on(config_manager.load()).unwrap_or_default();
            // New entitlement policy starts with a usable default mode, never deletes models/history.
            if trial_available
                && (initial_config.stt_provider != "sensevoice"
                    || initial_config.polish_enabled
                    || initial_config.translate_enabled)
            {
                initial_config.stt_provider = "sensevoice".to_string();
                initial_config.polish_enabled = false;
                initial_config.translate_enabled = false;
                tauri::async_runtime::block_on(config_manager.save(&initial_config))?;
            }
            let shortcut = parse_hotkey(&initial_config.hotkey).unwrap_or_else(default_shortcut);

            app.manage(config_manager);
            app.manage(history_store);
            app.manage(dictionary_store);
            app.manage(pipeline_handle);
            app.manage(HotkeyModeCache(Arc::new(Mutex::new(
                initial_config.hotkey_mode.clone(),
            ))));
            app.manage(CloseToTrayCache(Arc::new(Mutex::new(
                initial_config.close_to_tray,
            ))));
            app.manage(SessionTokenStore(Arc::new(Mutex::new(String::new()))));
            app.manage(stt::sensevoice::SenseVoiceRecognizerCache::default());
            app.manage(stt::funasr_runtime::FunAsrRuntime::default());
            app.manage(LocalLlmServerState(Arc::new(Mutex::new(
                llm::local_server::LocalLlmServer::new(),
            ))));

            // Sync auto-start state with system
            {
                use tauri_plugin_autostart::ManagerExt;
                let autolaunch = app.handle().autolaunch();
                let is_enabled = autolaunch.is_enabled().unwrap_or(false);
                if initial_config.auto_start && !is_enabled {
                    let _ = autolaunch.enable();
                } else if !initial_config.auto_start && is_enabled {
                    let _ = autolaunch.disable();
                }
            }

            // Register global shortcut from config
            let handler = build_shortcut_handler(app_handle.clone());
            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(handler)
                    .build(),
            )?;
            if let Err(e) = app.global_shortcut().register(shortcut) {
                tracing::warn!(
                    "Failed to register shortcut '{}' (may be occupied): {e}",
                    initial_config.hotkey
                );
            }

            // System tray
            let tray_menu = build_tray_menu(&app_handle, false, true)
                .map_err(|e| anyhow::anyhow!("Failed to build tray menu: {}", e))?;

            let tray = TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .expect("default window icon missing")
                        .clone(),
                )
                .menu(&tray_menu)
                .tooltip("PopSpeak")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show_capsule" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = handle.state::<storage::ConfigManager>();
                            if let Err(error) =
                                set_capsule_enabled(handle.clone(), state, true).await
                            {
                                tracing::warn!("Unable to enable capsule: {error}");
                            }
                        });
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    "show_hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let visible = window.is_visible().unwrap_or(false);
                            if visible {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                            refresh_tray(app);
                        }
                    }
                    "record" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let pipeline = handle.state::<pipeline::PipelineHandle>();
                            if pipeline.current_state() == pipeline::PipelineState::Idle {
                                if let Err(e) = pipeline.start().await {
                                    tracing::error!("Tray start recording failed: {}", e);
                                }
                            } else if pipeline.current_state() == pipeline::PipelineState::Recording
                            {
                                if let Err(e) = pipeline.stop().await {
                                    tracing::error!("Tray stop recording failed: {}", e);
                                }
                            }
                        });
                    }
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("tray:settings", ());
                            let _ = window.show();
                            let _ = window.set_focus();
                            refresh_tray(app);
                        }
                    }
                    "history" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("tray:history", ());
                            let _ = window.show();
                            let _ = window.set_focus();
                            refresh_tray(app);
                        }
                    }
                    "account" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("navigate", "#/account");
                            let _ = window.show();
                            let _ = window.set_focus();
                            refresh_tray(app);
                        }
                    }
                    "about" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("tray:about", ());
                            let _ = window.show();
                            let _ = window.set_focus();
                            refresh_tray(app);
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let should_show = matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        }
                    );
                    if should_show {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            refresh_tray(app);
                        }
                    }
                })
                .build(app)?;

            app.manage(TrayHandle {
                tray: Mutex::new(tray),
            });

            // Close-to-tray: intercept window close
            if let Some(main_window) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let close_to_tray = *crate::lock_or_recover!(
                            handle.state::<CloseToTrayCache>().0,
                            "close_to_tray_cache"
                        );
                        if close_to_tray {
                            api.prevent_close();
                            // Save window state before hiding (skip if minimized)
                            if let Some(w) = handle.get_webview_window("main") {
                                if let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) {
                                    if pos.x > -1000
                                        && pos.y > -1000
                                        && size.width >= 720
                                        && size.height >= 480
                                    {
                                        let ws = WindowState {
                                            x: pos.x,
                                            y: pos.y,
                                            width: size.width,
                                            height: size.height,
                                        };
                                        if let Ok(store) = handle.store("settings.json") {
                                            if let Ok(val) = serde_json::to_value(&ws) {
                                                store.set("window_state", val);
                                                let _ = store.save();
                                            }
                                        }
                                    }
                                }
                                let _ = w.hide();
                            }
                            refresh_tray(&handle);
                        }
                    }
                });
            }

            // Restore window state from previous session
            if let Ok(store) = app.handle().store("settings.json") {
                if let Some(val) = store.get("window_state") {
                    if let Ok(ws) = serde_json::from_value::<WindowState>(val.clone()) {
                        // Validate: skip if coordinates are off-screen (e.g. -32000 from minimized state)
                        if ws.x > -1000 && ws.y > -1000 && ws.width >= 720 && ws.height >= 480 {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.set_position(tauri::Position::Physical(
                                    tauri::PhysicalPosition::new(ws.x, ws.y),
                                ));
                                let _ = window.set_size(tauri::Size::Physical(
                                    tauri::PhysicalSize::new(ws.width, ws.height),
                                ));
                            }
                        }
                    }
                }
            }

            // Start minimized: only show window if not configured to start minimized
            if !initial_config.start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            tracing::info!("PopSpeak started");

            // Copy any ggml-*.bin models shipped as bundle resources into the
            // writable app data dir on first launch, so `local-whisper` boots
            // without an internet connection.
            stt::model_manager::install_bundled_models(&app_handle);

            if initial_config.stt_provider == "sensevoice" {
                let prewarm_handle = app_handle.clone();
                let language = initial_config.sensevoice_language.clone();
                let threads = initial_config.sensevoice_num_threads;
                let model_dir = if !initial_config.sensevoice_use_custom_dir
                    || initial_config.sensevoice_model_dir.trim().is_empty()
                {
                    None
                } else {
                    Some(initial_config.sensevoice_model_dir.clone())
                };
                tauri::async_runtime::spawn(async move {
                    if let Err(error) =
                        stt::sensevoice::prewarm(prewarm_handle, language, threads, model_dir).await
                    {
                        tracing::warn!("SenseVoice prewarm failed: {error}");
                    }
                });
            }

            if initial_config.capsule_enabled && initial_config.capsule_preview_enabled {
                let preview_handle = app_handle.clone();
                let preview_threads = initial_config.sensevoice_num_threads.clamp(1, 4);
                tauri::async_runtime::spawn(async move {
                    if let Err(error) =
                        stt::sensevoice::prewarm_preview(preview_handle, preview_threads).await
                    {
                        tracing::warn!("Live preview prewarm failed: {error}");
                    }
                });
            }

            // FunASR is a true resident native process: when its optional model
            // is installed, load the encoder and Qwen3 once during application
            // startup even before the first recording.
            let funasr_handle = app_handle.clone();
            let funasr_config = initial_config.clone();
            tauri::async_runtime::spawn(async move {
                if require_activation(&funasr_handle, "advanced-recognition").is_err() {
                    return;
                }
                let custom_dir = (funasr_config.funasr_use_custom_dir
                    && !funasr_config.funasr_model_dir.trim().is_empty())
                .then_some(funasr_config.funasr_model_dir.as_str());
                match stt::funasr_manager::paths(&funasr_handle, custom_dir) {
                    Ok(paths) if paths.ready => {
                        let runtime = funasr_handle
                            .state::<stt::funasr_runtime::FunAsrRuntime>()
                            .inner()
                            .clone();
                        if let Err(error) =
                            runtime.start(paths, funasr_config.funasr_num_threads).await
                        {
                            tracing::warn!("FunASR resident startup failed: {error}");
                        }
                    }
                    Ok(_) => tracing::info!("FunASR is optional and has not been downloaded"),
                    Err(error) => tracing::warn!("FunASR startup path resolution failed: {error}"),
                }
            });

            if is_activated
                && initial_config.polish_enabled
                && initial_config.llm_provider == "local-llama"
            {
                let local_handle = app_handle.clone();
                let local_config = initial_config.clone();
                tauri::async_runtime::spawn(async move {
                    let state = local_handle.state::<LocalLlmServerState>();
                    let start_config = llm::local_server::StartConfig {
                        port: local_config.local_llm_port,
                        num_threads: local_config.local_llm_threads,
                        ctx_size: local_config.local_llm_ctx_size,
                    };
                    let model_dir = if local_config.local_llm_model_dir.trim().is_empty() {
                        None
                    } else {
                        Some(local_config.local_llm_model_dir.as_str())
                    };
                    let server = state.0.lock().unwrap_or_else(|error| error.into_inner());
                    if let Err(error) = server.start(
                        &local_handle,
                        &local_config.local_llm_model,
                        start_config,
                        model_dir,
                    ) {
                        tracing::warn!("Local LLM auto-start skipped: {error}");
                    }
                });
            }

            // P1-2: Pre-warm HTTP connection pool in background
            let warm_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let pipeline = warm_handle.state::<pipeline::PipelineHandle>();
                pipeline.pre_warm().await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quit_app,
            start_recording,
            stop_recording,
            abort_recording,
            check_accessibility_permission,
            request_accessibility_permission,
            list_audio_input_devices,
            check_for_updates,
            get_config,
            get_activation_status,
            activate_license,
            update_config,
            set_capsule_enabled,
            patch_capsule_preferences,
            test_stt_connection,
            test_llm_connection,
            bench_stt_connection,
            bench_llm_connection,
            fetch_llm_models,
            get_history,
            get_reward_summary,
            clear_history,
            delete_history_entry,
            get_dictionary,
            add_dictionary_entry,
            update_dictionary_entry,
            remove_dictionary_entry,
            update_hotkey,
            pause_hotkey,
            resume_hotkey,
            set_auto_start,
            set_session_token,
            get_local_model_paths,
            get_native_asr_catalog,
            get_native_asr_paths,
            download_native_asr_model,
            delete_native_asr_model,
            cancel_native_asr_download,
            get_funasr_paths,
            get_funasr_runtime_status,
            download_funasr,
            cancel_funasr_download,
            verify_funasr,
            remove_funasr,
            restart_funasr_runtime,
            open_funasr_model_directory,
            download_local_model,
            get_local_llm_paths,
            download_local_llm,
            cancel_local_llm_download,
            start_local_llm,
            stop_local_llm,
            local_llm_health,
            update_history_entry,
            hide_editor_window,
            get_sensevoice_paths,
            open_sensevoice_model_directory,
            download_sensevoice,
        ])
        .run(app_context())
        .expect("error while running tauri application");
}
