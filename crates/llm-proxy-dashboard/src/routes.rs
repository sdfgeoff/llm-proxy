use axum::{
    extract::{Form, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use llm_proxy_core::auth::{
    generate_proxy_api_key, generate_session_token, hash_admin_password, hash_lookup_token,
    verify_admin_password,
};
use serde::Deserialize;
use time::Duration;

use crate::{
    auth::{require_admin, session_token_from_headers, AuthState, SESSION_COOKIE},
    dashboard_page::dashboard_page,
    pages::{render_keys_page, render_upstream_secrets_page},
    render::{internal_error, page},
    DashboardState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SetupForm {
    token: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoginForm {
    password: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateKeyForm {
    label: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpstreamSecretForm {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteUpstreamSecretForm {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DashboardQuery {
    pub(crate) period: Option<String>,
}

pub(crate) async fn index(
    State(state): State<DashboardState>,
    Query(query): Query<DashboardQuery>,
    headers: HeaderMap,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Ok(AuthState::Authenticated) => dashboard_page(&state, query).await,
        Err(response) => response,
    }
}

pub(crate) async fn health() -> impl IntoResponse {
    "ok"
}

pub(crate) async fn setup_page(State(state): State<DashboardState>) -> Response {
    match state.database.has_admin_account().await {
        Ok(true) => Redirect::to("/login").into_response(),
        Ok(false) => Html(page(
            "Setup",
            r#"
<h1>Setup</h1>
<form method="post" action="/setup">
  <label>Setup token <input name="token" type="password" required></label>
  <label>Password <input name="password" type="password" minlength="8" required></label>
  <button type="submit">Create admin</button>
</form>
"#,
        ))
        .into_response(),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn setup(
    State(state): State<DashboardState>,
    Form(form): Form<SetupForm>,
) -> Response {
    match state.database.has_admin_account().await {
        Ok(true) => return Redirect::to("/login").into_response(),
        Ok(false) => {}
        Err(_) => return internal_error(),
    }

    if state.setup_token.as_deref() != Some(form.token.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            Html(page("Setup", "<p>Invalid setup token.</p>")),
        )
            .into_response();
    }

    if form.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Html(page(
                "Setup",
                "<p>Password must be at least 8 characters.</p>",
            )),
        )
            .into_response();
    }

    let Ok(password_hash) = hash_admin_password(&form.password) else {
        return internal_error();
    };

    match state.database.set_admin_password_hash(&password_hash).await {
        Ok(()) => Redirect::to("/login").into_response(),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn login_page(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Authenticated) => Redirect::to("/").into_response(),
        Ok(AuthState::Unauthenticated) => Html(page(
            "Login",
            r#"
<h1>Login</h1>
<form method="post" action="/login">
  <label>Password <input name="password" type="password" required></label>
  <button type="submit">Log in</button>
</form>
"#,
        ))
        .into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn login(
    State(state): State<DashboardState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let Ok(Some(password_hash)) = state.database.admin_password_hash().await else {
        return Redirect::to("/setup").into_response();
    };

    if !verify_admin_password(&form.password, &password_hash) {
        return (
            StatusCode::UNAUTHORIZED,
            Html(page("Login", "<p>Invalid password.</p>")),
        )
            .into_response();
    }

    let token = generate_session_token();
    let session_hash = hash_lookup_token(&token);
    if state
        .database
        .create_admin_session(&session_hash, Duration::days(7))
        .await
        .is_err()
    {
        return internal_error();
    }

    let mut response = Redirect::to("/").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
            7 * 24 * 60 * 60
        ))
        .expect("session cookie should be header-safe"),
    );
    response
}

pub(crate) async fn logout(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
    if let Some(token) = session_token_from_headers(&headers) {
        let _ = state
            .database
            .delete_admin_session(&hash_lookup_token(&token))
            .await;
    }

    let mut response = Redirect::to("/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("llm_proxy_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"),
    );
    response
}

pub(crate) async fn keys_page(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => render_keys_page(&state, None).await,
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn create_key(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<CreateKeyForm>,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let label = form.label.trim();
    if label.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Html(page("API Keys", "<p>Key label is required.</p>")),
        )
            .into_response();
    }

    let token = generate_proxy_api_key();
    let hash = hash_lookup_token(&token);
    match state.database.create_proxy_api_key(label, &hash).await {
        Ok(_) => render_keys_page(&state, Some(token)).await,
        Err(_) => internal_error(),
    }
}

pub(crate) async fn upstream_secrets_page(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => render_upstream_secrets_page(&state, None).await,
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn upsert_upstream_secret(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<UpstreamSecretForm>,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let name = form.name.trim();
    if name.is_empty() || form.value.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Html(page(
                "Upstream Secrets",
                "<p>Secret name and value are required.</p>",
            )),
        )
            .into_response();
    }

    let encrypted = match state.master_key.encrypt(&form.value) {
        Ok(encrypted) => encrypted,
        Err(_) => return internal_error(),
    };
    match state
        .database
        .upsert_upstream_secret(name, &encrypted.ciphertext, &encrypted.nonce)
        .await
    {
        Ok(()) => render_upstream_secrets_page(&state, Some("Secret saved.")).await,
        Err(_) => internal_error(),
    }
}

pub(crate) async fn delete_upstream_secret(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<DeleteUpstreamSecretForm>,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state
        .database
        .delete_upstream_secret(form.name.trim())
        .await
    {
        Ok(()) => render_upstream_secrets_page(&state, Some("Secret deleted.")).await,
        Err(_) => internal_error(),
    }
}
