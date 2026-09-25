//! Offline OCR through the Windows language packs already installed on the PC.
//! No image bytes or recognized text are sent to a service.

use std::path::Path;

use windows::{
    core::HSTRING,
    Graphics::Imaging::{BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat},
    Media::Ocr::OcrEngine,
    Storage::{FileAccessMode, StorageFile},
};

const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;

fn validate_image(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("请选择本机图片文件。".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff"
    ) {
        return Err("暂仅支持 PNG、JPEG、BMP 和 TIFF 图片。".into());
    }
    let metadata = std::fs::metadata(path).map_err(|_| "无法读取所选图片。".to_string())?;
    if !metadata.is_file() {
        return Err("请选择图片文件，而不是文件夹。".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片必须大于 0 且不超过 40 MB。".into());
    }
    Ok(())
}

pub fn recognize_path(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    validate_image(path)?;
    // WinRT StorageFile rejects paths containing `..` even when the path is
    // absolute and readable through std::fs.
    let resolved = path
        .canonicalize()
        .map_err(|_| "无法读取所选图片。".to_string())?;

    let languages = OcrEngine::AvailableRecognizerLanguages()
        .map_err(|error| format!("无法读取 Windows OCR 语言包：{error}"))?;
    let count = languages
        .Size()
        .map_err(|error| format!("无法读取 Windows OCR 语言包：{error}"))?;
    if count == 0 {
        return Err("Windows 尚未安装可用的 OCR 语言包，请安装对应语言后重试。".into());
    }

    let absolute = resolved
        .to_str()
        .ok_or_else(|| "图片路径包含 Windows 无法读取的字符。".to_string())?;
    let winrt_path = if let Some(unc) = absolute.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        absolute
            .strip_prefix(r"\\?\")
            .unwrap_or(absolute)
            .to_string()
    };
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(winrt_path.as_str()))
        .and_then(|operation| operation.get())
        .map_err(|error| format!("无法打开图片：{error}"))?;
    let stream = file
        .OpenAsync(FileAccessMode::Read)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("无法读取图片：{error}"))?;
    let decoder = BitmapDecoder::CreateAsync(&stream)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("无法解码图片：{error}"))?;
    let width = decoder
        .PixelWidth()
        .map_err(|error| format!("无法读取图片宽度：{error}"))?;
    let height = decoder
        .PixelHeight()
        .map_err(|error| format!("无法读取图片高度：{error}"))?;
    let max_dimension = OcrEngine::MaxImageDimension()
        .map_err(|error| format!("无法读取 Windows OCR 尺寸限制：{error}"))?;
    // Reject before materializing decoded pixels. A small compressed image can
    // otherwise expand into an unexpectedly large bitmap in memory.
    if width == 0 || height == 0 || width > max_dimension || height > max_dimension {
        return Err(format!(
            "图片尺寸超过 Windows OCR 限制（最长边 {max_dimension} 像素），请缩小后重试。"
        ));
    }
    let bitmap = decoder
        .GetSoftwareBitmapConvertedAsync(BitmapPixelFormat::Bgra8, BitmapAlphaMode::Premultiplied)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("无法处理图片：{error}"))?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .or_else(|_| {
            let first = languages.GetAt(0)?;
            OcrEngine::TryCreateFromLanguage(&first)
        })
        .map_err(|error| format!("无法初始化 Windows OCR：{error}"))?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .and_then(|operation| operation.get())
        .map_err(|error| format!("文字识别失败：{error}"))?;
    let text = result
        .Text()
        .map_err(|error| format!("无法读取识别结果：{error}"))?
        .to_string();
    let text = text.trim();
    if text.is_empty() {
        return Err("没有在图片中识别到文字。".into());
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::{recognize_path, validate_image};
    use std::path::Path;

    #[test]
    fn rejects_relative_paths() {
        assert!(validate_image(Path::new("image.png")).is_err());
    }

    #[test]
    fn rejects_non_images_before_reading() {
        assert!(validate_image(Path::new("C:\\temp\\notes.txt")).is_err());
    }

    #[test]
    #[ignore = "requires an installed Windows OCR language pack"]
    fn recognizes_the_included_preview_image() {
        let image =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/assets/popspeak-preview.png");
        let result = recognize_path(image.to_str().expect("fixture path should be Unicode"))
            .expect("Windows OCR should read the screenshot");
        assert!(!result.is_empty());
    }
}
