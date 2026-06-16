# Changelog

All notable changes to `tiez-slim-linux` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.3.0] - 2026-06-08

### Added

- **App blacklist + private mode** (#3): configurable app blacklist with wildcard matching on WM_CLASS; private mode toggle (Ctrl+Alt+P) pauses all clipboard capture; status bar shows lock icon when active
- **Primary Selection tracking** (#2): XFixes-based monitoring of X11 PRIMARY selection (mouse middle-click paste); entries tagged with selection badge; independent from CLIPBOARD; degrades gracefully to arboard polling when XFixes unavailable
- **Regex Actions system** (#1): regex-pattern-to-command automation engine; toolbar button for quick trigger; right-click context menu integration; auto-trigger mode with 5-second undo window; settings panel with full CRUD, live test, and test run
- **Export/Import + auto backup** (#5): export all history, tags, and settings to JSON; import from JSON with deduplication; auto-backup on app close with configurable retention count; data management panel with export/import/backup/open-backup-dir actions
- **Fuzzy search** (#10): nucleo-matcher powered fuzzy search with typo tolerance and CJK support; results ranked by relevance with character highlighting; toggleable between fuzzy and substring modes
- **Database encryption** (#7, opt-in `secure_storage` feature): AES-256-GCM encryption for sensitive entries; keys managed via system keyring (GNOME Keyring / KWallet); transparent encrypt/decrypt on write/read; LRU cache for read performance; batch migration support
- **KDE Connect sync** (#4): default-enabled bidirectional clipboard sync with Android via KDE Connect protocol; settings panel with device discovery, device-list pairing, and connection status; echo guard to prevent sync loops
- **CLI (`tiez-cli`)** (#6): Unix domain socket IPC for script integration; subcommands: `list`, `search`, `paste`, `pin`, `tag`, `delete`, `add`, `status`; `--json` flag for machine-readable output; works offline for `paste` (direct DB access); Sway/Hyprland integration guide and rofi script
- **i18n** (#8): full bilingual support (zh-CN + en-US), 754 translation keys at 100% coverage; rust-i18n v4 with `t!()` macro; auto-detect system locale; all user-visible strings externalized

### Changed

- Settings panel expanded from 7 to 10 implemented tabs (General, Shortcuts, Clipboard, Appearance, Default Apps, Tags, Data, Privacy, Actions, Sync)
- Status bar shows sync connection chip when KDE Connect device is connected
- CLI `status` command now includes sync/KDE Connect state in JSON output

### Technical

- `src/app.rs` settings extracted into 10 panel modules under `src/ui/settings/`
- New modules: `actions/`, `blacklist.rs`, `clipboard.rs` (primary selection), `encryption/`, `export/`, `ipc.rs`, `search/`, `sync/`
- New binary: `src/bin/tiez_cli.rs` (tiez-cli)
- 754 i18n keys across 2 locales
- 203+ unit tests passing

## [0.2.0] - 2026-05-31

### Added

- Initial native egui/eframe implementation
- Clipboard history with text, rich text, image, and file support
- Emoji page with Twemoji SVG rendering
- Symbol page with Unicode symbol categories
- Settings panel with General, Shortcuts, Clipboard, Appearance, Default Apps, Tags, Data
- macOS visual theme (light/dark/system)
- X11 global hotkey, system tray, edge docking
- Sound effects for copy/paste
- SQLite persistence with rusqlite
