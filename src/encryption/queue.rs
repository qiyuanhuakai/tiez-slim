use crate::encryption::SecureStore;
use crate::storage::Storage;
use std::sync::Arc;
use std::thread::JoinHandle;

const ENCRYPTED_PREFIX: &str = "enc:v1:";

pub struct MigrationQueue {
    handle: Option<JoinHandle<()>>,
}

impl MigrationQueue {
    pub fn start(storage: Storage, encryptor: Arc<dyn SecureStore + Send + Sync>) -> Self {
        let handle = std::thread::spawn(move || {
            run_migration(&storage, &encryptor);
        });
        Self {
            handle: Some(handle),
        }
    }

    pub fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MigrationQueue {
    fn drop(&mut self) {
        self.join();
    }
}

fn run_migration(storage: &Storage, encryptor: &Arc<dyn SecureStore + Send + Sync>) {
    let ids = match unencrypted_sensitive_ids(storage) {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("encryption migration: failed to query entries: {e}");
            return;
        }
    };

    if ids.is_empty() {
        return;
    }

    let total = ids.len();
    eprintln!("encryption migration: encrypting {total} historical entries");

    for (i, id) in ids.iter().enumerate() {
        match encrypt_entry(storage, encryptor, *id) {
            Ok(()) => {
                if (i + 1) % 50 == 0 || i + 1 == total {
                    eprintln!("encryption migration: {}/{total} done", i + 1);
                }
            }
            Err(e) => {
                eprintln!("encryption migration: failed to encrypt entry {id}: {e}");
            }
        }
    }

    eprintln!("encryption migration: complete ({total} entries)");
}

fn unencrypted_sensitive_ids(storage: &Storage) -> anyhow::Result<Vec<i64>> {
    let conn = storage.conn().lock().expect("storage mutex poisoned");
    let mut stmt = conn.prepare(
        "SELECT id FROM clipboard_history
         WHERE sensitive = 1 AND content NOT LIKE 'enc:v1:%'
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

fn encrypt_entry(
    storage: &Storage,
    encryptor: &Arc<dyn SecureStore + Send + Sync>,
    id: i64,
) -> anyhow::Result<()> {
    let conn = storage.conn().lock().expect("storage mutex poisoned");
    let content: String = conn.query_row(
        "SELECT content FROM clipboard_history WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    )?;

    if content.starts_with(ENCRYPTED_PREFIX) {
        return Ok(());
    }

    let encrypted = encryptor.encrypt(content.as_bytes())?;
    let encrypted_str = String::from_utf8(encrypted)
        .map_err(|_| anyhow::anyhow!("encrypted output is not valid UTF-8"))?;

    conn.execute(
        "UPDATE clipboard_history SET content = ?1 WHERE id = ?2",
        rusqlite::params![encrypted_str, id],
    )?;
    Ok(())
}
