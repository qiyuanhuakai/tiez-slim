use crate::model::{ClipboardEntry, ClipboardKind};
use crate::platform;
use arboard::Clipboard;
use crossbeam_channel::Sender;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    Captured(ClipboardEntry),
    ToggleWindow,
    FocusSearch,
    PasteLatestRich,
    SequentialPaste,
    OpenSettings,
    Quit,
    Status(String),
    Error(String),
}

pub fn start_watcher(sender: Sender<ClipboardEvent>) {
    thread::Builder::new()
        .name("clipboard-watcher".to_string())
        .spawn(move || watch_loop(sender))
        .expect("spawn clipboard watcher");
}

pub fn set_text(content: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|err| format!("初始化剪贴板失败: {err}"))?;
    clipboard
        .set_text(content.to_string())
        .map_err(|err| format!("写入剪贴板失败: {err}"))
}

pub fn set_entry(entry: &ClipboardEntry, paste_with_format: bool) -> Result<(), String> {
    match entry.kind {
        ClipboardKind::Image if entry.content.starts_with("data:image/") => {
            set_image_from_data_url(&entry.content)
        }
        ClipboardKind::File | ClipboardKind::Video | ClipboardKind::Image
            if entry.is_external || !entry.content.starts_with("data:image/") =>
        {
            set_file_list(&entry.content)
        }
        ClipboardKind::RichText if paste_with_format => {
            if let Some(html) = entry.html_content.as_deref() {
                set_html(&entry.content, html)
            } else {
                set_text(&entry.content)
            }
        }
        _ => set_text(&entry.content),
    }
}

fn watch_loop(sender: Sender<ClipboardEvent>) {
    let mut last_seen = String::new();
    let mut last_image_fingerprint = String::new();
    let mut last_html_fingerprint = String::new();
    let mut last_file_fingerprint = String::new();
    // Keep a single arboard::Clipboard instance across iterations; constructing
    // one is expensive (X11/GTK init). Recreate only when every probe fails,
    // which signals the underlying connection is broken.
    let mut clipboard: Option<Clipboard> = None;
    loop {
        if clipboard.is_none() {
            match Clipboard::new() {
                Ok(c) => clipboard = Some(c),
                Err(err) => {
                    let _ = sender.try_send(ClipboardEvent::Error(err.to_string()));
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
            }
        }
        let mut handled = false;
        if let Some(clipboard) = clipboard.as_mut() {
            if let Ok(paths) = clipboard.get().file_list() {
                handled = true;
                let paths = paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                let fingerprint = string_fingerprint(&paths.join("\n"));
                if !paths.is_empty() && fingerprint != last_file_fingerprint {
                    let entry = ClipboardEntry::captured_files(paths, platform::active_app_name());
                    let sent_or_skipped = match entry {
                        Some(e) => sender.try_send(ClipboardEvent::Captured(e)).is_ok(),
                        None => true,
                    };
                    if sent_or_skipped {
                        last_file_fingerprint = fingerprint;
                        last_seen.clear();
                        last_image_fingerprint.clear();
                        last_html_fingerprint.clear();
                    }
                }
            } else if let Ok(html) = clipboard.get().html() {
                handled = true;
                let text = clipboard.get_text().unwrap_or_default();
                let fingerprint = string_fingerprint(&(text.clone() + "\u{1f}" + &html));
                if !html.trim().is_empty() && fingerprint != last_html_fingerprint {
                    let entry =
                        ClipboardEntry::captured_rich_text(text, html, platform::active_app_name());
                    let sent_or_skipped = match entry {
                        Some(e) => sender.try_send(ClipboardEvent::Captured(e)).is_ok(),
                        None => true,
                    };
                    if sent_or_skipped {
                        last_html_fingerprint = fingerprint;
                        last_seen.clear();
                        last_image_fingerprint.clear();
                        last_file_fingerprint.clear();
                    }
                }
            } else if let Ok(image) = clipboard.get_image() {
                handled = true;
                let fingerprint = image_fingerprint(image.width, image.height, &image.bytes);
                if !fingerprint.is_empty()
                    && fingerprint != last_image_fingerprint
                    && let Some(data_url) =
                        image_to_png_data_url(image.width, image.height, image.bytes.as_ref())
                {
                    let entry =
                        ClipboardEntry::captured_image(data_url, platform::active_app_name());
                    let sent_or_skipped = match entry {
                        Some(e) => sender.try_send(ClipboardEvent::Captured(e)).is_ok(),
                        None => true,
                    };
                    if sent_or_skipped {
                        last_image_fingerprint = fingerprint;
                        last_seen.clear();
                        last_file_fingerprint.clear();
                        last_html_fingerprint.clear();
                    }
                }
            } else if let Ok(text) = clipboard.get_text() {
                handled = true;
                if text != last_seen {
                    let entry =
                        ClipboardEntry::captured_text(text.clone(), platform::active_app_name());
                    let sent_or_skipped = match entry {
                        Some(e) => sender.try_send(ClipboardEvent::Captured(e)).is_ok(),
                        None => true,
                    };
                    if sent_or_skipped {
                        last_seen = text;
                        last_image_fingerprint.clear();
                        last_file_fingerprint.clear();
                        last_html_fingerprint.clear();
                    }
                }
            }
        }
        if !handled {
            clipboard = None;
        }
        thread::sleep(Duration::from_millis(700));
    }
}

fn image_fingerprint(width: usize, height: usize, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let mut hasher = Sha256::new();
    hasher.update(width.to_le_bytes());
    hasher.update(height.to_le_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn string_fingerprint(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn set_html(text: &str, html: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|err| format!("初始化剪贴板失败: {err}"))?;
    clipboard
        .set_html(html.to_string(), Some(text.to_string()))
        .map_err(|err| format!("写入富文本剪贴板失败: {err}"))
}

fn set_file_list(content: &str) -> Result<(), String> {
    let paths = content
        .lines()
        .map(normalize_file_path)
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err("文件剪贴板内容为空".to_string());
    }
    #[cfg(target_os = "linux")]
    if set_file_list_with_xclip(&paths).is_ok() {
        return Ok(());
    }
    let mut clipboard = Clipboard::new().map_err(|err| format!("初始化剪贴板失败: {err}"))?;
    clipboard
        .set()
        .file_list(&paths)
        .map_err(|err| format!("写入文件剪贴板失败: {err}"))
}

#[cfg(target_os = "linux")]
fn set_file_list_with_xclip(paths: &[PathBuf]) -> Result<(), String> {
    let payload = format!(
        "copy\n{}\n",
        paths
            .iter()
            .map(|p| path_to_file_uri(p))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut child = Command::new("xclip")
        .args([
            "-selection",
            "clipboard",
            "-loops",
            "1",
            "-t",
            "x-special/gnome-copied-files",
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("xclip 不可用: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(payload.as_bytes())
            .map_err(|err| format!("写入 xclip 失败: {err}"))?;
    } else {
        return Err("打开 xclip stdin 失败".to_string());
    }
    std::thread::sleep(Duration::from_millis(40));
    if let Some(status) = child
        .try_wait()
        .map_err(|err| format!("检查 xclip 状态失败: {err}"))?
    {
        if status.success() {
            return Ok(());
        }
        return Err(format!("xclip 写入文件剪贴板失败: {status}"));
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(target_os = "linux")]
fn path_to_file_uri(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("file://{}", percent_encode_uri_path(&path))
}

#[cfg(target_os = "linux")]
fn percent_encode_uri_path(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect::<Vec<_>>(),
        })
        .collect()
}

fn normalize_file_path(value: &str) -> PathBuf {
    let trimmed = value.trim();
    if let Some(path) = trimmed.strip_prefix("file://") {
        PathBuf::from(percent_decode(path))
    } else {
        PathBuf::from(trimmed)
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            output.push(hex);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn image_to_png_data_url(width: usize, height: usize, bytes: &[u8]) -> Option<String> {
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, bytes.to_vec())?;
    let mut encoded = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
        .ok()?;
    Some(format!("data:image/png;base64,{}", encode_base64(&encoded)))
}

fn set_image_from_data_url(content: &str) -> Result<(), String> {
    let payload = content
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(content);
    let bytes =
        decode_base64(payload.trim()).map_err(|err| format!("解析图片剪贴板内容失败: {err}"))?;
    let rgba = image::load_from_memory(&bytes)
        .map_err(|err| format!("读取图片失败: {err}"))?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut clipboard = Clipboard::new().map_err(|err| format!("初始化剪贴板失败: {err}"))?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(rgba.into_raw()),
        })
        .map_err(|err| format!("写入图片剪贴板失败: {err}"))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(b0 >> 2) as usize] as char);
        output.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn decode_base64(value: &str) -> Result<Vec<u8>, &'static str> {
    let cleaned = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if cleaned.len() % 4 != 0 {
        return Err("base64 长度无效");
    }
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let a = decode_base64_byte(chunk[0])?;
        let b = decode_base64_byte(chunk[1])?;
        let c = if chunk[2] == b'=' {
            64
        } else {
            decode_base64_byte(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            64
        } else {
            decode_base64_byte(chunk[3])?
        };
        out.push((a << 2) | (b >> 4));
        if c != 64 {
            out.push(((b & 0b0000_1111) << 4) | (c >> 2));
        }
        if d != 64 {
            out.push(((c & 0b0000_0011) << 6) | d);
        }
    }
    Ok(out)
}

fn decode_base64_byte(byte: u8) -> Result<u8, &'static str> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("base64 字符无效"),
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardEvent;
    use crossbeam_channel::{TrySendError, bounded};

    #[test]
    fn bounded_channel_rejects_when_full() {
        let (tx, rx) = bounded::<ClipboardEvent>(2);
        tx.try_send(ClipboardEvent::ToggleWindow)
            .expect("first send");
        tx.try_send(ClipboardEvent::FocusSearch)
            .expect("second send");
        assert!(matches!(
            tx.try_send(ClipboardEvent::Quit),
            Err(TrySendError::Full(_))
        ));
        let drained = rx.try_recv().expect("drain first event");
        assert!(matches!(drained, ClipboardEvent::ToggleWindow));
        tx.try_send(ClipboardEvent::Quit).expect("send after drain");
    }
}
