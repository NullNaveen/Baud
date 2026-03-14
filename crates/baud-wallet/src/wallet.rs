//! AES-256-GCM encrypted wallet with Argon2id key derivation.
//!
//! Each wallet file stores one or more keypairs, encrypted at rest with a
//! password-derived key. The on-disk format is a JSON envelope containing
//! the Argon2 salt, AES-GCM nonce, and ciphertext.

use std::path::Path;

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{password_hash::SaltString, Argon2, Params};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use baud_core::crypto::KeyPair;

/// Errors returned by wallet operations.
#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("wrong password or corrupted wallet")]
    DecryptionFailed,
    #[error("wallet file already exists: {0}")]
    AlreadyExists(String),
    #[error("wallet file not found: {0}")]
    NotFound(String),
    #[error("duplicate label: {0}")]
    DuplicateLabel(String),
    #[error("label not found: {0}")]
    LabelNotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A single key entry inside the wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletEntry {
    /// Human-readable label (e.g. "agent-1", "validator").
    pub label: String,
    /// Hex-encoded 32-byte Ed25519 public address.
    pub address: String,
    /// Hex-encoded 32-byte Ed25519 secret key (only present in plaintext form).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
}

/// The plaintext payload that gets encrypted.
#[derive(Serialize, Deserialize)]
struct WalletPlaintext {
    entries: Vec<PlaintextEntry>,
}

impl Drop for WalletPlaintext {
    fn drop(&mut self) {
        for entry in &mut self.entries {
            entry.secret_key.zeroize();
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct PlaintextEntry {
    label: String,
    address: String,
    secret_key: String,
}

/// On-disk envelope for the encrypted wallet.
#[derive(Serialize, Deserialize)]
struct WalletEnvelope {
    version: u32,
    /// Argon2 salt (base64-encoded via SaltString).
    kdf_salt: String,
    /// Argon2 parameters for reproducibility.
    kdf_params: KdfParams,
    /// Hex-encoded 12-byte AES-GCM nonce.
    nonce: String,
    /// Hex-encoded AES-256-GCM ciphertext.
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct KdfParams {
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

/// Handle for an encrypted wallet file.
pub struct EncryptedWallet {
    path: std::path::PathBuf,
}

impl EncryptedWallet {
    /// Open or reference a wallet at the given path.
    pub fn at(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Create a new wallet file with an initial keypair.
    /// Fails if the file already exists.
    pub fn create(
        &self,
        password: &str,
        label: &str,
    ) -> Result<WalletEntry, WalletError> {
        if self.path.exists() {
            return Err(WalletError::AlreadyExists(
                self.path.display().to_string(),
            ));
        }

        let kp = KeyPair::generate();
        let entry = PlaintextEntry {
            label: label.to_string(),
            address: kp.address().to_hex(),
            secret_key: kp.secret_hex(),
        };

        let plaintext = WalletPlaintext {
            entries: vec![entry.clone()],
        };
        self.write_encrypted(password, &plaintext)?;

        Ok(WalletEntry {
            label: entry.label,
            address: entry.address,
            secret_key: None, // Don't leak on create
        })
    }

    /// Import an existing secret key into the wallet.
    pub fn import_key(
        &self,
        password: &str,
        label: &str,
        secret_hex: &str,
    ) -> Result<WalletEntry, WalletError> {
        let kp = KeyPair::from_secret_hex(secret_hex)
            .map_err(|_| WalletError::DecryptionFailed)?;

        let mut plaintext = if self.path.exists() {
            self.read_encrypted(password)?
        } else {
            WalletPlaintext {
                entries: Vec::new(),
            }
        };

        if plaintext.entries.iter().any(|e| e.label == label) {
            return Err(WalletError::DuplicateLabel(label.to_string()));
        }

        let entry = PlaintextEntry {
            label: label.to_string(),
            address: kp.address().to_hex(),
            secret_key: secret_hex.to_string(),
        };
        plaintext.entries.push(entry);
        self.write_encrypted(password, &plaintext)?;

        Ok(WalletEntry {
            label: label.to_string(),
            address: kp.address().to_hex(),
            secret_key: None,
        })
    }

    /// List all entries (addresses only, no secrets).
    pub fn list(&self, password: &str) -> Result<Vec<WalletEntry>, WalletError> {
        if !self.path.exists() {
            return Err(WalletError::NotFound(self.path.display().to_string()));
        }
        let plaintext = self.read_encrypted(password)?;
        Ok(plaintext
            .entries
            .iter()
            .map(|e| WalletEntry {
                label: e.label.clone(),
                address: e.address.clone(),
                secret_key: None,
            })
            .collect())
    }

    /// Export the secret key for a specific label.
    pub fn export(
        &self,
        password: &str,
        label: &str,
    ) -> Result<WalletEntry, WalletError> {
        if !self.path.exists() {
            return Err(WalletError::NotFound(self.path.display().to_string()));
        }
        let plaintext = self.read_encrypted(password)?;
        let entry = plaintext
            .entries
            .iter()
            .find(|e| e.label == label)
            .ok_or_else(|| WalletError::LabelNotFound(label.to_string()))?;

        Ok(WalletEntry {
            label: entry.label.clone(),
            address: entry.address.clone(),
            secret_key: Some(entry.secret_key.clone()),
        })
    }

    // ── Crypto internals ───────────────────────────────────────

    fn derive_key(password: &str, salt: &SaltString) -> [u8; 32] {
        let params = Self::argon2_params();
        let argon2 = Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            params,
        );
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(
                password.as_bytes(),
                salt.as_str().as_bytes(),
                &mut key,
            )
            .expect("argon2 hash should not fail with valid params");
        key
    }

    fn argon2_params() -> Params {
        // 64 MiB memory, 3 iterations, 1 thread — secure yet fast enough for CLI.
        Params::new(64 * 1024, 3, 1, Some(32)).expect("valid argon2 params")
    }

    fn write_encrypted(
        &self,
        password: &str,
        plaintext: &WalletPlaintext,
    ) -> Result<(), WalletError> {
        let salt = SaltString::generate(&mut OsRng);
        let mut key = Self::derive_key(password, &salt);

        let cipher = Aes256Gcm::new_from_slice(&key)
            .expect("32-byte key is valid for AES-256-GCM");
        key.zeroize();

        let mut nonce_bytes = [0u8; 12];
        rand::RngCore::fill_bytes(&mut OsRng, &mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let json_bytes = serde_json::to_vec(plaintext)?;
        let ciphertext = cipher
            .encrypt(nonce, json_bytes.as_slice())
            .expect("AES-GCM encryption should not fail");

        let params = Self::argon2_params();
        let envelope = WalletEnvelope {
            version: 1,
            kdf_salt: salt.as_str().to_string(),
            kdf_params: KdfParams {
                m_cost: params.m_cost(),
                t_cost: params.t_cost(),
                p_cost: params.p_cost(),
            },
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(&ciphertext),
        };

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&envelope)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    fn read_encrypted(
        &self,
        password: &str,
    ) -> Result<WalletPlaintext, WalletError> {
        let data = std::fs::read_to_string(&self.path)?;
        let envelope: WalletEnvelope = serde_json::from_str(&data)?;

        let salt = SaltString::from_b64(envelope.kdf_salt.as_str())
            .map_err(|_| WalletError::DecryptionFailed)?;

        let mut key = Self::derive_key(password, &salt);
        let cipher = Aes256Gcm::new_from_slice(&key)
            .expect("32-byte key is valid for AES-256-GCM");
        key.zeroize();

        let nonce_bytes =
            hex::decode(&envelope.nonce).map_err(|_| WalletError::DecryptionFailed)?;
        if nonce_bytes.len() != 12 {
            return Err(WalletError::DecryptionFailed);
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = hex::decode(&envelope.ciphertext)
            .map_err(|_| WalletError::DecryptionFailed)?;

        let json_bytes = cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| WalletError::DecryptionFailed)?;

        let plaintext: WalletPlaintext =
            serde_json::from_slice(&json_bytes).map_err(|_| WalletError::DecryptionFailed)?;

        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let wallet_path = dir.path().join("test.wallet");
        let w = EncryptedWallet::at(&wallet_path);

        let entry = w.create("hunter2", "agent-1").unwrap();
        assert_eq!(entry.label, "agent-1");
        assert!(entry.secret_key.is_none()); // Secret not returned on create

        let entries = w.list("hunter2").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "agent-1");
        assert_eq!(entries[0].address, entry.address);
    }

    #[test]
    fn wrong_password_fails() {
        let dir = tempfile::tempdir().unwrap();
        let wallet_path = dir.path().join("test.wallet");
        let w = EncryptedWallet::at(&wallet_path);

        w.create("correct-password", "key1").unwrap();
        let err = w.list("wrong-password").unwrap_err();
        assert!(matches!(err, WalletError::DecryptionFailed));
    }

    #[test]
    fn import_and_export() {
        let dir = tempfile::tempdir().unwrap();
        let wallet_path = dir.path().join("test.wallet");
        let w = EncryptedWallet::at(&wallet_path);

        // Generate a keypair and import its secret
        let kp = KeyPair::generate();
        let secret = kp.secret_hex();
        let addr = kp.address().to_hex();

        let entry = w.import_key("pw", "imported", &secret).unwrap();
        assert_eq!(entry.address, addr);

        // Export should reveal the secret
        let exported = w.export("pw", "imported").unwrap();
        assert_eq!(exported.secret_key.as_deref(), Some(secret.as_str()));
    }

    #[test]
    fn duplicate_label_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let wallet_path = dir.path().join("test.wallet");
        let w = EncryptedWallet::at(&wallet_path);

        let kp = KeyPair::generate();
        w.import_key("pw", "dup", &kp.secret_hex()).unwrap();

        let kp2 = KeyPair::generate();
        let err = w.import_key("pw", "dup", &kp2.secret_hex()).unwrap_err();
        assert!(matches!(err, WalletError::DuplicateLabel(_)));
    }

    #[test]
    fn multiple_keys() {
        let dir = tempfile::tempdir().unwrap();
        let wallet_path = dir.path().join("test.wallet");
        let w = EncryptedWallet::at(&wallet_path);

        let kp1 = KeyPair::generate();
        let kp2 = KeyPair::generate();

        w.import_key("pw", "key-a", &kp1.secret_hex()).unwrap();
        w.import_key("pw", "key-b", &kp2.secret_hex()).unwrap();

        let entries = w.list("pw").unwrap();
        assert_eq!(entries.len(), 2);
    }
}
