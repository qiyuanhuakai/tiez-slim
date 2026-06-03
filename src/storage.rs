use crate::model::{ClipboardEntry, ClipboardKind, MAX_ENTRIES, RETENTION_DAYS};
use anyhow::{Context, Result};
use chrono::{Duration, Local};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const APP_DATA_DIR: &str = "tiez-slim-linux";
const LEGACY_DATA_DIR: &str = "myclipboard";

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
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
            anyhow::bail!("数据库路径必须包含目录");
        };
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建数据库目录失败: {}", parent.display()))?;
        let config_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(APP_DATA_DIR);
        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("创建配置目录失败: {}", config_dir.display()))?;
        std::fs::write(config_dir.join("datapath.txt"), path.display().to_string())
            .context("保存数据库路径配置失败")
    }

    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建数据目录失败: {}", parent.display()))?;
        }
        let conn = Connection::open(&path).context("连接 SQLite 失败")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -8_000)?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
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
                content_hash TEXT NOT NULL UNIQUE
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
            CREATE INDEX IF NOT EXISTS idx_entry_tags_tag
                ON entry_tags(tag);",
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
        let hash = content_hash(entry.kind.as_str(), &hash_input);
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
                     html_content = ?5, source_app_path = ?6, is_external = ?7
                 WHERE id = ?8",
                params![
                    entry.timestamp,
                    entry.source_app,
                    entry.preview,
                    entry.kind.as_str(),
                    entry.html_content,
                    entry.source_app_path,
                    entry.is_external as i64,
                    id
                ],
            )?;
            id
        } else {
            tx.execute(
                "INSERT INTO clipboard_history
                 (content_type, content, html_content, source_app, source_app_path, timestamp,
                  preview, is_external, pinned_order, content_hash)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    entry.kind.as_str(),
                    entry.content,
                    entry.html_content,
                    entry.source_app,
                    entry.source_app_path,
                    entry.timestamp,
                    entry.preview,
                    entry.is_external as i64,
                    entry.pinned_order,
                    hash
                ],
            )?;
            tx.last_insert_rowid()
        };

        tx.commit()?;
        drop(conn);
        self.enforce_limit()?;
        Ok(id)
    }

    pub fn list(&self, query: &str) -> Result<Vec<ClipboardEntry>> {
        self.list_filtered(query, None, None)
    }

    pub fn list_filtered(
        &self,
        query: &str,
        kind: Option<&ClipboardKind>,
        tag: Option<&str>,
    ) -> Result<Vec<ClipboardEntry>> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        let mut sql =
            "SELECT DISTINCT h.id, h.content_type, h.content, h.html_content, h.source_app,
                h.source_app_path, h.timestamp, h.preview, h.is_pinned, h.use_count,
                h.is_external, h.pinned_order
             FROM clipboard_history h
             LEFT JOIN entry_tags t ON t.entry_id = h.id
             WHERE 1 = 1"
                .to_string();
        let mut values = Vec::new();

        let trimmed_query = query.trim();
        if !trimmed_query.is_empty() {
            sql.push_str(
                " AND (h.content LIKE ? OR h.preview LIKE ? OR h.source_app LIKE ? OR t.tag LIKE ?)",
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
                " AND EXISTS (
                    SELECT 1 FROM entry_tags filter_tags
                    WHERE filter_tags.entry_id = h.id AND filter_tags.tag = ?
                )",
            );
            values.push(tag.to_string());
        }
        sql.push_str(" ORDER BY h.is_pinned DESC, h.pinned_order ASC, h.timestamp DESC LIMIT 300");

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(values.iter()), row_to_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(|mut entry| {
                entry.tags = self.tags_for_entry_locked(&conn, entry.id)?;
                Ok(entry)
            })
            .collect()
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute("DELETE FROM entry_tags WHERE entry_id = ?1", params![id])?;
        conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear_unpinned(&self) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "DELETE FROM entry_tags WHERE entry_id IN (
                SELECT id FROM clipboard_history WHERE is_pinned = 0
            )",
            [],
        )?;
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
            "DELETE FROM entry_tags WHERE entry_id IN (
                SELECT id FROM clipboard_history WHERE is_pinned = 0 AND timestamp < ?1
            )",
            params![cutoff],
        )?;
        conn.execute(
            "DELETE FROM clipboard_history WHERE is_pinned = 0 AND timestamp < ?1",
            params![cutoff],
        )?;
        Ok(())
    }

    fn enforce_limit(&self) -> Result<()> {
        let conn = self.conn.lock().expect("storage mutex poisoned");
        conn.execute(
            "DELETE FROM entry_tags
             WHERE entry_id IN (
                SELECT id FROM clipboard_history
                WHERE is_pinned = 0
                ORDER BY timestamp DESC
                LIMIT -1 OFFSET ?1
             )",
            params![MAX_ENTRIES as i64],
        )?;
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

    fn tags_for_entry_locked(&self, conn: &Connection, id: i64) -> Result<Vec<String>> {
        let mut stmt =
            conn.prepare("SELECT tag FROM entry_tags WHERE entry_id = ?1 ORDER BY tag")?;
        Ok(stmt
            .query_map(params![id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
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

fn content_hash(kind: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
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
}
