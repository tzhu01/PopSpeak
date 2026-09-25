//! Fail early when a saved mode needs an optional native executable absent from
//! this installation. Model downloads cannot supply a missing runtime.

use std::path::Path;

use tauri::AppHandle;

use crate::{llm, storage::AppConfig, stt};

#[derive(Debug, Default)]
struct RuntimeReadiness {
    whisper: bool,
    funasr: bool,
    local_llm: bool,
}

fn unavailable_reason(config: &AppConfig, ready: &RuntimeReadiness) -> Option<&'static str> {
    match config.stt_provider.as_str() {
        "native-asr" if !stt::native_asr_manager::PUBLIC_CATALOG_ENABLED => {
            return Some(stt::native_asr_manager::PUBLIC_CATALOG_NOTICE);
        }
        "local-whisper" if !ready.whisper => {
            return Some("本安装包未提供 Whisper 离线运行组件；仅下载模型无法使用。请选择 SenseVoice 或安装包含该组件的版本。");
        }
        "funasr-nano" if !ready.funasr => {
            return Some("本安装包未提供 Fun-ASR 离线运行组件；仅下载模型无法使用。请选择 SenseVoice 或安装包含该组件的版本。");
        }
        _ => {}
    }
    if config.polish_enabled && config.llm_provider == "local-llama" && !ready.local_llm {
        return Some("本安装包未提供本地 AI 润色运行组件；仅下载模型无法使用。请关闭 AI 润色、选择其他服务，或安装包含该组件的版本。");
    }
    None
}

/// Check only the executables required by the active configuration. A full
/// installation continues to work, including a user-supplied Whisper CLI.
pub(crate) fn validate(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let whisper = config.stt_provider == "local-whisper"
        && (Path::new(config.whisper_cli_path.trim()).is_file()
            || stt::model_manager::cli_path(app).is_ok_and(|path| path.is_file()));
    let funasr = config.stt_provider == "funasr-nano"
        && stt::funasr_manager::paths(
            app,
            (config.funasr_use_custom_dir && !config.funasr_model_dir.trim().is_empty())
                .then_some(config.funasr_model_dir.as_str()),
        )
        .is_ok_and(|paths| paths.runtime_ready);
    let local_llm = config.polish_enabled
        && config.llm_provider == "local-llama"
        && llm::local_server::server_path(app).is_ok_and(|path| path.is_file());
    let ready = RuntimeReadiness {
        whisper,
        funasr,
        local_llm,
    };
    unavailable_reason(config, &ready).map_or(Ok(()), |reason| Err(reason.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_selected_local_recognizer_requires_its_runtime() {
        let mut config = AppConfig::default();
        let absent = RuntimeReadiness::default();
        assert!(unavailable_reason(&config, &absent).is_none());

        config.stt_provider = "local-whisper".into();
        assert!(unavailable_reason(&config, &absent)
            .unwrap()
            .contains("Whisper"));
        let whisper_ready = RuntimeReadiness {
            whisper: true,
            ..Default::default()
        };
        assert!(unavailable_reason(&config, &whisper_ready).is_none());

        config.stt_provider = "funasr-nano".into();
        assert!(unavailable_reason(&config, &absent)
            .unwrap()
            .contains("Fun-ASR"));
        let funasr_ready = RuntimeReadiness {
            funasr: true,
            ..Default::default()
        };
        assert!(unavailable_reason(&config, &funasr_ready).is_none());

        config.stt_provider = "native-asr".into();
        assert!(unavailable_reason(&config, &absent)
            .unwrap()
            .contains("许可与下载源审核"));
    }

    #[test]
    fn local_llm_is_required_only_when_polishing_is_enabled() {
        let mut config = AppConfig::default();
        let absent = RuntimeReadiness::default();
        assert!(unavailable_reason(&config, &absent).is_none());

        config.polish_enabled = true;
        assert!(unavailable_reason(&config, &absent)
            .unwrap()
            .contains("本地 AI 润色"));
        let local_llm_ready = RuntimeReadiness {
            local_llm: true,
            ..Default::default()
        };
        assert!(unavailable_reason(&config, &local_llm_ready).is_none());

        config.llm_provider = "ollama".into();
        assert!(unavailable_reason(&config, &absent).is_none());
    }

    #[test]
    fn selected_stt_error_takes_priority_over_local_llm_error() {
        let config = AppConfig {
            stt_provider: "local-whisper".into(),
            polish_enabled: true,
            llm_provider: "local-llama".into(),
            ..AppConfig::default()
        };
        let absent = RuntimeReadiness::default();
        assert!(unavailable_reason(&config, &absent)
            .unwrap()
            .contains("Whisper"));
    }
}
