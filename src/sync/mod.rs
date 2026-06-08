//! Device sync via KDE Connect protocol.
//!
//! Manages an isolated tokio runtime (started in a background thread) for
//! KDE Connect communication, keeping the main app free of a tokio dependency
//! at the top level.

#[cfg(feature = "kde_connect")]
use crate::storage::Storage;

#[allow(dead_code)]
#[cfg(feature = "kde_connect")]
const SYNC_SERVICE_TYPE: &str = "_kdeconnect._tcp.local.";

#[cfg(feature = "kde_connect")]
pub struct SyncManager {
    #[allow(dead_code)]
    storage: Storage,
    device_id: String,
    state: SyncState,
}

#[cfg(feature = "kde_connect")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    Disabled,
    Discovering,
    Pairing { device_name: String },
    Connected { device_name: String },
    Error(String),
}

#[cfg(feature = "kde_connect")]
pub struct Discovery {
    device_id: String,
    device_name: String,
}

#[cfg(feature = "kde_connect")]
pub struct Pairing {
    #[allow(dead_code)]
    device_id: String,
    #[allow(dead_code)]
    peer_id: String,
    trusted: bool,
}

#[cfg(feature = "kde_connect")]
pub struct ClipboardPlugin {
    device_id: String,
    #[allow(dead_code)]
    storage: Storage,
}

#[cfg(feature = "kde_connect")]
impl SyncManager {
    pub fn new(storage: Storage) -> Self {
        let device_id = load_or_create_device_id(&storage);
        Self {
            storage,
            device_id,
            state: SyncState::Disabled,
        }
    }

    pub fn state(&self) -> &SyncState {
        &self.state
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn enable(&mut self) {
        self.state = SyncState::Discovering;
    }

    pub fn disable(&mut self) {
        self.state = SyncState::Disabled;
    }
}

#[cfg(feature = "kde_connect")]
impl Discovery {
    pub fn new(device_id: String, device_name: String) -> Self {
        Self {
            device_id,
            device_name,
        }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }
}

#[cfg(feature = "kde_connect")]
impl Pairing {
    pub fn new(device_id: String, peer_id: String) -> Self {
        Self {
            device_id,
            peer_id,
            trusted: false,
        }
    }

    pub fn is_trusted(&self) -> bool {
        self.trusted
    }

    pub fn accept(&mut self) {
        self.trusted = true;
    }

    pub fn reject(&mut self) {
        self.trusted = false;
    }
}

#[cfg(feature = "kde_connect")]
impl ClipboardPlugin {
    pub fn new(device_id: String, storage: Storage) -> Self {
        Self { device_id, storage }
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[cfg(feature = "kde_connect")]
fn load_or_create_device_id(storage: &Storage) -> String {
    const KEY: &str = "sync.device_id";
    if let Ok(Some(id)) = storage.get_setting(KEY) {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = storage.set_setting(KEY, &id);
    id
}
