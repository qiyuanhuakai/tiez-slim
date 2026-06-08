use chrono::{DateTime, Local, TimeZone};
use regex::Regex;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::OnceLock;

pub const MAX_ENTRIES: usize = 1_000;
pub const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
pub const RETENTION_DAYS: i64 = 30;

const CODE_KEYWORDS: &[&str] = &[
    "fn", "let", "const", "class", "function", "import", "export", "use", "impl", "struct", "enum",
    "trait", "def", "pub", "cargo", "rustup", "npm",
];

static PHONE_RE: OnceLock<Regex> = OnceLock::new();
static IDCARD_RE: OnceLock<Regex> = OnceLock::new();
static EMAIL_RE: OnceLock<Regex> = OnceLock::new();
static SECRET_RE: OnceLock<Regex> = OnceLock::new();

fn phone_re() -> &'static Regex {
    PHONE_RE.get_or_init(|| {
        Regex::new(r"(?:\+?86)?[-\s\(]*1[3-9]\d{1}[-\s\)]*\d{4}[-\s]*\d{4}").unwrap()
    })
}

fn idcard_re() -> &'static Regex {
    IDCARD_RE.get_or_init(|| Regex::new(r"\b\d{17}[\dXx]\b").unwrap())
}

fn email_re() -> &'static Regex {
    EMAIL_RE.get_or_init(|| Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap())
}

fn secret_re() -> &'static Regex {
    SECRET_RE.get_or_init(|| {
        Regex::new(r"(?i)(?:sk-[a-zA-Z0-9]{20,}|pk-[a-zA-Z0-9]{20,}|ghp_[a-zA-Z0-9]{30,}|AIza[a-zA-Z0-9_-]{30,}|AKIA[A-Z0-9]{16}|ya29\.[a-zA-Z0-9_-]{20,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)").unwrap()
    })
}

/// Heuristic password detection – avoids Rust-unsupported regex lookahead.
fn is_password_like(value: &str) -> bool {
    let len = value.len();
    if !(8..=64).contains(&len) {
        return false;
    }
    if value.contains(' ') || value.contains('\n') {
        return false;
    }
    let has_lower = value.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = value.bytes().any(|b| b.is_ascii_uppercase());
    let has_digit = value.bytes().any(|b| b.is_ascii_digit());
    let has_special = value.bytes().any(|b| b"@$!%*?&".contains(&b));
    has_lower && has_upper && has_digit && has_special
}

/// Rule-based sensitive detection with pluggable kinds and custom regex rules.
pub fn looks_sensitive_with_rules(
    text: &str,
    enabled_kinds: &[&str],
    custom_rules: &[Regex],
) -> bool {
    if text.len() > 5000 {
        return false;
    }

    for kind in enabled_kinds {
        match *kind {
            "phone" if phone_re().is_match(text) => {
                return true;
            }
            "idcard" if idcard_re().is_match(text) => {
                return true;
            }
            "email" if email_re().is_match(text) => {
                return true;
            }
            "secret" if secret_re().is_match(text) => {
                return true;
            }
            "password" if is_password_like(text) => {
                return true;
            }
            _ => {}
        }
    }
    for rule in custom_rules {
        if rule.is_match(text) {
            return true;
        }
    }

    false
}

/// Compile custom regex rule strings (one per line) into validated [`Regex`] objects.
/// Returns `Err` with per-line errors if any pattern is invalid.
#[allow(dead_code)]
pub fn compile_custom_rules(raw: &str) -> Result<Vec<Regex>, Vec<(usize, regex::Error)>> {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match Regex::new(line) {
            Ok(re) => rules.push(re),
            Err(e) => errors.push((i, e)),
        }
    }
    if errors.is_empty() {
        Ok(rules)
    } else {
        Err(errors)
    }
}

/// Origin of a clipboard capture: normal CLIPBOARD selection (Ctrl+C) or
/// PRIMARY selection (mouse highlight).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectionSource {
    #[default]
    #[serde(rename = "clipboard")]
    Clipboard,
    #[serde(rename = "primary")]
    Primary,
}

impl SelectionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clipboard => "clipboard",
            Self::Primary => "primary",
        }
    }
}

impl From<&str> for SelectionSource {
    fn from(s: &str) -> Self {
        match s {
            "primary" => Self::Primary,
            _ => Self::Clipboard,
        }
    }
}

/// Fast fingerprint for a string (not cryptographically secure).
/// Used for echo suppression and primary-selection dedup.
pub fn string_fingerprint(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

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
    #[serde(default)]
    pub source: SelectionSource,
}

/// Lightweight projection of [`ClipboardEntry`] used for list rendering.
///
/// Excludes the bulky `content` and `html_content` fields so a large history
/// can be listed cheaply. The full entry can be loaded on demand via
/// `Storage::get_entry(id)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardEntrySummary {
    pub id: i64,
    pub kind: ClipboardKind,
    pub source_app: String,
    pub source_app_path: Option<String>,
    pub timestamp: i64,
    pub preview: String,
    pub is_pinned: bool,
    pub tags: Vec<String>,
    pub use_count: i64,
    pub is_external: bool,
    pub pinned_order: i64,
    pub sensitive: bool,
    #[serde(default)]
    pub source: SelectionSource,
}

impl ClipboardEntrySummary {
    pub fn is_sensitive(&self) -> bool {
        self.sensitive
    }

    pub fn formatted_time(&self) -> String {
        let dt: DateTime<Local> = Local
            .timestamp_millis_opt(self.timestamp)
            .single()
            .unwrap_or_else(Local::now);
        dt.format("%m-%d %H:%M:%S").to_string()
    }
}

/// Project a full entry into a [`ClipboardEntrySummary`], dropping
/// `content` and `html_content`. The `sensitive` flag is taken from
/// `entry.is_sensitive()`, which is also what `Storage::save_entry_with_dedup`
/// stores in the `sensitive` column at write time.
#[allow(dead_code)]
pub fn make_summary(entry: &ClipboardEntry) -> ClipboardEntrySummary {
    ClipboardEntrySummary {
        id: entry.id,
        kind: entry.kind.clone(),
        source_app: entry.source_app.clone(),
        source_app_path: entry.source_app_path.clone(),
        timestamp: entry.timestamp,
        preview: entry.preview.clone(),
        is_pinned: entry.is_pinned,
        tags: entry.tags.clone(),
        use_count: entry.use_count,
        is_external: entry.is_external,
        pinned_order: entry.pinned_order,
        sensitive: entry.is_sensitive(),
        source: entry.source.clone(),
    }
}

impl ClipboardEntry {
    pub fn captured_text(content: String, source_app: String) -> Option<Self> {
        Self::captured_text_with_source(content, source_app, None)
    }

    pub fn captured_text_with_source(
        content: String,
        source_app: String,
        source: Option<SelectionSource>,
    ) -> Option<Self> {
        let trimmed = content.trim_matches('\0').trim().to_string();
        if trimmed.is_empty() || trimmed.len() > MAX_CONTENT_BYTES {
            return None;
        }

        Some(Self {
            id: 0,
            kind: detect_kind(&trimmed),
            preview: make_preview(&trimmed, 500).into_owned(),
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
            source: source.unwrap_or_default(),
        })
    }

    pub fn captured_image(data_url: String, source_app: String) -> Option<Self> {
        if data_url.is_empty() || data_url.len() > MAX_CONTENT_BYTES {
            return None;
        }

        Some(Self {
            id: 0,
            kind: ClipboardKind::Image,
            preview: t!("preview.image_clipboard").to_string(),
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
            source: SelectionSource::default(),
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
            preview: make_preview(&preview_source, 500).into_owned(),
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
            source: SelectionSource::default(),
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
            t!("preview.files_count", count = normalized.len()).to_string()
        };

        Some(Self {
            id: 0,
            kind,
            preview: make_preview(&preview, 500).into_owned(),
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
            source: SelectionSource::default(),
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
    // Fast path: case-sensitive prefix checks on the original value avoid
    // the full `to_ascii_lowercase()` allocation for the common case where
    // URLs / file paths / data URLs are already lowercase (the convention).
    if value.starts_with("data:image/") {
        return ClipboardKind::Image;
    }
    if value.starts_with("data:video/") {
        return ClipboardKind::Video;
    }
    if value.starts_with("file://") {
        return ClipboardKind::File;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return ClipboardKind::Url;
    }
    if value.lines().all(looks_like_file_path) {
        return ClipboardKind::File;
    }

    // Slow path: case-insensitive fallback for less common uppercase prefixes
    // (e.g. `HTTP://` or `Data:Image/...`). Uses a byte-wise compare helper
    // that does not allocate a lowercased copy of the value.
    if starts_with_ignore_ascii_case(value, "data:image/") {
        return ClipboardKind::Image;
    }
    if starts_with_ignore_ascii_case(value, "data:video/") {
        return ClipboardKind::Video;
    }
    if starts_with_ignore_ascii_case(value, "file://") {
        return ClipboardKind::File;
    }
    if starts_with_ignore_ascii_case(value, "http://")
        || starts_with_ignore_ascii_case(value, "https://")
    {
        return ClipboardKind::Url;
    }

    if looks_like_code(value) {
        return ClipboardKind::Code;
    }
    ClipboardKind::Text
}

/// Byte-wise case-insensitive prefix check that does not allocate.
fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    if value.len() < prefix.len() {
        return false;
    }
    value
        .as_bytes()
        .iter()
        .zip(prefix.as_bytes().iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
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
    if value.len() > 8192 {
        return false;
    }
    let mut score = 0u32;
    for kw in CODE_KEYWORDS {
        if value.contains(kw) {
            score += 1;
        }
    }
    if value.contains(';') {
        score += 1;
    }
    if value.contains('{') && value.contains('}') {
        score += 1;
    }
    if value.contains("</") {
        score += 1;
    }
    score >= 2
}

pub(crate) fn looks_sensitive(value: &str) -> bool {
    const DEFAULT_KINDS: &[&str] = &["phone", "idcard", "email", "secret", "password"];
    looks_sensitive_with_rules(value, DEFAULT_KINDS, &[])
}

pub fn make_preview(value: &str, limit: usize) -> Cow<'_, str> {
    // Fast path: a value with no ASCII whitespace needs no compaction, so
    // skip the `split_whitespace().collect::<Vec<_>>().join(" ")` round-trip
    // (which allocates a `Vec<&str>` and a new `String`).
    if !value.bytes().any(|b| b.is_ascii_whitespace()) {
        if value.chars().count() <= limit {
            return Cow::Borrowed(value);
        }
        let mut out = value
            .chars()
            .take(limit.saturating_sub(3))
            .collect::<String>();
        out.push_str("...");
        return Cow::Owned(out);
    }

    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= limit {
        return Cow::Owned(compact);
    }

    let mut out = compact
        .chars()
        .take(limit.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_password_like ──

    #[test]
    fn is_password_like_detects_mixed_password() {
        assert!(is_password_like("Abcdef1!"));
        assert!(is_password_like("P@ssw0rd"));
    }

    #[test]
    fn is_password_like_accepts_all_special_chars() {
        for ch in b"@$!%*?&" {
            let pw = format!("Abcdef1{}", *ch as char);
            assert!(
                is_password_like(&pw),
                "should accept special '{}'",
                *ch as char
            );
        }
    }

    #[test]
    fn is_password_like_rejects_non_listed_special() {
        assert!(!is_password_like("Abcdef1#"));
        assert!(!is_password_like("Abcdef1^"));
    }

    #[test]
    fn is_password_like_rejects_plain_word() {
        assert!(!is_password_like("password"));
        assert!(!is_password_like("abcdefgh"));
    }

    #[test]
    fn is_password_like_rejects_too_short() {
        assert!(!is_password_like("Ab1!"));
    }

    #[test]
    fn is_password_like_rejects_too_long() {
        let long = "Aa1!".to_string() + &"x".repeat(70);
        assert!(!is_password_like(&long));
    }

    #[test]
    fn is_password_like_rejects_spaces() {
        assert!(!is_password_like("Abc 1234!"));
    }

    // ── Regex accessors ──

    #[test]
    fn phone_re_matches_chinese_mobile() {
        assert!(phone_re().is_match("13812345678"));
        assert!(phone_re().is_match("+86 138 1234 5678"));
        assert!(!phone_re().is_match("12345"));
    }

    #[test]
    fn idcard_re_matches_18_digit() {
        assert!(idcard_re().is_match("11010119900307451X"));
        assert!(idcard_re().is_match("110101199003074518"));
        assert!(!idcard_re().is_match("12345"));
    }

    #[test]
    fn email_re_matches_standard_email() {
        assert!(email_re().is_match("user@example.com"));
        assert!(!email_re().is_match("not-an-email"));
    }

    #[test]
    fn secret_re_matches_known_prefixes() {
        assert!(secret_re().is_match("sk-abcdefghijklmnopqrstuvwxyz1234"));
        assert!(secret_re().is_match("pk-abcdefghijklmnopqrstuvwxyz1234"));
        assert!(secret_re().is_match("ghp_abcdefghijklmnopqrstuvwxyz12345678"));
        assert!(secret_re().is_match("AIzaSyxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(secret_re().is_match("AKIAIOSFODNN7EXAMPLE"));
        assert!(secret_re().is_match("ya29.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"));
        assert!(!secret_re().is_match("hello world"));
    }

    #[test]
    fn secret_re_matches_private_key() {
        assert!(secret_re().is_match("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(secret_re().is_match("-----BEGIN PRIVATE KEY-----"));
        assert!(!secret_re().is_match("-----BEGIN CERTIFICATE-----"));
    }

    // ── looks_like_code ──

    #[test]
    fn code_keywords_has_17_entries() {
        assert_eq!(CODE_KEYWORDS.len(), 17);
    }

    #[test]
    fn looks_like_code_requires_at_least_two_signals() {
        assert!(!looks_like_code("let x = 1"));
        assert!(looks_like_code("let x = 1; const Y = 2;"));
    }

    #[test]
    fn looks_like_code_braces_plus_keyword() {
        assert!(looks_like_code("fn foo() { }"));
    }

    #[test]
    fn looks_like_code_semicolons_plus_keyword() {
        assert!(looks_like_code("let x = 1;"));
        assert!(!looks_like_code("let x = 1"));
    }

    #[test]
    fn looks_like_code_html_tags() {
        assert!(looks_like_code("<div>hello</div>\nimport foo"));
        assert!(!looks_like_code("just < some text"));
    }

    #[test]
    fn looks_like_code_plain_text_rejected() {
        assert!(!looks_like_code("Hello, world!"));
        assert!(!looks_like_code("12345"));
    }

    #[test]
    fn looks_like_code_struct_impl() {
        assert!(looks_like_code("struct Foo { bar: i32 }"));
    }

    #[test]
    fn looks_like_code_rejects_long_single_line_text() {
        let long = "a".repeat(8193);
        assert!(!looks_like_code(&long));
    }

    // ── looks_sensitive_with_rules ──

    #[test]
    fn looks_sensitive_with_rules_phone() {
        let kinds = ["phone"];
        assert!(looks_sensitive_with_rules(
            "call me at 13812345678",
            &kinds,
            &[]
        ));
        assert!(!looks_sensitive_with_rules("no digits here", &kinds, &[]));
    }

    #[test]
    fn looks_sensitive_with_rules_email() {
        let kinds = ["email"];
        assert!(looks_sensitive_with_rules(
            "send to user@example.com",
            &kinds,
            &[]
        ));
    }

    #[test]
    fn looks_sensitive_with_rules_password() {
        let kinds = ["password"];
        assert!(looks_sensitive_with_rules("Abcdef1!", &kinds, &[]));
        assert!(!looks_sensitive_with_rules("plaintext", &kinds, &[]));
    }

    #[test]
    fn looks_sensitive_with_rules_skips_long_text() {
        let kinds = ["phone"];
        let long_text = "x".repeat(5001);
        assert!(!looks_sensitive_with_rules(&long_text, &kinds, &[]));
    }

    #[test]
    fn looks_sensitive_with_rules_custom_regex() {
        let custom = vec![Regex::new(r"ORDER-\d{6}").unwrap()];
        assert!(looks_sensitive_with_rules(
            "see ORDER-123456 for details",
            &[],
            &custom
        ));
        assert!(!looks_sensitive_with_rules("nothing here", &[], &custom));
    }

    #[test]
    fn looks_sensitive_with_rules_all_five_kinds() {
        let all_kinds: &[&str] = &["phone", "idcard", "email", "secret", "password"];
        assert!(looks_sensitive_with_rules("13812345678", all_kinds, &[]));
        assert!(looks_sensitive_with_rules(
            "user@example.com",
            all_kinds,
            &[]
        ));
        assert!(looks_sensitive_with_rules("Abcdef1!", all_kinds, &[]));
        assert!(!looks_sensitive_with_rules(
            "plain text nothing",
            all_kinds,
            &[]
        ));
    }

    // ── compile_custom_rules ──

    #[test]
    fn compile_custom_rules_valid_and_invalid() {
        let raw = "\\d{3}\n[invalid\nhello";
        let result = compile_custom_rules(raw);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, 1);
    }

    #[test]
    fn compile_custom_rules_all_valid() {
        let raw = "\\d{3}\nhello\nORDER-\\d+";
        let result = compile_custom_rules(raw);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn compile_custom_rules_empty_input() {
        let result = compile_custom_rules("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn compile_custom_rules_skips_comments_and_blanks() {
        let raw = "# comment\n\n\\d{3}\n  \n# another\nhello";
        let result = compile_custom_rules(raw).unwrap();
        assert_eq!(result.len(), 2);
    }

    // ── looks_sensitive (integration via is_sensitive) ──

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

    #[test]
    fn looks_sensitive_detects_phone_number() {
        assert!(looks_sensitive("call me at 13812345678"));
    }

    #[test]
    fn looks_sensitive_detects_email() {
        assert!(looks_sensitive("send to user@example.com"));
    }

    #[test]
    fn looks_sensitive_detects_password_like() {
        assert!(looks_sensitive("P@ssw0rd123"));
    }

    #[test]
    fn looks_sensitive_rejects_plain_text() {
        assert!(!looks_sensitive("hello world"));
    }

    // ── make_summary ──

    #[test]
    fn make_summary_drops_content_and_html_content() {
        let mut entry = ClipboardEntry::captured_text("hello world".to_string(), "src".to_string())
            .expect("valid entry");
        entry.id = 42;
        entry.tags.push("note".to_string());
        entry.use_count = 7;
        entry.is_pinned = true;

        let summary = make_summary(&entry);
        assert_eq!(summary.id, 42);
        assert_eq!(summary.kind, ClipboardKind::Text);
        assert_eq!(summary.source_app, "src");
        assert_eq!(summary.preview, entry.preview);
        assert_eq!(summary.tags, vec!["note".to_string()]);
        assert_eq!(summary.use_count, 7);
        assert!(summary.is_pinned);
        assert!(!summary.is_sensitive());
    }

    #[test]
    fn make_summary_propagates_sensitive_flag() {
        let entry =
            ClipboardEntry::captured_text("call 13812345678".to_string(), "src".to_string())
                .expect("valid entry");
        let summary = make_summary(&entry);
        assert!(summary.is_sensitive());
    }

    #[test]
    fn test_selection_source() {
        assert_eq!(SelectionSource::Clipboard.as_str(), "clipboard");
        assert_eq!(SelectionSource::Primary.as_str(), "primary");
        assert_eq!(SelectionSource::default(), SelectionSource::Clipboard);
        assert_eq!(SelectionSource::from("primary"), SelectionSource::Primary);
        assert_eq!(
            SelectionSource::from("clipboard"),
            SelectionSource::Clipboard
        );
        assert_eq!(SelectionSource::from("unknown"), SelectionSource::Clipboard);

        let json = serde_json::to_string(&SelectionSource::Primary).unwrap();
        assert_eq!(json, "\"primary\"");
        let roundtrip: SelectionSource = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip, SelectionSource::Primary);

        let json_clip = serde_json::to_string(&SelectionSource::Clipboard).unwrap();
        assert_eq!(json_clip, "\"clipboard\"");
        let roundtrip_clip: SelectionSource = serde_json::from_str(&json_clip).unwrap();
        assert_eq!(roundtrip_clip, SelectionSource::Clipboard);
    }

    #[test]
    fn test_captured_text_with_source() {
        let entry = ClipboardEntry::captured_text_with_source(
            "hello".to_string(),
            "test".to_string(),
            Some(SelectionSource::Primary),
        )
        .expect("valid entry");
        assert_eq!(entry.source, SelectionSource::Primary);

        let entry_default = ClipboardEntry::captured_text("hello".to_string(), "test".to_string())
            .expect("valid entry");
        assert_eq!(entry_default.source, SelectionSource::Clipboard);
    }

    #[test]
    fn test_make_summary_propagates_source() {
        let entry = ClipboardEntry::captured_text_with_source(
            "test".to_string(),
            "src".to_string(),
            Some(SelectionSource::Primary),
        )
        .expect("valid entry");
        let summary = make_summary(&entry);
        assert_eq!(summary.source, SelectionSource::Primary);
    }
}
