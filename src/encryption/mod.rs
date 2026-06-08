//! AES-256-GCM encryption for sensitive clipboard entries.
//!
//! Uses `aes-gcm` for symmetric encryption and `keyring` for OS-level
//! key storage. Gated behind the `secure_storage` Cargo feature.
//!
//! Ciphertext format: `enc:v1:<base64(nonce || ciphertext)>`

pub mod queue;

use anyhow::Result;
#[cfg(feature = "secure_storage")]
use anyhow::Context;

/// Encrypts and decrypts clipboard entry content.
pub trait SecureStore {
    /// Encrypt `plaintext` and return `enc:v1:<base64(nonce ‖ ciphertext)>`.
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    /// Decrypt payload produced by [`SecureStore::encrypt`].
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

/// AES-256-GCM encryptor backed by a master key stored in the OS keyring.
pub struct KeyringBackend {
    #[cfg(feature = "secure_storage")]
    cipher: aes_gcm::Aes256Gcm,
}

impl std::fmt::Debug for KeyringBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringBackend").finish_non_exhaustive()
    }
}

impl KeyringBackend {
    /// Create a new backend, retrieving or generating the master key via
    /// the OS keyring.
    pub fn new() -> Result<Self> {
        #[cfg(feature = "secure_storage")]
        {
            let cipher = load_or_create_cipher()?;
            Ok(Self { cipher })
        }
        #[cfg(not(feature = "secure_storage"))]
        {
            anyhow::bail!("encryption disabled: enable secure_storage feature")
        }
    }

    #[cfg(feature = "secure_storage")]
    pub fn with_key(key: &[u8; 32]) -> Result<Self> {
        use aes_gcm::KeyInit;
        let cipher =
            aes_gcm::Aes256Gcm::new_from_slice(key).context("invalid AES-256 key")?;
        Ok(Self { cipher })
    }
}

#[cfg(feature = "secure_storage")]
const KEYRING_SERVICE: &str = "tiez-slim-linux";
#[cfg(feature = "secure_storage")]
const KEYRING_ACCOUNT: &str = "master";
#[cfg(feature = "secure_storage")]
const PREFIX: &str = "enc:v1:";

#[cfg(feature = "secure_storage")]
fn load_or_create_cipher() -> Result<aes_gcm::Aes256Gcm> {
    use aes_gcm::KeyInit;

    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .context("failed to access keyring")?;

    let key_bytes: [u8; 32] = match entry.get_password() {
        Ok(encoded) => {
            let bytes =
                base64_decode(&encoded).context("failed to decode master key")?;
            bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("master key has wrong length"))?
        }
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            getrandom::getrandom(&mut key)
                .map_err(|e| anyhow::anyhow!("key generation failed: {e}"))?;
            let encoded = base64_encode(&key);
            entry
                .set_password(&encoded)
                .context("failed to store master key in keyring")?;
            key
        }
        Err(e) => {
            anyhow::bail!("keyring unavailable (SSH session?): {e}");
        }
    };

    aes_gcm::Aes256Gcm::new_from_slice(&key_bytes).context("invalid AES-256 key")
}

#[cfg(feature = "secure_storage")]
impl SecureStore for KeyringBackend {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::Aead;

        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes)
            .map_err(|e| anyhow::anyhow!("nonce generation failed: {e}"))?;
        let nonce = aes_gcm::aead::Nonce::<aes_gcm::Aes256Gcm>::from_slice(&nonce_bytes);

        let ct = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ct);

        Ok(format!("{PREFIX}{}", base64_encode(&combined)).into_bytes())
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::Aead;

        let text =
            std::str::from_utf8(ciphertext).context("ciphertext is not valid UTF-8")?;

        let encoded = text
            .strip_prefix(PREFIX)
            .context("invalid ciphertext format: missing enc:v1: prefix")?;

        let combined =
            base64_decode(encoded).context("invalid ciphertext format: bad base64")?;

        if combined.len() < 12 {
            anyhow::bail!("ciphertext too short: need at least 12 bytes for nonce");
        }

        let (nonce_bytes, ct_bytes) = combined.split_at(12);
        let nonce = aes_gcm::aead::Nonce::<aes_gcm::Aes256Gcm>::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, ct_bytes)
            .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))
    }
}

#[cfg(not(feature = "secure_storage"))]
impl SecureStore for KeyringBackend {
    fn encrypt(&self, _plaintext: &[u8]) -> Result<Vec<u8>> {
        anyhow::bail!("encryption disabled: enable secure_storage feature")
    }

    fn decrypt(&self, _ciphertext: &[u8]) -> Result<Vec<u8>> {
        anyhow::bail!("encryption disabled: enable secure_storage feature")
    }
}

#[cfg(feature = "secure_storage")]
fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[cfg(feature = "secure_storage")]
fn base64_decode(encoded: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("base64 decode failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "secure_storage")]
    fn test_backend() -> KeyringBackend {
        KeyringBackend::with_key(&[42u8; 32]).unwrap()
    }

    #[cfg(feature = "secure_storage")]
    fn test_backend_with_key(key: &[u8; 32]) -> KeyringBackend {
        KeyringBackend::with_key(key).unwrap()
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn encrypt_decrypt_roundtrip() {
        let backend = test_backend();
        for i in 0..100u32 {
            let plaintext = format!("test plaintext {i} — random data 你好");
            let ct = backend.encrypt(plaintext.as_bytes()).unwrap();
            let pt = backend.decrypt(&ct).unwrap();
            assert_eq!(pt, plaintext.as_bytes());
        }
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn nonce_uniqueness() {
        let backend = test_backend();
        let plaintext = b"same plaintext every time";
        let mut ciphertexts = std::collections::HashSet::new();
        for _ in 0..100 {
            let ct = backend.encrypt(plaintext).unwrap();
            assert!(ciphertexts.insert(ct), "duplicate ciphertext detected");
        }
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn invalid_base64_returns_error() {
        let backend = test_backend();
        let result = backend.decrypt(b"enc:v1:!!!not-base64!!!");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("bad base64"), "unexpected error: {msg}");
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn missing_prefix_returns_error() {
        let backend = test_backend();
        let result = backend.decrypt(b"no-prefix-here");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("missing enc:v1: prefix"),
        );
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn wrong_key_returns_error() {
        let backend1 = test_backend_with_key(&[1u8; 32]);
        let backend2 = test_backend_with_key(&[2u8; 32]);
        let ct = backend1.encrypt(b"secret data").unwrap();
        let result = backend2.decrypt(&ct);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("decryption failed"),
        );
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn empty_plaintext_roundtrip() {
        let backend = test_backend();
        let ct = backend.encrypt(b"").unwrap();
        let pt = backend.decrypt(&ct).unwrap();
        assert_eq!(pt, b"");
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn large_plaintext_roundtrip() {
        let backend = test_backend();
        let plaintext = vec![0xABu8; 1024 * 1024];
        let ct = backend.encrypt(&plaintext).unwrap();
        let pt = backend.decrypt(&ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn ciphertext_format_prefix() {
        let backend = test_backend();
        let ct = backend.encrypt(b"hello").unwrap();
        let text = std::str::from_utf8(&ct).unwrap();
        assert!(
            text.starts_with("enc:v1:"),
            "unexpected prefix: {}",
            &text[..20.min(text.len())],
        );
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn ciphertext_too_short_returns_error() {
        use base64::Engine;
        let backend = test_backend();
        let short_payload = base64::engine::general_purpose::STANDARD.encode(&[0u8; 8]);
        let input = format!("enc:v1:{short_payload}");
        let result = backend.decrypt(input.as_bytes());
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("too short"),
            "expected 'too short' error for 8-byte payload",
        );
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn utf8_error_on_binary_ciphertext() {
        let backend = test_backend();
        let result = backend.decrypt(&[0xFF, 0xFE, 0xFD]);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("not valid UTF-8"),
        );
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn different_plaintexts_produce_different_ciphertexts() {
        let backend = test_backend();
        let ct_a = backend.encrypt(b"alpha").unwrap();
        let ct_b = backend.encrypt(b"beta").unwrap();
        assert_ne!(ct_a, ct_b);
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn roundtrip_with_cjk_and_emoji() {
        let backend = test_backend();
        let plaintext = "你好世界 🌍🦀 — émojis & Ünïcödé";
        let ct = backend.encrypt(plaintext.as_bytes()).unwrap();
        let pt = backend.decrypt(&ct).unwrap();
        assert_eq!(std::str::from_utf8(&pt).unwrap(), plaintext);
    }

    #[test]
    #[cfg(feature = "secure_storage")]
    fn tampered_ciphertext_fails_auth_tag() {
        let backend = test_backend();
        let mut ct = backend.encrypt(b"auth tag test").unwrap();
        let last = ct.last_mut().unwrap();
        *last ^= 0xFF;
        assert!(backend.decrypt(&ct).is_err());
    }

    #[test]
    #[cfg(not(feature = "secure_storage"))]
    fn disabled_feature_returns_error() {
        let result = KeyringBackend::new();
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("enable secure_storage feature"),
            "unexpected error: {msg}",
        );
    }

    #[test]
    #[cfg(not(feature = "secure_storage"))]
    fn disabled_encrypt_returns_error() {
        use super::SecureStore;
        let backend = super::KeyringBackend {};
        let result = backend.encrypt(b"test");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("enable secure_storage"),
        );
    }

    #[test]
    #[cfg(not(feature = "secure_storage"))]
    fn disabled_decrypt_returns_error() {
        use super::SecureStore;
        let backend = super::KeyringBackend {};
        let result = backend.decrypt(b"enc:v1:dGVzdA==");
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("enable secure_storage"),
        );
    }
}
