use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("failed to hash password: {0}")]
    PasswordHash(argon2::password_hash::Error),
}

pub fn generate_proxy_api_key() -> String {
    format!("lp_{}_{}", Uuid::now_v7(), Uuid::now_v7())
}

pub fn generate_session_token() -> String {
    format!("ls_{}_{}", Uuid::now_v7(), Uuid::now_v7())
}

pub fn hash_lookup_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn hash_admin_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(AuthError::PasswordHash)
}

pub fn verify_admin_password(password: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_stable_without_revealing_token() {
        let token = "lp_test";

        let hash = hash_lookup_token(token);

        assert_eq!(hash, hash_lookup_token(token));
        assert_ne!(hash, token);
    }

    #[test]
    fn admin_password_hash_round_trips() {
        let hash = hash_admin_password("correct horse").expect("hash password");

        assert!(verify_admin_password("correct horse", &hash));
        assert!(!verify_admin_password("wrong", &hash));
    }
}
