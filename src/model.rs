use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 1_000;
pub const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
pub const RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardKind {
    Text,
    Url,
    Code,
    File,
    Image,
    Video,
    RichText,
}

impl ClipboardKind {
    pub const ALL: [ClipboardKind; 7] = [
        ClipboardKind::Text,
        ClipboardKind::Url,
        ClipboardKind::Code,
        ClipboardKind::File,
        ClipboardKind::Image,
        ClipboardKind::Video,
        ClipboardKind::RichText,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ClipboardKind::Text => "text",
            ClipboardKind::Url => "url",
            ClipboardKind::Code => "code",
            ClipboardKind::File => "file",
            ClipboardKind::Image => "image",
            ClipboardKind::Video => "video",
            ClipboardKind::RichText => "rich_text",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ClipboardKind::Text => "text",
            ClipboardKind::Url => "url",
            ClipboardKind::Code => "code",
            ClipboardKind::File => "file",
            ClipboardKind::Image => "image",
            ClipboardKind::Video => "video",
            ClipboardKind::RichText => "rich",
        }
    }
}

impl From<&str> for ClipboardKind {
    fn from(value: &str) -> Self {
        match value {
            "text" => ClipboardKind::Text,
            "url" => ClipboardKind::Url,
            "code" => ClipboardKind::Code,
            "file" => ClipboardKind::File,
            "image" => ClipboardKind::Image,
            "video" => ClipboardKind::Video,
            "rich_text" => ClipboardKind::RichText,
            _ => ClipboardKind::Text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntry {
    pub id: i64,
    pub kind: ClipboardKind,
    pub content: String,
    pub html_content: Option<String>,
    pub source_app: String,
    pub source_app_path: Option<String>,
    pub timestamp: i64,
    pub preview: String,
    pub is_pinned: bool,
    pub tags: Vec<String>,
    pub use_count: i64,
    pub is_external: bool,
    pub pinned_order: i64,
}

impl ClipboardEntry {
    pub fn captured_text(content: String, source_app: String) -> Option<Self> {
        let trimmed = content.trim_matches('\0').trim().to_string();
        if trimmed.is_empty() || trimmed.len() > MAX_CONTENT_BYTES {
            return None;
        }

        Some(Self {
            id: 0,
            kind: detect_kind(&trimmed),
            preview: make_preview(&trimmed, 500),
            content: trimmed,
            html_content: None,
            source_app,
            source_app_path: None,
            timestamp: Local::now().timestamp_millis(),
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
        })
    }

    pub fn captured_image(data_url: String, source_app: String) -> Option<Self> {
        if data_url.is_empty() || data_url.len() > MAX_CONTENT_BYTES {
            return None;
        }

        Some(Self {
            id: 0,
            kind: ClipboardKind::Image,
            preview: "图片剪贴板内容".to_string(),
            content: data_url,
            html_content: None,
            source_app,
            source_app_path: None,
            timestamp: Local::now().timestamp_millis(),
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
        })
    }

    pub fn captured_rich_text(text: String, html: String, source_app: String) -> Option<Self> {
        let trimmed_text = text.trim_matches('\0').trim().to_string();
        let trimmed_html = html.trim_matches('\0').trim().to_string();
        if trimmed_html.is_empty() || trimmed_text.len() + trimmed_html.len() > MAX_CONTENT_BYTES {
            return None;
        }

        let preview_source = if trimmed_text.is_empty() {
            strip_html_tags(&trimmed_html)
        } else {
            trimmed_text.clone()
        };

        Some(Self {
            id: 0,
            kind: ClipboardKind::RichText,
            preview: make_preview(&preview_source, 500),
            content: preview_source,
            html_content: Some(trimmed_html),
            source_app,
            source_app_path: None,
            timestamp: Local::now().timestamp_millis(),
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external: false,
            pinned_order: 0,
        })
    }

    pub fn captured_files(paths: Vec<String>, source_app: String) -> Option<Self> {
        let normalized = paths
            .into_iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        let content = normalized.join("\n");
        if content.is_empty() || content.len() > MAX_CONTENT_BYTES {
            return None;
        }

        let kind = if normalized.len() == 1 && looks_like_video_path(&normalized[0]) {
            ClipboardKind::Video
        } else if normalized.len() == 1 && looks_like_image_path(&normalized[0]) {
            ClipboardKind::Image
        } else {
            ClipboardKind::File
        };
        let preview = if normalized.len() == 1 {
            normalized[0].clone()
        } else {
            format!("{} 个文件", normalized.len())
        };

        Some(Self {
            id: 0,
            kind,
            preview: make_preview(&preview, 500),
            content,
            html_content: None,
            source_app,
            source_app_path: None,
            timestamp: Local::now().timestamp_millis(),
            is_pinned: false,
            tags: Vec::new(),
            use_count: 0,
            is_external: true,
            pinned_order: 0,
        })
    }

    pub fn formatted_time(&self) -> String {
        let dt: DateTime<Local> = Local
            .timestamp_millis_opt(self.timestamp)
            .single()
            .unwrap_or_else(Local::now);
        dt.format("%m-%d %H:%M:%S").to_string()
    }

    pub fn is_sensitive(&self) -> bool {
        let tagged_sensitive = self.tags.iter().any(|tag| {
            let tag = tag.to_ascii_lowercase();
            tag == "sensitive" || tag == "密码" || tag == "password" || tag == "secret"
        });
        tagged_sensitive
            || matches!(
                self.kind,
                ClipboardKind::Text
                    | ClipboardKind::Url
                    | ClipboardKind::Code
                    | ClipboardKind::RichText
            ) && looks_sensitive(&self.content)
    }
}

fn detect_kind(value: &str) -> ClipboardKind {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("data:image/") {
        return ClipboardKind::Image;
    }
    if lower.starts_with("data:video/") {
        return ClipboardKind::Video;
    }
    if lower.starts_with("file://") || value.lines().all(looks_like_file_path) {
        return ClipboardKind::File;
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return ClipboardKind::Url;
    }
    if looks_like_code(value) {
        return ClipboardKind::Code;
    }
    ClipboardKind::Text
}

fn looks_like_file_path(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && (value.starts_with('/') || value.starts_with("~/") || value.starts_with("file://"))
}

fn looks_like_image_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".tiff"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn looks_like_video_path(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn strip_html_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for char in value.chars() {
        match char {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                output.push(' ');
            }
            _ if !in_tag => output.push(char),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_code(value: &str) -> bool {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    let code_tokens = [
        "fn ",
        "let ",
        "const ",
        "class ",
        "function ",
        "import ",
        "export ",
        "use ",
        "impl ",
        "struct ",
        "enum ",
        "trait ",
        "def ",
        "pub ",
        "cargo ",
        "rustup ",
        "npm ",
        "git ",
    ];
    trimmed.contains("{") && trimmed.contains("}")
        || trimmed.contains("=>")
        || trimmed.contains("::")
        || code_tokens.iter().any(|token| lower.contains(token))
}

fn looks_sensitive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let sensitive_tokens = [
        "password",
        "passwd",
        "secret",
        "token",
        "apikey",
        "api_key",
        "access_key",
        "private_key",
        "authorization:",
        "bearer ",
        "ssh-rsa",
        "-----begin",
    ];
    if sensitive_tokens.iter().any(|token| lower.contains(token)) {
        return true;
    }
    let digits = value.chars().filter(|char| char.is_ascii_digit()).count();
    let at_count = value.matches('@').count();
    digits >= 11 || (at_count == 1 && value.contains('.') && !value.contains(' '))
}

pub fn make_preview(value: &str, limit: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        return compact;
    }

    let mut out = compact
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_text_entries_do_not_use_content_sensitive_heuristics() {
        let image = ClipboardEntry::captured_image(
            "data:image/png;base64,passwordtoken@example.com12345678901".to_string(),
            "test".to_string(),
        )
        .expect("image entry");
        assert!(!image.is_sensitive());

        let file = ClipboardEntry::captured_files(
            vec!["/tmp/password-token-12345678901.png".to_string()],
            "test".to_string(),
        )
        .expect("file entry");
        assert!(!file.is_sensitive());
    }

    #[test]
    fn explicit_sensitive_tag_still_marks_non_text_entries() {
        let mut image = ClipboardEntry::captured_image(
            "data:image/png;base64,plain".to_string(),
            "test".to_string(),
        )
        .expect("image entry");
        image.tags.push("sensitive".to_string());
        assert!(image.is_sensitive());
    }
}
