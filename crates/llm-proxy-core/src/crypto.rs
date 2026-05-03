use std::{
    fs, io,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};
use rand_core::RngCore;
use thiserror::Error;

const MASTER_KEY_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct MasterKey {
    bytes: [u8; MASTER_KEY_BYTES],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedValue {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("failed to read master key {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write master key {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid master key length")]
    InvalidKeyLength,
    #[error("invalid master key encoding")]
    InvalidKeyEncoding,
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
}

impl MasterKey {
    pub fn load_or_create(path: &Path) -> Result<Self, CryptoError> {
        if path.exists() {
            return Self::load(path);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| CryptoError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let mut bytes = [0_u8; MASTER_KEY_BYTES];
        OsRng.fill_bytes(&mut bytes);
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        fs::write(path, format!("{encoded}\n")).map_err(|source| CryptoError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        restrict_file_permissions(path)?;

        Ok(Self { bytes })
    }

    pub fn load(path: &Path) -> Result<Self, CryptoError> {
        let encoded = fs::read_to_string(path).map_err(|source| CryptoError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded.trim())
            .map_err(|_| CryptoError::InvalidKeyEncoding)?;
        Self::from_bytes(&bytes)
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<EncryptedValue, CryptoError> {
        self.encrypt_bytes(plaintext.as_bytes())
    }

    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<EncryptedValue, CryptoError> {
        let cipher = XChaCha20Poly1305::new(&self.bytes.into());
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::Encrypt)?;
        Ok(EncryptedValue {
            ciphertext,
            nonce: nonce.to_vec(),
        })
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, CryptoError> {
        let plaintext = self.decrypt_bytes(ciphertext, nonce)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::Decrypt)
    }

    pub fn decrypt_bytes(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = XChaCha20Poly1305::new(&self.bytes.into());
        let nonce = XNonce::from_slice(nonce);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CryptoError::Decrypt)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let bytes: [u8; MASTER_KEY_BYTES] = bytes
            .try_into()
            .map_err(|_| CryptoError::InvalidKeyLength)?;
        Ok(Self { bytes })
    }
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), CryptoError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|source| CryptoError::Read {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions).map_err(|source| CryptoError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), CryptoError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_and_decrypts_values() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = MasterKey::load_or_create(&dir.path().join("master.key")).expect("key");

        let encrypted = key.encrypt("secret-value").expect("encrypt");
        let decrypted = key
            .decrypt(&encrypted.ciphertext, &encrypted.nonce)
            .expect("decrypt");

        assert_eq!(decrypted, "secret-value");
        assert_ne!(encrypted.ciphertext, b"secret-value");
    }
}
