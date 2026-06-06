# SRC KNOWLEDGE BASE

## OVERVIEW

`src/` contains the full Rust app: startup, clipboard watcher, model heuristics, SQLite persistence, egui UI, and platform hooks.

## WHERE TO LOOK

| Task | Location | Notes |
| --- | --- | --- |
| App bootstrap | `main.rs` | Keep startup small: open storage, cleanup, configure `eframe::NativeOptions`, launch app. |
| UI rendering and commands | `app.rs` | `ClipboardApp` methods are grouped by state updates first, drawing later, helpers at the bottom. |
| Emoji grouping data | `emoji_data.rs` | Auto-generated Twemoji + Unicode `emoji-test.txt` / CLDR groups; regenerate with `../scripts/generate_emoji_data.py`, do not hand-edit. |
| Clipboard polling | `clipboard.rs` | Watcher thread sends `ClipboardEvent::Captured` or `ClipboardEvent::Error` over `crossbeam_channel`. |
| Entry classification | `model.rs` | URL/file/data URL/code/sensitive heuristics are string-based and intentionally conservative. |
| SQLite persistence | `storage.rs` | Schema creation/migration, settings table, entries/tags queries, and storage tests. |
| OS behavior | `platform/mod.rs` | `cfg(target_os)` re-exports Linux/Windows implementations and generic fallback functions. |

## RUNTIME FLOW

1. `main.rs` calls `Storage::open_default()` and `cleanup_expired()`.
2. `ClipboardApp::new` configures fonts/style, loads preferences/tags, starts `start_watcher`.
3. `clipboard.rs` polls `arboard::Clipboard::get_text()` and builds `ClipboardEntry::captured_text`.
4. `ClipboardApp::drain_events` saves captures through `Storage`, refreshes entries, and updates status counters.
5. UI actions call storage methods for copy count, delete, pin, tag, clear, and preference persistence.

## CONVENTIONS

- Preserve `anyhow::Context` at top-level fallible startup; lower-level UI/storage methods usually return displayable status strings or `rusqlite::Result`.
- `ClipboardKind::as_str()` values are persisted. Treat them as schema-facing strings.
- `ClipboardEntry::captured_text` rejects empty text and values over `MAX_CONTENT_BYTES`; keep this before save.
- Sensitive masking combines tag names (`sensitive`, `密码`, `password`, `secret`) with content heuristics.
- Settings are stored in SQLite as JSON strings keyed by constants, including `PREFERENCES_KEY` in `app.rs`.
- `emoji_data.rs` is generated data; update the generator script rather than editing the emoji arrays manually.
- Platform modules should expose `active_app_name`, `platform_note`, and `capabilities` through `platform/mod.rs`.

## 主题与样式

- `src/ui/theme.rs` 定义 `MacosTokens` 结构体，包含 Light/Dark 两套完整调色板常量
- `MacosTokens::light()` 和 `MacosTokens::dark()` 分别返回对应模式的实例
- `src/ui/widgets.rs` 提供 `macos_toggle` 和 `macos_range_slider` 自定义控件
- `src/app.rs` 中的 `resolve_theme()` 函数根据 `color_mode` 设置（`"light"`/`"dark"`/`"system"`）选择主题
- 系统模式下检测 GTK 主题名称判断暗/亮，回退到 Light
- 所有设置项使用 `macos_toggle` 控件，保持 macOS 风格一致性

## ANTI-PATTERNS

- Do not bypass `Storage` from UI code with ad hoc SQL.
- Do not change `ClipboardKind` serialized names without a migration.
- Do not make clipboard polling update egui state directly; use `ClipboardEvent` channel boundaries.
- Do not treat `.zig-cache/`, `.dvui-cache/`, `.opencode/`, `.sisyphus/`, or `target/` as source when navigating.

## TESTS

```bash
cargo test
```

Current tests are storage-level unit tests in `storage.rs`; add new focused tests there for schema/query behavior.
