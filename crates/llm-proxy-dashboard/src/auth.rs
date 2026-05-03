use axum::{
    http::{header, HeaderMap},
    response::Response,
};
use llm_proxy_core::auth::hash_lookup_token;

use crate::{render::internal_error, DashboardState};

pub(crate) const SESSION_COOKIE: &str = "llm_proxy_session";

pub(crate) enum AuthState {
    NeedsSetup,
    Unauthenticated,
    Authenticated,
}

pub(crate) async fn require_admin(
    state: &DashboardState,
    headers: &HeaderMap,
) -> Result<AuthState, Response> {
    if !state
        .database
        .has_admin_account()
        .await
        .map_err(|_| internal_error())?
    {
        return Ok(AuthState::NeedsSetup);
    }

    let Some(token) = session_token_from_headers(headers) else {
        return Ok(AuthState::Unauthenticated);
    };

    let session_hash = hash_lookup_token(&token);
    let exists = state
        .database
        .admin_session_exists(&session_hash)
        .await
        .map_err(|_| internal_error())?;
    if exists {
        Ok(AuthState::Authenticated)
    } else {
        Ok(AuthState::Unauthenticated)
    }
}

pub(crate) fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == SESSION_COOKIE).then(|| value.to_owned())
    })
}
