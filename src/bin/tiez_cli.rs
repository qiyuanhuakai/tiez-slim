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

#[derive(Parser)]
#[command(
    name = "tiez-cli",
    about = "tiez-slim clipboard manager CLI",
    version
)]
struct Cli {
    /// Output in JSON format (machine-readable, jq-parsable).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: SubCommand,
}

#[derive(Subcommand)]
enum SubCommand {
    /// List recent clipboard entries.
    List {
        /// Filter by kind (text, url, code, file, image, video, rich_text).
        #[arg(long)]
        kind: Option<String>,
        /// Filter by tag.
        #[arg(long)]
        tag: Option<String>,
    },
    /// Search clipboard history.
    Search {
        /// Search query.
        query: String,
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
        tags: Vec<String>,
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
        SubCommand::List { kind, tag } => cmd_list(&socket_path, kind, tag, cli.json),
        SubCommand::Search { query } => cmd_search(&socket_path, &query, cli.json),
        SubCommand::Paste { id, rich } => cmd_paste(&socket_path, id, rich, cli.json),
        SubCommand::Pin { id, unpin } => cmd_pin(&socket_path, id, unpin),
        SubCommand::Tag { id, tags } => cmd_tag(&socket_path, id, &tags),
        SubCommand::Delete { id } => cmd_delete(&socket_path, id),
        SubCommand::Status => cmd_status(&socket_path, cli.json),
        SubCommand::Add { content, entry_type } => {
            cmd_add(&socket_path, &content, entry_type, cli.json)
        }
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
    kind: Option<String>,
    tag: Option<String>,
    json_output: bool,
) -> i32 {
    let mut args = serde_json::Map::new();
    if let Some(k) = &kind {
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
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
    } else {
        let entries: Vec<ClipboardEntrySummary> =
            serde_json::from_value(data).unwrap_or_default();
        if entries.is_empty() {
            println!("{}", t!("cli.history_list_hint"));
        } else {
            for entry in &entries {
                println_entry(entry);
            }
            println!("\n{}: {}", t!("history.count", count = entries.len()), entries.len());
        }
    }
    0
}

fn cmd_search(
    socket_path: &std::path::Path,
    query: &str,
    json_output: bool,
) -> i32 {
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
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
    } else {
        let entries: Vec<ClipboardEntrySummary> =
            serde_json::from_value(data).unwrap_or_default();
        if entries.is_empty() {
            println!("{}", t!("search.no_match"));
        } else {
            for entry in &entries {
                println_entry(entry);
            }
            println!("\n{}: {}", t!("history.count", count = entries.len()), entries.len());
        }
    }
    0
}

fn cmd_paste(
    socket_path: &std::path::Path,
    id: i64,
    rich: bool,
    json_output: bool,
) -> i32 {
    let args = serde_json::json!({ "id": id });

    let resp = match send_ipc(socket_path, "paste", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
    } else if rich {
        let html = data
            .get("html_content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if html.is_empty() {
            let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
            print!("{content}");
        } else {
            print!("{html}");
        }
    } else {
        let content = data.get("content").and_then(|v| v.as_str()).unwrap_or("");
        print!("{content}");
    }
    0
}

fn cmd_pin(
    socket_path: &std::path::Path,
    id: i64,
    _unpin: bool,
) -> i32 {
    let args = serde_json::json!({ "id": id });

    let resp = match send_ipc(socket_path, "pin", args) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let action = if _unpin { t!("common.unpin") } else { t!("common.pin") };
    println!("{action} id={id}");
    0
}

fn cmd_tag(
    socket_path: &std::path::Path,
    id: i64,
    tags: &[String],
) -> i32 {
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

fn cmd_status(
    socket_path: &std::path::Path,
    json_output: bool,
) -> i32 {
    let resp = match send_ipc(socket_path, "status", serde_json::Value::Null) {
        Ok(r) => r,
        Err(e) => return handle_error(&e),
    };

    if !resp.ok {
        return handle_ipc_response_error(&resp);
    }

    let data = resp.data.unwrap_or_default();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
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
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
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
    let msg = match err {
        IpcError::ConnectionRefused => {
            t!("cli.error_db_connect", err = err.to_string()).to_string()
        }
        IpcError::InvalidJson(detail) => {
            t!("cli.error_parse", err = detail).to_string()
        }
        IpcError::NotFound => t!("cli.error_not_found", id = "?").to_string(),
        IpcError::Io(io_err) => {
            t!("cli.error_io", err = io_err.to_string()).to_string()
        }
        _ => {
            t!("cli.error_io", err = err.to_string()).to_string()
        }
    };
    eprintln!("{msg}");
    err.exit_code()
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
        code
    } else {
        eprintln!("{}", t!("cli.error_io", err = "unknown error"));
        1
    }
}

// ── Display helpers ───────────────────────────────────────────────────

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
        assert!(matches!(cli.command, SubCommand::List { kind: None, tag: None }));
    }

    #[test]
    fn cli_args_parse_list_with_filters() {
        let cli = Cli::parse_from(["tiez-cli", "--json", "list", "--kind", "text", "--tag", "work"]);
        assert!(cli.json);
        if let SubCommand::List { kind, tag } = cli.command {
            assert_eq!(kind.as_deref(), Some("text"));
            assert_eq!(tag.as_deref(), Some("work"));
        } else {
            panic!("expected List subcommand");
        }
    }

    #[test]
    fn cli_args_parse_search() {
        let cli = Cli::parse_from(["tiez-cli", "search", "hello world"]);
        if let SubCommand::Search { query } = cli.command {
            assert_eq!(query, "hello world");
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
        if let SubCommand::Tag { id, tags } = cli.command {
            assert_eq!(id, 3);
            assert_eq!(tags, vec!["work", "important"]);
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
        if let SubCommand::Add { content, entry_type } = cli.command {
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
}
