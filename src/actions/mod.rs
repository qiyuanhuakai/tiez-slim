//! Action system for executing external commands on clipboard entries.
//!
//! Provides the `Action` model and a simple thread-pool executor built on
//! `std::process::Command` + `std::thread::spawn` (no tokio runtime).
//!
//! // TODO(T11): Implement Action struct + CRUD (Wave 1)
//! // TODO(T12): Implement Action executor with std::thread::spawn pool (Wave 1)
//! // TODO(T13): Implement action configuration persistence (Wave 1)
//! // TODO(T14): Implement action UI panel (Wave 1)
//! // TODO(T15): Implement action parameter substitution (Wave 1)
//! // TODO(T16): Implement action hotkey binding (Wave 1)
