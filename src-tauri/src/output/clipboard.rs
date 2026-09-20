use anyhow::Result;
use async_trait::async_trait;
use std::borrow::Cow;

use super::{OutputMode, TextOutput};

/// Delay after writing to clipboard before simulating paste.
const CLIPBOARD_SETTLE_MS: u64 = 20;
const CLIPBOARD_RESTORE_MS: u64 = 450;

/// Copy without pasting. Used when focus changed while ASR was running so text
/// is recoverable without being injected into the wrong application.
pub fn copy_only(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| anyhow::anyhow!("Failed to access clipboard: {error}"))?;
    clipboard
        .set_text(text)
        .map_err(|error| anyhow::anyhow!("Failed to set clipboard: {error}"))
}

pub struct ClipboardOutput;

pub(crate) enum ClipboardBackup {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
}

pub(crate) fn snapshot(clipboard: &mut arboard::Clipboard) -> Option<ClipboardBackup> {
    clipboard
        .get_text()
        .ok()
        .map(ClipboardBackup::Text)
        .or_else(|| {
            clipboard
                .get_image()
                .ok()
                .map(|image| ClipboardBackup::Image {
                    width: image.width,
                    height: image.height,
                    bytes: image.bytes.into_owned(),
                })
        })
}

pub(crate) fn restore(clipboard: &mut arboard::Clipboard, backup: ClipboardBackup) {
    match backup {
        ClipboardBackup::Text(previous_text) => {
            let _ = clipboard.set_text(previous_text);
        }
        ClipboardBackup::Image {
            width,
            height,
            bytes,
        } => {
            let _ = clipboard.set_image(arboard::ImageData {
                width,
                height,
                bytes: Cow::Owned(bytes),
            });
        }
    }
}

impl Default for ClipboardOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardOutput {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TextOutput for ClipboardOutput {
    async fn type_text(&self, text: &str) -> Result<()> {
        let text = text.to_string();
        tokio::task::spawn_blocking(move || {
            let mut clipboard = arboard::Clipboard::new()
                .map_err(|e| anyhow::anyhow!("Failed to access clipboard: {}", e))?;

            let previous = snapshot(&mut clipboard);

            clipboard
                .set_text(&text)
                .map_err(|e| anyhow::anyhow!("Failed to set clipboard: {}", e))?;

            std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_SETTLE_MS));

            // On macOS: try osascript Cmd+V (needs Accessibility permission).
            // If that fails, silently fall back — text is already on clipboard.
            #[cfg(target_os = "macos")]
            {
                let status = std::process::Command::new("osascript")
                    .args([
                        "-e",
                        r#"tell application "System Events" to keystroke "v" using command down"#,
                    ])
                    .status();
                match status {
                    Ok(s) if s.success() => {}
                    _ => anyhow::bail!("Paste simulation unavailable; text remains on clipboard"),
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                use enigo::{Direction, Enigo, Key, Keyboard, Settings};
                let mut enigo = Enigo::new(&Settings::default())
                    .map_err(|error| anyhow::anyhow!("Paste simulation unavailable: {error}"))?;
                enigo.key(Key::Control, Direction::Press)?;
                let paste_result = enigo.key(Key::Unicode('v'), Direction::Click);
                let release_result = enigo.key(Key::Control, Direction::Release);
                paste_result?;
                release_result?;
            }

            // Restore only when neither the target app nor the user changed the
            // clipboard after our paste. Fallback copies intentionally remain.
            std::thread::sleep(std::time::Duration::from_millis(CLIPBOARD_RESTORE_MS));
            if clipboard.get_text().ok().as_deref() == Some(text.as_str()) {
                if let Some(previous) = previous {
                    restore(&mut clipboard, previous);
                }
            }

            Ok(())
        })
        .await?
    }

    fn mode(&self) -> OutputMode {
        OutputMode::Clipboard
    }
}
