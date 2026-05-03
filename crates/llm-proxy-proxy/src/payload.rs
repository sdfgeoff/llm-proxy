use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use llm_proxy_core::MasterKey;
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{macros::format_description, OffsetDateTime};

#[derive(Debug, Clone, Copy)]
pub enum PayloadKind {
    Request,
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedPayload {
    pub relative_path: String,
    pub raw_bytes: u64,
    pub raw_sha256: String,
}

#[derive(Debug, Error)]
pub enum PayloadArchiveError {
    #[error("payload IO error: {0}")]
    Io(#[from] io::Error),
    #[error("payload compression error: {0}")]
    Compression(io::Error),
    #[error("payload encryption error: {0}")]
    Encryption(#[from] llm_proxy_core::crypto::CryptoError),
    #[error("payload timestamp formatting error: {0}")]
    Time(#[from] time::error::Format),
}

pub fn archive_payload(
    root: &Path,
    master_key: &MasterKey,
    request_id: &str,
    kind: PayloadKind,
    payload: &[u8],
) -> Result<ArchivedPayload, PayloadArchiveError> {
    let now = OffsetDateTime::now_utc();
    let date = now.format(format_description!("[year]-[month]-[day]"))?;
    let hour = now.format(format_description!("[hour]"))?;
    let prefix = match kind {
        PayloadKind::Request => "req",
        PayloadKind::Response => "res",
    };
    let filename = format!("{prefix}_{request_id}.zst.enc");
    let relative = PathBuf::from(date).join(hour).join(filename);
    let full_path = root.join(&relative);

    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let compressed = zstd::encode_all(payload, 3).map_err(PayloadArchiveError::Compression)?;
    let encrypted = master_key.encrypt_bytes(&compressed)?;

    let mut file = fs::File::create(&full_path)?;
    file.write_all(&encrypted.nonce)?;
    file.write_all(&encrypted.ciphertext)?;
    file.sync_all()?;

    Ok(ArchivedPayload {
        relative_path: relative.to_string_lossy().into_owned(),
        raw_bytes: payload.len() as u64,
        raw_sha256: hash_payload(payload),
    })
}

fn hash_payload(payload: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_payload_to_encrypted_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = MasterKey::load_or_create(&dir.path().join("master.key")).expect("key");
        let root = dir.path().join("payloads");

        let archived = archive_payload(
            &root,
            &key,
            "request-id",
            PayloadKind::Request,
            br#"{"hello":"world"}"#,
        )
        .expect("archive payload");

        let stored = fs::read(root.join(&archived.relative_path)).expect("stored payload");
        assert_eq!(archived.raw_bytes, 17);
        assert!(!stored.windows(5).any(|window| window == b"hello"));
    }
}
