use std::{fs, io, path::Path};

use llm_proxy_core::MasterKey;
use thiserror::Error;

const XCHACHA20_NONCE_BYTES: usize = 24;

#[derive(Debug, Error)]
pub(crate) enum PayloadReadError {
    #[error("payload path is invalid")]
    InvalidPath,
    #[error("payload file is malformed")]
    Malformed,
    #[error("payload IO error: {0}")]
    Io(#[from] io::Error),
    #[error("payload decryption error: {0}")]
    Decryption(#[from] llm_proxy_core::crypto::CryptoError),
    #[error("payload decompression error: {0}")]
    Decompression(io::Error),
}

pub(crate) fn read_payload(
    root: &Path,
    master_key: &MasterKey,
    relative_path: &str,
) -> Result<Vec<u8>, PayloadReadError> {
    if relative_path.contains("..") || relative_path.starts_with('/') {
        return Err(PayloadReadError::InvalidPath);
    }

    let stored = fs::read(root.join(relative_path))?;
    if stored.len() <= XCHACHA20_NONCE_BYTES {
        return Err(PayloadReadError::Malformed);
    }
    let (nonce, ciphertext) = stored.split_at(XCHACHA20_NONCE_BYTES);
    let compressed = master_key.decrypt_bytes(ciphertext, nonce)?;
    zstd::decode_all(compressed.as_slice()).map_err(PayloadReadError::Decompression)
}
