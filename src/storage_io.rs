//! Storage I/O helpers: export, import, and preview operations.

use crate::model::ClipboardEntry;
use crate::storage::Storage;
use anyhow::{Context, Result, anyhow};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub const EXPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportBundle {
    pub schema_version: u32,
    pub exported_at: i64,
    pub app_version: String,
    pub encryption_version: u32,
    pub settings: serde_json::Value,
    pub tags: Vec<Tag>,
    pub entries: Vec<ClipboardEntry>,
    pub actions: Vec<serde_json::Value>,
    pub snippets: Vec<serde_json::Value>,
    pub emoji_favorites: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportScope {
    SettingsOnly,
    HistoryOnly,
    SettingsAndHistory,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Default)]
pub struct ImportStats {
    pub entries_added: usize,
    pub entries_skipped: usize,
}

impl Storage {
    pub fn export_to(
        &self,
        path: &Path,
        scope: ExportScope,
        _decrypt_sensitive: bool,
    ) -> Result<usize> {
        let conn = self.conn().lock().expect("storage mutex poisoned");
        let entries = if matches!(
            scope,
            ExportScope::HistoryOnly | ExportScope::SettingsAndHistory | ExportScope::All
        ) {
            let mut stmt = conn.prepare("SELECT id, content_type, content, html_content, source_app, source_app_path, timestamp, preview, is_pinned, use_count, is_external, pinned_order FROM clipboard_history ORDER BY is_pinned DESC, pinned_order ASC, timestamp DESC")?;
            let rows = stmt.query_map([], |row| {
                let id: i64 = row.get(0)?;
                let content_type: String = row.get(1)?;
                let content: String = row.get(2)?;
                let html_content: Option<String> = row.get(3)?;
                let source_app: String = row.get(4)?;
                let source_app_path: Option<String> = row.get(5)?;
                let timestamp: i64 = row.get(6)?;
                let preview: String = row.get(7)?;
                let is_pinned: bool = row.get::<_, i64>(8)? != 0;
                let use_count: i64 = row.get(9)?;
                let is_external: bool = row.get::<_, i64>(10)? != 0;
                let pinned_order: i64 = row.get(11)?;
                let mut tag_stmt = conn
                    .prepare("SELECT tag FROM entry_tags WHERE entry_id = ?1 ORDER BY tag")
                    .unwrap();
                let tags: Vec<String> = tag_stmt
                    .query_map(params![id], |r| r.get::<_, String>(0))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(ClipboardEntry {
                    id,
                    kind: crate::model::ClipboardKind::from(content_type.as_str()),
                    content,
                    html_content,
                    source_app,
                    source_app_path,
                    timestamp,
                    preview,
                    is_pinned,
                    tags,
                    use_count,
                    is_external,
                    pinned_order,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let tags = if matches!(scope, ExportScope::All) {
            let mut stmt = conn.prepare("SELECT name, color FROM saved_tags ORDER BY name")?;
            stmt.query_map([], |row| {
                Ok(Tag {
                    name: row.get(0)?,
                    color: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let settings: serde_json::Value = if matches!(
            scope,
            ExportScope::SettingsOnly | ExportScope::SettingsAndHistory | ExportScope::All
        ) {
            let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
            let map: HashMap<String, String> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<HashMap<_, _>>>()?;
            serde_json::to_value(&map).unwrap_or(serde_json::Value::Object(Default::default()))
        } else {
            serde_json::Value::Object(Default::default())
        };
        let emoji_favorites: Vec<PathBuf> = if matches!(scope, ExportScope::All) {
            use rusqlite::OptionalExtension;
            if let Ok(Some(raw)) = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1",
                    params!["app.emoji_favorites"],
                    |row| row.get::<_, String>(0),
                )
                .optional()
            {
                serde_json::from_str(&raw).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        drop(conn);
        let count = entries.len();
        let bundle = ExportBundle {
            schema_version: EXPORT_SCHEMA_VERSION,
            exported_at: chrono::Utc::now().timestamp(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            encryption_version: 0,
            settings,
            tags,
            entries,
            actions: Vec::new(),
            snippets: Vec::new(),
            emoji_favorites,
        };
        let file = File::create(path)
            .with_context(|| format!("Failed to create export file: {}", path.display()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &bundle)
            .context("Failed to write export bundle JSON")?;
        Ok(count)
    }

    pub fn import_from(&self, path: &Path, mode: ImportMode) -> Result<ImportStats> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open import file: {}", path.display()))?;
        let reader = BufReader::new(file);
        let bundle: ExportBundle = serde_json::from_reader(reader)
            .map_err(|err| anyhow!("Failed to parse bundle: {}", err))?;
        if bundle.schema_version > EXPORT_SCHEMA_VERSION {
            return Err(anyhow!(
                "Bundle schema version {} is newer than supported {}",
                bundle.schema_version,
                EXPORT_SCHEMA_VERSION
            ));
        }
        let mut stats = ImportStats::default();
        let mut conn = self.conn().lock().expect("storage mutex poisoned");
        let tx = conn.transaction()?;
        if mode == ImportMode::Replace {
            tx.execute("DELETE FROM entry_tags", [])?;
            tx.execute("DELETE FROM clipboard_history", [])?;
            tx.execute("DELETE FROM saved_tags", [])?;
            tx.execute("DELETE FROM settings", [])?;
        }
        if let serde_json::Value::Object(map) = &bundle.settings {
            let now = chrono::Local::now().timestamp_millis();
            for (key, value) in map {
                let json_str = value.to_string();
                tx.execute("INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at", params![key, json_str, now])?;
            }
        }
        for tag in &bundle.tags {
            tx.execute(
                "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2)",
                params![tag.name, tag.color],
            )?;
        }
        for entry in &bundle.entries {
            let kind_str = entry.kind.as_str();
            let hash = compute_content_hash(kind_str, &entry.content);
            let is_sensitive = entry.is_sensitive() as i64;
            if mode == ImportMode::Merge {
                let inserted = tx.execute("INSERT OR IGNORE INTO clipboard_history (content_type, content, html_content, source_app, source_app_path, timestamp, preview, is_pinned, use_count, is_external, pinned_order, content_hash, sensitive) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)", params![kind_str, entry.content, entry.html_content, entry.source_app, entry.source_app_path, entry.timestamp, entry.preview, entry.is_pinned as i64, entry.use_count, entry.is_external as i64, entry.pinned_order, hash, is_sensitive])?;
                if inserted == 0 {
                    stats.entries_skipped += 1;
                    continue;
                }
                stats.entries_added += 1;
            } else {
                tx.execute("INSERT INTO clipboard_history (content_type, content, html_content, source_app, source_app_path, timestamp, preview, is_pinned, use_count, is_external, pinned_order, content_hash, sensitive) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)", params![kind_str, entry.content, entry.html_content, entry.source_app, entry.source_app_path, entry.timestamp, entry.preview, entry.is_pinned as i64, entry.use_count, entry.is_external as i64, entry.pinned_order, hash, is_sensitive])?;
                stats.entries_added += 1;
            }
            let new_id = tx.last_insert_rowid();
            for tag_name in &entry.tags {
                tx.execute(
                    "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2)",
                    params![tag_name, "#4f46e5"],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    params![new_id, tag_name],
                )?;
            }
        }
        tx.commit()?;
        Ok(stats)
    }
}

fn compute_content_hash(kind: &str, content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ClipboardEntry;
    use crate::storage::Storage;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static DB_COUNTER: AtomicU64 = AtomicU64::new(0);
    fn temp_storage() -> Storage {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiez-slim-io-test-{}-{nanos}-{counter}.db",
            std::process::id()
        ));
        Storage::open(path).expect("open temp db")
    }
    fn temp_json_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "tiez-slim-io-test-{}-{nanos}-{counter}.json",
            std::process::id()
        ))
    }
    #[test]
    fn roundtrip_export_import_preserves_entry_count() {
        let storage = temp_storage();
        for (content, app) in &[
            ("hello world", "app1"),
            ("https://example.com", "app2"),
            ("let x = 1;", "app3"),
        ] {
            let entry = ClipboardEntry::captured_text(content.to_string(), app.to_string())
                .expect("valid entry");
            storage.save_entry(&entry).expect("save");
        }
        let path = temp_json_path();
        let exported = storage
            .export_to(&path, ExportScope::All, false)
            .expect("export");
        assert_eq!(exported, 3);
        let storage2 = temp_storage();
        let stats = storage2
            .import_from(&path, ImportMode::Merge)
            .expect("import");
        assert_eq!(stats.entries_added, 3);
        assert_eq!(stats.entries_skipped, 0);
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn merge_mode_skips_duplicates() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("duplicate me".to_string(), "app".to_string())
            .expect("valid entry");
        storage.save_entry(&entry).expect("save");
        let path = temp_json_path();
        storage
            .export_to(&path, ExportScope::All, false)
            .expect("export");
        let stats = storage
            .import_from(&path, ImportMode::Merge)
            .expect("import");
        assert_eq!(stats.entries_added, 0);
        assert_eq!(stats.entries_skipped, 1);
        let _ = std::fs::remove_file(&path);
    }
}
