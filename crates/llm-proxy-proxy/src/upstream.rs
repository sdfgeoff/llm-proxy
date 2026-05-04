use axum::http::header;
use llm_proxy_core::MasterKey;
use serde_json::Value;
use tracing::error;
use url::Url;

use crate::ProxyState;

pub(crate) enum SecretLoadError {
    Missing,
    Internal,
}

pub(crate) async fn fetch_upstream_models(state: &ProxyState, url: Url) -> Result<Vec<Value>, ()> {
    let url_str = url.as_str().to_owned();
    let response = state.client.get(url.clone()).send().await.map_err(|e| {
        error!(error = %e, url = %url_str, "failed to fetch upstream models");
        ()
    })?;
    if !response.status().is_success() {
        return Err(());
    }
    let body = response.json::<Value>().await.map_err(|e| {
        error!(error = %e, url = %url_str, "failed to parse upstream models response");
        ()
    })?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) async fn load_upstream_secret(
    state: &ProxyState,
    name: &str,
) -> Result<String, SecretLoadError> {
    let Some(secret) = state
        .database
        .upstream_secret(name)
        .await
        .map_err(|_| SecretLoadError::Internal)?
    else {
        return Err(SecretLoadError::Missing);
    };

    decrypt_secret(&state.master_key, &secret.encrypted_value, &secret.nonce)
}

pub(crate) fn route_url(base_url: &Url, path: &str) -> Result<Url, ()> {
    base_url.join(path.trim_start_matches('/')).map_err(|_| ())
}

pub(crate) fn content_type_or_json(
    headers: &reqwest::header::HeaderMap,
) -> axum::http::HeaderValue {
    headers
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| axum::http::HeaderValue::from_static("application/json"))
}

fn decrypt_secret(
    master_key: &MasterKey,
    encrypted_value: &[u8],
    nonce: &[u8],
) -> Result<String, SecretLoadError> {
    master_key
        .decrypt(encrypted_value, nonce)
        .map_err(|_| SecretLoadError::Internal)
}
