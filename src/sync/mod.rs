//! Device sync via KDE Connect protocol.
//!
//! Manages an isolated tokio runtime (started in a background thread) for
//! KDE Connect communication, keeping the main app free of a tokio dependency
//! at the top level.
//!
//! // TODO(T28): Implement KDE Connect discovery + pairing (Wave 3)
//! // TODO(T29): Implement isolated tokio runtime for kdeconnect-proto (Wave 3)
//! // TODO(T30): Implement clipboard sync send/receive (Wave 3)
//! // TODO(T31): Implement sync settings UI panel (Wave 3)
