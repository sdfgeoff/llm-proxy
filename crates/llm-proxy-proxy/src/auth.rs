use axum::http::{header, HeaderMap};
use llm_proxy_core::auth::hash_lookup_token;
use llm_proxy_db::ProxyApiKey;

use crate::ProxyState;

pub(crate) enum AuthFailure {
    Unauthorized,
    Internal,
}

pub(crate) async fn authenticate_proxy_key(
    state: &ProxyState,
    headers: &HeaderMap,
) -> Result<ProxyApiKey, AuthFailure> {
    let Some(token) = bearer_token(headers) else {
        return Err(AuthFailure::Unauthorized);
    };

    match state
        .database
        .proxy_api_key_by_hash(&hash_lookup_token(token))
        .await
    {
        Ok(Some(key)) => Ok(key),
        Ok(None) => Err(AuthFailure::Unauthorized),
        Err(_) => Err(AuthFailure::Internal),
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    if let Some(token) = anthropic_api_key(headers) {
        return Some(token);
    }
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

fn anthropic_api_key(headers: &HeaderMap) -> Option<&str> {
    headers.get("x-api-key")?.to_str().ok()
}
