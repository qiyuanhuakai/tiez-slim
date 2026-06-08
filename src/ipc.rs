//! Inter-process communication for CLI ↔ GUI coordination.
//!
//! Provides a Unix domain socket server so that the `tiez-cli` binary can
//! send commands to a running GUI instance. Protocol is JSON Lines: one
//! JSON object per line, terminated by `\n`.
//!
//! - **Request**: `{"cmd":"list","args":{...}}`
//! - **Success**: `{"ok":true,"data":...}`
//! - **Error**:   `{"ok":false,"error":{"code":N,"message":"..."}}`

use crate::model::{ClipboardEntry, ClipboardKind};
use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

// ── Error type ────────────────────────────────────────────────────────

/// Errors returned by the IPC layer. Each variant maps to a distinct CLI
/// exit code so that `tiez-cli` can translate them into user-visible
/// diagnostics.
///
/// | Variant            | Exit code |
/// |--------------------|-----------|
/// | `ConnectionRefused`| 2         |
/// | `InvalidJson`      | 3         |
/// | `UnknownCommand`   | 4         |
/// | `NotFound`         | 5         |
/// | `IpcDisabled`      | 6         |
/// | other              | 1         |
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("connection refused – is tiez-slim running?")]
    ConnectionRefused,
    #[error("IPC timed out")]
    Timeout,
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("unknown command (code {0})")]
    UnknownCommand(i32),
    #[error("entry not found")]
    NotFound,
    #[error("IPC is disabled")]
    IpcDisabled,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Storage(String),
}

impl IpcError {
    /// Map error variant to a CLI exit code (see struct-level table).
    pub fn exit_code(&self) -> i32 {
        match self {
            IpcError::ConnectionRefused => 2,
            IpcError::InvalidJson(_) => 3,
            IpcError::UnknownCommand(code) => *code,
            IpcError::NotFound => 5,
            IpcError::IpcDisabled => 6,
            _ => 1,
        }
    }
}

// ── JSON Lines protocol types ─────────────────────────────────────────

/// A single request line sent by the CLI.
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcRequest {
    pub cmd: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

/// A single response line sent back by the server.
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcErrorBody>,
}

/// Structured error payload embedded in a failed [`IpcResponse`].
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcErrorBody {
    pub code: i32,
    pub message: String,
}

impl IpcResponse {
    /// Build a success response wrapping arbitrary JSON data.
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    /// Build an error response from an [`IpcError`].
    pub fn err(e: &IpcError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(IpcErrorBody {
                code: e.exit_code(),
                message: e.to_string(),
            }),
        }
    }
}

// ── Server ────────────────────────────────────────────────────────────

/// IPC server that listens on a Unix domain socket and dispatches
/// clipboard commands to the shared [`Storage`].
pub struct IpcServer {
    pub socket_path: PathBuf,
}

impl IpcServer {
    /// Resolve the default socket path:
    /// 1. `$XDG_RUNTIME_DIR/tiez-slim-linux.sock`
    /// 2. `/tmp/tiez-slim-$UID.sock`
    pub fn socket_path_default() -> PathBuf {
        if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let p = PathBuf::from(runtime).join("tiez-slim-linux.sock");
            if p.parent().is_some_and(|d| d.exists()) {
                return p;
            }
        }
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/tiez-slim-{uid}.sock"))
    }

    /// Start the IPC server in a background thread.
    ///
    /// - Removes a stale socket file if one already exists at `socket_path`.
    /// - Creates the listener with 0600 permissions.
    /// - Spawns a daemon thread that accepts connections in a loop.
    ///
    /// Returns the server handle so the caller can inspect the socket path.
    pub fn start(storage: Arc<Storage>, socket_path: PathBuf) -> Result<Self, IpcError> {
        // Clean up stale socket from a previous crashed instance.
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Ensure parent directory exists.
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)?;

        // Restrict to owner read/write (0600).
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&socket_path, perms)?;

        let path_clone = socket_path.clone();
        thread::Builder::new()
            .name("ipc-server".into())
            .spawn(move || {
                Self::accept_loop(listener, storage, &path_clone);
            })
            .map_err(IpcError::Io)?;

        Ok(Self { socket_path })
    }

    /// Accept loop – runs until the listener is dropped or the process exits.
    fn accept_loop(listener: UnixListener, storage: Arc<Storage>, _socket_path: &Path) {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let storage = storage.clone();
                    thread::Builder::new()
                        .name("ipc-handler".into())
                        .spawn(move || {
                            Self::handle_connection(stream, &storage);
                        })
                        .ok();
                }
                Err(e) => {
                    eprintln!("[ipc] accept error: {e}");
                }
            }
        }
    }

    /// Read one JSON Lines request, dispatch, write one JSON Lines response.
    fn handle_connection(stream: UnixStream, storage: &Storage) {
        let peer = stream.peer_addr().ok();
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();

        // Read exactly one request line (the CLI sends one request per connection).
        match reader.read_line(&mut line) {
            Ok(0) => return, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("[ipc] read error from {peer:?}: {e}");
                return;
            }
        }

        let line = line.trim();
        if line.is_empty() {
            return;
        }

        let request: IpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let resp = IpcResponse::err(&IpcError::InvalidJson(e.to_string()));
                Self::write_response(&stream, &resp);
                return;
            }
        };

        let response = Self::dispatch(&request.cmd, &request.args, storage);
        Self::write_response(&stream, &response);
    }

    /// Serialize and write a single JSON Lines response.
    fn write_response(mut stream: &UnixStream, resp: &IpcResponse) {
        if let Ok(mut json) = serde_json::to_string(resp) {
            json.push('\n');
            let _ = stream.write_all(json.as_bytes());
            let _ = stream.flush();
        }
    }

    /// Route a parsed request to the appropriate handler.
    fn dispatch(cmd: &str, args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        match cmd {
            "list" => Self::cmd_list(args, storage),
            "search" => Self::cmd_search(args, storage),
            "paste" => Self::cmd_paste(args, storage),
            "pin" => Self::cmd_pin(args, storage),
            "tag" => Self::cmd_tag(args, storage),
            "delete" => Self::cmd_delete(args, storage),
            "status" => Self::cmd_status(storage),
            "add" => Self::cmd_add(args, storage),
            other => IpcResponse::err(&IpcError::UnknownCommand(Self::unknown_cmd_code(other))),
        }
    }

    /// Derive a stable numeric code for an unknown command name.
    fn unknown_cmd_code(name: &str) -> i32 {
        // Simple hash: sum of bytes mod 100 + 100, guaranteed > 4
        let sum: u32 = name.bytes().map(|b| b as u32).sum();
        (sum % 100 + 100) as i32
    }

    // ── Command handlers ──────────────────────────────────────────────

    /// `list` — return recent clipboard entries as summaries.
    /// Optional args: `{"kind":"text","tag":"work","query":"hello"}`
    fn cmd_list(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let kind = args
            .get("kind")
            .and_then(|v| v.as_str())
            .map(ClipboardKind::from);
        let tag = args.get("tag").and_then(|v| v.as_str());

        match storage.list_summaries_filtered(query, kind.as_ref(), tag) {
            Ok(entries) => IpcResponse::ok(serde_json::to_value(entries).unwrap_or_default()),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }

    /// `search` — alias for `list` with `query` required.
    fn cmd_search(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        // search is semantically the same as list with a query
        Self::cmd_list(args, storage)
    }

    /// `paste` — return the full content of an entry by id.
    /// Args: `{"id":123}`
    fn cmd_paste(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let id = match args.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => {
                return IpcResponse::err(&IpcError::InvalidJson("missing 'id' field".into()));
            }
        };

        match storage.get_entry(id) {
            Ok(Some(entry)) => {
                let _ = storage.increment_use_count(id);
                IpcResponse::ok(serde_json::to_value(&entry).unwrap_or_default())
            }
            Ok(None) => IpcResponse::err(&IpcError::NotFound),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }

    /// `pin` — toggle pin state of an entry.
    /// Args: `{"id":123}`
    fn cmd_pin(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let id = match args.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => {
                return IpcResponse::err(&IpcError::InvalidJson("missing 'id' field".into()));
            }
        };

        match storage.toggle_pin(id) {
            Ok(()) => IpcResponse::ok(serde_json::json!({"toggled": id})),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }

    /// `tag` — set tags on an entry.
    /// Args: `{"id":123,"tags":["work","important"]}`
    fn cmd_tag(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let id = match args.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => {
                return IpcResponse::err(&IpcError::InvalidJson("missing 'id' field".into()));
            }
        };

        let tags: Vec<String> = args
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        match storage.set_tags(id, &tags) {
            Ok(()) => IpcResponse::ok(serde_json::json!({"tagged": id, "tags": tags})),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }

    /// `delete` — remove an entry by id.
    /// Args: `{"id":123}`
    fn cmd_delete(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let id = match args.get("id").and_then(|v| v.as_i64()) {
            Some(id) => id,
            None => {
                return IpcResponse::err(&IpcError::InvalidJson("missing 'id' field".into()));
            }
        };

        match storage.delete(id) {
            Ok(()) => IpcResponse::ok(serde_json::json!({"deleted": id})),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }

    /// `status` — return basic server status.
    fn cmd_status(storage: &Storage) -> IpcResponse {
        let entry_count = storage.list_all_summaries().map(|v| v.len()).unwrap_or(0);
        let tags = storage.saved_tags().unwrap_or_default();

        let sync_device_id = storage
            .get_setting("sync.device_id")
            .ok()
            .flatten()
            .unwrap_or_default();

        IpcResponse::ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "entry_count": entry_count,
            "saved_tags": tags,
            "sync": {
                "enabled": !sync_device_id.is_empty(),
                "device_id": sync_device_id,
                "state": "idle",
            },
        }))
    }

    /// `add` — create a new clipboard entry from text.
    /// Args: `{"text":"hello world"}`
    fn cmd_add(args: &serde_json::Value, storage: &Storage) -> IpcResponse {
        let text = match args.get("text").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t,
            _ => {
                return IpcResponse::err(&IpcError::InvalidJson(
                    "missing or empty 'text' field".into(),
                ));
            }
        };

        let entry = match ClipboardEntry::captured_text(text.to_string(), "cli".to_string()) {
            Some(e) => e,
            None => {
                return IpcResponse::err(&IpcError::InvalidJson("empty or invalid text".into()));
            }
        };
        match storage.save_entry(&entry) {
            Ok(id) => IpcResponse::ok(serde_json::json!({"id": id})),
            Err(e) => IpcResponse::err(&IpcError::Storage(e.to_string())),
        }
    }
}

// ── Client helper (for tiez-cli) ─────────────────────────────────────

/// Send a single JSON Lines request to the IPC server and return the
/// parsed response. This is a convenience function for `tiez-cli`.
pub fn send_request(socket_path: &Path, request: &IpcRequest) -> Result<IpcResponse, IpcError> {
    let stream = UnixStream::connect(socket_path).map_err(|_| IpcError::ConnectionRefused)?;

    // Write request.
    let mut writer = &stream;
    let mut json =
        serde_json::to_string(request).map_err(|e| IpcError::InvalidJson(e.to_string()))?;
    json.push('\n');
    writer.write_all(json.as_bytes())?;
    writer.flush()?;

    // Read response.
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() {
        return Err(IpcError::InvalidJson("empty response".into()));
    }

    serde_json::from_str::<IpcResponse>(line).map_err(|e| IpcError::InvalidJson(e.to_string()))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_error_exit_codes() {
        assert_eq!(IpcError::ConnectionRefused.exit_code(), 2);
        assert_eq!(IpcError::InvalidJson("x".into()).exit_code(), 3);
        assert_eq!(IpcError::UnknownCommand(42).exit_code(), 42);
        assert_eq!(IpcError::NotFound.exit_code(), 5);
        assert_eq!(IpcError::IpcDisabled.exit_code(), 6);
        assert_eq!(IpcError::Timeout.exit_code(), 1);
        // Io maps to 1
        let io_err = IpcError::Io(std::io::Error::other("test"));
        assert_eq!(io_err.exit_code(), 1);
        assert_eq!(IpcError::Storage("x".into()).exit_code(), 1);
    }

    #[test]
    fn ipc_error_display() {
        let e = IpcError::ConnectionRefused;
        assert!(e.to_string().contains("tiez-slim"));

        let e = IpcError::InvalidJson("bad input".into());
        assert!(e.to_string().contains("bad input"));

        let e = IpcError::UnknownCommand(7);
        assert!(e.to_string().contains('7'));

        let e = IpcError::NotFound;
        assert!(e.to_string().contains("not found"));

        let e = IpcError::IpcDisabled;
        assert!(e.to_string().contains("disabled"));
    }

    #[test]
    fn ipc_request_deserialize() {
        let req: IpcRequest =
            serde_json::from_str(r#"{"cmd":"list","args":{"kind":"text"}}"#).unwrap();
        assert_eq!(req.cmd, "list");
        assert_eq!(req.args["kind"], "text");
    }

    #[test]
    fn ipc_request_missing_args_defaults() {
        let req: IpcRequest = serde_json::from_str(r#"{"cmd":"status"}"#).unwrap();
        assert_eq!(req.cmd, "status");
        assert_eq!(req.args, serde_json::Value::Null);
    }

    #[test]
    fn ipc_response_success_serializes() {
        let resp = IpcResponse::ok(serde_json::json!({"count": 42}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""ok":true"#));
        assert!(json.contains(r#""count":42"#));
        assert!(!json.contains("error"));
    }

    #[test]
    fn ipc_response_error_serializes() {
        let resp = IpcResponse::err(&IpcError::NotFound);
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""ok":false"#));
        assert!(json.contains(r#""code":5"#));
        assert!(!json.contains(r#""data""#));
    }

    #[test]
    fn ipc_error_body_roundtrip() {
        let body = IpcErrorBody {
            code: 3,
            message: "bad json".into(),
        };
        let json = serde_json::to_string(&body).unwrap();
        let parsed: IpcErrorBody = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, 3);
        assert_eq!(parsed.message, "bad json");
    }

    #[test]
    fn socket_path_default_uses_xdg_runtime() {
        let old = std::env::var("XDG_RUNTIME_DIR").ok();

        // SAFETY: test runs single-threaded in a temp context.
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        }
        let path = IpcServer::socket_path_default();
        assert_eq!(path, PathBuf::from("/run/user/1000/tiez-slim-linux.sock"));

        match old {
            Some(v) => unsafe { std::env::set_var("XDG_RUNTIME_DIR", v) },
            None => unsafe { std::env::remove_var("XDG_RUNTIME_DIR") },
        }
    }

    #[test]
    fn socket_path_default_fallback_to_tmp() {
        let old = std::env::var("XDG_RUNTIME_DIR").ok();
        // SAFETY: test runs single-threaded in a temp context.
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }

        let path = IpcServer::socket_path_default();
        let uid = unsafe { libc::getuid() };
        assert_eq!(path, PathBuf::from(format!("/tmp/tiez-slim-{uid}.sock")));

        if let Some(v) = old {
            unsafe {
                std::env::set_var("XDG_RUNTIME_DIR", v);
            }
        }
    }

    #[test]
    fn unknown_cmd_code_is_deterministic() {
        let code1 = IpcServer::unknown_cmd_code("foobar");
        let code2 = IpcServer::unknown_cmd_code("foobar");
        assert_eq!(code1, code2);
        // Must be >= 100 (guaranteed by implementation)
        assert!(code1 >= 100);
    }

    #[test]
    fn roundtrip_request_response_via_mock() {
        // Simulate the serialization path without a real socket.
        let req = IpcRequest {
            cmd: "list".into(),
            args: serde_json::json!({"query": "test"}),
        };
        let req_json = serde_json::to_string(&req).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&req_json).unwrap();
        assert_eq!(parsed.cmd, "list");

        let resp = IpcResponse::ok(serde_json::json!({"entries": []}));
        let resp_json = serde_json::to_string(&resp).unwrap();
        let parsed_resp: IpcResponse = serde_json::from_str(&resp_json).unwrap();
        assert!(parsed_resp.ok);
        assert!(parsed_resp.error.is_none());
    }

    #[test]
    fn server_start_and_status_roundtrip() {
        use std::time::Duration;

        // Create a temporary storage.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_ipc.db");
        let storage = Arc::new(Storage::open(db_path).unwrap());
        storage.cleanup_expired().unwrap();

        let sock_path = dir.path().join("test.sock");
        let server = IpcServer::start(storage, sock_path.clone()).unwrap();

        // Give the server thread a moment to bind.
        std::thread::sleep(Duration::from_millis(50));

        // Send a status request.
        let req = IpcRequest {
            cmd: "status".into(),
            args: serde_json::Value::Null,
        };
        let resp = send_request(&server.socket_path, &req).unwrap();
        assert!(resp.ok);
        let data = resp.data.unwrap();
        assert_eq!(data["version"], env!("CARGO_PKG_VERSION"));
        assert!(data["entry_count"].as_u64().is_some());

        // Cleanup.
        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn server_list_empty() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_ipc_list.db");
        let storage = Arc::new(Storage::open(db_path).unwrap());
        storage.cleanup_expired().unwrap();

        let sock_path = dir.path().join("test_list.sock");
        let server = IpcServer::start(storage, sock_path.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let req = IpcRequest {
            cmd: "list".into(),
            args: serde_json::Value::Null,
        };
        let resp = send_request(&server.socket_path, &req).unwrap();
        assert!(resp.ok);
        let entries = resp.data.unwrap();
        assert!(entries.as_array().unwrap().is_empty());

        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn server_unknown_command() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_ipc_unk.db");
        let storage = Arc::new(Storage::open(db_path).unwrap());
        storage.cleanup_expired().unwrap();

        let sock_path = dir.path().join("test_unk.sock");
        let server = IpcServer::start(storage, sock_path.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let req = IpcRequest {
            cmd: "nonexistent".into(),
            args: serde_json::Value::Null,
        };
        let resp = send_request(&server.socket_path, &req).unwrap();
        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(err.code >= 100); // unknown command codes >= 100
        assert!(err.message.contains("unknown"));

        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn server_delete_not_found() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_ipc_del.db");
        let storage = Arc::new(Storage::open(db_path).unwrap());
        storage.cleanup_expired().unwrap();

        let sock_path = dir.path().join("test_del.sock");
        let server = IpcServer::start(storage, sock_path.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        // Deleting non-existent id should succeed (SQLite DELETE is idempotent).
        let req = IpcRequest {
            cmd: "delete".into(),
            args: serde_json::json!({"id": 999999}),
        };
        let resp = send_request(&server.socket_path, &req).unwrap();
        assert!(resp.ok);

        let _ = std::fs::remove_file(&sock_path);
    }

    #[test]
    fn socket_permissions_are_0600() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_ipc_perm.db");
        let storage = Arc::new(Storage::open(db_path).unwrap());
        storage.cleanup_expired().unwrap();

        let sock_path = dir.path().join("test_perm.sock");
        let _server = IpcServer::start(storage, sock_path.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let meta = std::fs::metadata(&sock_path).unwrap();
        let mode = meta.permissions().mode() & 0o7777;
        assert_eq!(
            mode, 0o600,
            "socket permissions should be 0600, got {mode:o}"
        );

        let _ = std::fs::remove_file(&sock_path);
    }
}
