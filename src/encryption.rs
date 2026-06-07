//! AES-256-GCM encryption for sensitive clipboard entries.
//!
//! Uses `aes-gcm` for symmetric encryption and `keyring` for OS-level
//! key storage. Gated behind the `secure_storage` Cargo feature.
//!
//! // TODO(T25): Implement EncryptionKey management via keyring (Wave 3)
//! // TODO(T26): Implement entry encrypt/decrypt pipeline (Wave 3)
//! // TODO(T27): Implement encryption settings UI panel (Wave 3)
