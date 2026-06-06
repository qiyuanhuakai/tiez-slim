# PROJECT KNOWLEDGE BASE

**Generated:** 2026-05-31 16:05:38 CST
**Commit:** none yet
**Branch:** unborn HEAD

## OVERVIEW

`tiez-slim-linux` is a Rust native clipboard manager. The original upstream is `https://github.com/jimuzhe/tiez-clipboard`; this project is migrated from the user's heavily modified branch at `https://github.com/qiyuanhuakai/tiez-clipboard` (also available locally at `../tiez-clipboard`) to remove React/Tauri/WebView overhead. The app is a single `eframe/egui` binary with SQLite persistence, clipboard polling, CJK font handling, and Linux/X11 platform integration. UI-facing app name should be the shorter `tiez-slim`.

## STRUCTURE

```text
tiez-slim-linux/
├── Cargo.toml        # Rust 2024 binary crate, GUI + SQLite dependencies
├── README.md         # Chinese project overview, run commands, platform notes
└── src/
    ├── main.rs       # opens storage, cleanup, launches eframe
    ├── app.rs        # main UI/controller; largest file
    ├── clipboard.rs  # arboard polling watcher and write-back
    ├── emoji_data.rs # generated Twemoji + Unicode CLDR emoji grouping data
    ├── model.rs      # entry model, kind detection, sensitivity heuristics
    ├── storage.rs    # rusqlite schema, queries, tags, tests
    ├── ui/           # macOS-style theme tokens and custom widgets
    └── platform/     # cfg-selected Linux/Windows platform hooks
```

Generated/cache directories such as `.zig-cache/`, `.dvui-cache/`, `.opencode/`, `.sisyphus/`, and `target/` are not project source.

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| Startup, dev-mode flag, window size | `src/main.rs` | `--dev`, `TIEZ_SLIM_LINUX_DEV=1`, legacy `MYCLIPBOARD_DEV=1`, or `devtools` feature enables debug UI. |
| Clipboard capture path | `src/clipboard.rs` -> `src/model.rs` -> `src/storage.rs` | Polls text every 700 ms; deduplication happens in storage. |
| Main UI, shortcuts, filters, preferences | `src/app.rs` | Large `egui` surface; preserve dense TieZ-style layout. |
| Emoji grouping data | `src/emoji_data.rs` | Auto-generated from `twemoji-assets` + Unicode `emoji-test.txt`; regenerate with `scripts/generate_emoji_data.py`, do not hand-edit. |
| Theme tokens, macOS-style widgets | `src/ui/` | `MacosTokens` for light/dark palettes; `macos_toggle` and `macos_range_slider` for settings UI. |
| Schema, migrations, tags, retention | `src/storage.rs` | Only current unit tests live here. |
| Kind/sensitive detection | `src/model.rs` | `MAX_ENTRIES`, `MAX_CONTENT_BYTES`, `RETENTION_DAYS` live here; 5 built-in sensitive kinds: phone, idcard, email, secret, password. |
| Platform capabilities | `src/platform/` | Linux has X11 active-window support; Windows is a placeholder. |

## CODE MAP

| Symbol | Type | Location | Role |
| --- | --- | --- | --- |
| `main` | function | `src/main.rs` | Open DB, clean expired rows, launch `ClipboardApp`. |
| `ClipboardApp` | struct | `src/app.rs` | Owns UI state, event receiver, filters, settings, and debug counters. |
| `start_watcher` | function | `src/clipboard.rs` | Spawns `clipboard-watcher` thread. |
| `ClipboardEntry::captured_text` | constructor | `src/model.rs` | Trims, bounds, classifies, previews captured text. |
| `Storage` | struct | `src/storage.rs` | SQLite connection owner and persistence API. |
| `MacosTokens` | struct | `src/ui/theme.rs` | Light/dark palette constants derived from TieZ CSS variables. |
| `macos_toggle` | function | `src/ui/widgets.rs` | macOS-style toggle switch used across settings pages. |
| `PlatformCapabilities` | struct | `src/platform/mod.rs` | Text surfaced in settings/platform notes. |

## CONVENTIONS

- User-facing docs and many status/error strings are Chinese; keep new user-visible copy consistent.
- UI is native `egui`, not web. Do not introduce Tauri/WebView/React paths for this app.
- `rusqlite` uses the `bundled` feature; tests create temporary SQLite files instead of mocks.
- Clipboard support is text-first through `arboard`; image/file/rich clipboard formats are roadmap items, not implemented runtime behavior.
- Linux-specific behavior assumes X11 APIs where available and falls back to desktop environment names.

## COMMANDS

```bash
cargo run
cargo run -- --dev
TIEZ_SLIM_LINUX_DEV=1 cargo run
scripts/generate_emoji_data.py
cargo test
cargo build --release
```

## NOTES

- There is no root CI workflow and no custom test runner.
- The repository currently has no Git commit; `HEAD` is unborn.
- `src/app.rs` is the main complexity hotspot. Prefer small behavior-preserving edits there and verify the GUI path when changing interaction code.
- `src/storage.rs` owns persistence invariants: max 1000 entries, 30-day cleanup for non-pinned rows, tag cleanup, and migration coverage.

## 同步来源

This project is migrated from the user's `qiyuanhuakai/tiez-clipboard` branch (local `../tiez-clipboard`), whose original upstream is `jimuzhe/tiez-clipboard` (React + Tauri 2 + WebView). Key sync points:

| Feature | TieZ Source | tiez-slim-linux Implementation |
| --- | --- | --- |
| macOS theme tokens | CSS variables in `theme.tokens.css` | `src/ui/theme.rs` (`MacosTokens`) |
| Toggle switch widget | React component | `src/ui/widgets.rs` (`macos_toggle`) |
| Sensitive detection | JavaScript heuristics | `src/model.rs` (`looks_sensitive_with_rules`) |
| Dark/light mode | CSS class `.dark-mode` | `resolve_theme()` in `src/app.rs` |

The `MacosTokens` struct maps TieZ CSS variables to Rust constants for consistent visual appearance across light and dark modes.
