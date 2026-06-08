use crate::actions::{Action, ActionKind};
use crate::encryption::SecureStore;
use crate::snippets::Snippet;
use crate::model::{
    ClipboardEntry, ClipboardEntrySummary, ClipboardKind, MAX_ENTRIES, RETENTION_DAYS,
    SelectionSource,
};
use anyhow::{Context, Result};
use chrono::{Duration, Local};
use lru::LruCache;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use rust_i18n::t;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const APP_DATA_DIR: &str = "tiez-slim-linux";
const LEGACY_DATA_DIR: &str = "myclipboard";
const ENCRYPTED_PREFIX: &str = "enc:v1:";
const DECRYPTED_CACHE_CAP: usize = 256;

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
    encryptor: Option<Arc<dyn SecureStore + Send + Sync>>,
    decrypted_cache: Arc<Mutex<LruCache<i64, String>>>,
}

impl Storage {
    pub fn default_path() -> PathBuf {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let path = base.join(APP_DATA_DIR).join("clipboard.db");
        let legacy_path = base.join(LEGACY_DATA_DIR).join("clipboard.db");
        if !path.exists() && legacy_path.exists() {
            legacy_path
        } else {
            path
        }
    }

    pub fn path_from_redirect_file() -> Option<PathBuf> {
        let data_dir = dirs::data_dir()?;
        let redirect = data_dir.join(APP_DATA_DIR).join("datapath.txt");
        let legacy_redirect = data_dir.join(LEGACY_DATA_DIR).join("datapath.txt");
        let value = std::fs::read_to_string(&redirect)
            .or_else(|_| std::fs::read_to_string(legacy_redirect))
            .ok()?;
        let path = PathBuf::from(value.trim());
        (!path.as_os_str().is_empty()).then_some(path)
    }

    pub fn write_redirect_path(path: PathBuf) -> Result<()> {
        let Some(parent) = path.parent() else {
            anyhow::bail!("{}", t!("storage_error.db_path_required"));
        };
        std::fs::create_dir_all(parent).with_context(|| {
            t!(
                "storage_error.create_db_dir_failed",
                path = parent.display()
            )
        })?;
        let config_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DATA_DIR);
        std::fs::create_dir_all(&config_dir).with_context(|| {
            t!(
                "storage_error.create_config_dir_failed",
                path = config_dir.display()
            )
        })?;
        std::fs::write(config_dir.join("datapath.txt"), path.display().to_string())
            .context(t!("storage_error.save_path_config_failed"))
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                t!(
                    "storage_error.create_data_dir_failed",
                    path = parent.display()
                )
            })?;
        }
        let conn = Connection::open(&path).context(t!("storage_error.connect_sqlite_failed"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -8_000)?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
            encryptor: None,
            decrypted_cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(DECRYPTED_CACHE_CAP).unwrap(),
            ))),
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn set_encryptor(&mut self, encryptor: Arc<dyn SecureStore + Send + Sync>) {
        self.encryptor = Some(encryptor);
    }

    pub fn has_encryptor(&self) -> bool {
        self.encryptor.is_some()
    }

    pub fn clear_decrypted_cache(&self) {
        if let Ok(mut cache) = self.decrypted_cache.lock() {
            cache.clear();
        }
    }

    pub fn encrypt_sensitive_content(&self, content: &str, is_sensitive: bool) -> Result<String> {
        if !is_sensitive {
            return Ok(content.to_string());
        }
        let Some(ref enc) = self.encryptor else {
            return Ok(content.to_string());
        };
        if content.starts_with(ENCRYPTED_PREFIX) {
            return Ok(content.to_string());
        }
        let ct = enc.encrypt(content.as_bytes())?;
        String::from_utf8(ct).context("encrypted content is not valid UTF-8")
    }

    pub fn decrypt_content(&self, id: i64, raw: &str) -> String {
        if !raw.starts_with(ENCRYPTED_PREFIX) {
            return raw.to_string();
        }
        {
            let Ok(mut cache) = self.decrypted_cache.lock() else {
                return raw.to_string();
            };
            if let Some(cached) = cache.get(&id) {
                return cached.clone();
            }
        }
        let Some(ref enc) = self.encryptor else {
            return raw.to_string();
        };
        let Ok(decrypted) = enc.decrypt(raw.as_bytes()) else {
            return raw.to_string();
        };
        let Ok(text) = String::from_utf8(decrypted) else {
            return raw.to_string();
        };
        if let Ok(mut cache) = self.decrypted_cache.lock() {
            cache.put(id, text.clone());
        }
        text
    }

    pub(crate) fn conn(&self) -> &std::sync::Mutex<rusqlite::Connection> {
        &self.conn
    }
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_type TEXT NOT NULL,
                content TEXT NOT NULL,
                html_content TEXT,
                source_app TEXT NOT NULL,
                source_app_path TEXT,
                timestamp INTEGER NOT NULL,
                preview TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                use_count INTEGER NOT NULL DEFAULT 0,
                is_external INTEGER NOT NULL DEFAULT 0,
                pinned_order INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT NOT NULL UNIQUE,
                sensitive INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS saved_tags (
                name TEXT PRIMARY KEY,
                color TEXT NOT NULL DEFAULT '#4f46e5'
            );
            CREATE TABLE IF NOT EXISTS entry_tags (
                entry_id INTEGER NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag),
                FOREIGN KEY(entry_id) REFERENCES clipboard_history(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_clipboard_pinned_time
                ON clipboard_history(is_pinned, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_clipboard_preview
                ON clipboard_history(preview);
            CREATE INDEX IF NOT EXISTS idx_clipboard_timestamp
                ON clipboard_history(timestamp);
            CREATE INDEX IF NOT EXISTS idx_entry_tags_tag
                ON entry_tags(tag);

            CREATE TABLE IF NOT EXISTS actions (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                pattern TEXT NOT NULL,
                command TEXT NOT NULL,
                icon TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                auto_trigger INTEGER NOT NULL DEFAULT 0,
                auto_trigger_primary INTEGER NOT NULL DEFAULT 0,
                toolbar_button INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                is_builtin INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_actions_enabled ON actions(enabled);
            CREATE INDEX IF NOT EXISTS idx_actions_sort ON actions(sort_order);

            CREATE TABLE IF NOT EXISTS snippets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                template TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                icon TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                use_count INTEGER NOT NULL DEFAULT 0,
                last_used_at INTEGER,
                created_at INTEGER NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_snippets_name ON snippets(name);
            CREATE INDEX IF NOT EXISTS idx_snippets_enabled ON snippets(enabled);",
        )?;
        ensure_column(&conn, "clipboard_history", "html_content", "TEXT")?;
        ensure_column(&conn, "clipboard_history", "source_app_path", "TEXT")?;
        ensure_column(&conn, "clipboard_history", "content_hash", "TEXT")?;
        ensure_column(
            &conn,
            "clipboard_history",
            "is_external",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &conn,
            "clipboard_history",
            "pinned_order",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &conn,
            "clipboard_history",
            "sensitive",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        ensure_column(
            &conn,
            "clipboard_history",
            "source",
            "TEXT NOT NULL DEFAULT 'clipboard'",
        )?;
        let backfill_done: bool = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'migration.sensitive_backfill_v1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|v| v == "1")
            .unwrap_or(false);
        if !backfill_done {
            conn.execute(
                "UPDATE clipboard_history
                 SET sensitive = 1
                 WHERE sensitive = 0
                   AND content_type IN ('text', 'url', 'code', 'rich_text')
                   AND (
                     content LIKE '%@%'
                     OR lower(content) LIKE '%password%'
                     OR lower(content) LIKE '%secret%'
                     OR lower(content) LIKE '%token%'
                   )",
                [],
            )?;
            conn.execute(
                "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                params![
                    "migration.sensitive_backfill_v1",
                    "1",
                    Local::now().timestamp_millis()
                ],
            )?;
        }
        conn.execute(
            "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2), (?3, ?4)",
            params!["sensitive", "#4f46e5", "密码", "#dc2626"],
        )?;
        Ok(())
    }

    pub fn save_entry(&self, entry: &ClipboardEntry) -> Result<i64> {
        self.save_entry_with_dedup(entry, true)
    }

    pub fn save_entry_with_dedup(&self, entry: &ClipboardEntry, deduplicate: bool) -> Result<i64> {
        let hash_input = if deduplicate {
            entry_hash_identity(entry)
        } else {
            format!(
                "{}\u{1f}{}\u{1f}{}",
                entry.timestamp,
                entry.content,
                entry.html_content.as_deref().unwrap_or("")
            )
        };
        let hash = content_hash(entry.kind.as_str(), entry.source.as_str(), &hash_input);
        let sensitive = entry.is_sensitive() as i64;
        let stored_content = self.encrypt_sensitive_content(&entry.content, entry.is_sensitive())?;
        let mut conn = self.conn.lock().expect("storage mutex poisoned");
        let tx = conn.transaction()?;

        let existing: Option<i64> = if deduplicate {
            tx.query_row(
                "SELECT id FROM clipboard_history WHERE content_hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()?
        } else {
            None
        };

        let id = if let Some(id) = existing {
            tx.execute(
                "UPDATE clipboard_history
                 SET timestamp = ?1, source_app = ?2, preview = ?3, content_type = ?4,
                     content = ?5, html_content = ?6, source_app_path = ?7, is_external = ?8,
                     sensitive = ?9, source = ?10
                 WHERE id = ?11",
                params![
                    entry.timestamp,
                    entry.source_app,
                    entry.preview,
                    entry.kind.as_str(),
                    stored_content,
                    entry.html_content,
                    entry.source_app_path,
                    entry.is_external as i64,
                    sensitive,
                    entry.source.as_str(),
                    id
                ],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO clipboard_history
                 (content_type, content, html_content, source_app, source_app_path, timestamp,
                  preview, is_external, pinned_order, content_hash, sensitive, source)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    entry.kind.as_str(),
                    stored_content,
                    entry.html_content,
                    entry.source_app,
                    entry.source_app_path,
                    entry.timestamp,
                    entry.preview,
                    entry.is_external as i64,
                    entry.pinned_order,
                    hash,
                    sensitive,
                    entry.source.as_str()
                ],
            )?;
            tx.last_insert_rowid()
        };

        tx.commit()?;
        drop(conn);
        self.enforce_limit()?;
        Ok(id)
    }

    #[allow(dead_code)]
    pub fn list(&self, query: &str) -> Result<Vec<ClipboardEntry>> {
        self.list_filtered(query, None, None)
    }

    #[allow(dead_code)]
    pub fn list_filtered(
        &self,
        query: &str,
        kind: Option<&ClipboardKind>,
        tag: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let (where_sql, values) = build_where_clause(query, kind, tag);
        let sql = format!(
            "SELECT h.id, h.content_type, h.content, h.html_content, h.source_app,
                h.source_app_path, h.timestamp, h.preview, h.is_pinned, h.use_count,
                h.is_external, h.pinned_order, h.source
             FROM clipboard_history h
             WHERE {where_sql}
             ORDER BY h.is_pinned DESC, h.pinned_order ASC, h.timestamp DESC LIMIT 300"
        );
        let mut entries = {
            let mut stmt = conn.prepare(&sql)?;
            stmt.query_map(params_from_iter(values.iter()), row_to_entry)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
        let tags_by_id = fetch_tags_batch(&conn, &ids)?;
        drop(conn);
        for entry in &mut entries {
            if let Some(tags) = tags_by_id.get(&entry.id) {
                entry.tags = tags.clone();
            }
            entry.content = self.decrypt_content(entry.id, &entry.content);
        }
        Ok(entries)
    }

    pub fn list_summaries_filtered(
        &self,
        query: &str,
        kind: Option<&ClipboardKind>,
        tag: Option<&str>,
    ) -> Result<Vec<ClipboardEntrySummary>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let (where_sql, values) = build_where_clause(query, kind, tag);
        let sql = format!(
            "SELECT h.id, h.content_type, h.source_app, h.source_app_path, h.timestamp,
                h.preview, h.is_pinned, h.use_count, h.is_external, h.pinned_order,
                h.sensitive, h.source
             FROM clipboard_history h
             WHERE {where_sql}
             ORDER BY h.is_pinned DESC, h.pinned_order ASC, h.timestamp DESC LIMIT 300"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut summaries = stmt
            .query_map(params_from_iter(values.iter()), row_to_summary)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let ids: Vec<i64> = summaries.iter().map(|s| s.id).collect();
        let tags_by_id = fetch_tags_batch(&conn, &ids)?;
        for summary in &mut summaries {
            if let Some(tags) = tags_by_id.get(&summary.id) {
                summary.tags = tags.clone();
            }
        }
        Ok(summaries)
    }

    pub fn list_all_summaries(&self) -> Result<Vec<ClipboardEntrySummary>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let sql = "SELECT h.id, h.content_type, h.source_app, h.source_app_path, h.timestamp,
                h.preview, h.is_pinned, h.use_count, h.is_external, h.pinned_order,
                h.sensitive, h.source
             FROM clipboard_history h
             ORDER BY h.is_pinned DESC, h.pinned_order ASC, h.timestamp DESC LIMIT 300";
        let mut stmt = conn.prepare(sql)?;
        let mut summaries = stmt
            .query_map([], row_to_summary)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let ids: Vec<i64> = summaries.iter().map(|s| s.id).collect();
        let tags_by_id = fetch_tags_batch(&conn, &ids)?;
        for summary in &mut summaries {
            if let Some(tags) = tags_by_id.get(&summary.id) {
                summary.tags = tags.clone();
            }
        }
        Ok(summaries)
    }

    pub fn get_entry(&self, id: i64) -> Result<Option<ClipboardEntry>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut entry = {
            let mut stmt = conn.prepare(
                "SELECT id, content_type, content, html_content, source_app, source_app_path,
                    timestamp, preview, is_pinned, use_count, is_external, pinned_order, source
                 FROM clipboard_history WHERE id = ?1",
            )?;
            match stmt.query_row(params![id], row_to_entry).optional()? {
                Some(entry) => entry,
                None => return Ok(None),
            }
        };
        let tags_by_id = fetch_tags_batch(&conn, &[entry.id])?;
        drop(conn);
        entry.content = self.decrypt_content(entry.id, &entry.content);
        if let Some(tags) = tags_by_id.get(&entry.id) {
            entry.tags = tags.clone();
        }
        Ok(Some(entry))
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_unpinned(&self) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM clipboard_history WHERE is_pinned = 0", [])?;
        Ok(())
    }

    pub fn toggle_pin(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE clipboard_history
             SET is_pinned = CASE is_pinned WHEN 0 THEN 1 ELSE 0 END
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, Local::now().timestamp_millis()],
        )?;
        Ok(())
    }

    pub fn set_tags(&self, id: i64, tags: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().expect("storage mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM entry_tags WHERE entry_id = ?1", params![id])?;
        for tag in tags {
            tx.execute(
                "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                params![id, tag],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2)",
                params![tag, "#4f46e5"],
            )?;
        }
        // Recompute cached sensitive so UI masking tracks the tag set.
        let (raw_content, content_type): (String, String) = tx
            .query_row(
                "SELECT content, content_type FROM clipboard_history WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .unwrap_or_default();
        let content = self.decrypt_content(id, &raw_content);
        let kind = ClipboardKind::from(content_type.as_str());
        let tagged_sensitive = tags.iter().any(|tag| {
            let tag = tag.to_ascii_lowercase();
            tag == "sensitive" || tag == "密码" || tag == "password" || tag == "secret"
        });
        let content_sensitive = matches!(
            kind,
            ClipboardKind::Text
                | ClipboardKind::Url
                | ClipboardKind::Code
                | ClipboardKind::RichText
        ) && crate::model::looks_sensitive(&content);
        let is_sensitive = (tagged_sensitive || content_sensitive) as i64;
        tx.execute(
            "UPDATE clipboard_history SET sensitive = ?1 WHERE id = ?2",
            params![is_sensitive, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn saved_tags(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare("SELECT name FROM saved_tags ORDER BY name")?;
        Ok(stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_saved_tag(&self, name: &str) -> Result<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT OR IGNORE INTO saved_tags (name, color) VALUES (?1, ?2)",
            params![trimmed, "#4f46e5"],
        )?;
        Ok(())
    }

    pub fn delete_saved_tag(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM saved_tags WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn saved_tag_color(&self, name: &str) -> Result<String> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare("SELECT color FROM saved_tags WHERE name = ?1")?;
        let result = stmt.query_row(params![name], |row| row.get::<_, String>(0));
        match result {
            Ok(color) => Ok(color),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok("#4f46e5".to_string()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update_saved_tag_color(&self, name: &str, color: &str) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE saved_tags SET color = ?1 WHERE name = ?2",
            params![color, name],
        )?;
        Ok(())
    }

    pub fn count_entries_for_tag(&self, tag: &str) -> Result<usize> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let count: usize = conn.query_row(
            "SELECT COUNT(DISTINCT entry_id) FROM entry_tags WHERE tag = ?1",
            params![tag],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn mark_used(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE clipboard_history
             SET use_count = use_count + 1, timestamp = ?1
             WHERE id = ?2",
            params![Local::now().timestamp_millis(), id],
        )?;
        Ok(())
    }

    pub fn increment_use_count(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE clipboard_history SET use_count = use_count + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn cleanup_expired(&self) -> Result<()> {
        let cutoff = (Local::now() - Duration::days(RETENTION_DAYS)).timestamp_millis();
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "DELETE FROM clipboard_history WHERE is_pinned = 0 AND timestamp < ?1",
            params![cutoff],
        )?;
        Ok(())
    }

    fn enforce_limit(&self) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let unpinned_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE is_pinned = 0",
            [],
            |row| row.get(0),
        )?;
        if unpinned_count <= MAX_ENTRIES as i64 {
            return Ok(());
        }
        conn.execute(
            "DELETE FROM clipboard_history
             WHERE id IN (
                SELECT id FROM clipboard_history
                WHERE is_pinned = 0
                ORDER BY timestamp DESC
                LIMIT -1 OFFSET ?1
             )",
            params![MAX_ENTRIES as i64],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    fn tags_for_entry_locked(&self, conn: &Connection, id: i64) -> Result<Vec<String>> {
        let mut stmt =
            conn.prepare("SELECT tag FROM entry_tags WHERE entry_id = ?1 ORDER BY tag")?;
        Ok(stmt
            .query_map(params![id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn save_action(&self, action: &Action) -> Result<i64> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "INSERT OR REPLACE INTO actions
             (id, name, kind, pattern, command, icon, enabled,
              auto_trigger, auto_trigger_primary, toolbar_button,
              sort_order, is_builtin, created_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                action.id as i64,
                action.name,
                serde_json::to_string(&action.kind).unwrap_or_default(),
                action.pattern,
                action.command,
                action.icon,
                action.enabled as i64,
                action.auto_trigger as i64,
                action.auto_trigger_primary as i64,
                action.toolbar_button as i64,
                action.sort_order,
                action.is_builtin as i64,
                action.created_at,
                action.last_used_at,
            ],
        )?;
        Ok(action.id as i64)
    }

    pub fn load_actions(&self) -> Result<Vec<Action>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, kind, pattern, command, icon, enabled,
                    auto_trigger, auto_trigger_primary, toolbar_button,
                    sort_order, is_builtin, created_at, last_used_at
             FROM actions ORDER BY sort_order ASC, id ASC",
        )?;
        let actions = stmt
            .query_map([], |row| {
                let kind_str: String = row.get(2)?;
                let kind: ActionKind =
                    serde_json::from_str(&kind_str).unwrap_or(ActionKind::ShellCommand);
                Ok(Action {
                    id: row.get::<_, i64>(0)? as u64,
                    name: row.get(1)?,
                    kind,
                    pattern: row.get(3)?,
                    command: row.get(4)?,
                    icon: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? != 0,
                    auto_trigger: row.get::<_, i64>(7)? != 0,
                    auto_trigger_primary: row.get::<_, i64>(8)? != 0,
                    toolbar_button: row.get::<_, i64>(9)? != 0,
                    sort_order: row.get(10)?,
                    is_builtin: row.get::<_, i64>(11)? != 0,
                    created_at: row.get(12)?,
                    last_used_at: row.get(13)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(actions)
    }

    pub fn delete_action(&self, id: u64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM actions WHERE id = ?1", params![id as i64])?;
        Ok(())
    }

    pub fn update_action_last_used(&self, id: u64, at: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "UPDATE actions SET last_used_at = ?1 WHERE id = ?2",
            params![at, id as i64],
        )?;
        Ok(())
    }

    pub fn save_snippet(&self, snippet: &Snippet) -> Result<i64> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let tags_json = serde_json::to_string(&snippet.tags).unwrap_or_else(|_| "[]".into());
        if snippet.id == 0 {
            conn.execute(
                "INSERT INTO snippets
                 (name, template, description, icon, enabled, use_count, last_used_at, created_at, tags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    snippet.name,
                    snippet.template,
                    snippet.description,
                    snippet.icon,
                    snippet.enabled as i64,
                    snippet.use_count,
                    snippet.last_used_at,
                    snippet.created_at,
                    tags_json,
                ],
            )?;
            Ok(conn.last_insert_rowid())
        } else {
            conn.execute(
                "INSERT OR REPLACE INTO snippets
                 (id, name, template, description, icon, enabled, use_count, last_used_at, created_at, tags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    snippet.id,
                    snippet.name,
                    snippet.template,
                    snippet.description,
                    snippet.icon,
                    snippet.enabled as i64,
                    snippet.use_count,
                    snippet.last_used_at,
                    snippet.created_at,
                    tags_json,
                ],
            )?;
            Ok(snippet.id)
        }
    }

    pub fn load_snippets(&self) -> Result<Vec<Snippet>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, name, template, description, icon, enabled,
                    use_count, last_used_at, created_at, tags
             FROM snippets ORDER BY use_count DESC, name ASC",
        )?;
        let snippets = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(9)?;
                let tags: Vec<String> =
                    serde_json::from_str(&tags_str).unwrap_or_default();
                Ok(Snippet {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    template: row.get(2)?,
                    description: row.get(3)?,
                    icon: row.get(4)?,
                    enabled: row.get::<_, i64>(5)? != 0,
                    use_count: row.get(6)?,
                    last_used_at: row.get(7)?,
                    created_at: row.get(8)?,
                    tags,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(snippets)
    }

    pub fn delete_snippet(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn increment_snippet_use_count(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE snippets SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }
}

fn build_where_clause(
    query: &str,
    kind: Option<&ClipboardKind>,
    tag: Option<&str>,
) -> (String, Vec<String>) {
    let mut sql = String::from("1 = 1");
    let mut values = Vec::new();

    let trimmed_query = query.trim();
    if !trimmed_query.is_empty() {
        sql.push_str(
            " AND (h.content LIKE ? OR h.preview LIKE ? OR h.source_app LIKE ? OR EXISTS (\
             SELECT 1 FROM entry_tags qt WHERE qt.entry_id = h.id AND qt.tag LIKE ?))",
        );
        let like = format!("%{trimmed_query}%");
        values.extend([like.clone(), like.clone(), like.clone(), like]);
    }
    if let Some(kind) = kind {
        sql.push_str(" AND h.content_type = ?");
        values.push(kind.as_str().to_string());
    }
    if let Some(tag) = tag {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM entry_tags ft WHERE ft.entry_id = h.id AND ft.tag = ?)",
        );
        values.push(tag.to_string());
    }
    (sql, values)
}

fn fetch_tags_batch(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, Vec<String>>> {
    let mut tags_by_id: HashMap<i64, Vec<String>> = HashMap::new();
    if ids.is_empty() {
        return Ok(tags_by_id);
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT entry_id, tag FROM entry_tags WHERE entry_id IN ({placeholders}) ORDER BY tag"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(ids.iter()), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (entry_id, tag) = row?;
        tags_by_id.entry(entry_id).or_default().push(tag);
    }
    Ok(tags_by_id)
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEntry> {
    Ok(ClipboardEntry {
        id: row.get(0)?,
        kind: ClipboardKind::from(row.get::<_, String>(1)?.as_str()),
        content: row.get(2)?,
        html_content: row.get(3)?,
        source_app: row.get(4)?,
        source_app_path: row.get(5)?,
        timestamp: row.get(6)?,
        preview: row.get(7)?,
        is_pinned: row.get::<_, i64>(8)? != 0,
        tags: Vec::new(),
        use_count: row.get(9)?,
        is_external: row.get::<_, i64>(10)? != 0,
        pinned_order: row.get(11)?,
        source: SelectionSource::from(
            row.get::<_, Option<String>>(12)?
                .unwrap_or_default()
                .as_str(),
        ),
    })
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEntrySummary> {
    Ok(ClipboardEntrySummary {
        id: row.get(0)?,
        kind: ClipboardKind::from(row.get::<_, String>(1)?.as_str()),
        source_app: row.get(2)?,
        source_app_path: row.get(3)?,
        timestamp: row.get(4)?,
        preview: row.get(5)?,
        is_pinned: row.get::<_, i64>(6)? != 0,
        tags: Vec::new(),
        use_count: row.get(7)?,
        is_external: row.get::<_, i64>(8)? != 0,
        pinned_order: row.get(9)?,
        sensitive: row.get::<_, i64>(10)? != 0,
        source: SelectionSource::from(
            row.get::<_, Option<String>>(11)?
                .unwrap_or_default()
                .as_str(),
        ),
    })
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !columns.iter().any(|existing| existing == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn content_hash(kind: &str, source: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(source.as_bytes());
    hasher.update([0]);
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn entry_hash_identity(entry: &ClipboardEntry) -> String {
    if matches!(entry.kind, ClipboardKind::RichText) {
        format!(
            "{}\u{1f}{}",
            entry.content,
            entry.html_content.as_deref().unwrap_or("")
        )
    } else {
        entry.content.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            "tiez-slim-linux-test-{}-{nanos}-{counter}.db",
            std::process::id()
        ));
        Storage::open(path).expect("open temp db")
    }

    #[test]
    fn saving_same_content_deduplicates() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("hello".to_string(), "test".to_string())
            .expect("valid entry");
        storage.save_entry(&entry).expect("first save");
        storage.save_entry(&entry).expect("second save");

        let entries = storage.list("").expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "hello");
    }

    #[test]
    fn save_entry_can_preserve_duplicates_when_dedup_disabled() {
        let storage = temp_storage();
        let mut entry = ClipboardEntry::captured_text("repeat".to_string(), "test".to_string())
            .expect("valid entry");
        storage
            .save_entry_with_dedup(&entry, false)
            .expect("first save");
        entry.timestamp += 1;
        storage
            .save_entry_with_dedup(&entry, false)
            .expect("second save");

        let entries = storage.list("repeat").expect("list");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn clear_keeps_pinned_entries() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("pin me".to_string(), "test".to_string())
            .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");
        storage.toggle_pin(id).expect("pin");
        storage.clear_unpinned().expect("clear");

        let entries = storage.list("").expect("list");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_pinned);
    }

    #[test]
    fn tags_can_be_saved_and_searched() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("tagged text".to_string(), "test".to_string())
            .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");
        storage
            .set_tags(id, &["代码".to_string(), "todo".to_string()])
            .expect("set tags");

        let entries = storage.list("代码").expect("search by tag");
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].tags,
            vec!["todo".to_string(), "代码".to_string()]
        );
    }

    #[test]
    fn list_filtered_matches_kind_and_tag() {
        let storage = temp_storage();
        let url =
            ClipboardEntry::captured_text("https://example.com".to_string(), "test".to_string())
                .expect("valid url");
        let text = ClipboardEntry::captured_text("plain text".to_string(), "test".to_string())
            .expect("valid text");
        let url_id = storage.save_entry(&url).expect("save url");
        storage.save_entry(&text).expect("save text");
        storage
            .set_tags(url_id, &["work".to_string()])
            .expect("set url tag");

        let urls = storage
            .list_filtered("", Some(&ClipboardKind::Url), None)
            .expect("filter url");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].kind, ClipboardKind::Url);

        let tagged_urls = storage
            .list_filtered("", Some(&ClipboardKind::Url), Some("work"))
            .expect("filter url tag");
        assert_eq!(tagged_urls.len(), 1);
        assert_eq!(tagged_urls[0].content, "https://example.com");

        let missing = storage
            .list_filtered("", Some(&ClipboardKind::Text), Some("work"))
            .expect("filter impossible combination");
        assert!(missing.is_empty());
    }

    #[test]
    fn saved_tags_can_be_added_and_removed_without_entry_tag_loss() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("catalog entry".to_string(), "test".to_string())
            .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");
        storage.add_saved_tag("later").expect("add saved tag");
        assert!(
            storage
                .saved_tags()
                .expect("list tags")
                .contains(&"later".to_string())
        );

        storage
            .set_tags(id, &["later".to_string()])
            .expect("set entry tag");
        storage.delete_saved_tag("later").expect("delete saved tag");
        assert!(
            !storage
                .saved_tags()
                .expect("list tags")
                .contains(&"later".to_string())
        );

        let entries = storage.list("later").expect("search entry tag");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tags, vec!["later".to_string()]);
    }

    #[test]
    fn settings_round_trip_supports_tiez_slim_linux_state_keys() {
        let storage = temp_storage();
        storage
            .set_setting("app.emoji_favorites", "[\"😀\",\"/tmp/sticker.png\"]")
            .expect("set emoji favorites");
        storage
            .set_setting("ui.tiez_slim_linux", "{\"emoji_panel_enabled\":true}")
            .expect("set preferences");

        assert_eq!(
            storage
                .get_setting("app.emoji_favorites")
                .expect("get emoji favorites"),
            Some("[\"😀\",\"/tmp/sticker.png\"]".to_string())
        );
        assert_eq!(
            storage
                .get_setting("ui.tiez_slim_linux")
                .expect("get prefs"),
            Some("{\"emoji_panel_enabled\":true}".to_string())
        );
    }

    #[test]
    fn image_entries_are_persisted_and_filtered() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_image(
            "data:image/png;base64,iVBORw0KGgo=".to_string(),
            "test".to_string(),
        )
        .expect("valid image entry");
        storage.save_entry(&entry).expect("save image");

        let images = storage
            .list_filtered("", Some(&ClipboardKind::Image), None)
            .expect("filter images");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].kind, ClipboardKind::Image);
        assert!(images[0].content.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn rich_text_entries_preserve_html_content() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_rich_text(
            "Hello world".to_string(),
            "<p>Hello <strong>world</strong></p>".to_string(),
            "test".to_string(),
        )
        .expect("valid rich text entry");
        storage.save_entry(&entry).expect("save rich text");

        let rich = storage
            .list_filtered("", Some(&ClipboardKind::RichText), None)
            .expect("filter rich text");
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].content, "Hello world");
        assert_eq!(
            rich[0].html_content.as_deref(),
            Some("<p>Hello <strong>world</strong></p>")
        );
    }

    #[test]
    fn rich_text_dedup_keeps_different_html_variants() {
        let storage = temp_storage();
        let first = ClipboardEntry::captured_rich_text(
            "Hello".to_string(),
            "<p><strong>Hello</strong></p>".to_string(),
            "test".to_string(),
        )
        .expect("valid first rich text");
        let second = ClipboardEntry::captured_rich_text(
            "Hello".to_string(),
            "<p><em>Hello</em></p>".to_string(),
            "test".to_string(),
        )
        .expect("valid second rich text");
        storage.save_entry(&first).expect("save first");
        storage.save_entry(&second).expect("save second");

        let rich = storage
            .list_filtered("", Some(&ClipboardKind::RichText), None)
            .expect("filter rich text");
        assert_eq!(rich.len(), 2);
        assert!(rich
            .iter()
            .any(|entry| entry.html_content.as_deref() == Some("<p><strong>Hello</strong></p>")));
        assert!(
            rich.iter()
                .any(|entry| entry.html_content.as_deref() == Some("<p><em>Hello</em></p>"))
        );
    }

    #[test]
    fn file_entries_are_external_and_filterable() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_files(
            vec!["/tmp/a.txt".to_string(), "/tmp/b.txt".to_string()],
            "test".to_string(),
        )
        .expect("valid file entry");
        storage.save_entry(&entry).expect("save file");

        let files = storage
            .list_filtered("", Some(&ClipboardKind::File), None)
            .expect("filter files");
        assert_eq!(files.len(), 1);
        assert!(files[0].is_external);
        assert_eq!(files[0].content, "/tmp/a.txt\n/tmp/b.txt");
    }

    #[test]
    fn increment_use_count_does_not_move_entry_to_top() {
        let storage = temp_storage();
        let older = ClipboardEntry::captured_text("older".to_string(), "test".to_string())
            .expect("valid older");
        let mut newer = ClipboardEntry::captured_text("newer".to_string(), "test".to_string())
            .expect("valid newer");
        newer.timestamp = older.timestamp + 1;
        let older_id = storage.save_entry(&older).expect("save older");
        storage.save_entry(&newer).expect("save newer");
        storage
            .increment_use_count(older_id)
            .expect("increment older");

        let entries = storage.list("").expect("list");
        assert_eq!(entries[0].content, "newer");
        assert_eq!(entries[1].content, "older");
        assert_eq!(entries[1].use_count, 1);
    }

    #[test]
    fn deleting_entry_removes_tag_links() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("delete tagged".to_string(), "test".to_string())
            .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");
        storage
            .set_tags(id, &["gone".to_string()])
            .expect("set tags");
        storage.delete(id).expect("delete");

        let entries = storage.list("gone").expect("search deleted tag");
        assert!(entries.is_empty());
    }

    #[test]
    fn old_schema_is_extended_on_open() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiez-slim-linux-old-schema-{}-{nanos}.db",
            std::process::id()
        ));
        {
            let conn = Connection::open(&path).expect("create old db");
            conn.execute_batch(
                "CREATE TABLE clipboard_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    content TEXT NOT NULL,
                    source_app TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    preview TEXT NOT NULL,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    use_count INTEGER NOT NULL DEFAULT 0,
                    content_hash TEXT NOT NULL UNIQUE
                );",
            )
            .expect("create old schema");
        }

        let storage = Storage::open(path).expect("open migrates old schema");
        let entry =
            ClipboardEntry::captured_text("https://example.com".to_string(), "test".to_string())
                .expect("valid entry");
        storage.save_entry(&entry).expect("save after migrate");
        let entries = storage.list("example").expect("list after migrate");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, ClipboardKind::Url);
    }

    #[test]
    fn list_summaries_filtered_returns_summaries_without_content() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("hello world".to_string(), "test".to_string())
            .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");

        let summaries = storage
            .list_summaries_filtered("", Some(&ClipboardKind::Text), None)
            .expect("list summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, id);
        assert_eq!(summaries[0].kind, ClipboardKind::Text);
        assert_eq!(summaries[0].preview, "hello world");
        assert!(!summaries[0].is_sensitive());
    }

    #[test]
    fn list_summaries_filtered_persists_sensitive_flag() {
        let storage = temp_storage();
        let entry = ClipboardEntry::captured_text("13812345678".to_string(), "test".to_string())
            .expect("valid entry");
        storage.save_entry(&entry).expect("save");

        let summaries = storage
            .list_summaries_filtered("", None, None)
            .expect("list summaries");
        assert_eq!(summaries.len(), 1);
        assert!(
            summaries[0].is_sensitive(),
            "phone-number content should be marked sensitive on save"
        );
    }

    #[test]
    fn get_entry_returns_full_entry_with_content_and_tags() {
        let storage = temp_storage();
        let entry =
            ClipboardEntry::captured_text("lazy loaded text".to_string(), "test".to_string())
                .expect("valid entry");
        let id = storage.save_entry(&entry).expect("save");
        storage
            .set_tags(id, &["work".to_string()])
            .expect("set tag");

        let loaded = storage
            .get_entry(id)
            .expect("get entry")
            .expect("entry exists");
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.content, "lazy loaded text");
        assert_eq!(loaded.tags, vec!["work".to_string()]);
        assert_eq!(loaded.kind, ClipboardKind::Text);

        let missing = storage.get_entry(9_999_999).expect("get missing");
        assert!(missing.is_none());
    }

    #[test]
    fn test_actions_crud() {
        let storage = temp_storage();

        let mut action = Action::new("Open URL", "^https?://", "xdg-open %1");
        action.kind = ActionKind::Open;
        action.icon = "🌐".to_string();
        action.sort_order = 1;
        let id = storage.save_action(&action).expect("save action");
        assert_eq!(id, action.id as i64);

        let actions = storage.load_actions().expect("load actions");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Open URL");
        assert_eq!(actions[0].kind, ActionKind::Open);
        assert_eq!(actions[0].icon, "🌐");
        assert_eq!(actions[0].sort_order, 1);
        assert!(actions[0].enabled);
        assert!(actions[0].last_used_at.is_none());

        storage
            .update_action_last_used(action.id, 1700000000)
            .expect("update last used");
        let actions = storage.load_actions().expect("load after update");
        assert_eq!(actions[0].last_used_at, Some(1700000000));

        storage.delete_action(action.id).expect("delete action");
        let actions = storage.load_actions().expect("load after delete");
        assert!(actions.is_empty());
    }

    #[test]
    fn test_actions_save_upsert() {
        let storage = temp_storage();

        let mut action = Action::new("Test", "pattern", "cmd %1");
        action.sort_order = 0;
        storage.save_action(&action).expect("first save");

        action.name = "Updated Test".to_string();
        action.sort_order = 5;
        storage.save_action(&action).expect("upsert");

        let actions = storage.load_actions().expect("load");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Updated Test");
        assert_eq!(actions[0].sort_order, 5);
    }

    #[test]
    fn test_actions_ordering() {
        let storage = temp_storage();

        let mut a = Action::new("B", "b", "cmd");
        a.sort_order = 2;
        a.id = 2;
        let mut b = Action::new("A", "a", "cmd");
        b.sort_order = 1;
        b.id = 1;
        let mut c = Action::new("C", "c", "cmd");
        c.sort_order = 1;
        c.id = 3;

        storage.save_action(&a).expect("save a");
        storage.save_action(&b).expect("save b");
        storage.save_action(&c).expect("save c");

        let actions = storage.load_actions().expect("load");
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0].name, "A");
        assert_eq!(actions[1].name, "C");
        assert_eq!(actions[2].name, "B");
    }

    #[test]
    fn test_actions_migration_creates_table() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tiez-slim-linux-actions-migration-{}-{nanos}.db",
            std::process::id()
        ));
        {
            let conn = Connection::open(&path).expect("create old db");
            conn.execute_batch(
                "CREATE TABLE clipboard_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    content TEXT NOT NULL,
                    source_app TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    preview TEXT NOT NULL,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    use_count INTEGER NOT NULL DEFAULT 0,
                    content_hash TEXT NOT NULL UNIQUE
                );",
            )
            .expect("create old schema");
        }

        let storage = Storage::open(path).expect("open migrates old schema");
        let action = Action::new("Migrated", "test", "echo %1");
        storage.save_action(&action).expect("save after migrate");
        let actions = storage.load_actions().expect("load after migrate");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Migrated");
    }

    #[test]
    fn test_source_migration() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tiez-slim-linux-source-migration-{}-{nanos}-{counter}.db",
            std::process::id()
        ));
        {
            let conn = Connection::open(&path).expect("create old db");
            conn.execute_batch(
                "CREATE TABLE clipboard_history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_type TEXT NOT NULL,
                    content TEXT NOT NULL,
                    source_app TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    preview TEXT NOT NULL,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    use_count INTEGER NOT NULL DEFAULT 0,
                    content_hash TEXT NOT NULL UNIQUE
                );",
            )
            .expect("create old schema");
        }

        let storage = Storage::open(path).expect("open migrates old schema");

        {
            let conn = storage.conn.lock().expect("poisoned");
            let mut stmt = conn
                .prepare("PRAGMA table_info(clipboard_history)")
                .expect("prepare");
            let columns: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query")
                .filter_map(|r| r.ok())
                .collect();
            assert!(
                columns.contains(&"source".to_string()),
                "source column should exist after migration"
            );
        }

        let entry_clip =
            ClipboardEntry::captured_text("clipboard text".to_string(), "test".to_string())
                .expect("valid entry");
        storage.save_entry(&entry_clip).expect("save clipboard");

        let entry_primary = ClipboardEntry::captured_text_with_source(
            "primary text".to_string(),
            "test".to_string(),
            Some(SelectionSource::Primary),
        )
        .expect("valid entry");
        storage.save_entry(&entry_primary).expect("save primary");

        let entries = storage.list("").expect("list all");
        assert_eq!(entries.len(), 2);
        let clip_entry = entries
            .iter()
            .find(|e| e.content == "clipboard text")
            .expect("find clipboard entry");
        assert_eq!(clip_entry.source, SelectionSource::Clipboard);
        let primary_entry = entries
            .iter()
            .find(|e| e.content == "primary text")
            .expect("find primary entry");
        assert_eq!(primary_entry.source, SelectionSource::Primary);
    }

    #[test]
    fn test_source_coexist_dedup() {
        let storage = temp_storage();

        let entry_clip =
            ClipboardEntry::captured_text("same content".to_string(), "test".to_string())
                .expect("valid entry");
        storage.save_entry(&entry_clip).expect("save clipboard");

        let entry_primary = ClipboardEntry::captured_text_with_source(
            "same content".to_string(),
            "test".to_string(),
            Some(SelectionSource::Primary),
        )
        .expect("valid entry");
        storage.save_entry(&entry_primary).expect("save primary");

        let entries = storage.list("").expect("list all");
        assert_eq!(
            entries.len(),
            2,
            "same content from different sources should coexist"
        );

        storage
            .save_entry(&entry_clip)
            .expect("save clipboard again");
        let entries = storage.list("").expect("list after re-save");
        assert_eq!(
            entries.len(),
            2,
            "re-saving clipboard should deduplicate within same source"
        );
    }

    #[test]
    fn test_source_persisted_in_summaries() {
        let storage = temp_storage();

        let entry = ClipboardEntry::captured_text_with_source(
            "summary test".to_string(),
            "test".to_string(),
            Some(SelectionSource::Primary),
        )
        .expect("valid entry");
        storage.save_entry(&entry).expect("save");

        let summaries = storage
            .list_summaries_filtered("", None, None)
            .expect("list summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].source, SelectionSource::Primary);
    }

    #[test]
    fn test_snippets_crud() {
        let storage = temp_storage();

        let mut snippet = Snippet::new("GitHub PR", "https://github.com/{{org}}/{{repo}}/pull/{{num}}");
        snippet.description = "Open a PR link".to_string();
        snippet.icon = "🔗".to_string();
        snippet.tags = vec!["dev".to_string(), "github".to_string()];
        let id = storage.save_snippet(&snippet).expect("save snippet");
        assert!(id > 0);

        let snippets = storage.load_snippets().expect("load snippets");
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "GitHub PR");
        assert_eq!(snippets[0].template, "https://github.com/{{org}}/{{repo}}/pull/{{num}}");
        assert_eq!(snippets[0].description, "Open a PR link");
        assert_eq!(snippets[0].icon, "🔗");
        assert!(snippets[0].enabled);
        assert_eq!(snippets[0].use_count, 0);
        assert!(snippets[0].last_used_at.is_none());
        assert_eq!(snippets[0].tags, vec!["dev".to_string(), "github".to_string()]);

        storage.increment_snippet_use_count(id).expect("increment");
        let snippets = storage.load_snippets().expect("load after increment");
        assert_eq!(snippets[0].use_count, 1);
        assert!(snippets[0].last_used_at.is_some());

        storage.delete_snippet(id).expect("delete snippet");
        let snippets = storage.load_snippets().expect("load after delete");
        assert!(snippets.is_empty());
    }

    #[test]
    fn test_snippets_ordering() {
        let storage = temp_storage();

        let mut a = Snippet::new("Alpha", "aaa");
        a.use_count = 5;
        storage.save_snippet(&a).expect("save a");

        let mut b = Snippet::new("Beta", "bbb");
        b.use_count = 10;
        storage.save_snippet(&b).expect("save b");

        let mut c = Snippet::new("Gamma", "ccc");
        c.use_count = 10;
        storage.save_snippet(&c).expect("save c");

        let snippets = storage.load_snippets().expect("load");
        assert_eq!(snippets.len(), 3);
        assert_eq!(snippets[0].name, "Beta");
        assert_eq!(snippets[1].name, "Gamma");
        assert_eq!(snippets[2].name, "Alpha");
    }

    #[test]
    fn test_snippets_upsert() {
        let storage = temp_storage();

        let mut snippet = Snippet::new("Test", "hello");
        let id = storage.save_snippet(&snippet).expect("first save");

        snippet.id = id;
        snippet.name = "Updated Test".to_string();
        snippet.template = "world".to_string();
        storage.save_snippet(&snippet).expect("upsert");

        let snippets = storage.load_snippets().expect("load");
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].name, "Updated Test");
        assert_eq!(snippets[0].template, "world");
    }
}
