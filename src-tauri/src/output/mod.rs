pub mod clipboard;
pub mod keyboard;

use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputMode {
    Keyboard,
    Clipboard,
}

#[async_trait]
pub trait TextOutput: Send + Sync {
    async fn type_text(&self, text: &str) -> Result<()>;
    fn mode(&self) -> OutputMode;
}

/// Sanitize text before outputting it via keyboard or clipboard.
/// Removes control characters (except tab and newline) to prevent
/// accidental injection of escape sequences or other unwanted behavior.
pub fn sanitize_output(text: &str) -> String {
    text.chars()
        .filter(|c| {
            let cp = *c as u32;
            // Allow printable ASCII, tab, newline, and all non-ASCII (Unicode letters/symbols)
            // Block: null, bell, backspace, form feed, carriage return (we normalize later),
            // escape, delete, and C1 control codes.
            match cp {
                0x09 | 0x0A => true,     // tab, newline
                0x20..=0x7E => true,     // printable ASCII
                0xA0..=0x10FFFF => true, // Unicode beyond C1 controls (includes CJK)
                _ => false,              // control characters
            }
        })
        .collect()
}

pub fn create_output(mode: OutputMode) -> Box<dyn TextOutput> {
    match mode {
        OutputMode::Keyboard => Box::new(keyboard::KeyboardOutput::new()),
        OutputMode::Clipboard => Box::new(clipboard::ClipboardOutput::new()),
    }
}
