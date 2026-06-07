//! Action system for executing external commands on clipboard entries.
//!
//! Provides the `Action` model and a simple thread-pool executor built on
//! `std::process::Command` + `std::thread::spawn` (no tokio runtime).
//!
//! Actions are persisted in the `actions` SQLite table (see `storage.rs`).
//! Each action has a regex or glob `pattern` that determines which clipboard
//! entries it applies to, and a `command` template with `%1` placeholder
//! for the matched content.
//!
//! // TODO(T12): Implement Action executor with std::thread::spawn pool (Wave 1)
//! // TODO(T13): Implement action configuration persistence (Wave 1)
//! // TODO(T14): Implement action UI panel (Wave 1)
//! // TODO(T15): Implement action parameter substitution (Wave 1)
//! // TODO(T16): Implement action hotkey binding (Wave 1)

use serde::{Deserialize, Serialize};

/// The kind of action to perform when triggered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Open the matched content with the system default handler.
    Open,
    /// Open the matched content with a specific application.
    OpenWith,
    /// Copy the result of a command substitution to the clipboard.
    Copy,
    /// Execute a shell command.
    ShellCommand,
}

/// A user-defined or built-in action that operates on clipboard entries.
///
/// Actions define a `pattern` (regex or glob) to match clipboard content,
/// and a `command` template to execute when triggered. Built-in actions
/// ship with the app; user-created ones are stored alongside them.
///
/// Serde uses `#[serde(default)]` on optional fields so that loading
/// older JSON (missing new fields) never panics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Unique ID (u64 unix nanoseconds for auto-generation).
    pub id: u64,
    /// User-friendly name, e.g. "在 Firefox 打开".
    pub name: String,
    /// The kind of action.
    pub kind: ActionKind,
    /// Regex pattern (e.g. `^https?://`) or glob (e.g. `*password*`).
    pub pattern: String,
    /// Command template (e.g. `firefox %1`).
    pub command: String,
    /// Emoji or unicode icon (e.g. "🦊" or "🌐").
    #[serde(default)]
    pub icon: String,
    /// Whether this action is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Auto-trigger on every capture (Q6: default false).
    #[serde(default)]
    pub auto_trigger: bool,
    /// Also auto-trigger on PRIMARY selection changes.
    #[serde(default)]
    pub auto_trigger_primary: bool,
    /// Show as a toolbar button (Q11: default false).
    #[serde(default)]
    pub toolbar_button: bool,
    /// UI sort order.
    #[serde(default)]
    pub sort_order: i32,
    /// Built-in vs user-created action (Q5: default false).
    #[serde(default)]
    pub is_builtin: bool,
    /// Unix seconds when the action was created.
    #[serde(default)]
    pub created_at: i64,
    /// Last time this action was used (unix seconds).
    #[serde(default)]
    pub last_used_at: Option<i64>,
}

fn default_true() -> bool {
    true
}

impl Action {
    /// Create a new action with sensible defaults.
    ///
    /// - `id` is set to current unix nanoseconds for uniqueness.
    /// - `kind` defaults to `ShellCommand`; override after construction.
    /// - `enabled` defaults to `true`.
    /// - All boolean flags default to `false`.
    /// - `created_at` is set to current unix seconds.
    pub fn new(name: &str, pattern: &str, command: &str) -> Self {
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;
        Self {
            id: now,
            name: name.to_string(),
            kind: ActionKind::ShellCommand,
            pattern: pattern.to_string(),
            command: command.to_string(),
            icon: String::new(),
            enabled: true,
            auto_trigger: false,
            auto_trigger_primary: false,
            toolbar_button: false,
            sort_order: 0,
            is_builtin: false,
            created_at: chrono::Utc::now().timestamp(),
            last_used_at: None,
        }
    }
}

/// A serializable collection of actions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionList {
    pub actions: Vec<Action>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_new_sets_defaults() {
        let action = Action::new("Test", "^https://", "firefox %1");
        assert_eq!(action.name, "Test");
        assert_eq!(action.kind, ActionKind::ShellCommand);
        assert_eq!(action.pattern, "^https://");
        assert_eq!(action.command, "firefox %1");
        assert!(action.icon.is_empty());
        assert!(action.enabled);
        assert!(!action.auto_trigger);
        assert!(!action.auto_trigger_primary);
        assert!(!action.toolbar_button);
        assert!(!action.is_builtin);
        assert_eq!(action.sort_order, 0);
        assert!(action.last_used_at.is_none());
        assert!(action.created_at > 0);
        assert!(action.id > 0);
    }

    #[test]
    fn action_serialize_roundtrip() {
        let action = Action::new("Open URL", "^https?://", "xdg-open %1");
        let json = serde_json::to_string(&action).expect("serialize");
        let restored: Action = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.name, action.name);
        assert_eq!(restored.kind, action.kind);
        assert_eq!(restored.pattern, action.pattern);
        assert_eq!(restored.command, action.command);
        assert_eq!(restored.enabled, action.enabled);
        assert_eq!(restored.id, action.id);
        assert_eq!(restored.created_at, action.created_at);
    }

    #[test]
    fn action_kind_serialize_snake_case() {
        assert_eq!(
            serde_json::to_string(&ActionKind::Open).unwrap(),
            "\"open\""
        );
        assert_eq!(
            serde_json::to_string(&ActionKind::OpenWith).unwrap(),
            "\"open_with\""
        );
        assert_eq!(
            serde_json::to_string(&ActionKind::Copy).unwrap(),
            "\"copy\""
        );
        assert_eq!(
            serde_json::to_string(&ActionKind::ShellCommand).unwrap(),
            "\"shell_command\""
        );
    }

    #[test]
    fn action_kind_deserialize_snake_case() {
        assert_eq!(
            serde_json::from_str::<ActionKind>("\"open\"").unwrap(),
            ActionKind::Open
        );
        assert_eq!(
            serde_json::from_str::<ActionKind>("\"open_with\"").unwrap(),
            ActionKind::OpenWith
        );
        assert_eq!(
            serde_json::from_str::<ActionKind>("\"copy\"").unwrap(),
            ActionKind::Copy
        );
        assert_eq!(
            serde_json::from_str::<ActionKind>("\"shell_command\"").unwrap(),
            ActionKind::ShellCommand
        );
    }

    #[test]
    fn action_deserialize_missing_optional_fields() {
        let json = r#"{
            "id": 12345,
            "name": "Old Action",
            "kind": "shell_command",
            "pattern": "test",
            "command": "echo %1",
            "created_at": 1000
        }"#;
        let action: Action = serde_json::from_str(json).expect("deserialize old format");
        assert_eq!(action.name, "Old Action");
        assert!(action.enabled);
        assert!(!action.auto_trigger);
        assert!(action.icon.is_empty());
        assert!(action.last_used_at.is_none());
    }

    #[test]
    fn action_list_default_is_empty() {
        let list = ActionList::default();
        assert!(list.actions.is_empty());
    }

    #[test]
    fn action_serialize_deserialize_full() {
        let mut action = Action::new("Firefox URL", "^https?://", "firefox %1");
        action.kind = ActionKind::Open;
        action.icon = "🦊".to_string();
        action.auto_trigger = true;
        action.toolbar_button = true;
        action.sort_order = 5;
        action.is_builtin = true;
        action.last_used_at = Some(1700000000);

        let json = serde_json::to_string_pretty(&action).expect("serialize");
        let restored: Action = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.kind, ActionKind::Open);
        assert_eq!(restored.icon, "🦊");
        assert!(restored.auto_trigger);
        assert!(restored.toolbar_button);
        assert_eq!(restored.sort_order, 5);
        assert!(restored.is_builtin);
        assert_eq!(restored.last_used_at, Some(1700000000));
    }
}
