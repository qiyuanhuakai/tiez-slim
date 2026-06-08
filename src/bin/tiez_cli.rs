//! `tiez-cli` — command-line interface for the tiez-slim clipboard manager.
//!
//! Communicates with a running `tiez-slim` GUI instance over a Unix domain
//! socket using the JSON Lines IPC protocol defined in `src/ipc.rs`.

rust_i18n::i18n!("locales", fallback = "en-US");

use clap::{Parser, Subcommand};
use rust_i18n::t;
use tiez_slim_linux::ipc::{IpcError, IpcRequest, IpcResponse, IpcServer};
use tiez_slim_linux::model::ClipboardEntrySummary;

// ── CLI definition ────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "tiez-cli", about = "tiez-slim clipboard manager CLI", version)]
struct Cli {
    /// Output in JSON format (machine-readable, jq-parsable).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Subcommand, Debug)]
enum SubCommand {
    /// List recent clipboard entries.
    List {
        /// Maximum number of entries to show.
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by kind (text, url, code, file, image, video, rich_text).
        #[arg(long = "type")]
        type_: Option<String>,
        /// Filter by tag.
        #[arg(long)]
        tag: Option<String>,
    },
    /// Search clipboard history.
    Search {
        /// Search query.
        query: String,
        /// Search mode (reserved for future use).
        #[arg(long)]
        mode: Option<String>,
    },
    /// Print the full content of an entry.
    Paste {
        /// Entry ID.
        id: i64,
        /// Output rich content (HTML) if available.
        #[arg(long)]
        rich: bool,
    },
    /// Toggle pin state of an entry.
    Pin {
        /// Entry ID.
        id: i64,
        /// Unpin instead of pin.
        #[arg(long)]
        unpin: bool,
    },
    /// Set tags on an entry.
    Tag {
        /// Entry ID.
        id: i64,
        /// Tags to set (replaces existing).
        tag: Vec<String>,
    },
    /// Delete an entry.
    Delete {
        /// Entry ID.
        id: i64,
    },
    /// Show tiez-slim server status.
    Status,
    /// Add a new clipboard entry.
    Add {
        /// Text content to add.
        content: String,
        /// Entry type hint (text, url, code, etc.).
        #[arg(long = "type")]
        entry_type: Option<String>,
    },
}

// ── Main ──────────────────────────────────────────────────────────────

fn main() {
    detect_locale();
    let cli = Cli::parse();
    let socket_path = IpcServer::socket_path_default();

    let exit_code = match cli.command {
        SubCommand::List {
            limit,
            type_,
            tag,
        } => cmd_list(&socket_path, limit, type_, tag, cli.json),
        SubCommand::Search { query, mode: _ } => {
            cmd_search(&socket_path, &query, cli.json)
        }
        SubCommand::Paste { id, rich } => cmd_paste(&socket_path, id, rich, cli.json),
        SubCommand::Pin { id, unpin } => cmd_pin(&socket_path, id, unpin),
        SubCommand::Tag { id, tag } => cmd_tag(&socket_path, id, &tag),
        SubCommand::Delete { id } => cmd_delete(&socket_path, id),
        SubCommand::Status => cmd_status(&socket_path, cli.json),
        SubCommand::Add {
            content,
            entry_type,
        } => cmd_add(&socket_path, &content, entry_type, cli.json),
    };

    std::process::exit(exit_code);
}

// ── Locale detection ──────────────────────────────────────────────────

fn detect_locale() {
    let lang = std::env::var("TIEZ_SLIM_LANG")
        .or_else(|_| std::env::var("LANG"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .unwrap_or_default();

    let locale = if lang.starts_with("zh") {
        "zh-CN"
    } else {
        "en-US"
    };
    rust_i18n::set_locale(locale);
}

// ── Command implementations ───────────────────────────────────────────

fn cmd_list(
    socket_path: &std::path::Path,
    limit: Option<usize>,
    type_: Option<String>,
    tag: Option<String>,
    json_output: bool,
) -> i32 {
    let mut args = serde_json::Map::new();
    if let Some(k) = &type_ {
        args.insert("kind".into(), serde_json::Value::String(k.clone()));
    }
    if let Some(t) = &tag {
        args.insert("tag".into(), serde_json::Value::String(t.clone()));
    }

    let resp = match send_ipc(socket_path, "list", serde_json::Value::Object(args)) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        let mut entries: serde_json::Value = data;
        if let Some(n) = limit {
            if let Some(arr) = entries.as_array_mut() {
                arr.truncate(n);
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
    } else {
        let mut entries: Vec<ClipboardEntrySummary> =
            serde_json::from_value(data).unwrap_or_default();
        if let Some(n) = limit {
            entries.truncate(n);
        }
        if entries.is_empty() {
            println!("{}", t!("cli.history_list_hint"));
        } else {
            for entry in &entries {
                println_entry(entry);
            }
            println!("\n{}", t!("history.count", count = entries.len()));
        }
    }
    0
}

fn cmd_search(socket_path: &std::path::Path, query: &str, json_output: bool) -> i32 {
    let args = serde_json::json!({ "query": query });

    let resp = match send_ipc(socket_path, "search", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_default()
        );
    } else {
        let entries: Vec<ClipboardEntrySummary> = serde_json::from_value(data).unwrap_or_default();
        if entries.is_empty() {
            println!("{}", t!("search.no_match"));
        } else {
            for entry in &entries {
                println_entry(entry);
            }
            println!("\n{}", t!("history.count", count = entries.len()));
        }
    }
    0
}

fn cmd_paste(socket_path: &std::path::Path, id: i64, rich: bool, json_output: bool) -> i32 {
    let args = serde_json::json!({ "id": id });

    let resp = match send_ipc(socket_path, "paste", args) {
        Ok(r) => r,
        Err(IpcError::ConnectionRefused) => return paste_from_db(id, rich),
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_default()
        );
    } else {
        let content = if rich {
            data.get("html_content")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| data.get("content").and_then(|v| v.as_str()))
                .unwrap_or("")
        } else {
            data.get("content").and_then(|v| v.as_str()).unwrap_or("")
        };
        if let Err(e) = write_to_clipboard(content) {
            eprintln!("Error: {e}");
            return 1;
        }
        println!("Copied entry #{id} to clipboard ({} chars)", content.len());
    }
    0
}

fn cmd_pin(socket_path: &std::path::Path, id: i64, _unpin: bool) -> i32 {
    let args = serde_json::json!({ "id": id });

    let resp = match send_ipc(socket_path, "pin", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let action = if _unpin {
        t!("common.unpin")
    } else {
        t!("common.pin")
    };
    println!("{action} id={id}");
    0
}

fn cmd_tag(socket_path: &std::path::Path, id: i64, tags: &[String]) -> i32 {
    let args = serde_json::json!({
        "id": id,
        "tags": tags,
    });

    let resp = match send_ipc(socket_path, "tag", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let tags_str = if tags.is_empty() {
        "—".to_string()
    } else {
        tags.join(", ")
    };
    println!("id={id} tags=[{tags_str}]");
    0
}

fn cmd_delete(socket_path: &std::path::Path, id: i64) -> i32 {
    let args = serde_json::json!({ "id": id });

    let resp = match send_ipc(socket_path, "delete", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    println!("{}", t!("history.deleted_record"));
    0
}

fn cmd_status(socket_path: &std::path::Path, json_output: bool) -> i32 {
    let resp = match send_ipc(socket_path, "status", serde_json::Value::Null) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_default()
        );
    } else {
        let version = data
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let entry_count = data
            .get("entry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let tags = data
            .get("saved_tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "—".to_string());

        println!("{} {} (v{version})", t!("app.name"), t!("app.title"));
        println!("  {}: v{version}", t!("cli.title"));
        println!("  {}: {entry_count}", t!("cli.history"));
        println!("  {}: [{tags}]", t!("history.tag_label"));

        if let Some(sync) = data.get("sync") {
            let enabled = sync.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            let device_id = sync
                .get("device_id")
                .and_then(|v| v.as_str())
                .unwrap_or("—");
            let state = sync.get("state").and_then(|v| v.as_str()).unwrap_or("disabled");
            println!("  Sync (KDE Connect):");
            println!("    enabled: {enabled}");
            println!("    device_id: {device_id}");
            println!("    state: {state}");
        }
    }
    0
}

fn cmd_add(
    socket_path: &std::path::Path,
    content: &str,
    _entry_type: Option<String>,
    json_output: bool,
) -> i32 {
    let args = serde_json::json!({ "text": content });

    let resp = match send_ipc(socket_path, "add", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&data).unwrap_or_default()
        );
    } else {
        let new_id = data.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        println!("{}: id={new_id}", t!("cli.success_done"));
    }
    0
}

// ── IPC helpers ───────────────────────────────────────────────────────

fn send_ipc(
    socket_path: &std::path::Path,
    cmd: &str,
    args: serde_json::Value,
) -> Result<IpcResponse, IpcError> {
    let request = IpcRequest {
        cmd: cmd.to_string(),
        args,
    };
    tiez_slim_linux::ipc::send_request(socket_path, &request)
}

fn handle_error(err: &IpcError) -> i32 {
    match err {
        IpcError::ConnectionRefused => {
            eprintln!("Error: connection refused – GUI not running.");
            eprintln!("Start with: systemctl --user start tiez-slim-linux");
            2
        }
        IpcError::InvalidJson(detail) => {
            eprintln!("{}", t!("cli.error_parse", err = detail));
            3
        }
        IpcError::NotFound => {
            eprintln!("{}", t!("cli.error_not_found", id = "?"));
            5
        }
        IpcError::UnknownCommand(code) => {
            eprintln!("Error: unknown command (code {code})");
            4
        }
        IpcError::Io(io_err) => {
            eprintln!("{}", t!("cli.error_io", err = io_err.to_string()));
            1
        }
        _ => {
            eprintln!("Error: {err}");
            err.exit_code()
        }
    }
}

fn handle_ipc_response_error(resp: &IpcResponse) -> i32 {
    if let Some(err_body) = &resp.error {
        let code = err_body.code;
        let msg = if code == 5 {
            t!("cli.error_not_found", id = "?").to_string()
        } else {
            err_body.message.clone()
        };
        eprintln!("{msg}");
        // Server codes >= 100 indicate unknown command → CLI exit 4
        if code >= 100 {
            4
        } else {
            code
        }
    } else {
        eprintln!("{}", t!("cli.error_io", err = "unknown error"));
        1
    }
}

// ── Display helpers ───────────────────────────────────────────────────

/// Paste fallback: open the database directly when GUI is not running.
fn paste_from_db(id: i64, rich: bool) -> i32 {
    use tiez_slim_linux::storage::Storage;

    let db_path = Storage::path_from_redirect_file().unwrap_or_else(Storage::default_path);
    let storage = match Storage::open(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: cannot open database: {e}");
            return 1;
        }
    };
    match storage.get_entry(id) {
        Ok(Some(entry)) => {
            let content = if rich {
                entry.html_content.as_deref().unwrap_or(&entry.content)
            } else {
                &entry.content
            };
            if let Err(e) = write_to_clipboard(content) {
                eprintln!("Error: {e}");
                return 1;
            }
            let _ = storage.increment_use_count(id);
            println!("Copied entry #{id} to clipboard ({} chars)", content.len());
            0
        }
        Ok(None) => {
            eprintln!("Error: entry #{id} not found");
            5
        }
        Err(e) => {
            eprintln!("Error: database error: {e}");
            1
        }
    }
}

/// Write text to the system clipboard.
///
/// Tries `wl-copy` (Wayland), then `xclip` (X11), then falls back to
/// `arboard`. Content may be lost on X11 after process exit with arboard.
fn write_to_clipboard(content: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
        if let Some(stdin) = child.stdin.as_mut() {
            if stdin.write_all(content.as_bytes()).is_ok() {
                drop(child.stdin.take());
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return Ok(());
                }
            }
        }
    }

    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(stdin) = child.stdin.as_mut() {
            if stdin.write_all(content.as_bytes()).is_ok() {
                drop(child.stdin.take());
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return Ok(());
                }
            }
        }
    }

    let mut cb =
        arboard::Clipboard::new().map_err(|e| format!("clipboard not available: {e}"))?;
    cb.set_text(content.to_owned())
        .map_err(|e| format!("failed to set clipboard: {e}"))?;
    Ok(())
}

fn println_entry(entry: &ClipboardEntrySummary) {
    let pin_marker = if entry.is_pinned { "📌 " } else { "" };
    let kind_label = entry.kind.label();
    let preview = truncate_str(&entry.preview, 120);
    let tags = if entry.tags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", entry.tags.join(", "))
    };
    println!(
        "  {pin_marker}#{:<5} {:<8} {}{}",
        entry.id, kind_label, preview, tags
    );
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_error_exit_codes_match() {
        assert_eq!(IpcError::ConnectionRefused.exit_code(), 2);
        assert_eq!(IpcError::InvalidJson("x".into()).exit_code(), 3);
        assert_eq!(IpcError::NotFound.exit_code(), 5);
        assert_eq!(IpcError::IpcDisabled.exit_code(), 6);
        assert_eq!(IpcError::Timeout.exit_code(), 1);
    }

    #[test]
    fn cli_args_parse_list() {
        let cli = Cli::parse_from(["tiez-cli", "list"]);
        assert!(!cli.json);
        assert!(matches!(
            cli.command,
            SubCommand::List {
                limit: None,
                type_: None,
                tag: None
            }
        ));
    }

    #[test]
    fn cli_args_parse_list_with_filters() {
        let cli = Cli::parse_from([
            "tiez-cli", "--json", "list", "--limit", "5", "--type", "text", "--tag", "work",
        ]);
        assert!(cli.json);
        if let SubCommand::List { limit, type_, tag } = cli.command {
            assert_eq!(limit, Some(5));
            assert_eq!(type_.as_deref(), Some("text"));
            assert_eq!(tag.as_deref(), Some("work"));
        } else {
            panic!("expected List subcommand");
        }
    }

    #[test]
    fn cli_args_parse_search() {
        let cli = Cli::parse_from(["tiez-cli", "search", "hello world"]);
        if let SubCommand::Search { query, mode } = cli.command {
            assert_eq!(query, "hello world");
            assert!(mode.is_none());
        } else {
            panic!("expected Search subcommand");
        }
    }

    #[test]
    fn cli_args_parse_search_with_mode() {
        let cli = Cli::parse_from(["tiez-cli", "search", "test", "--mode", "fuzzy"]);
        if let SubCommand::Search { query, mode } = cli.command {
            assert_eq!(query, "test");
            assert_eq!(mode.as_deref(), Some("fuzzy"));
        } else {
            panic!("expected Search subcommand");
        }
    }

    #[test]
    fn cli_args_parse_paste() {
        let cli = Cli::parse_from(["tiez-cli", "paste", "42", "--rich"]);
        if let SubCommand::Paste { id, rich } = cli.command {
            assert_eq!(id, 42);
            assert!(rich);
        } else {
            panic!("expected Paste subcommand");
        }
    }

    #[test]
    fn cli_args_parse_pin_unpin() {
        let cli = Cli::parse_from(["tiez-cli", "pin", "7", "--unpin"]);
        if let SubCommand::Pin { id, unpin } = cli.command {
            assert_eq!(id, 7);
            assert!(unpin);
        } else {
            panic!("expected Pin subcommand");
        }
    }

    #[test]
    fn cli_args_parse_tag() {
        let cli = Cli::parse_from(["tiez-cli", "tag", "3", "work", "important"]);
        if let SubCommand::Tag { id, tag } = cli.command {
            assert_eq!(id, 3);
            assert_eq!(tag, vec!["work", "important"]);
        } else {
            panic!("expected Tag subcommand");
        }
    }

    #[test]
    fn cli_args_parse_delete() {
        let cli = Cli::parse_from(["tiez-cli", "delete", "99"]);
        if let SubCommand::Delete { id } = cli.command {
            assert_eq!(id, 99);
        } else {
            panic!("expected Delete subcommand");
        }
    }

    #[test]
    fn cli_args_parse_status() {
        let cli = Cli::parse_from(["tiez-cli", "--json", "status"]);
        assert!(cli.json);
        assert!(matches!(cli.command, SubCommand::Status));
    }

    #[test]
    fn cli_args_parse_add() {
        let cli = Cli::parse_from(["tiez-cli", "add", "hello world", "--type", "text"]);
        if let SubCommand::Add {
            content,
            entry_type,
        } = cli.command
        {
            assert_eq!(content, "hello world");
            assert_eq!(entry_type.as_deref(), Some("text"));
        } else {
            panic!("expected Add subcommand");
        }
    }

    #[test]
    fn json_output_roundtrip() {
        let resp = IpcResponse::ok(serde_json::json!({"version": "0.2.0", "entry_count": 5}));
        let json = serde_json::to_string_pretty(&resp.data.unwrap()).unwrap();
        assert!(json.contains("0.2.0"));
        assert!(json.contains("5"));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"], "0.2.0");
    }

    #[test]
    fn truncate_str_short_string() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn truncate_str_long_string() {
        let result = truncate_str("a".repeat(200).as_str(), 50);
        assert!(result.len() <= 50 + 3);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_str_exact_boundary() {
        let s = "a".repeat(120);
        assert_eq!(truncate_str(&s, 120), s);
    }

    #[test]
    fn error_exit_code_connection_refused() {
        let code = handle_error(&IpcError::ConnectionRefused);
        assert_eq!(code, 2);
    }

    #[test]
    fn error_exit_code_not_found() {
        let code = handle_error(&IpcError::NotFound);
        assert_eq!(code, 5);
    }

    #[test]
    fn ipc_response_error_code_reflected() {
        let resp = IpcResponse {
            ok: false,
            data: None,
            error: Some(tiez_slim_linux::ipc::IpcErrorBody {
                code: 5,
                message: "entry not found".into(),
            }),
        };
        let code = handle_ipc_response_error(&resp);
        assert_eq!(code, 5);
    }

    #[test]
    fn unknown_command_exit_code_is_4() {
        let resp = IpcResponse {
            ok: false,
            data: None,
            error: Some(tiez_slim_linux::ipc::IpcErrorBody {
                code: 112,
                message: "unknown command (code 112)".into(),
            }),
        };
        let code = handle_ipc_response_error(&resp);
        assert_eq!(code, 4);
    }

    #[test]
    fn help_shows_all_subcommands() {
        let err = Cli::try_parse_from(["tiez-cli", "--help"]).unwrap_err();
        let help = err.to_string();
        for sub in &["list", "search", "paste", "pin", "tag", "delete", "status", "add"] {
            assert!(help.contains(sub), "help missing subcommand: {sub}");
        }
    }
}
