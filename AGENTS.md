# PROJECT KNOWLEDGE BASE

**Generated:** 2026-05-31 16:05:38 CST
**Commit:** none yet
**Branch:** unborn HEAD

## OVERVIEW

Rust native clipboard manager migrated from `../tiez-clipboard` to remove React/Tauri/WebView overhead. The app is a single `eframe/egui` binary with SQLite persistence, text-first clipboard polling, CJK font handling, and Linux/X11 platform integration.

## STRUCTURE

```text
myclipboard/
├── Cargo.toml        # Rust 2024 binary crate, GUI + SQLite dependencies
├── README.md         # Chinese project overview, run commands, platform notes
└── src/
    ├── main.rs       # opens storage, cleanup, launches eframe
    ├── app.rs        # main UI/controller; largest file
    ├── clipboard.rs  # arboard polling watcher and write-back
    ├── model.rs      # entry model, kind detection, sensitivity heuristics
    ├── storage.rs    # rusqlite schema, queries, tags, tests
    └── platform/     # cfg-selected Linux/Windows platform hooks
```

Generated/cache directories such as `.zig-cache/`, `.dvui-cache/`, `.opencode/`, `.sisyphus/`, and `target/` are not project source.

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| Startup, dev-mode flag, window size | `src/main.rs` | `--dev`, `MYCLIPBOARD_DEV=1`, or `devtools` feature enables debug UI. |
| Clipboard capture path | `src/clipboard.rs` -> `src/model.rs` -> `src/storage.rs` | Polls text every 700 ms; deduplication happens in storage. |
| Main UI, shortcuts, filters, preferences | `src/app.rs` | Large `egui` surface; preserve dense TieZ-style layout. |
| Schema, migrations, tags, retention | `src/storage.rs` | Only current unit tests live here. |
| Kind/sensitive detection | `src/model.rs` | `MAX_ENTRIES`, `MAX_CONTENT_BYTES`, `RETENTION_DAYS` live here. |
| Platform capabilities | `src/platform/` | Linux has X11 active-window support; Windows is a placeholder. |

## CODE MAP

| Symbol | Type | Location | Role |
| --- | --- | --- | --- |
| `main` | function | `src/main.rs` | Open DB, clean expired rows, launch `ClipboardApp`. |
| `ClipboardApp` | struct | `src/app.rs` | Owns UI state, event receiver, filters, settings, and debug counters. |
| `start_watcher` | function | `src/clipboard.rs` | Spawns `clipboard-watcher` thread. |
| `ClipboardEntry::captured_text` | constructor | `src/model.rs` | Trims, bounds, classifies, previews captured text. |
| `Storage` | struct | `src/storage.rs` | SQLite connection owner and persistence API. |
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
MYCLIPBOARD_DEV=1 cargo run
cargo test
cargo build --release
```

## NOTES

- There is no root CI workflow and no custom test runner.
- The repository currently has no Git commit; `HEAD` is unborn.
- `src/app.rs` is the main complexity hotspot. Prefer small behavior-preserving edits there and verify the GUI path when changing interaction code.
- `src/storage.rs` owns persistence invariants: max 1000 entries, 30-day cleanup for non-pinned rows, tag cleanup, and migration coverage.
