use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::llm::PolishMode;

const LEGACY_APP_IDENTIFIERS: &[&str] = &["com.popspeak.app", "com.opentypeless.app"];

fn stt_provider_uses_dedicated_credential(provider: &str) -> bool {
    provider == "volcengine-seedasr"
}

fn config_for_disk(config: &AppConfig) -> AppConfig {
    let mut persisted = config.clone();
    #[cfg(windows)]
    {
        persisted.stt_api_key.clear();
        persisted.llm_api_key.clear();
    }
    persisted
}

fn copy_directory_missing(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory_missing(&entry.path(), &target)?;
        } else if file_type.is_file() && !target.exists() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn backup_database(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    let temporary = destination.with_extension("db.migrating");
    if temporary.exists() {
        std::fs::remove_file(&temporary)?;
    }
    let source_connection = Connection::open(source)?;
    let mut destination_connection = Connection::open(&temporary)?;
    let result = (|| -> Result<()> {
        let backup =
            rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)?;
        backup.run_to_completion(64, std::time::Duration::from_millis(20), None)?;
        drop(backup);
        drop(destination_connection);
        std::fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

/// Preserve settings, history, learned vocabulary and downloaded models across
/// the OpenTypeless → PopSpeak identifier changes. Existing PopSpeak files always
/// win, so the migration is idempotent and never overwrites current user data.
pub fn migrate_legacy_install_data(app: &tauri::AppHandle) -> Result<()> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    if let Some(parent) = data_dir.parent() {
        for identifier in LEGACY_APP_IDENTIFIERS {
            let legacy = parent.join(identifier);
            if !legacy.is_dir() {
                continue;
            }
            for filename in ["settings.json", PENDING_TRANSCRIPT_FILE] {
                let source = legacy.join(filename);
                let destination = data_dir.join(filename);
                if source.is_file() && !destination.exists() {
                    std::fs::copy(source, destination)?;
                }
            }
            let destination_db = data_dir.join("popspeak.db");
            if !destination_db.exists() {
                let source_db = [legacy.join("popspeak.db"), legacy.join("opentypeless.db")]
                    .into_iter()
                    .find(|path| path.is_file());
                if let Some(source_db) = source_db {
                    backup_database(&source_db, &destination_db)?;
                }
            }
            copy_directory_missing(&legacy.join("models"), &data_dir.join("models"))?;
        }
    }

    let local_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&local_dir)?;
    if let Some(parent) = local_dir.parent() {
        for identifier in LEGACY_APP_IDENTIFIERS {
            let legacy = parent.join(identifier);
            if !legacy.is_dir() {
                continue;
            }
            copy_directory_missing(&legacy.join("models"), &local_dir.join("models"))?;
            copy_directory_missing(&legacy.join("llm_models"), &local_dir.join("llm_models"))?;
        }
    }
    Ok(())
}

#[cfg(windows)]
mod credentials;

#[cfg(windows)]
const STT_CREDENTIAL_TARGET: &str = "PopSpeak/STT API Key";
#[cfg(windows)]
const LLM_CREDENTIAL_TARGET: &str = "PopSpeak/LLM API Key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub native_asr: crate::stt::native_asr::NativeAsrConfig,
    pub custom_cloud: crate::stt::custom_cloud::CustomCloudConfig,
    pub stt_provider: String,
    pub stt_api_key: String,
    #[serde(default = "default_volcengine_auth_mode")]
    pub volcengine_auth_mode: String,
    #[serde(default)]
    pub volcengine_app_id: String,
    /// SeedASR uses its own plaintext credential so changing the active STT
    /// provider cannot erase the user's long-lived Access Token / API Key.
    #[serde(default)]
    pub volcengine_credential: String,
    pub stt_base_url: String,
    pub stt_model: String,
    pub stt_language: String,
    pub llm_provider: String,
    pub llm_api_key: String,
    pub llm_model: String,
    pub llm_base_url: String,
    pub polish_enabled: bool,
    pub polish_mode: PolishMode,
    pub translate_enabled: bool,
    pub target_lang: String,
    pub hotkey: String,
    pub hotkey_mode: String,
    pub output_mode: String,
    /// Automatically close the result editor after a configurable idle period.
    #[serde(default = "default_true")]
    pub editor_auto_hide_enabled: bool,
    /// Idle seconds before the result editor closes. Clamped to a usable range.
    #[serde(default = "default_editor_auto_hide_seconds")]
    pub editor_auto_hide_seconds: u32,
    pub selected_text_enabled: bool,
    pub theme: String,
    pub auto_start: bool,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub max_recording_seconds: u32,
    pub ui_language: String,
    /// Whether the dictation capsule is available. Enabled by default.
    pub capsule_enabled: bool,
    /// Keep the capsule above normal application windows.
    pub capsule_always_on_top: bool,
    pub capsule_auto_hide: bool,
    /// Show a live transcript popover while dictating. Existing installations
    /// opt in when this new field is absent, without changing visibility choices.
    #[serde(default = "default_true")]
    pub capsule_preview_enabled: bool,
    /// Preferred microphone name. Empty means the operating-system default.
    #[serde(default)]
    pub audio_device_name: String,
    /// Conservative CPU VAD trims leading/trailing silence with a pre-roll.
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
    /// Lightweight CPU audio conditioning; no GPU or network is used.
    #[serde(default = "default_true")]
    pub noise_suppression_enabled: bool,
    pub whisper_cli_path: String,
    pub whisper_model_path: String,
    pub whisper_lora_path: String,
    // SenseVoice offline STT configuration
    #[serde(default = "default_sensevoice_language")]
    pub sensevoice_language: String,
    #[serde(default = "default_sensevoice_threads")]
    pub sensevoice_num_threads: i32,
    /// Whether SenseVoice should use the user-selected directory instead of
    /// the portable `./models/sensevoice` directory.
    #[serde(default)]
    pub sensevoice_use_custom_dir: bool,
    /// User-selected SenseVoice model directory. Ignored unless the explicit
    /// custom-directory switch above is true.
    #[serde(default)]
    pub sensevoice_model_dir: String,
    /// Whether FunASR uses a user-selected model directory instead of the
    /// executable-relative `./models/funasr-nano` directory.
    #[serde(default)]
    pub funasr_use_custom_dir: bool,
    #[serde(default)]
    pub funasr_model_dir: String,
    #[serde(default = "default_funasr_threads")]
    pub funasr_num_threads: u32,
    // Application-managed Qwen polisher (served by llama.cpp) configuration
    #[serde(default = "default_local_llm_model")]
    pub local_llm_model: String,
    #[serde(default = "default_local_llm_port")]
    pub local_llm_port: u16,
    #[serde(default = "default_local_llm_threads")]
    pub local_llm_threads: u32,
    #[serde(default = "default_local_llm_ctx_size")]
    pub local_llm_ctx_size: u32,
    /// 自定义模型目录，空字符串表示使用默认（AppData/models/llm）
    #[serde(default)]
    pub local_llm_model_dir: String,
}

fn default_sensevoice_language() -> String {
    "auto".to_string()
}
fn default_volcengine_auth_mode() -> String {
    "app-token".to_string()
}
fn cpu_worker_threads(maximum: usize) -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| (parallelism.get() / 2).clamp(2, maximum))
        .unwrap_or(2)
}

fn default_sensevoice_threads() -> i32 {
    cpu_worker_threads(4) as i32
}
fn default_funasr_threads() -> u32 {
    cpu_worker_threads(8) as u32
}
fn default_true() -> bool {
    true
}
fn default_editor_auto_hide_seconds() -> u32 {
    5
}
fn default_local_llm_model() -> String {
    "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string()
}
fn default_local_llm_port() -> u16 {
    11434
}
fn default_local_llm_threads() -> u32 {
    cpu_worker_threads(4) as u32
}
fn default_local_llm_ctx_size() -> u32 {
    2048
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            native_asr: crate::stt::native_asr::NativeAsrConfig::default(),
            stt_provider: "sensevoice".to_string(),
            custom_cloud: crate::stt::custom_cloud::CustomCloudConfig::default(),
            stt_api_key: String::new(),
            volcengine_auth_mode: default_volcengine_auth_mode(),
            volcengine_app_id: String::new(),
            volcengine_credential: String::new(),
            stt_base_url: String::new(),
            stt_model: String::new(),
            stt_language: "multi".to_string(),
            llm_provider: "local-llama".to_string(),
            llm_api_key: String::new(),
            llm_model: "qwen2.5-0.5b-instruct".to_string(),
            llm_base_url: "http://127.0.0.1:11434/v1".to_string(),
            polish_enabled: false,
            polish_mode: PolishMode::Fast,
            translate_enabled: false,
            target_lang: "en".to_string(),
            #[cfg(target_os = "macos")]
            hotkey: "Alt+/".to_string(),
            #[cfg(not(target_os = "macos"))]
            hotkey: "Ctrl+/".to_string(),
            hotkey_mode: "hold".to_string(),
            output_mode: "clipboard".to_string(),
            editor_auto_hide_enabled: true,
            editor_auto_hide_seconds: default_editor_auto_hide_seconds(),
            selected_text_enabled: false,
            theme: "system".to_string(),
            auto_start: false,
            close_to_tray: true,
            start_minimized: false,
            max_recording_seconds: 30,
            ui_language: "zh".to_string(),
            capsule_enabled: true,
            capsule_always_on_top: true,
            capsule_auto_hide: false,
            capsule_preview_enabled: true,
            audio_device_name: String::new(),
            vad_enabled: true,
            noise_suppression_enabled: true,
            whisper_cli_path: String::new(),
            whisper_model_path: String::new(),
            whisper_lora_path: String::new(),
            sensevoice_language: default_sensevoice_language(),
            sensevoice_num_threads: default_sensevoice_threads(),
            sensevoice_use_custom_dir: false,
            sensevoice_model_dir: String::new(),
            funasr_use_custom_dir: false,
            funasr_model_dir: String::new(),
            funasr_num_threads: default_funasr_threads(),
            local_llm_model: default_local_llm_model(),
            local_llm_port: default_local_llm_port(),
            local_llm_threads: default_local_llm_threads(),
            local_llm_ctx_size: default_local_llm_ctx_size(),
            local_llm_model_dir: String::new(),
        }
    }
}

impl AppConfig {
    fn normalize_managed_local_resources(&mut self) {
        self.editor_auto_hide_seconds = self.editor_auto_hide_seconds.clamp(3, 60);
        if !matches!(self.volcengine_auth_mode.as_str(), "app-token" | "api-key") {
            self.volcengine_auth_mode = default_volcengine_auth_mode();
        }
        if matches!(
            self.stt_provider.as_str(),
            "deepgram"
                | "assemblyai"
                | "glm-asr"
                | "openai-whisper"
                | "groq-whisper"
                | "siliconflow"
                | "xiaomi-mimo"
        ) {
            // These legacy cloud presets are no longer exposed by the streamlined UI.
            // Migrate an old saved choice to the same safe offline default shown in Settings.
            self.stt_provider = "sensevoice".to_string();
        }
        if self.stt_provider == "native-asr"
            && !crate::stt::native_asr_manager::PUBLIC_CATALOG_ENABLED
        {
            // Existing installations can keep their downloaded files, but a
            // public installer must not resume a model pending license review.
            self.stt_provider = "sensevoice".to_string();
        }
        if !matches!(
            self.llm_provider.as_str(),
            "local-llama" | "ollama" | "openrouter" | "cloud"
        ) {
            self.llm_provider = "local-llama".to_string();
        }
        if !self.sensevoice_use_custom_dir {
            // Older builds persisted an absolute development path even though
            // the UI described it as automatic indexing. Explicit mode avoids
            // carrying that machine-specific path into portable releases.
            self.sensevoice_model_dir.clear();
        }
        if !self.funasr_use_custom_dir {
            self.funasr_model_dir.clear();
        }
        self.funasr_num_threads = self.funasr_num_threads.clamp(1, 16);
        self.native_asr.num_threads = self.native_asr.num_threads.clamp(1, 16);
        if self.llm_provider == "local-llama" {
            // The bundled Qwen polisher is application-managed in the portable
            // distribution. Discard legacy absolute development paths and keep
            // the endpoint derived from the internal port.
            self.local_llm_model_dir.clear();
            self.local_llm_model = default_local_llm_model();
            self.llm_model = "qwen2.5-0.5b-instruct".to_string();
            self.llm_base_url = format!("http://127.0.0.1:{}/v1", self.local_llm_port);
        }
    }

    /// Migrate old Whisper-compatible settings once, never mix another vendor's credentials.
    fn migrate_custom_cloud(&mut self) {
        if self.stt_provider == "custom-whisper"
            && self.custom_cloud.vendor == "whisper"
            && self.custom_cloud.endpoint.is_empty()
            && self.custom_cloud.api_key.is_empty()
            && !self.stt_base_url.is_empty()
        {
            self.custom_cloud.endpoint.clone_from(&self.stt_base_url);
            self.custom_cloud.model.clone_from(&self.stt_model);
            self.custom_cloud.api_key.clone_from(&self.stt_api_key);
        }
    }
}

fn config_from_store_value(value: &serde_json::Value) -> AppConfig {
    let is_legacy_capsule_config = value.get("capsule_enabled").is_none();
    let mut config = serde_json::from_value::<AppConfig>(value.clone()).unwrap_or_default();
    if is_legacy_capsule_config {
        // The previous UI exposed only the inverse "hide when idle" option.
        // On upgrade, make the newly explicit capsule feature visible once so
        // users do not mistake a persisted auto-hide value for a missing window.
        // After the new fields are saved, all three user choices are preserved.
        config.capsule_enabled = true;
        config.capsule_always_on_top = true;
        config.capsule_auto_hide = false;
    }
    config
}

// ─── ConfigManager (tauri-plugin-store backed) ───

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsulePreferencesPatch {
    pub capsule_enabled: Option<bool>,
    pub capsule_always_on_top: Option<bool>,
    pub capsule_auto_hide: Option<bool>,
    pub capsule_preview_enabled: Option<bool>,
}

impl CapsulePreferencesPatch {
    fn apply(&self, config: &mut AppConfig) {
        if let Some(value) = self.capsule_enabled {
            config.capsule_enabled = value;
        }
        if let Some(value) = self.capsule_always_on_top {
            config.capsule_always_on_top = value;
        }
        if let Some(value) = self.capsule_auto_hide {
            config.capsule_auto_hide = value;
        }
        if let Some(value) = self.capsule_preview_enabled {
            config.capsule_preview_enabled = value;
        }
    }
}

pub struct ConfigManager {
    app_handle: tauri::AppHandle,
    cache: Mutex<Option<AppConfig>>,
}

impl ConfigManager {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            cache: Mutex::new(None),
        }
    }

    pub async fn load(&self) -> Result<AppConfig> {
        if let Some(config) = crate::lock_or_recover!(self.cache, "config_cache").clone() {
            return Ok(config);
        }

        let mut config = match self.app_handle.store("settings.json") {
            Ok(store) => match store.get("app_config") {
                Some(val) => config_from_store_value(&val),
                None => AppConfig::default(),
            },
            Err(_) => AppConfig::default(),
        };
        config.normalize_managed_local_resources();

        #[cfg(windows)]
        {
            // Generic provider secrets migrate to Windows Credential Manager.
            // SeedASR owns a dedicated plaintext field so its credential remains
            // available even while SenseVoice or another provider is selected.
            let legacy_stt = config.stt_api_key.clone();
            let legacy_llm = config.llm_api_key.clone();
            let uses_dedicated_stt = stt_provider_uses_dedicated_credential(&config.stt_provider);
            if uses_dedicated_stt
                && config.volcengine_credential.is_empty()
                && !legacy_stt.is_empty()
            {
                config.volcengine_credential.clone_from(&legacy_stt);
            }
            if !legacy_stt.is_empty() && !uses_dedicated_stt {
                credentials::save(STT_CREDENTIAL_TARGET, &legacy_stt)?;
            } else if legacy_stt.is_empty() && !uses_dedicated_stt {
                if let Some(secret) = credentials::load(STT_CREDENTIAL_TARGET) {
                    config.stt_api_key = secret;
                }
            }
            if uses_dedicated_stt {
                config.stt_api_key.clear();
            }
            if !legacy_llm.is_empty() {
                credentials::save(LLM_CREDENTIAL_TARGET, &legacy_llm)?;
            } else if let Some(secret) = credentials::load(LLM_CREDENTIAL_TARGET) {
                config.llm_api_key = secret;
            }
            if !legacy_stt.is_empty() || !legacy_llm.is_empty() {
                let sanitized = config_for_disk(&config);
                let store = self.app_handle.store("settings.json")?;
                store.set("app_config", serde_json::to_value(sanitized)?);
                store.save()?;
            }
        }

        config.migrate_custom_cloud();
        *crate::lock_or_recover!(self.cache, "config_cache") = Some(config.clone());
        Ok(config)
    }

    pub async fn save(&self, config: &AppConfig) -> Result<()> {
        self.save_with_capsule_policy(config, false).await
    }

    /// Main settings drafts do not own the immediately persisted capsule fields.
    /// Keep concurrent changes from the floating window instead of replaying an
    /// older main-window snapshot. Other callers retain normal save semantics.
    pub async fn save_main_settings(&self, config: &AppConfig) -> Result<()> {
        self.save_with_capsule_policy(config, true).await
    }

    async fn save_with_capsule_policy(
        &self,
        config: &AppConfig,
        preserve_capsule: bool,
    ) -> Result<()> {
        let mut normalized = config.clone();
        normalized.normalize_managed_local_resources();

        #[cfg(windows)]
        {
            if !stt_provider_uses_dedicated_credential(&normalized.stt_provider) {
                credentials::save(STT_CREDENTIAL_TARGET, &normalized.stt_api_key)?;
            }
            credentials::save(LLM_CREDENTIAL_TARGET, &normalized.llm_api_key)?;
        }

        let store = self
            .app_handle
            .store("settings.json")
            .map_err(|e| anyhow::anyhow!("Failed to open store: {}", e))?;
        persist_then_cache(&self.cache, normalized, preserve_capsule, |confirmed| {
            let val = serde_json::to_value(config_for_disk(confirmed))?;
            let previous = store.get("app_config");
            store.set("app_config", val);
            if let Err(error) = store.save() {
                // Roll back the store's in-memory value as well as retaining the
                // active cache, so a failed save cannot silently switch engines.
                if let Some(value) = previous {
                    store.set("app_config", value);
                } else {
                    store.delete("app_config");
                }
                return Err(anyhow::anyhow!("{}", error));
            }
            Ok(())
        })
    }

    /// Persist only capsule preferences against the latest cached configuration.
    /// This avoids activation checks, credential migration and unrelated drafts.
    pub async fn patch_capsule_preferences(
        &self,
        patch: CapsulePreferencesPatch,
    ) -> Result<AppConfig> {
        let loaded = self.load().await?;
        let mut cache = crate::lock_or_recover!(self.cache, "config_cache");
        let mut config = cache.clone().unwrap_or(loaded);
        patch.apply(&mut config);
        let store = self.app_handle.store("settings.json")?;
        let previous = store.get("app_config");
        let mut disk = previous
            .clone()
            .unwrap_or(serde_json::to_value(config_for_disk(&config))?);
        let object = disk
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid settings object"))?;
        for (name, value) in [
            ("capsule_enabled", patch.capsule_enabled),
            ("capsule_always_on_top", patch.capsule_always_on_top),
            ("capsule_auto_hide", patch.capsule_auto_hide),
            ("capsule_preview_enabled", patch.capsule_preview_enabled),
        ] {
            if let Some(value) = value {
                object.insert(name.into(), serde_json::Value::Bool(value));
            }
        }
        store.set("app_config", disk);
        if let Err(error) = store.save() {
            if let Some(value) = previous {
                store.set("app_config", value);
            } else {
                store.delete("app_config");
            }
            return Err(error.into());
        }
        *cache = Some(config.clone());
        Ok(config)
    }
}

fn persist_then_cache(
    cache: &Mutex<Option<AppConfig>>,
    mut config: AppConfig,
    preserve_capsule: bool,
    persist: impl FnOnce(&AppConfig) -> Result<()>,
) -> Result<()> {
    let mut guard = crate::lock_or_recover!(cache, "config_cache");
    if preserve_capsule {
        if let Some(latest) = guard.as_ref() {
            config.capsule_enabled = latest.capsule_enabled;
            config.capsule_always_on_top = latest.capsule_always_on_top;
            config.capsule_auto_hide = latest.capsule_auto_hide;
            config.capsule_preview_enabled = latest.capsule_preview_enabled;
        }
    }
    persist(&config)?;
    *guard = Some(config);
    Ok(())
}

// ─── HistoryStore (SQLite backed) ───

/// Maximum number of history entries to retain. Older entries are pruned on insert.
const MAX_HISTORY_ENTRIES: u32 = 5000;

const REWARD_DAILY_LIMIT: i64 = 100;
const REWARD_MINIMUM_DURATION_MS: i64 = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct RewardActivity {
    pub history_id: i64,
    pub points: i64,
    pub credited_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RewardsSummary {
    pub total_points: i64,
    pub today_points: i64,
    pub daily_limit: i64,
    pub minimum_duration_ms: i64,
    pub local_day: String,
    /// Local experience points are not redeemable without future server verification.
    pub redemption_available: bool,
    pub recent_activity: Vec<RewardActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: String,
    pub app_name: String,
    pub app_type: String,
    pub raw_text: String,
    pub polished_text: String,
    pub language: Option<String>,
    pub duration_ms: Option<i64>,
}

const PENDING_TRANSCRIPT_FILE: &str = "pending-transcript.json";

fn pending_transcript_paths(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let data_dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    let destination = data_dir.join(PENDING_TRANSCRIPT_FILE);
    Ok((
        destination.clone(),
        destination.with_extension("json.tmp"),
        destination.with_extension("json.bak"),
    ))
}

/// Persist a completed transcript before it is delivered to another app. This
/// tiny local journal is recovered into History after an unexpected exit.
pub fn save_pending_transcript(app: &tauri::AppHandle, entry: &HistoryEntry) -> Result<()> {
    use std::io::Write;

    let (destination, temporary, backup) = pending_transcript_paths(app)?;
    let bytes = serde_json::to_vec(entry)?;
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;

    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    if destination.exists() {
        std::fs::rename(&destination, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, &destination) {
        if backup.exists() && !destination.exists() {
            let _ = std::fs::rename(&backup, &destination);
        }
        return Err(error.into());
    }
    if backup.exists() {
        std::fs::remove_file(backup)?;
    }
    Ok(())
}

pub fn load_pending_transcript(app: &tauri::AppHandle) -> Result<Option<HistoryEntry>> {
    let (destination, _, backup) = pending_transcript_paths(app)?;
    let source = if destination.exists() {
        destination
    } else if backup.exists() {
        backup
    } else {
        return Ok(None);
    };
    let bytes = std::fs::read(source)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

pub fn clear_pending_transcript(app: &tauri::AppHandle) -> Result<()> {
    let (destination, temporary, backup) = pending_transcript_paths(app)?;
    for path in [destination, temporary, backup] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

impl HistoryStore {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                app_name TEXT NOT NULL DEFAULT '',
                app_type TEXT NOT NULL DEFAULT '',
                raw_text TEXT NOT NULL DEFAULT '',
                polished_text TEXT NOT NULL DEFAULT '',
                language TEXT,
                duration_ms INTEGER
            );",
        )?;
        // No foreign key on history_id: deleting/pruning history must neither
        // remove earned points nor let that history ID earn points again.
        // Capture the pre-feature high-water mark once; old history is never
        // retroactively awarded, including after a restart or history edit.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reward_policy (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                history_start_id INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO reward_policy (id, history_start_id)
            SELECT 1, COALESCE(MAX(id), 0) FROM history;
            CREATE TABLE IF NOT EXISTS reward_ledger (
                history_id INTEGER PRIMARY KEY,
                points INTEGER NOT NULL CHECK (points IN (0, 1)),
                local_day TEXT NOT NULL,
                credited_at TEXT NOT NULL,
                reason TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reward_receipts (
                recognition_key TEXT PRIMARY KEY,
                history_id INTEGER NOT NULL UNIQUE
            );
            CREATE INDEX IF NOT EXISTS reward_ledger_day ON reward_ledger(local_day);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub async fn add(&self, entry: HistoryEntry) -> Result<i64> {
        let now = chrono::Local::now();
        self.insert_history_at(
            entry,
            &now.format("%Y-%m-%d").to_string(),
            &now.to_rfc3339(),
        )
    }

    fn insert_history_at(
        &self,
        entry: HistoryEntry,
        reward_day: &str,
        credited_at: &str,
    ) -> Result<i64> {
        let mut conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO history (created_at, app_name, app_type, raw_text, polished_text, language, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.created_at,
                entry.app_name,
                entry.app_type,
                entry.raw_text,
                entry.polished_text,
                entry.language,
                entry.duration_ms,
            ],
        )?;
        let id = transaction.last_insert_rowid();
        record_recognition_reward(&transaction, id, &entry, reward_day, credited_at)?;

        // Prune old entries beyond the retention limit
        transaction.execute(
            "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT ?1)",
            rusqlite::params![MAX_HISTORY_ENTRIES],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub async fn rewards_summary(&self) -> Result<RewardsSummary> {
        self.rewards_summary_for_day(&chrono::Local::now().format("%Y-%m-%d").to_string())
    }

    fn rewards_summary_for_day(&self, local_day: &str) -> Result<RewardsSummary> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let (total_points, today_points) = conn.query_row(
            "SELECT COALESCE(SUM(points), 0),
                    COALESCE(SUM(CASE WHEN local_day = ?1 THEN points ELSE 0 END), 0)
             FROM reward_ledger",
            rusqlite::params![local_day],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let mut statement = conn.prepare(
            "SELECT history_id, points, credited_at FROM reward_ledger
             WHERE points > 0 ORDER BY history_id DESC LIMIT 10",
        )?;
        let recent_activity = statement
            .query_map([], |row| {
                Ok(RewardActivity {
                    history_id: row.get(0)?,
                    points: row.get(1)?,
                    credited_at: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(RewardsSummary {
            total_points,
            today_points,
            daily_limit: REWARD_DAILY_LIMIT,
            minimum_duration_ms: REWARD_MINIMUM_DURATION_MS,
            local_day: local_day.into(),
            redemption_available: false,
            recent_activity,
        })
    }

    pub async fn update_polished(&self, id: i64, polished_text: &str) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        conn.execute(
            "UPDATE history SET polished_text = ?1 WHERE id = ?2",
            rusqlite::params![polished_text, id],
        )?;
        Ok(())
    }

    pub async fn list(&self, limit: u32, offset: u32) -> Result<Vec<HistoryEntry>> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let mut stmt = conn.prepare(
            "SELECT id, created_at, app_name, app_type, raw_text, polished_text, language, duration_ms
             FROM history ORDER BY id DESC LIMIT ?1 OFFSET ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                app_name: row.get(2)?,
                app_type: row.get(3)?,
                raw_text: row.get(4)?,
                polished_text: row.get(5)?,
                language: row.get(6)?,
                duration_ms: row.get(7)?,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub async fn remove(&self, id: i64) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        conn.execute("DELETE FROM history WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    pub async fn clear(&self) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        conn.execute("DELETE FROM history", [])?;
        Ok(())
    }
}

/// Called only while inserting a newly completed recognition in the same SQLite
/// transaction. There is intentionally no command for granting client-supplied
/// points, reprocessing old history, or redeeming the local balance.
fn record_recognition_reward(
    conn: &Connection,
    history_id: i64,
    entry: &HistoryEntry,
    local_day: &str,
    credited_at: &str,
) -> Result<()> {
    let history_start_id: i64 = conn.query_row(
        "SELECT history_start_id FROM reward_policy WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    if history_id <= history_start_id {
        return Ok(());
    }
    let already_evaluated: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM reward_ledger WHERE history_id = ?1)",
        rusqlite::params![history_id],
        |row| row.get(0),
    )?;
    if already_evaluated {
        return Ok(());
    }
    // A completed transcript's recovery journal may survive a successful insert
    // if cleanup fails. Replaying it creates a new history ID, but must not mint
    // a second point. Store only a digest, never transcript text, in this ledger.
    use sha2::{Digest, Sha256};
    let receipt_data = serde_json::to_vec(&(
        &entry.created_at,
        &entry.app_name,
        &entry.app_type,
        &entry.raw_text,
        &entry.language,
        entry.duration_ms,
    ))?;
    let recognition_key = format!("{:x}", Sha256::digest(receipt_data));
    let new_recognition = conn.execute(
        "INSERT OR IGNORE INTO reward_receipts (recognition_key, history_id) VALUES (?1, ?2)",
        rusqlite::params![recognition_key, history_id],
    )? == 1;
    let today_points: i64 = conn.query_row(
        "SELECT COALESCE(SUM(points), 0) FROM reward_ledger WHERE local_day = ?1",
        rusqlite::params![local_day],
        |row| row.get(0),
    )?;
    let (points, reason) = if !new_recognition {
        (0, "recovery_replay")
    } else if entry.duration_ms.unwrap_or(0) < REWARD_MINIMUM_DURATION_MS {
        (0, "too_short")
    } else if !entry.raw_text.chars().any(char::is_alphanumeric) {
        (0, "empty_transcript")
    } else if today_points >= REWARD_DAILY_LIMIT {
        (0, "daily_limit")
    } else {
        (1, "recognition")
    };
    conn.execute(
        "INSERT OR IGNORE INTO reward_ledger
         (history_id, points, local_day, credited_at, reason) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![history_id, points, local_day, credited_at, reason],
    )?;
    Ok(())
}

// ─── DictionaryStore (SQLite backed) ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryEntry {
    pub id: i64,
    pub word: String,
    pub pronunciation: Option<String>,
    /// A form previously produced by ASR that should map to `word`.
    pub correction_from: Option<String>,
}

pub struct DictionaryStore {
    conn: Mutex<Connection>,
}

impl DictionaryStore {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dictionary (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT NOT NULL,
                pronunciation TEXT,
                correction_from TEXT
            );",
        )?;
        let has_correction_from = {
            let mut statement = conn.prepare("PRAGMA table_info(dictionary)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            let has_column = columns
                .filter_map(|column| column.ok())
                .any(|name| name == "correction_from");
            drop(statement);
            has_column
        };
        if !has_correction_from {
            conn.execute("ALTER TABLE dictionary ADD COLUMN correction_from TEXT", [])?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub async fn add(
        &self,
        word: &str,
        pronunciation: Option<&str>,
        correction_from: Option<&str>,
    ) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let correction_from = correction_from
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let existing = conn.query_row(
            "SELECT id FROM dictionary
             WHERE word = ?1 AND COALESCE(correction_from, '') = COALESCE(?2, '')
             LIMIT 1",
            rusqlite::params![word, correction_from],
            |row| row.get::<_, i64>(0),
        );
        match existing {
            Ok(id) => {
                conn.execute(
                    "UPDATE dictionary SET pronunciation = ?1 WHERE id = ?2",
                    rusqlite::params![pronunciation, id],
                )?;
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                conn.execute(
                    "INSERT INTO dictionary (word, pronunciation, correction_from)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![word, pronunciation, correction_from],
                )?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub async fn remove(&self, id: i64) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        conn.execute(
            "DELETE FROM dictionary WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: i64,
        word: &str,
        pronunciation: Option<&str>,
        correction_from: Option<&str>,
    ) -> Result<()> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let pronunciation = pronunciation
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let correction_from = correction_from
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let changed = conn.execute(
            "UPDATE dictionary
             SET word = ?1, pronunciation = ?2, correction_from = ?3
             WHERE id = ?4",
            rusqlite::params![word.trim(), pronunciation, correction_from, id],
        )?;
        if changed == 0 {
            anyhow::bail!("dictionary entry {id} does not exist");
        }
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<DictionaryEntry>> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let mut stmt = conn.prepare(
            "SELECT id, word, pronunciation, correction_from FROM dictionary ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DictionaryEntry {
                id: row.get(0)?,
                word: row.get(1)?,
                pronunciation: row.get(2)?,
                correction_from: row.get(3)?,
            })
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub async fn words(&self) -> Vec<String> {
        let conn = crate::lock_or_recover!(self.conn, "storage_conn");
        let mut stmt = match conn.prepare("SELECT word FROM dictionary") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reward_entry(duration_ms: Option<i64>, raw_text: &str) -> HistoryEntry {
        HistoryEntry {
            id: 0,
            created_at: "2026-09-18T10:00:00+08:00".into(),
            app_name: "test".into(),
            app_type: String::new(),
            raw_text: raw_text.into(),
            polished_text: raw_text.into(),
            language: Some("zh".into()),
            duration_ms,
        }
    }

    #[tokio::test]
    async fn rewards_are_persistent_and_history_deletion_does_not_erase_or_reaward() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("history.db");
        let store = HistoryStore::new(path.clone()).unwrap();
        let entry = reward_entry(Some(2000), "今天完成一段语音输入。");
        let id = store
            .insert_history_at(entry.clone(), "2026-09-18", "2026-09-18T10:00:00+08:00")
            .unwrap();
        {
            let connection = store.conn.lock().unwrap();
            record_recognition_reward(
                &connection,
                id,
                &entry,
                "2026-09-18",
                "2026-09-18T10:01:00+08:00",
            )
            .unwrap();
        }
        assert_eq!(
            store
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            1
        );
        store.remove(id).await.unwrap();
        store
            .update_polished(id, "再次编辑不会获得积分")
            .await
            .unwrap();
        {
            let connection = store.conn.lock().unwrap();
            record_recognition_reward(
                &connection,
                id,
                &entry,
                "2026-09-19",
                "2026-09-19T10:00:00+08:00",
            )
            .unwrap();
        }
        store.clear().await.unwrap();
        drop(store);
        let reopened = HistoryStore::new(path).unwrap();
        let summary = reopened.rewards_summary_for_day("2026-09-19").unwrap();
        assert_eq!(summary.total_points, 1);
        assert_eq!(summary.today_points, 0);
        assert_eq!(summary.recent_activity[0].history_id, id);
        assert!(!summary.redemption_available);
    }

    #[test]
    fn rewards_require_two_seconds_and_effective_speech_text() {
        let temporary = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temporary.path().join("history.db")).unwrap();
        for entry in [
            reward_entry(None, "无时长"),
            reward_entry(Some(-1), "负时长"),
            reward_entry(Some(1999), "不到两秒"),
            reward_entry(Some(2000), "  \n\t  "),
            reward_entry(Some(2000), "。？！…"),
            reward_entry(Some(2000), "🎤"),
        ] {
            store
                .insert_history_at(entry, "2026-09-18", "2026-09-18T10:00:00+08:00")
                .unwrap();
        }
        assert_eq!(
            store
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            0
        );
        for text in ["刚好两秒", "English speech", "1234"] {
            store
                .insert_history_at(
                    reward_entry(Some(2000), text),
                    "2026-09-18",
                    "2026-09-18T10:00:00+08:00",
                )
                .unwrap();
        }
        assert_eq!(
            store
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            3
        );
    }

    #[test]
    fn rewards_daily_cap_and_new_day_are_enforced_transactionally() {
        let temporary = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temporary.path().join("history.db")).unwrap();
        for index in 0..103 {
            let mut entry = reward_entry(Some(3000), "有效识别");
            entry.created_at = format!(
                "2026-09-18T10:{:02}:{:02}+08:00",
                index / 20,
                index % 20 * 3
            );
            store
                .insert_history_at(entry, "2026-09-18", "2026-09-18T10:00:00+08:00")
                .unwrap();
        }
        let first_day = store.rewards_summary_for_day("2026-09-18").unwrap();
        assert_eq!(first_day.today_points, 100);
        assert_eq!(first_day.total_points, 100);
        assert_eq!(first_day.recent_activity.len(), 10);
        store
            .insert_history_at(
                reward_entry(Some(3000), "次日有效识别"),
                "2026-09-19",
                "2026-09-19T10:00:00+08:00",
            )
            .unwrap();
        let second_day = store.rewards_summary_for_day("2026-09-19").unwrap();
        assert_eq!(second_day.today_points, 1);
        assert_eq!(second_day.total_points, 101);
    }

    #[tokio::test]
    async fn rewards_never_backfill_existing_history_or_award_edits() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("history.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE history (
                id INTEGER PRIMARY KEY AUTOINCREMENT, created_at TEXT NOT NULL,
                app_name TEXT NOT NULL DEFAULT '', app_type TEXT NOT NULL DEFAULT '',
                raw_text TEXT NOT NULL DEFAULT '', polished_text TEXT NOT NULL DEFAULT '',
                language TEXT, duration_ms INTEGER
             );
             INSERT INTO history (created_at, raw_text, duration_ms)
             VALUES ('2026-09-17T10:00:00+08:00', '旧版本历史', 6000);",
            )
            .unwrap();
        drop(connection);
        let store = HistoryStore::new(path.clone()).unwrap();
        store.update_polished(1, "修订旧历史").await.unwrap();
        {
            let connection = store.conn.lock().unwrap();
            record_recognition_reward(
                &connection,
                1,
                &reward_entry(Some(6000), "旧版本历史"),
                "2026-09-18",
                "2026-09-18T10:00:00+08:00",
            )
            .unwrap();
        }
        assert_eq!(
            store
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            0
        );
        drop(store);
        let reopened = HistoryStore::new(path).unwrap();
        assert_eq!(
            reopened
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            0
        );
        reopened
            .insert_history_at(
                reward_entry(Some(2000), "新识别"),
                "2026-09-18",
                "2026-09-18T10:00:00+08:00",
            )
            .unwrap();
        assert_eq!(
            reopened
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            1
        );
    }

    #[tokio::test]
    async fn rewards_failure_rolls_back_history_insert() {
        let temporary = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(temporary.path().join("history.db")).unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER simulated_reward_failure BEFORE INSERT ON reward_ledger
             BEGIN SELECT RAISE(ABORT, 'simulated ledger failure'); END;",
            )
            .unwrap();
        assert!(store
            .insert_history_at(
                reward_entry(Some(2000), "不能只写入一半"),
                "2026-09-18",
                "2026-09-18T10:00:00+08:00"
            )
            .is_err());
        assert!(store.list(10, 0).await.unwrap().is_empty());
        assert_eq!(
            store
                .rewards_summary_for_day("2026-09-18")
                .unwrap()
                .total_points,
            0
        );
    }

    #[tokio::test]
    async fn rewards_recovery_replay_cannot_award_twice_even_after_history_deletion() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("history.db");
        let store = HistoryStore::new(path.clone()).unwrap();
        let entry = reward_entry(Some(2000), "恢复日志中的成功识别");
        let first_id = store
            .insert_history_at(entry.clone(), "2026-09-18", "2026-09-18T10:00:00+08:00")
            .unwrap();
        store.remove(first_id).await.unwrap();
        drop(store);
        let reopened = HistoryStore::new(path).unwrap();
        let replay_id = reopened
            .insert_history_at(entry.clone(), "2026-09-19", "2026-09-19T10:00:00+08:00")
            .unwrap();
        assert!(replay_id > first_id);
        let replayed = reopened.rewards_summary_for_day("2026-09-19").unwrap();
        assert_eq!(replayed.total_points, 1);
        assert_eq!(replayed.today_points, 0);
        // Saying the same sentence in a distinct later recording is legitimate.
        let mut new_recording = entry;
        new_recording.created_at = "2026-09-19T10:03:00+08:00".into();
        reopened
            .insert_history_at(new_recording, "2026-09-19", "2026-09-19T10:03:00+08:00")
            .unwrap();
        assert_eq!(
            reopened
                .rewards_summary_for_day("2026-09-19")
                .unwrap()
                .total_points,
            2
        );
    }

    #[test]
    fn live_preview_defaults_on_without_overriding_saved_capsule_preferences() {
        let defaults = AppConfig::default();
        assert!(defaults.capsule_enabled);
        assert!(!defaults.capsule_auto_hide);
        assert!(defaults.capsule_preview_enabled);
        let prior = config_from_store_value(&serde_json::json!({
            "capsule_enabled": false,
            "capsule_auto_hide": true
        }));
        assert!(!prior.capsule_enabled);
        assert!(prior.capsule_auto_hide);
        assert!(prior.capsule_preview_enabled);
        let opted_out = config_from_store_value(&serde_json::json!({
            "capsule_enabled": true,
            "capsule_preview_enabled": false
        }));
        assert!(!opted_out.capsule_preview_enabled);
    }

    #[test]
    fn capsule_preference_patch_never_changes_recognition_or_secrets() {
        let mut config = AppConfig::default();
        config.stt_provider = "funasr-nano".into();
        config.stt_api_key = "test-api-secret".into();
        config.volcengine_credential = "test-dedicated-token".into();
        config.hotkey = "Alt+/".into();
        let patch: CapsulePreferencesPatch = serde_json::from_value(serde_json::json!({
            "capsule_enabled": false,
            "capsule_preview_enabled": false,
        }))
        .unwrap();
        patch.apply(&mut config);
        assert!(!config.capsule_enabled);
        assert!(!config.capsule_preview_enabled);
        assert!(config.capsule_always_on_top);
        assert_eq!(config.stt_provider, "funasr-nano");
        assert_eq!(config.stt_api_key, "test-api-secret");
        assert_eq!(config.volcengine_credential, "test-dedicated-token");
        assert_eq!(config.hotkey, "Alt+/");
        assert!(
            serde_json::from_value::<CapsulePreferencesPatch>(serde_json::json!({
                "stt_provider": "sensevoice"
            }))
            .is_err()
        );
    }

    #[test]
    fn custom_vendor_credentials_survive_disk_roundtrip_and_mode_change() {
        let mut config = AppConfig::default();
        config.custom_cloud.vendor = "tencent".into();
        config.custom_cloud.app_id = "test-app".into();
        config.custom_cloud.api_key = "test-key".into();
        config.custom_cloud.api_secret = "test-secret".into();
        let disk = serde_json::to_value(config_for_disk(&config)).unwrap();
        let restored = config_from_store_value(&disk);
        assert_eq!(restored.custom_cloud, config.custom_cloud);
        assert_eq!(restored.stt_provider, "sensevoice");
    }

    #[test]
    fn legacy_custom_transcription_migration_never_overwrites_vendor_credentials() {
        let mut config = AppConfig {
            stt_provider: "custom-whisper".into(),
            stt_base_url: "https://example.invalid/v1/audio/transcriptions".into(),
            stt_model: "example".into(),
            stt_api_key: "legacy-test-key".into(),
            ..AppConfig::default()
        };
        config.migrate_custom_cloud();
        assert_eq!(config.custom_cloud.api_key, "legacy-test-key");
        config.custom_cloud.vendor = "tencent".into();
        config.custom_cloud.api_key = "different-test-key".into();
        config.migrate_custom_cloud();
        assert_eq!(config.custom_cloud.api_key, "different-test-key");
    }

    #[test]
    fn removed_menu_presets_normalize_to_local_choices_without_deleting_credentials() {
        let mut config = AppConfig {
            stt_provider: "xiaomi-mimo".into(),
            llm_provider: "deepseek".into(),
            llm_api_key: "test-secret".into(),
            ..AppConfig::default()
        };
        config.normalize_managed_local_resources();
        assert_eq!(config.stt_provider, "sensevoice");
        assert_eq!(config.llm_provider, "local-llama");
        assert_eq!(config.llm_api_key, "test-secret");
    }

    #[test]
    fn saved_native_asr_selection_migrates_to_safe_offline_default() {
        let mut config = AppConfig {
            stt_provider: "native-asr".into(),
            ..AppConfig::default()
        };
        config.normalize_managed_local_resources();
        assert_eq!(config.stt_provider, "sensevoice");
    }

    #[test]
    fn volcengine_seedasr_credential_persists_independently() {
        assert!(stt_provider_uses_dedicated_credential("volcengine-seedasr"));
        assert!(!stt_provider_uses_dedicated_credential("custom-whisper"));
        assert!(!stt_provider_uses_dedicated_credential("sensevoice"));

        let seedasr = AppConfig {
            stt_provider: "sensevoice".to_string(),
            volcengine_credential: "persistent-access-token".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(
            config_for_disk(&seedasr).volcengine_credential,
            "persistent-access-token"
        );

        let regular_cloud = AppConfig {
            stt_provider: "custom-whisper".to_string(),
            stt_api_key: "credential-manager-secret".to_string(),
            ..AppConfig::default()
        };
        #[cfg(windows)]
        assert!(config_for_disk(&regular_cloud).stt_api_key.is_empty());
    }

    #[test]
    fn managed_local_llm_discards_stale_external_configuration() {
        let mut config = AppConfig {
            llm_provider: "local-llama".to_string(),
            local_llm_model_dir: r"D:\old\source-tree\resources\models".to_string(),
            local_llm_model: "missing.gguf".to_string(),
            llm_model: "wrong-model".to_string(),
            llm_base_url: "http://stale.invalid/v1".to_string(),
            ..AppConfig::default()
        };

        config.normalize_managed_local_resources();

        assert!(config.local_llm_model_dir.is_empty());
        assert_eq!(config.local_llm_model, default_local_llm_model());
        assert_eq!(config.llm_model, "qwen2.5-0.5b-instruct");
        assert_eq!(
            config.llm_base_url,
            format!("http://127.0.0.1:{}/v1", config.local_llm_port)
        );
    }

    #[test]
    fn sensevoice_requires_explicit_custom_directory_mode() {
        let mut legacy = AppConfig {
            sensevoice_model_dir: r"D:\old\developer\model".to_string(),
            sensevoice_use_custom_dir: false,
            ..AppConfig::default()
        };
        legacy.normalize_managed_local_resources();
        assert!(legacy.sensevoice_model_dir.is_empty());

        let mut explicit = AppConfig {
            sensevoice_model_dir: r"E:\SpeechModels\sensevoice".to_string(),
            sensevoice_use_custom_dir: true,
            ..AppConfig::default()
        };
        explicit.normalize_managed_local_resources();
        assert_eq!(explicit.sensevoice_model_dir, r"E:\SpeechModels\sensevoice");
    }

    #[test]
    fn funasr_directory_mode_and_thread_count_are_normalized() {
        let mut default_mode = AppConfig {
            funasr_model_dir: r"D:\old\developer\funasr".to_string(),
            funasr_use_custom_dir: false,
            funasr_num_threads: 99,
            ..AppConfig::default()
        };
        default_mode.normalize_managed_local_resources();
        assert!(default_mode.funasr_model_dir.is_empty());
        assert_eq!(default_mode.funasr_num_threads, 16);

        let mut custom_mode = AppConfig {
            funasr_model_dir: r"E:\SpeechModels\funasr".to_string(),
            funasr_use_custom_dir: true,
            funasr_num_threads: 0,
            ..AppConfig::default()
        };
        custom_mode.normalize_managed_local_resources();
        assert_eq!(custom_mode.funasr_model_dir, r"E:\SpeechModels\funasr");
        assert_eq!(custom_mode.funasr_num_threads, 1);
    }

    #[test]
    fn removed_cloud_stt_presets_migrate_to_sensevoice() {
        for provider in [
            "deepgram",
            "assemblyai",
            "glm-asr",
            "openai-whisper",
            "groq-whisper",
            "siliconflow",
        ] {
            let mut config = AppConfig {
                stt_provider: provider.to_string(),
                ..AppConfig::default()
            };
            config.normalize_managed_local_resources();
            assert_eq!(config.stt_provider, "sensevoice");
        }
    }

    #[test]
    fn seedasr_cloud_preset_is_preserved() {
        let mut config = AppConfig {
            stt_provider: "volcengine-seedasr".to_string(),
            stt_model: "volc.seedasr.sauc.duration".to_string(),
            ..AppConfig::default()
        };
        config.normalize_managed_local_resources();
        assert_eq!(config.stt_provider, "volcengine-seedasr");
        assert_eq!(config.stt_model, "volc.seedasr.sauc.duration");
    }

    #[test]
    fn legacy_capsule_config_is_migrated_to_visible_defaults_once() {
        let value = serde_json::json!({
            "capsule_auto_hide": true,
            "stt_provider": "sensevoice"
        });
        let migrated = config_from_store_value(&value);
        assert!(migrated.capsule_enabled);
        assert!(migrated.capsule_always_on_top);
        assert!(!migrated.capsule_auto_hide);

        let current = serde_json::json!({
            "capsule_enabled": false,
            "capsule_always_on_top": false,
            "capsule_auto_hide": true
        });
        let preserved = config_from_store_value(&current);
        assert!(!preserved.capsule_enabled);
        assert!(!preserved.capsule_always_on_top);
        assert!(preserved.capsule_auto_hide);
    }

    #[test]
    fn editor_overlay_settings_default_and_clamp() {
        let legacy = config_from_store_value(&serde_json::json!({
            "stt_provider": "sensevoice"
        }));
        assert!(legacy.editor_auto_hide_enabled);
        assert_eq!(legacy.editor_auto_hide_seconds, 5);

        let mut too_short = AppConfig {
            editor_auto_hide_seconds: 0,
            ..AppConfig::default()
        };
        too_short.normalize_managed_local_resources();
        assert_eq!(too_short.editor_auto_hide_seconds, 3);

        let mut too_long = AppConfig {
            editor_auto_hide_seconds: 120,
            ..AppConfig::default()
        };
        too_long.normalize_managed_local_resources();
        assert_eq!(too_long.editor_auto_hide_seconds, 60);
    }

    #[tokio::test]
    async fn dictionary_correction_can_be_edited_and_reused() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let store = DictionaryStore::new(temporary.path().join("dictionary.db"))
            .expect("create dictionary");
        store
            .add("展业", Some("zhan ye"), Some("专业"))
            .await
            .expect("add correction");
        let entry = store.list().await.expect("list dictionary").remove(0);

        store
            .update(entry.id, "展业词汇", None, Some("专业词汇"))
            .await
            .expect("edit correction");

        let updated = store.list().await.expect("list edited dictionary");
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].word, "展业词汇");
        assert_eq!(updated[0].correction_from.as_deref(), Some("专业词汇"));
        assert_eq!(updated[0].pronunciation, None);
    }

    #[tokio::test]
    async fn history_delete_is_scoped_persistent_and_idempotent() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("history.db");
        let store = HistoryStore::new(path.clone()).expect("create history");
        let mut ids = Vec::new();
        for text in ["保留第一条", "删除这一条", "保留第三条"] {
            ids.push(
                store
                    .add(HistoryEntry {
                        id: 0,
                        created_at: "2026-09-05T12:00:00".into(),
                        app_name: "test".into(),
                        app_type: String::new(),
                        raw_text: text.into(),
                        polished_text: text.into(),
                        language: Some("zh".into()),
                        duration_ms: Some(1000),
                    })
                    .await
                    .expect("insert history"),
            );
        }
        store.remove(ids[1]).await.expect("delete selected row");
        store.remove(ids[1]).await.expect("repeat delete is safe");
        store.remove(-1).await.expect("missing id is safe");
        store
            .update_polished(ids[1], "must not resurrect")
            .await
            .expect("stale editor save");
        drop(store);
        let reopened = HistoryStore::new(path).expect("reopen history");
        let remaining = reopened.list(200, 0).await.expect("read history");
        assert_eq!(
            remaining.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![ids[2], ids[0]]
        );
        assert_eq!(remaining[0].polished_text, "保留第三条");
        assert_eq!(remaining[1].raw_text, "保留第一条");
    }

    #[test]
    fn failed_config_persistence_never_changes_active_engine() {
        let original = AppConfig::default();
        let cache = Mutex::new(Some(original.clone()));
        let mut precision = original.clone();
        precision.stt_provider = "funasr-nano".into();
        assert!(persist_then_cache(&cache, precision.clone(), false, |_| {
            Err(anyhow::anyhow!("simulated disk failure"))
        })
        .is_err());
        assert_eq!(
            cache.lock().unwrap().as_ref().unwrap().stt_provider,
            original.stt_provider
        );
        persist_then_cache(&cache, precision, false, |_| Ok(())).unwrap();
        assert_eq!(
            cache.lock().unwrap().as_ref().unwrap().stt_provider,
            "funasr-nano"
        );
    }

    #[test]
    fn main_settings_preserves_latest_immediate_capsule_fields_atomically() {
        let mut latest = AppConfig::default();
        latest.capsule_enabled = false;
        latest.capsule_always_on_top = false;
        latest.capsule_auto_hide = true;
        latest.capsule_preview_enabled = false;
        let cache = Mutex::new(Some(latest));
        let mut draft = AppConfig::default();
        draft.hotkey = "Alt+/".into();
        persist_then_cache(&cache, draft, true, |persisted| {
            assert!(!persisted.capsule_enabled);
            assert!(!persisted.capsule_always_on_top);
            assert!(persisted.capsule_auto_hide);
            assert!(!persisted.capsule_preview_enabled);
            assert_eq!(persisted.hotkey, "Alt+/");
            assert!(
                cache.try_lock().is_err(),
                "merge and persistence share one lock"
            );
            Ok(())
        })
        .unwrap();
        let confirmed = cache.lock().unwrap().clone().unwrap();
        assert!(!confirmed.capsule_enabled);
        assert_eq!(confirmed.hotkey, "Alt+/");

        // Startup/other explicit saves must still be able to set these fields.
        persist_then_cache(&cache, AppConfig::default(), false, |persisted| {
            assert!(persisted.capsule_enabled);
            assert!(persisted.capsule_always_on_top);
            assert!(!persisted.capsule_auto_hide);
            assert!(persisted.capsule_preview_enabled);
            Ok(())
        })
        .unwrap();
        assert!(cache.lock().unwrap().as_ref().unwrap().capsule_enabled);
    }

    #[test]
    fn sqlite_backup_preserves_wal_data() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("legacy.db");
        let destination = temporary.path().join("popspeak.db");
        let connection = Connection::open(&source).expect("open source database");
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE history (text TEXT NOT NULL);
                 INSERT INTO history VALUES ('保留我的历史');",
            )
            .expect("seed source database");

        backup_database(&source, &destination).expect("backup legacy database");
        let migrated = Connection::open(destination).expect("open migrated database");
        let text: String = migrated
            .query_row("SELECT text FROM history", [], |row| row.get(0))
            .expect("read migrated row");
        assert_eq!(text, "保留我的历史");
    }

    #[test]
    fn directory_migration_never_overwrites_current_files() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let source = temporary.path().join("legacy");
        let destination = temporary.path().join("current");
        std::fs::create_dir_all(source.join("nested")).expect("create source");
        std::fs::create_dir_all(destination.join("nested")).expect("create destination");
        std::fs::write(source.join("nested/model.bin"), b"legacy").expect("write legacy");
        std::fs::write(destination.join("nested/model.bin"), b"current").expect("write current");
        std::fs::write(source.join("new.bin"), b"copied").expect("write new source");

        copy_directory_missing(&source, &destination).expect("copy missing files");
        assert_eq!(
            std::fs::read(destination.join("nested/model.bin")).expect("read current"),
            b"current"
        );
        assert_eq!(
            std::fs::read(destination.join("new.bin")).expect("read copied"),
            b"copied"
        );
    }
}
