//! Text snippet management and quick-insert.
//!
//! Provides snippet storage, variable expansion, and a searchable
//! snippet picker integrated with the clipboard workflow.

pub mod interpolate;

use serde::{Deserialize, Serialize};

/// A reusable text snippet with `{{var}}` placeholder support.
///
/// Snippets are persisted in the `snippets` SQLite table (see `storage.rs`).
/// Each snippet has a name, a template string with `{{variable}}` placeholders,
/// optional description/icon/tags, and usage tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    /// Unique ID (i64, auto-incremented by SQLite).
    pub id: i64,
    /// User-friendly name, e.g. "GitHub PR link".
    pub name: String,
    /// Template text with `{{var}}` placeholders, e.g. "https://github.com/{{org}}/{{repo}}/pull/{{number}}".
    pub template: String,
    /// Optional description of what this snippet does.
    #[serde(default)]
    pub description: String,
    /// Emoji or unicode icon (e.g. "🔗" or "📝").
    #[serde(default)]
    pub icon: String,
    /// Whether this snippet is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// How many times this snippet has been used.
    #[serde(default)]
    pub use_count: i64,
    /// Last time this snippet was used (unix seconds).
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Unix seconds when the snippet was created.
    #[serde(default)]
    pub created_at: i64,
    /// JSON array of tag names for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Snippet {
    /// Create a new snippet with sensible defaults.
    ///
    /// - `id` is set to 0 (will be assigned by SQLite on insert).
    /// - `enabled` defaults to `true`.
    /// - `use_count` defaults to 0.
    /// - `created_at` is set to current unix seconds.
    pub fn new(name: &str, template: &str) -> Self {
        Self {
            id: 0,
            name: name.to_string(),
            template: template.to_string(),
            description: String::new(),
            icon: String::new(),
            enabled: true,
            use_count: 0,
            last_used_at: None,
            created_at: chrono::Utc::now().timestamp(),
            tags: Vec::new(),
        }
    }
}
