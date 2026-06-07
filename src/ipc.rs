//! Inter-process communication for CLI ↔ GUI coordination.
//!
//! Provides a Unix domain socket server (or platform equivalent) so that
//! the `tiez-cli` binary can send commands to a running GUI instance.
//!
//! // TODO(T22): Implement IPC server (Unix socket) in main app (Wave 2)
//! // TODO(T23): Implement IPC client in tiez-cli binary (Wave 2)
