use serde::{Deserialize, Serialize};

use crate::llm::AppType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppContext {
    pub app_name: String,
    pub window_title: String,
    pub app_type: AppType,
}

impl Default for AppContext {
    fn default() -> Self {
        Self {
            app_name: String::new(),
            window_title: String::new(),
            app_type: AppType::General,
        }
    }
}

pub fn detect_current_app() -> AppContext {
    #[cfg(target_os = "windows")]
    {
        windows_detect()
    }
    #[cfg(target_os = "macos")]
    {
        macos_detect()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        AppContext::default()
    }
}

#[cfg(target_os = "macos")]
fn macos_detect() -> AppContext {
    use std::process::Command;

    let app_name = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let window_title = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events"
            set frontApp to first application process whose frontmost is true
            try
                get name of front window of frontApp
            on error
                return ""
            end try
        end tell"#,
        ])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let app_type = classify_app_context(&app_name, &window_title);
    AppContext {
        app_name,
        window_title,
        app_type,
    }
}

#[cfg(target_os = "windows")]
fn windows_detect() -> AppContext {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    unsafe {
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if hwnd.is_null() {
            return AppContext::default();
        }

        // Get window title
        let mut title_buf = [0u16; 512];
        let len = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
            hwnd,
            title_buf.as_mut_ptr(),
            title_buf.len() as i32,
        );
        let window_title = if len > 0 {
            OsString::from_wide(&title_buf[..len as usize])
                .to_string_lossy()
                .to_string()
        } else {
            String::new()
        };

        // Get process name
        let mut pid = 0u32;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);

        let app_name = get_process_name(pid).unwrap_or_default();
        let app_type = classify_app_context(&app_name, &window_title);

        AppContext {
            app_name,
            window_title,
            app_type,
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_process_name(pid: u32) -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    if pid == 0 {
        return None;
    }

    let handle = windows_sys::Win32::System::Threading::OpenProcess(
        windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
        0,
        pid,
    );
    if handle.is_null() {
        return None;
    }

    let mut buf = [0u16; 260];
    let mut size = buf.len() as u32;
    let ok = windows_sys::Win32::System::Threading::QueryFullProcessImageNameW(
        handle,
        0,
        buf.as_mut_ptr(),
        &mut size,
    );
    let _ = windows_sys::Win32::Foundation::CloseHandle(handle);

    if ok != 0 && size > 0 {
        let path = OsString::from_wide(&buf[..size as usize])
            .to_string_lossy()
            .to_string();
        path.rsplit('\\').next().map(|s| s.to_string())
    } else {
        None
    }
}

const EMAIL_APPS: &[&str] = &[
    "outlook",
    "gmail",
    "thunderbird",
    "mail",
    "qqmail",
    "163mail",
    "126mail",
    "foxmail",
];
const CHAT_APPS: &[&str] = &[
    "slack", "discord", "wechat", "telegram", "teams", "微信", "feishu", "飞书", "lark",
    "dingtalk", "钉钉", "qq", "tim", "line", "whatsapp",
];
const CODE_APPS: &[&str] = &[
    "code",
    "intellij",
    "vim",
    "nvim",
    "cursor",
    "vscode",
    "visual studio code",
    "xcode",
    "pycharm",
    "goland",
    "webstorm",
];
const DOCUMENT_APPS: &[&str] = &[
    "word",
    "docs",
    "notion",
    "obsidian",
    "typora",
    "wps",
    "wps office",
    "pages",
    "yuque",
    "语雀",
    "飞书文档",
    "feishudoc",
    "bear",
    "ulysses",
];

#[allow(dead_code)]
fn classify_app(app_name: &str) -> AppType {
    let name = app_name.to_lowercase();
    if EMAIL_APPS.iter().any(|k| name.contains(k)) {
        AppType::Email
    } else if CHAT_APPS.iter().any(|k| name.contains(k)) {
        AppType::Chat
    } else if CODE_APPS.iter().any(|k| name.contains(k)) {
        AppType::Code
    } else if DOCUMENT_APPS.iter().any(|k| name.contains(k)) {
        AppType::Document
    } else {
        AppType::General
    }
}

fn classify_app_context(app_name: &str, window_title: &str) -> AppType {
    let by_process = classify_app(app_name);
    if by_process != AppType::General {
        return by_process;
    }

    // Browser processes are generic, but their local window title often carries
    // enough product context (Gmail, Feishu, Notion, etc.). Only classification
    // is retained; the title itself never leaves the machine or enters History.
    classify_app(window_title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_title_supplies_local_context() {
        assert_eq!(
            classify_app_context("msedge.exe", "Inbox - Gmail - Microsoft Edge"),
            AppType::Email
        );
        assert_eq!(
            classify_app_context("chrome.exe", "Release notes - Notion"),
            AppType::Document
        );
    }

    #[test]
    fn native_process_classification_wins() {
        assert_eq!(
            classify_app_context("Code.exe", "Inbox - Gmail"),
            AppType::Code
        );
    }
}
