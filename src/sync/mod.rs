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
use std::collections::HashSet;
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
    Disable,
    Pair(String),
    Unpair(String),
    SendClipboard(String),
}

// ── Sync events (runtime → UI) ────────────────────────────────────────

#[cfg(feature = "kde_connect")]
#[derive(Debug, Clone)]
pub enum SyncEvent {
    DeviceDiscovered { id: String, name: String },
    DeviceConnected { id: String, name: String },
    PairRequested { id: String, name: String },
    PairComplete { id: String },
    PairFailed { id: String, reason: String },
    ClipboardReceived { content: String },
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
    pending_pair_requests: Vec<DiscoveredDevice>,
    cmd_tx: Option<crossbeam_channel::Sender<SyncCmd>>,
    event_rx: Option<crossbeam_channel::Receiver<SyncEvent>>,
    echo_guard: SyncEchoGuard,
    pending_clipboard: Option<String>,
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
            pending_pair_requests: Vec::new(),
            cmd_tx: None,
            event_rx: None,
            echo_guard: SyncEchoGuard::new(),
            pending_clipboard: None,
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

    /// Devices that have requested pairing and are awaiting explicit user confirmation.
    pub fn pending_pair_requests(&self) -> &[DiscoveredDevice] {
        &self.pending_pair_requests
    }

    /// Accept a pending pair request and dispatch the underlying pair command.
    /// Removes the request from the pending list regardless of cmd_tx availability.
    pub fn accept_pair_request(&mut self, device_id: &str) {
        if let Some(pos) = self
            .pending_pair_requests
            .iter()
            .position(|d| d.id == device_id)
        {
            let device = self.pending_pair_requests.remove(pos);
            self.state = SyncState::Pairing {
                device_name: device.name.clone(),
            };
            if let Some(tx) = &self.cmd_tx {
                let _ = tx.send(SyncCmd::Pair(device.id));
            }
        }
    }

    /// Reject a pending pair request without pairing.
    pub fn reject_pair_request(&mut self, device_id: &str) {
        self.pending_pair_requests.retain(|d| d.id != device_id);
    }

    /// Enable sync: start the background runtime with mDNS discovery + TLS.
    pub fn enable(&mut self) {
        self.state = SyncState::Discovering;

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        let storage = self.storage.clone();
        let device_id = self.device_id.clone();
        let echo_guard = self.echo_guard.clone();

        std::thread::Builder::new()
            .name("sync-runtime".into())
            .spawn(move || {
                sync_runtime(storage, device_id, cmd_rx, event_tx, echo_guard);
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
                    if !self.pending_pair_requests.iter().any(|d| d.id == id) {
                        self.pending_pair_requests.push(DiscoveredDevice {
                            id,
                            name,
                            paired: false,
                        });
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
                    self.state = SyncState::Connected { device_name: name };
                }
                SyncEvent::PairFailed { id: _, reason } => {
                    self.state = SyncState::Error(reason);
                }
                SyncEvent::ClipboardReceived { content } => {
                    self.pending_clipboard = Some(content);
                }
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

    /// Drain queued `ClipboardReceived` events. Returns the **most recent**
    /// clipboard content; earlier events received between polls are
    /// intentionally discarded because a single coalesced update is what the
    /// caller (clipboard watcher) actually wants to apply.
    pub fn take_clipboard_received(&mut self) -> Option<String> {
        self.pending_clipboard.take()
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
pub struct SyncEchoGuard {
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
        state.0 == hash && state.1.is_some_and(|at| at.elapsed() < SYNC_ECHO_WINDOW)
    }
}

#[cfg(feature = "kde_connect")]
impl Default for SyncEchoGuard {
    fn default() -> Self {
        Self::new()
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
pub struct TiezClipboardPlugin {
    echo_guard: SyncEchoGuard,
    event_tx: crossbeam_channel::Sender<SyncEvent>,
}

#[cfg(feature = "kde_connect")]
impl TiezClipboardPlugin {
    pub fn new(event_tx: crossbeam_channel::Sender<SyncEvent>, echo_guard: SyncEchoGuard) -> Self {
        Self {
            echo_guard,
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
        vec![
            NetworkPacketType::Clipboard,
            NetworkPacketType::ClipboardConnect,
        ]
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

    let key_arg = key_p
        .to_str()
        .ok_or_else(|| std::io::Error::other("non-UTF8 key path"))?
        .to_string();
    let cert_arg = cert_p
        .to_str()
        .ok_or_else(|| std::io::Error::other("non-UTF8 cert path"))?
        .to_string();

    // Use openssl to generate a self-signed EC certificate.
    let output = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-keyout",
            &key_arg,
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-days",
            "3650",
            "-nodes",
            "-subj",
            &format!("/O=KDE/OU=KDE Connect/CN={device_id}"),
            "-out",
            &cert_arg,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!(
            "openssl certificate generation failed: {stderr}"
        )));
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
                    .filter_map(|f| {
                        let device_id = f
                            .path()
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())?;
                        let cert = std::fs::read(f.path()).ok()?;
                        if cert.is_empty() {
                            return None;
                        }
                        Some((device_id, cert))
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
        let _ = std::fs::write(self.path.join(format!("{device_id}.pem")), &cert);
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
    echo_guard: SyncEchoGuard,
) {
    use kdeconnect_proto::{config::DeviceConfig, device::DeviceType, io::TokioIoImpl};

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

    let plugins: Vec<Box<dyn kdeconnect_proto::plugin::Plugin + Send + Sync>> = vec![Box::new(
        TiezClipboardPlugin::new(event_tx.clone(), echo_guard),
    )];
    let trust_handler = FileTrustHandler::new(trusted_devices_dir());
    let kdevice =
        kdeconnect_proto::device::Device::new(config, plugins, trust_handler, TokioIoImpl);

    let event_tx_mdns = event_tx.clone();
    let device_id_mdns = device_id.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("mdns-monitor".into())
        .spawn(move || {
            mdns_monitor_loop(&device_id_mdns, event_tx_mdns);
        })
    {
        eprintln!("[sync] mdns-monitor spawn failed: {e}");
    }

    let rt = tokio::runtime::Runtime::new().expect("create sync tokio runtime");
    rt.block_on(async move {
        let device = Arc::new(kdevice);
        let mut pair_complete_sent = HashSet::new();

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
                    let (id, name, paired) = link_device_info(&device, &link_id).await;
                    let _ = event_tx.send(SyncEvent::DeviceDiscovered {
                        id: id.clone(),
                        name: name.clone(),
                    });
                    let _ = event_tx.send(SyncEvent::DeviceConnected {
                        id: id.clone(),
                        name: name.clone(),
                    });
                    if paired && pair_complete_sent.insert(id.clone()) {
                        let _ = event_tx.send(SyncEvent::PairComplete { id: id.clone() });
                    }
                    device.accept_pair(&link_id).await;
                    let device_for_pair = Arc::clone(&device);
                    let event_tx_for_pair = event_tx.clone();
                    let link_id_for_pair = link_id.clone();
                    tokio::spawn(async move {
                        for _ in 0..40 {
                            tokio::time::sleep(core::time::Duration::from_millis(250)).await;
                            let (id, _, paired) =
                                link_device_info(&device_for_pair, &link_id_for_pair).await;
                            if paired {
                                let _ = event_tx_for_pair.send(SyncEvent::PairComplete { id });
                                break;
                            }
                        }
                    });
                }
                cmd = tokio_cmd_rx.recv() => {
                    match cmd {
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

#[cfg(feature = "kde_connect")]
async fn link_device_info<Io, UdpSocket, TcpStream, TcpListener, TlsStream>(
    device: &Arc<
        kdeconnect_proto::device::Device<Io, UdpSocket, TcpStream, TcpListener, TlsStream>,
    >,
    link_id: &str,
) -> (String, String, bool)
where
    Io: kdeconnect_proto::io::IoImpl<UdpSocket, TcpStream, TcpListener, TlsStream>
        + Unpin
        + 'static,
    UdpSocket: kdeconnect_proto::io::UdpSocketImpl + Unpin + 'static,
    TcpStream: kdeconnect_proto::io::TcpStreamImpl + Unpin + 'static,
    TcpListener: kdeconnect_proto::io::TcpListenerImpl<TcpStream> + Unpin + 'static,
    TlsStream: kdeconnect_proto::io::TlsStreamImpl + Unpin + 'static,
{
    let links = device.links().lock().await;
    let Some(link) = links.get(link_id) else {
        return (link_id.to_string(), link_id.to_string(), false);
    };
    let id = link.info.device_id.clone();
    let name = link.info.device_name.clone().unwrap_or_else(|| id.clone());
    let paired = link.pair_state == kdeconnect_proto::device::PairState::Paired;
    (id, name, paired)
}

// ── Supplementary mDNS monitoring ─────────────────────────────────────

/// Monitor for KDE Connect services on the network using `mdns-sd`.
/// This is supplementary to kdeconnect-proto's built-in mDNS discovery.
#[cfg(feature = "kde_connect")]
fn mdns_monitor_loop(_local_device_id: &str, event_tx: crossbeam_channel::Sender<SyncEvent>) {
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
    #[cfg(feature = "kde_connect")]
    use super::*;
    #[cfg(feature = "kde_connect")]
    use crate::storage::Storage;

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
        assert!(matches!(mgr.state(), SyncState::Pairing { .. }));

        mgr.state = SyncState::Connected {
            device_name: "Phone".into(),
        };
        assert!(matches!(mgr.state(), SyncState::Connected { .. }));
    }

    #[test]
    #[cfg(feature = "kde_connect")]
    fn test_poll_events_preserves_clipboard_received() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_sync_clipboard_event.db");
        let storage = Storage::open(db_path).unwrap();
        let mut mgr = SyncManager::new(storage);
        let (tx, rx) = crossbeam_channel::unbounded();
        mgr.event_rx = Some(rx);

        tx.send(SyncEvent::ClipboardReceived {
            content: "from phone".into(),
        })
        .unwrap();

        mgr.poll_events();
        assert_eq!(mgr.take_clipboard_received(), Some("from phone".into()));
        assert_eq!(mgr.take_clipboard_received(), None);
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
        let plugin = TiezClipboardPlugin::new(tx, SyncEchoGuard::new());

        let guard = plugin.echo_guard();
        assert!(!guard.should_suppress("anything"));

        guard.mark_remote_write("anything");
        assert!(guard.should_suppress("anything"));
    }
}
