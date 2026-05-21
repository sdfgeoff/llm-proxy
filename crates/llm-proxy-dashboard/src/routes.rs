use axum::{
    body::Body,
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode, Uri},
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
    render::internal_error,
    DashboardState,
};

/* ── embedded frontend (Vite dist output) ──────────────── */

const SPA_HTML: &[u8] = include_bytes!("../frontend/dist/index.html");
const SPA_CSS: &[u8] = include_bytes!("../frontend/dist/style.css");
const SPA_JS: &[u8] = include_bytes!("../frontend/dist/index.js");

/* ── static file helpers ──────────────────────────────── */

fn serve_static(content_type: &str, bytes: &'static [u8]) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=31536000")
        .body(Body::from(bytes))
        .unwrap()
}

/* ── public static routes ─────────────────────────────── */

pub(crate) async fn serve_css() -> Response {
    serve_static("text/css", SPA_CSS)
}

pub(crate) async fn serve_js() -> Response {
    serve_static("application/javascript", SPA_JS)
}

/* ── SPA shell ────────────────────────────────────────── */

pub(crate) async fn spa(
    uri: Uri,
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    let path = uri.path();
    // Allow unauthenticated access to /setup and /login
    if !matches!(path, "/setup" | "/login") {
        match require_admin(&state, &headers).await {
            Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
            Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
            Ok(AuthState::Authenticated) => {}
            Err(response) => return response,
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(SPA_HTML))
        .unwrap()
}

/* ── auth status API ──────────────────────────────────── */

pub(crate) async fn api_auth_status(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    let status = match require_admin(&state, &headers).await {
        Ok(AuthState::NeedsSetup) => "needs_setup",
        Ok(AuthState::Unauthenticated) => "unauthenticated",
        Ok(AuthState::Authenticated) => "authenticated",
        Err(_) => "unauthenticated",
    };
    json_response(serde_json::json!({ "status": status }).to_string())
}

/* ── auth routes (POST only, SPA handles the UI) ──────── */

#[derive(Debug, Deserialize)]
pub(crate) struct SetupForm {
    token: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoginForm {
    password: String,
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
        return (StatusCode::UNAUTHORIZED, json_response("\"Invalid setup token\"".into()))
            .into_response();
    }

    if form.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            json_response("\"Password must be at least 8 characters\"".into()),
        )
            .into_response();
    }

    let Ok(password_hash) = hash_admin_password(&form.password) else {
        return internal_error();
    };

    match state.database.set_admin_password_hash(&password_hash).await {
        Ok(()) => json_response(serde_json::json!({ "ok": true }).to_string()),
        Err(_) => internal_error(),
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
            json_response("\"Invalid password\"".into()),
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

    let mut response = json_response(serde_json::json!({ "ok": true }).to_string());
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

/* ── API endpoints ────────────────────────────────────── */

#[derive(Debug, Deserialize, Default)]
pub(crate) struct DashboardQuery {
    pub(crate) period: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateKeyForm {
    label: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SecretForm {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeleteSecretForm {
    name: String,
}

fn json_response(json: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json))
        .unwrap()
}

pub(crate) async fn api_charts(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Query(query): Query<DashboardQuery>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let period = llm_proxy_db::DashboardPeriod::from_query(query.period.as_deref());
    match state.database.dashboard_metrics(period).await {
        Ok(metrics) => json_response(serde_json::to_string(&metrics).unwrap_or_default()),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_requests(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state.database.recent_requests(100).await {
        Ok(requests) => json_response(serde_json::to_string(&requests).unwrap_or_default()),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_request_detail(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state.database.request_detail(&id).await {
        Ok(Some(detail)) => json_response(serde_json::to_string(&detail).unwrap_or_default()),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            json_response("\"Not found\"".into()),
        )
            .into_response(),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_keys(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state.database.list_proxy_api_keys().await {
        Ok(keys) => json_response(serde_json::to_string(&keys).unwrap_or_default()),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_create_key(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<CreateKeyForm>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let label = form.label.trim();
    if label.is_empty() {
        return (StatusCode::BAD_REQUEST, json_response("\"Label is required\"".into()))
            .into_response();
    }

    let token = generate_proxy_api_key();
    let hash = hash_lookup_token(&token);
    match state.database.create_proxy_api_key(label, &hash).await {
        Ok(_) => {
            let resp = serde_json::json!({ "token": token });
            json_response(resp.to_string())
        }
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_secrets(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state.database.list_upstream_secrets().await {
        Ok(secrets) => {
            let infos = secrets.iter().map(|s| s.to_info()).collect::<Vec<_>>();
            json_response(serde_json::to_string(&infos).unwrap_or_default())
        }
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_upsert_secret(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<SecretForm>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let name = form.name.trim();
    if name.is_empty() || form.value.is_empty() {
        return (StatusCode::BAD_REQUEST, json_response("\"Name and value are required\"".into()))
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
        Ok(()) => json_response("\"ok\"".into()),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn api_delete_secret(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Form(form): Form<DeleteSecretForm>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
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
        Ok(()) => json_response("\"ok\"".into()),
        Err(_) => internal_error(),
    }
}

/* ── payload download (file endpoint, not SPA) ─────────── */

pub(crate) async fn download_payload(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path((id, kind)): Path<(String, String)>,
) -> Response {
    let authenticated = require_admin(&state, &headers).await;
    match authenticated {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let detail = match state.database.request_detail(&id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => return (StatusCode::NOT_FOUND, Html::<&str>("<p>Not found</p>")).into_response(),
        Err(_) => return internal_error(),
    };

    let path = match kind.as_str() {
        "request" => detail.request_payload_path.as_deref(),
        "response" => detail.response_payload_path.as_deref(),
        _ => None,
    };

    let Some(path) = path else {
        return (StatusCode::NOT_FOUND, Html::<&str>("<p>Payload not found</p>")).into_response();
    };

    let relative_path = path;
    if relative_path.contains("..") || relative_path.starts_with('/') {
        return internal_error();
    }

    let stored = match std::fs::read(state.config.payload_dir.join(relative_path)) {
        Ok(data) => data,
        Err(_) => return internal_error(),
    };

    const NONCE_BYTES: usize = 24;
    if stored.len() <= NONCE_BYTES {
        return internal_error();
    }

    let (nonce, ciphertext) = stored.split_at(NONCE_BYTES);
    let compressed = match state.master_key.decrypt_bytes(ciphertext, nonce) {
        Ok(data) => data,
        Err(_) => return internal_error(),
    };
    let bytes = match zstd::decode_all(compressed.as_slice()) {
        Ok(data) => data,
        Err(_) => return internal_error(),
    };

    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}
