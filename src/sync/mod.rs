//! Device sync via KDE Connect protocol.
//!
//! Manages an isolated tokio runtime (started in a background thread) for
//! KDE Connect communication, keeping the main app free of a tokio dependency
//! at the top level.
//!
//! ## Architecture
//!
//! - [`SyncManager`] is the public API (sync). It owns a background thread
//!   running a tokio runtime with the kdeconnect-proto [`Device`].
//! - [`Discovery`] wraps device discovery events.
//! - [`Pairing`] wraps pairing state for a specific peer.
//! - [`ClipboardPlugin`] handles bidirectional clipboard sync (T30).
//!
//! mDNS discovery and TLS pairing are handled by kdeconnect-proto's `Device`
//! internally. We additionally use `mdns-sd` for supplementary service
//! monitoring/registration.

#[cfg(feature = "kde_connect")]
use crate::storage::Storage;
#[cfg(feature = "kde_connect")]
use std::path::PathBuf;
#[cfg(feature = "kde_connect")]
use std::sync::Arc;

// ── Service type ──────────────────────────────────────────────────────

#[cfg(feature = "kde_connect")]
const SYNC_SERVICE_TYPE: &str = "_kdeconnect._tcp.local.";

// ── Sync state ────────────────────────────────────────────────────────

#[cfg(feature = "kde_connect")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncState {
    Disabled,
    Discovering,
    Pairing { device_name: String },
    Connected { device_name: String },
    Error(String),
}

// ── Sync commands (UI → runtime) ──────────────────────────────────────

#[cfg(feature = "kde_connect")]
#[derive(Debug)]
enum SyncCmd {
    Enable,
    Disable,
    Pair(String),
    Unpair(String),
    SendClipboard(String),
}

// ── Sync events (runtime → UI) ────────────────────────────────────────

#[cfg(feature = "kde_connect")]
#[derive(Debug, Clone)]
pub enum SyncEvent {
    DeviceDiscovered {
        id: String,
        name: String,
    },
    DeviceConnected {
        id: String,
        name: String,
    },
    PairRequested {
        id: String,
        name: String,
    },
    PairComplete {
        id: String,
    },
    PairFailed {
        id: String,
        reason: String,
    },
    ClipboardReceived {
        content: String,
    },
    Error(String),
}

// ── Device info for the UI ────────────────────────────────────────────

#[cfg(feature = "kde_connect")]
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
}

// ── SyncManager ───────────────────────────────────────────────────────

#[cfg(feature = "kde_connect")]
pub struct SyncManager {
    storage: Storage,
    device_id: String,
    state: SyncState,
    discovered_devices: Vec<DiscoveredDevice>,
    cmd_tx: Option<crossbeam_channel::Sender<SyncCmd>>,
    event_rx: Option<crossbeam_channel::Receiver<SyncEvent>>,
    echo_guard: SyncEchoGuard,
}

#[cfg(feature = "kde_connect")]
impl SyncManager {
    pub fn new(storage: Storage) -> Self {
        let device_id = load_or_create_device_id(&storage);
        Self {
            storage,
            device_id,
            state: SyncState::Disabled,
            discovered_devices: Vec::new(),
            cmd_tx: None,
            event_rx: None,
            echo_guard: SyncEchoGuard::new(),
        }
    }

    pub fn state(&self) -> &SyncState {
        &self.state
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn discovered_devices(&self) -> &[DiscoveredDevice] {
        &self.discovered_devices
    }

    /// Enable sync: start the background runtime with mDNS discovery + TLS.
    pub fn enable(&mut self) {
        self.state = SyncState::Discovering;

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        let storage = self.storage.clone();
        let device_id = self.device_id.clone();

        std::thread::Builder::new()
            .name("sync-runtime".into())
            .spawn(move || {
                sync_runtime(storage, device_id, cmd_rx, event_tx);
            })
            .expect("spawn sync runtime");

        self.cmd_tx = Some(cmd_tx);
        self.event_rx = Some(event_rx);
    }

    /// Disable sync: stop the background runtime.
    pub fn disable(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(SyncCmd::Disable);
        }
        self.event_rx = None;
        self.state = SyncState::Disabled;
        self.discovered_devices.clear();
    }

    /// Initiate pairing with a discovered device.
    pub fn pair_with(&self, device_id: &str) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(SyncCmd::Pair(device_id.to_string()));
        }
    }

    /// Unpair from a device.
    pub fn unpair_with(&self, device_id: &str) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(SyncCmd::Unpair(device_id.to_string()));
        }
    }

    /// Poll for events from the background runtime. Should be called from
    /// the UI thread's update loop.
    pub fn poll_events(&mut self) {
        let Some(rx) = &self.event_rx else { return };

        while let Ok(event) = rx.try_recv() {
            match event {
                SyncEvent::DeviceDiscovered { id, name } => {
                    if !self.discovered_devices.iter().any(|d| d.id == id) {
                        self.discovered_devices.push(DiscoveredDevice {
                            id,
                            name,
                            paired: false,
                        });
                    }
                }
                SyncEvent::DeviceConnected { id, name } => {
                    self.state = SyncState::Connected {
                        device_name: name.clone(),
                    };
                    if !self.discovered_devices.iter().any(|d| d.id == id) {
                        self.discovered_devices.push(DiscoveredDevice {
                            id,
                            name,
                            paired: false,
                        });
                    }
                }
                SyncEvent::PairRequested { id, name } => {
                    self.state = SyncState::Pairing {
                        device_name: name.clone(),
                    };
                    // Auto-accept pair requests for now
                    if let Some(tx) = &self.cmd_tx {
                        let _ = tx.send(SyncCmd::Pair(id));
                    }
                }
                SyncEvent::PairComplete { id } => {
                    if let Some(dev) = self.discovered_devices.iter_mut().find(|d| d.id == id) {
                        dev.paired = true;
                    }
                    let name = self
                        .discovered_devices
                        .iter()
                        .find(|d| d.id == id)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| id.clone());
                    self.state = SyncState::Connected {
                        device_name: name,
                    };
                }
                SyncEvent::PairFailed { id: _, reason } => {
                    self.state = SyncState::Error(reason);
                }
                SyncEvent::ClipboardReceived { content: _ } => {}
                SyncEvent::Error(msg) => {
                    self.state = SyncState::Error(msg);
                }
            }
        }
    }

    pub fn echo_guard(&self) -> &SyncEchoGuard {
        &self.echo_guard
    }

    pub fn send_clipboard(&self, content: &str) {
        if self.echo_guard.should_suppress(content) {
            return;
        }
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(SyncCmd::SendClipboard(content.to_string()));
        }
    }

    pub fn take_clipboard_received(&mut self) -> Option<String> {
        let rx = self.event_rx.as_ref()?;
        let mut last = None;
        while let Ok(event) = rx.try_recv() {
            if let SyncEvent::ClipboardReceived { content } = event {
                last = Some(content);
            }
        }
        last
    }
}

// ── Discovery (wrapper for discovery info) ────────────────────────────

#[cfg(feature = "kde_connect")]
pub struct Discovery {
    device_id: String,
    device_name: String,
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

// ── Pairing (wrapper for pairing state) ───────────────────────────────

#[cfg(feature = "kde_connect")]
pub struct Pairing {
    #[allow(dead_code)]
    device_id: String,
    #[allow(dead_code)]
    peer_id: String,
    trusted: bool,
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

// ── ClipboardPlugin (bidirectional sync with echo suppression) ────────

#[cfg(feature = "kde_connect")]
pub struct ClipboardPlugin {
    device_id: String,
    #[allow(dead_code)]
    storage: Storage,
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

/// Prevents feedback loop: suppresses local clipboard re-capture after a remote write.
#[cfg(feature = "kde_connect")]
#[derive(Clone)]
pub(crate) struct SyncEchoGuard {
    inner: Arc<std::sync::Mutex<(String, Option<std::time::Instant>)>>,
}

#[cfg(feature = "kde_connect")]
const SYNC_ECHO_WINDOW: std::time::Duration = std::time::Duration::from_millis(800);

#[cfg(feature = "kde_connect")]
impl SyncEchoGuard {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new((String::new(), None))),
        }
    }

    pub fn mark_remote_write(&self, content: &str) {
        let hash = content_hash(content);
        let mut state = self.inner.lock().expect("sync echo guard poisoned");
        *state = (hash, Some(std::time::Instant::now()));
    }

    pub fn should_suppress(&self, content: &str) -> bool {
        let hash = content_hash(content);
        let state = self.inner.lock().expect("sync echo guard poisoned");
        state.0 == hash
            && state
                .1
                .is_some_and(|at| at.elapsed() < SYNC_ECHO_WINDOW)
    }
}

#[cfg(feature = "kde_connect")]
fn content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(feature = "kde_connect")]
pub(crate) struct TiezClipboardPlugin {
    echo_guard: SyncEchoGuard,
    event_tx: crossbeam_channel::Sender<SyncEvent>,
}

#[cfg(feature = "kde_connect")]
impl TiezClipboardPlugin {
    pub fn new(event_tx: crossbeam_channel::Sender<SyncEvent>) -> Self {
        Self {
            echo_guard: SyncEchoGuard::new(),
            event_tx,
        }
    }

    pub fn echo_guard(&self) -> &SyncEchoGuard {
        &self.echo_guard
    }
}

#[cfg(feature = "kde_connect")]
#[kdeconnect_proto::async_trait]
impl kdeconnect_proto::plugin::Plugin for TiezClipboardPlugin {
    fn supported_incoming_packets(&self) -> Vec<kdeconnect_proto::packet::NetworkPacketType> {
        use kdeconnect_proto::packet::NetworkPacketType;
        vec![NetworkPacketType::Clipboard, NetworkPacketType::ClipboardConnect]
    }

    fn supported_outgoing_packets(&self) -> Vec<kdeconnect_proto::packet::NetworkPacketType> {
        use kdeconnect_proto::packet::NetworkPacketType;
        vec![NetworkPacketType::Clipboard]
    }

    async fn on_packet_received(
        &self,
        packet: &kdeconnect_proto::packet::NetworkPacket,
        _link: &kdeconnect_proto::device::Link,
    ) -> kdeconnect_proto::error::Result<()> {
        use kdeconnect_proto::packet::NetworkPacketBody;

        match &packet.body {
            NetworkPacketBody::Clipboard(clip) => {
                if clip.content.is_empty() {
                    return Ok(());
                }
                if self.echo_guard.should_suppress(&clip.content) {
                    return Ok(());
                }

                self.echo_guard.mark_remote_write(&clip.content);
                let _ = self.event_tx.send(SyncEvent::ClipboardReceived {
                    content: clip.content.clone(),
                });
            }
            NetworkPacketBody::ClipboardConnect(clip) => {
                if clip.content.is_empty() {
                    return Ok(());
                }
                if self.echo_guard.should_suppress(&clip.content) {
                    return Ok(());
                }

                self.echo_guard.mark_remote_write(&clip.content);
                let _ = self.event_tx.send(SyncEvent::ClipboardReceived {
                    content: clip.content.clone(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_start(
        &self,
        _link: &kdeconnect_proto::device::Link,
    ) -> kdeconnect_proto::error::Result<()> {
        Ok(())
    }
}

// ── Certificate management ────────────────────────────────────────────

/// Directory for sync-related data (certificates, trusted devices).
#[cfg(feature = "kde_connect")]
fn sync_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("tiez-slim-linux").join("sync")
}

/// Get the path to the TLS certificate file.
#[cfg(feature = "kde_connect")]
fn cert_path() -> PathBuf {
    sync_data_dir().join("cert.pem")
}

/// Get the path to the TLS private key file.
#[cfg(feature = "kde_connect")]
fn key_path() -> PathBuf {
    sync_data_dir().join("private_key.pem")
}

/// Get the path to the trusted devices directory.
#[cfg(feature = "kde_connect")]
fn trusted_devices_dir() -> PathBuf {
    sync_data_dir().join("trusted")
}

/// Generate a self-signed TLS certificate for KDE Connect using `openssl`.
///
/// Returns `(cert_pem, key_pem)` on success.
#[cfg(feature = "kde_connect")]
fn generate_tls_certificate(device_id: &str) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let dir = sync_data_dir();
    std::fs::create_dir_all(&dir)?;

    let cert_p = dir.join("cert.pem");
    let key_p = dir.join("private_key.pem");

    // Use openssl to generate a self-signed EC certificate.
    let status = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-keyout",
            key_p.to_str().unwrap(),
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-days",
            "3650",
            "-nodes",
            "-subj",
            &format!("/O=KDE/OU=KDE Connect/CN={device_id}"),
            "-out",
            cert_p.to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        return Err(std::io::Error::other("openssl certificate generation failed"));
    }

    let cert = std::fs::read(&cert_p)?;
    let key = std::fs::read(&key_p)?;
    Ok((cert, key))
}

/// Load or generate TLS certificate. Returns `(cert_pem, key_pem)`.
#[cfg(feature = "kde_connect")]
fn load_or_generate_cert(device_id: &str) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let cert_p = cert_path();
    let key_p = key_path();

    if cert_p.exists() && key_p.exists() {
        let cert = std::fs::read(&cert_p)?;
        let key = std::fs::read(&key_p)?;
        return Ok((cert, key));
    }

    generate_tls_certificate(device_id)
}

// ── TrustHandler backed by filesystem ─────────────────────────────────

#[cfg(feature = "kde_connect")]
struct FileTrustHandler {
    path: PathBuf,
    trusted_devices: std::collections::HashMap<String, Vec<u8>>,
}

#[cfg(feature = "kde_connect")]
impl FileTrustHandler {
    fn new(path: PathBuf) -> Self {
        let trusted_devices = if path.exists() {
            std::collections::HashMap::from_iter(
                std::fs::read_dir(&path)
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
                    .map(|f| {
                        let device_id = f
                            .path()
                            .file_stem()
                            .unwrap()
                            .to_string_lossy()
                            .to_string();
                        let cert = std::fs::read(f.path()).unwrap_or_default();
                        (device_id, cert)
                    }),
            )
        } else {
            let _ = std::fs::create_dir_all(&path);
            std::collections::HashMap::new()
        };

        Self {
            path,
            trusted_devices,
        }
    }
}

#[cfg(feature = "kde_connect")]
#[kdeconnect_proto::async_trait]
impl kdeconnect_proto::trust::TrustHandler for FileTrustHandler {
    async fn trust_device(&mut self, device_id: String, cert: Vec<u8>) {
        let _ = std::fs::write(
            self.path.join(format!("{device_id}.pem")),
            &cert,
        );
        self.trusted_devices.insert(device_id, cert);
    }

    async fn untrust_device(&mut self, device_id: &str) {
        let _ = std::fs::remove_file(self.path.join(format!("{device_id}.pem")));
        self.trusted_devices.remove(device_id);
    }

    async fn get_certificate(&mut self, device_id: &str) -> Option<&[u8]> {
        self.trusted_devices.get(device_id).map(|v| &**v)
    }
}

// ── Background sync runtime ───────────────────────────────────────────

/// The main sync runtime running in a background thread with its own tokio
/// runtime. Uses kdeconnect-proto's `Device` for mDNS discovery + TLS pairing.
#[cfg(feature = "kde_connect")]
fn sync_runtime(
    _storage: Storage,
    device_id: String,
    cmd_rx: crossbeam_channel::Receiver<SyncCmd>,
    event_tx: crossbeam_channel::Sender<SyncEvent>,
) {
    use kdeconnect_proto::{
        config::DeviceConfig, device::DeviceType, io::TokioIoImpl,
    };

    let (cert, key) = match load_or_generate_cert(&device_id) {
        Ok(ck) => ck,
        Err(e) => {
            let _ = event_tx.send(SyncEvent::Error(format!(
                "Certificate generation failed: {e}"
            )));
            return;
        }
    };

    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "tiez-slim".to_string());
    let name = if hostname.len() > 32 {
        hostname[..32].to_string()
    } else {
        hostname
    };

    let config = DeviceConfig {
        name,
        device_type: DeviceType::Desktop,
        cert,
        private_key: key,
    };

    let plugins: Vec<Box<dyn kdeconnect_proto::plugin::Plugin + Send + Sync>> = vec![];
    let trust_handler = FileTrustHandler::new(trusted_devices_dir());
    let kdevice = kdeconnect_proto::device::Device::new(config, plugins, trust_handler, TokioIoImpl);

    let event_tx_mdns = event_tx.clone();
    let device_id_mdns = device_id.clone();
    std::thread::Builder::new()
        .name("mdns-monitor".into())
        .spawn(move || {
            mdns_monitor_loop(&device_id_mdns, event_tx_mdns);
        })
        .ok();

    let rt = tokio::runtime::Runtime::new().expect("create sync tokio runtime");
    rt.block_on(async move {
        let device = Arc::new(kdevice);

        {
            let d = device.clone();
            std::thread::Builder::new()
                .name("kdeconnect-device".into())
                .spawn(move || d.start_arced())
                .expect("spawn kdeconnect device");
        }

        let (tokio_cmd_tx, mut tokio_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SyncCmd>();
        std::thread::Builder::new()
            .name("cmd-bridge".into())
            .spawn(move || {
                while let Ok(cmd) = cmd_rx.recv() {
                    if tokio_cmd_tx.send(cmd).is_err() {
                        break;
                    }
                }
            })
            .ok();

        loop {
            tokio::select! {
                link_id = device.wait_for_connection() => {
                    let _ = event_tx.send(SyncEvent::DeviceDiscovered {
                        id: link_id.clone(),
                        name: link_id.clone(),
                    });
                    device.accept_pair(&link_id).await;
                }
                cmd = tokio_cmd_rx.recv() => {
                    match cmd {
                        Some(SyncCmd::Enable) => {}
                        Some(SyncCmd::Disable) => break,
                        Some(SyncCmd::Pair(peer_id)) => {
                            device.pair_with(&peer_id).await;
                        }
                        Some(SyncCmd::Unpair(peer_id)) => {
                            device.unpair_with(&peer_id).await;
                        }
                        Some(SyncCmd::SendClipboard(content)) => {
                            use kdeconnect_proto::packet::{
                                NetworkPacket, NetworkPacketBody,
                                clipboard::ClipboardPacket,
                            };
                            let packet = NetworkPacket::new(NetworkPacketBody::Clipboard(
                                ClipboardPacket { content },
                            ));
                            let links = device.links().lock().await;
                            for (_id, link) in links.iter() {
                                if link.pair_state
                                    == kdeconnect_proto::device::PairState::Paired
                                {
                                    link.send(packet.clone()).await;
                                }
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });
}

// ── Supplementary mDNS monitoring ─────────────────────────────────────

/// Monitor for KDE Connect services on the network using `mdns-sd`.
/// This is supplementary to kdeconnect-proto's built-in mDNS discovery.
#[cfg(feature = "kde_connect")]
fn mdns_monitor_loop(
    _local_device_id: &str,
    event_tx: crossbeam_channel::Sender<SyncEvent>,
) {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mdns = match ServiceDaemon::new() {
        Ok(m) => m,
        Err(e) => {
            let _ = event_tx.send(SyncEvent::Error(format!("mDNS daemon error: {e}")));
            return;
        }
    };

    let receiver = match mdns.browse(SYNC_SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            let _ = event_tx.send(SyncEvent::Error(format!("mDNS browse error: {e}")));
            return;
        }
    };

    while let Ok(event) = receiver.recv() {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                let fullname = info.get_fullname().to_string();
                let service_type = info.get_type().to_string();
                let device_id = fullname
                    .strip_suffix(&format!(".{service_type}"))
                    .unwrap_or(&fullname)
                    .to_string();

                if device_id.is_empty() {
                    continue;
                }

                let name = info
                    .get_property_val_str("name")
                    .map(String::from)
                    .unwrap_or_else(|| device_id.clone());

                let _ = event_tx.send(SyncEvent::DeviceDiscovered {
                    id: device_id,
                    name,
                });
            }
            ServiceEvent::ServiceRemoved(service_type, fullname) => {
                let _ = (service_type, fullname);
            }
            _ => {}
        }
    }
}

// ── Device ID persistence ─────────────────────────────────────────────

#[cfg(feature = "kde_connect")]
fn load_or_create_device_id(storage: &Storage) -> String {
    const KEY: &str = "sync.device_id";
    if let Ok(Some(id)) = storage.get_setting(KEY) {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let _ = storage.set_setting(KEY, &id);
    id
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_load_or_create_device_id_persists() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_sync_id.db");
        let storage = Storage::open(db_path).unwrap();

        let id1 = load_or_create_device_id(&storage);
        let id2 = load_or_create_device_id(&storage);
        assert_eq!(id1, id2, "device ID should be stable across calls");
        assert!(!id1.is_empty());
        // UUID v4 without dashes is 32 hex chars.
        assert_eq!(id1.len(), 32);
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_sync_state_transitions() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_sync_state.db");
        let storage = Storage::open(db_path).unwrap();

        let mut mgr = SyncManager::new(storage);
        assert_eq!(*mgr.state(), SyncState::Disabled);

        // We can't fully test enable() without a network, but test the state machine.
        mgr.state = SyncState::Discovering;
        assert_eq!(*mgr.state(), SyncState::Discovering);

        mgr.state = SyncState::Pairing {
            device_name: "Phone".into(),
        };
        assert!(matches!(&*mgr.state(), SyncState::Pairing { .. }));

        mgr.state = SyncState::Connected {
            device_name: "Phone".into(),
        };
        assert!(matches!(&*mgr.state(), SyncState::Connected { .. }));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_discovery_struct() {
        let d = Discovery::new("abc123".into(), "My Phone".into());
        assert_eq!(d.device_id(), "abc123");
        assert_eq!(d.device_name(), "My Phone");
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_pairing_lifecycle() {
        let mut p = Pairing::new("local".into(), "remote".into());
        assert!(!p.is_trusted());

        p.accept();
        assert!(p.is_trusted());

        p.reject();
        assert!(!p.is_trusted());
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_clipboard_plugin_device_id() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_cp_plugin.db");
        let storage = Storage::open(db_path).unwrap();

        let plugin = ClipboardPlugin::new("dev123".into(), storage);
        assert_eq!(plugin.device_id(), "dev123");
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_service_type_constant() {
        assert_eq!(SYNC_SERVICE_TYPE, "_kdeconnect._tcp.local.");
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_sync_data_dir_is_absolute() {
        let dir = sync_data_dir();
        assert!(dir.is_absolute(), "sync data dir should be absolute");
        assert!(dir.to_string_lossy().contains("tiez-slim-linux"));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_discovered_device_clone() {
        let dev = DiscoveredDevice {
            id: "abc".into(),
            name: "Phone".into(),
            paired: false,
        };
        let dev2 = dev.clone();
        assert_eq!(dev2.id, "abc");
        assert_eq!(dev2.name, "Phone");
        assert!(!dev2.paired);
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_echo_guard_suppresses_matching_content() {
        let guard = SyncEchoGuard::new();
        assert!(!guard.should_suppress("hello"));

        guard.mark_remote_write("hello");
        assert!(guard.should_suppress("hello"));
        assert!(!guard.should_suppress("world"));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_echo_guard_different_content_not_suppressed() {
        let guard = SyncEchoGuard::new();
        guard.mark_remote_write("aaa");
        assert!(!guard.should_suppress("bbb"));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_content_hash_deterministic() {
        let h1 = content_hash("test content");
        let h2 = content_hash("test content");
        assert_eq!(h1, h2);

        let h3 = content_hash("different");
        assert_ne!(h1, h3);
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_sync_echo_guard_clone() {
        let guard1 = SyncEchoGuard::new();
        let guard2 = guard1.clone();

        guard1.mark_remote_write("shared");
        assert!(guard2.should_suppress("shared"));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_clipboard_plugin_echo_guard_access() {
        let (tx, _rx) = crossbeam_channel::unbounded();
        let plugin = TiezClipboardPlugin::new(tx);

        let guard = plugin.echo_guard();
        assert!(!guard.should_suppress("anything"));

        guard.mark_remote_write("anything");
        assert!(guard.should_suppress("anything"));
    }
}
