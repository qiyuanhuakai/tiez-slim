//! Auto-backup on app close and SIGINT.

use anyhow::{Context, Result};
use chrono::Utc;
use rust_i18n::t;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

use crate::model::ClipboardEntry;
use crate::storage::Storage;

#[derive(Debug, Serialize)]
struct BackupFile {
    schema_version: u32,
    created_at: String,
    entry_count: usize,
    entries: Vec<ClipboardEntry>,
}

pub struct AutoBackup {
    data_dir: PathBuf,
    retention: i32,
}

impl AutoBackup {
    pub fn new(data_dir: PathBuf, retention: i32) -> Self {
        Self {
            data_dir,
            retention,
        }
    }

    fn backup_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Exports all entries to a timestamped JSON file and prunes old backups.
    ///
    /// Returns `Err` if the database has zero entries (nothing to back up).
    pub fn run_backup(&self, storage: &Storage) -> Result<PathBuf> {
        let entries = storage
            .list("")
            .context(t!("settings.backup.backup_failed"))?;

        if entries.is_empty() {
            return Err(anyhow::anyhow!("no entries to back up"));
        }

        let backup_dir = self.backup_dir();
        fs::create_dir_all(&backup_dir).with_context(|| {
            format!(
                "Failed to create backup directory: {}",
                backup_dir.display()
            )
        })?;

        let timestamp = Utc::now().format("%Y-%m-%dT%H%M%SZ");
        let filename = format!("clipboard-{timestamp}.json");
        let path = backup_dir.join(&filename);

        let backup = BackupFile {
            schema_version: 1,
            created_at: Utc::now().to_rfc3339(),
            entry_count: entries.len(),
            entries,
        };

        let json = serde_json::to_string_pretty(&backup)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write backup file: {}", path.display()))?;

        let _ = self.cleanup_old_backups();

        Ok(path)
    }

    fn cleanup_old_backups(&self) -> Result<()> {
        let backup_dir = self.backup_dir();
        if !backup_dir.exists() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(&backup_dir)?
            .filter_map(Result::ok)
            .filter(|e| {
                e.path().extension().is_some_and(|ext| ext == "json")
                    && e.path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.starts_with("clipboard-"))
            })
            .collect();

        if entries.len() <= self.retention as usize {
            return Ok(());
        }

        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        let to_delete = entries.len() - self.retention as usize;
        for entry in entries.iter().take(to_delete) {
            let _ = fs::remove_file(entry.path());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipboardEntry, ClipboardKind};
    use crate::storage::Storage;
    use std::fs;
    use std::path::Path;

    fn make_entry(content: &str) -> ClipboardEntry {
        ClipboardEntry {
            id: 0,
            kind: ClipboardKind::Text,
            content: content.to_string(),
            html_content: None,
            source_app: "test".to_string(),
            source_app_path: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
            preview: content.chars().take(100).collect(),
            is_pinned: false,
            tags: vec![],
            use_count: 0,
            is_external: false,
            pinned_order: 0,
        }
    }

    fn setup_storage_with_entries(dir: &Path, count: usize) -> Storage {
        let db_path = dir.join("test.db");
        let storage = Storage::open(db_path).unwrap();
        for i in 0..count {
            let entry = make_entry(&format!("entry-{i}"));
            storage.save_entry(&entry).unwrap();
        }
        storage
    }

    #[test]
    fn test_run_backup_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let db_dir = dir.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();
        let storage = setup_storage_with_entries(&db_dir, 3);

        let backup = AutoBackup::new(data_dir.clone(), 10);
        let path = backup.run_backup(&storage).unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("clipboard-"));
        assert!(path.to_string_lossy().ends_with(".json"));

        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["entry_count"], 3);
        assert!(parsed["entries"].as_array().unwrap().len() == 3);
    }

    #[test]
    fn test_run_backup_skips_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let db_dir = dir.path().join("db");
        fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("test.db");
        let storage = Storage::open(db_path).unwrap();

        let backup = AutoBackup::new(data_dir.clone(), 10);
        let result = backup.run_backup(&storage);

        assert!(result.is_err());
    }

    #[test]
    fn test_cleanup_old_backups() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let backup_dir = data_dir.join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        for i in 0..5 {
            let name = format!("clipboard-2026-01-0{i}T000000Z.json");
            let path = backup_dir.join(&name);
            fs::write(&path, "{}").unwrap();
            let time = std::time::SystemTime::now() - std::time::Duration::from_secs((5 - i) * 60);
            filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(time)).unwrap();
        }

        let backup = AutoBackup::new(data_dir, 3);
        backup.cleanup_old_backups().unwrap();

        let remaining: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();

        assert_eq!(remaining.len(), 3);
    }

    #[test]
    fn test_cleanup_preserves_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let backup_dir = data_dir.join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        for i in 0..3 {
            let name = format!("clipboard-2026-01-0{i}T000000Z.json");
            fs::write(backup_dir.join(&name), "{}").unwrap();
        }

        let backup = AutoBackup::new(data_dir, 5);
        backup.cleanup_old_backups().unwrap();

        let remaining: Vec<_> = fs::read_dir(&backup_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(remaining.len(), 3);
    }
}
