//! tiez-slim-linux library crate.
//!
//! Exposes the GUI app modules so that the `tiez-cli` binary can use the
//! shared data model and storage. The GUI binary (`main.rs`) uses these
//! modules via `use tiez_slim_linux::*`.

rust_i18n::i18n!("locales", fallback = "en-US");

pub mod actions;
pub mod app;
pub mod clipboard;
pub mod emoji_data;
pub mod encryption;
pub mod i18n;
pub mod ipc;
pub mod model;
pub mod platform;
pub mod search;
pub mod snippets;
pub mod sound;
pub mod storage;
pub mod storage_io;
pub mod sync;
pub mod ui;
