use crate::actions::executor::ActionExecutor;
use crate::actions::matcher::ActionMatcher;
use crate::blacklist::AppBlacklist;
use crate::model::{ClipboardEntry, ClipboardKind, SelectionSource};
use crate::platform;
use crate::storage::Storage;
use arboard::Clipboard;
use crossbeam_channel::Sender;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use rust_i18n::t;
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Duration (ms) within which primary selection writes from our own
/// watcher are suppressed to avoid echo loops.
const PRIMARY_ECHO_WINDOW: Duration = Duration::from_millis(500);

/// Tracks recent primary-selection writes so that our own xclip/xsel
/// write-back does not immediately re-enter the capture path.
///
/// The guard stores the content fingerprint + write instant of the last
/// write we performed.  When the watcher sees a primary selection whose
/// fingerprint matches within `PRIMARY_ECHO_WINDOW`, it skips capture.
#[derive(Clone)]
pub struct PrimaryEchoGuard {
    inner: Arc<Mutex<(String, Option<Instant>)>>,
}

impl PrimaryEchoGuard {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new((String::new(), None))),
        }
    }
}

impl Default for PrimaryEchoGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PrimaryEchoGuard {
    /// Record a write we just performed.
    pub fn mark_write(&self, content_hash: String) {
        let mut state = self.inner.lock().expect("echo guard poisoned");
        *state = (content_hash, Some(Instant::now()));
    }

    /// Returns `true` if `content_hash` matches the last write within the
    /// echo window.
    pub fn should_suppress(&self, content_hash: &str) -> bool {
        let state = self.inner.lock().expect("echo guard poisoned");
        state.0 == content_hash && state.1.is_some_and(|at| at.elapsed() < PRIMARY_ECHO_WINDOW)
    }
}

#[derive(Debug, Clone)]
pub enum ClipboardEvent {
    Captured(ClipboardEntry),
    ToggleWindow,
    FocusSearch,
    PasteLatestRich,
    SequentialPaste,
    OpenSettings,
    TogglePrivateMode,
    Quit,
    Status(String),
    Error(String),
}

pub fn start_watcher(
    sender: Sender<ClipboardEvent>,
    exclusion_patterns: Vec<String>,
    private_mode: Arc<AtomicBool>,
    storage: Storage,
    builtin_actions_enabled: bool,
    echo_guard: PrimaryEchoGuard,
) {
    thread::Builder::new()
        .name("clipboard-watcher".to_string())
        .spawn(move || {
            watch_loop(
                sender,
                exclusion_patterns,
                private_mode,
                storage,
                builtin_actions_enabled,
                echo_guard,
            )
        })
        .expect("spawn clipboard watcher");
}

pub fn set_text(content: &str) -> Result<(), String> {
    let mut clipboard =
        Clipboard::new().map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    clipboard
        .set_text(content.to_string())
        .map_err(|err| t!("clipboard.error.write_text_failed", err = err).to_string())
}

#[cfg(target_os = "linux")]
pub fn read_primary_text() -> Option<String> {
    let output = Command::new("xclip")
        .args(["-selection", "primary", "-o"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(not(target_os = "linux"))]
pub fn read_primary_text() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
pub fn write_primary_text(content: &str) -> Result<(), String> {
    let mut child = Command::new("xclip")
        .args(["-selection", "primary"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(content.as_bytes())
            .map_err(|err| t!("clipboard.error.write_text_failed", err = err).to_string())?;
    }
    let _ = child.wait();
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn write_primary_text(_content: &str) -> Result<(), String> {
    Err("Primary selection not supported".to_string())
}

#[cfg(target_os = "linux")]
pub fn simulate_middle_click() -> Result<(), String> {
    let status = Command::new("xdotool")
        .args(["click", "2"])
        .status()
        .map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(t!("clipboard.error.write_text_failed", err = status).to_string())
    }
}

#[cfg(not(target_os = "linux"))]
pub fn simulate_middle_click() -> Result<(), String> {
    Err("Middle click not supported".to_string())
}

pub fn paste_primary_entry(
    entry: &ClipboardEntry,
    echo_guard: &PrimaryEchoGuard,
) -> Result<(), String> {
    write_primary_text(&entry.content)?;
    echo_guard.mark_write(string_fingerprint(&entry.content));
    simulate_middle_click()
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

fn watch_loop(
    sender: Sender<ClipboardEvent>,
    exclusion_patterns: Vec<String>,
    private_mode: Arc<AtomicBool>,
    storage: Storage,
    builtin_actions_enabled: bool,
    echo_guard: PrimaryEchoGuard,
) {
    let blacklist = AppBlacklist::new(exclusion_patterns);
    let action_matcher = if builtin_actions_enabled {
        ActionMatcher::new(storage.load_actions().unwrap_or_default())
    } else {
        ActionMatcher::new(vec![])
    };
    let action_executor = ActionExecutor::new();
    let mut last_seen = String::new();
    let mut last_image_fingerprint = String::new();
    let mut last_html_fingerprint = String::new();
    let mut last_file_fingerprint = String::new();
    let mut last_primary_text = String::new();
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
            // Private mode short-circuits all capture paths: while the
            // user has it on, we never even probe the clipboard (the
            // probes below are skipped to avoid producing fingerprints
            // the user does not want captured, and to save CPU).
            if private_mode.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(700));
                continue;
            }
            if let Some(active_class) = platform::active_window_class()
                && blacklist.is_match(&active_class)
            {
                thread::sleep(Duration::from_millis(700));
                continue;
            }
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
                        last_seen = text.clone();
                        last_image_fingerprint.clear();
                        last_file_fingerprint.clear();
                        last_html_fingerprint.clear();
                        if builtin_actions_enabled
                            && let Some(matched) = action_matcher.find_first_match(&text)
                        {
                            action_executor.execute_async(&matched.action, &text);
                        }
                    }
                }
            }

            if let Some(primary_text) = read_primary_text() {
                let primary_hash = string_fingerprint(&primary_text);
                if primary_text != last_primary_text && !echo_guard.should_suppress(&primary_hash) {
                    let entry = ClipboardEntry::captured_text_with_source(
                        primary_text.clone(),
                        platform::active_app_name(),
                        Some(SelectionSource::Primary),
                    );
                    let sent_or_skipped = match entry {
                        Some(e) => sender.try_send(ClipboardEvent::Captured(e)).is_ok(),
                        None => true,
                    };
                    if sent_or_skipped {
                        last_primary_text = primary_text;
                        if builtin_actions_enabled
                            && let Some(matched) =
                                action_matcher.find_first_match(&last_primary_text)
                        {
                            action_executor.execute_async(&matched.action, &last_primary_text);
                        }
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
    let mut clipboard =
        Clipboard::new().map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    clipboard
        .set_html(html.to_string(), Some(text.to_string()))
        .map_err(|err| t!("clipboard.error.write_rich_failed", err = err).to_string())
}

pub fn set_file_list(content: &str) -> Result<(), String> {
    let paths = content
        .lines()
        .map(normalize_file_path)
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err(t!("clipboard.error.file_content_empty").to_string());
    }
    #[cfg(target_os = "linux")]
    let _ = set_file_list_with_xclip(&paths);

    let mut clipboard =
        Clipboard::new().map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    clipboard
        .set()
        .file_list(&paths)
        .map_err(|err| t!("clipboard.error.write_file_failed", err = err).to_string())
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
        .map_err(|err| t!("clipboard.error.xclip_unavailable", err = err).to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(payload.as_bytes())
            .map_err(|err| t!("clipboard.error.xclip_write_failed", err = err).to_string())?;
    } else {
        return Err(t!("clipboard.error.xclip_stdin_failed").to_string());
    }
    std::thread::sleep(Duration::from_millis(40));
    if let Some(status) = child
        .try_wait()
        .map_err(|err| t!("clipboard.error.xclip_status_failed", err = err).to_string())?
    {
        if status.success() {
            return Ok(());
        }
        return Err(t!("clipboard.error.xclip_file_failed", status = status).to_string());
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
    let bytes = decode_base64(payload.trim())
        .map_err(|err| t!("clipboard.error.image_parse_failed", err = err).to_string())?;
    let rgba = image::load_from_memory(&bytes)
        .map_err(|err| t!("clipboard.error.image_read_failed", err = err).to_string())?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut clipboard =
        Clipboard::new().map_err(|err| t!("clipboard.error.init_failed", err = err).to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(rgba.into_raw()),
        })
        .map_err(|err| t!("clipboard.error.write_image_failed", err = err).to_string())
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

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    let cleaned = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if cleaned.len() % 4 != 0 {
        return Err(t!("clipboard.error.base64_length_invalid").to_string());
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

fn decode_base64_byte(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(t!("clipboard.error.base64_char_invalid").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn test_echo_suppression() {
        let guard = PrimaryEchoGuard::new();
        let hash = string_fingerprint("test content");

        assert!(
            !guard.should_suppress(&hash),
            "should not suppress before any write"
        );

        guard.mark_write(hash.clone());

        assert!(
            guard.should_suppress(&hash),
            "should suppress within window after write"
        );

        let other_hash = string_fingerprint("other content");
        assert!(
            !guard.should_suppress(&other_hash),
            "should not suppress different content"
        );
    }

    #[test]
    fn test_echo_guard_independent_instances() {
        let guard1 = PrimaryEchoGuard::new();
        let guard2 = PrimaryEchoGuard::new();
        let hash = string_fingerprint("shared content");

        guard1.mark_write(hash.clone());

        assert!(guard1.should_suppress(&hash));
        assert!(
            !guard2.should_suppress(&hash),
            "separate guard instances should be independent"
        );
    }
}
