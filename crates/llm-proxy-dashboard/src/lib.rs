use std::{fmt::Write, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Form, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use llm_proxy_core::{
    auth::{
        generate_proxy_api_key, generate_session_token, hash_admin_password, hash_lookup_token,
        verify_admin_password,
    },
    Config,
};
use llm_proxy_db::Database;
use serde::Deserialize;
use time::Duration;
use tokio::net::TcpListener;
use tracing::info;

const SESSION_COOKIE: &str = "llm_proxy_session";

#[derive(Clone)]
pub struct DashboardState {
    config: Arc<Config>,
    database: Database,
    setup_token: Option<String>,
}

impl DashboardState {
    pub fn new(config: Arc<Config>, database: Database, setup_token: Option<String>) -> Self {
        Self {
            config,
            database,
            setup_token,
        }
    }
}

pub fn router(state: DashboardState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/setup", get(setup_page).post(setup))
        .route("/login", get(login_page).post(login))
        .route("/logout", post(logout))
        .route("/keys", get(keys_page).post(create_key))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: DashboardState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "admin server listening");
    axum::serve(listener, router(state)).await
}

#[derive(Debug, Deserialize)]
struct SetupForm {
    token: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginForm {
    password: String,
}

#[derive(Debug, Deserialize)]
struct CreateKeyForm {
    label: String,
}

async fn index(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Ok(AuthState::Authenticated) => dashboard_page(&state).into_response(),
        Err(response) => response,
    }
}

async fn setup_page(State(state): State<DashboardState>) -> Response {
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

async fn setup(State(state): State<DashboardState>, Form(form): Form<SetupForm>) -> Response {
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

async fn login_page(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
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

async fn login(State(state): State<DashboardState>, Form(form): Form<LoginForm>) -> Response {
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

async fn logout(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
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

async fn keys_page(State(state): State<DashboardState>, headers: HeaderMap) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => render_keys_page(&state, None).await,
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Err(response) => response,
    }
}

async fn create_key(
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

fn dashboard_page(state: &DashboardState) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>LLM Proxy</title>
</head>
<body>
  <main>
    <h1>LLM Proxy</h1>
    <p>Dashboard scaffold is running.</p>
    <dl>
      <dt>Proxy listen</dt><dd>{}</dd>
      <dt>Admin listen</dt><dd>{}</dd>
      <dt>Default route</dt><dd>{}</dd>
    </dl>
    <nav>
      <a href="/keys">Proxy API keys</a>
      <form method="post" action="/logout"><button type="submit">Log out</button></form>
    </nav>
  </main>
</body>
</html>"#,
        state.config.proxy_listen, state.config.admin_listen, state.config.default_route
    ))
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn render_keys_page(state: &DashboardState, new_key: Option<String>) -> Response {
    let Ok(keys) = state.database.list_proxy_api_keys().await else {
        return internal_error();
    };

    let mut body = String::new();
    body.push_str("<h1>Proxy API keys</h1>");
    if let Some(token) = new_key {
        let _ = write!(
            body,
            "<p>New key. This is shown once:</p><pre>{}</pre>",
            escape_html(&token)
        );
    }
    body.push_str(
        r#"
<form method="post" action="/keys">
  <label>Label <input name="label" required></label>
  <button type="submit">Create key</button>
</form>
<table>
  <thead><tr><th>Label</th><th>Created</th></tr></thead>
  <tbody>
"#,
    );
    for key in keys {
        let _ = write!(
            body,
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(&key.label),
            escape_html(&key.created_at)
        );
    }
    body.push_str("</tbody></table><p><a href=\"/\">Dashboard</a></p>");

    Html(page("API Keys", &body)).into_response()
}

enum AuthState {
    NeedsSetup,
    Unauthenticated,
    Authenticated,
}

async fn require_admin(state: &DashboardState, headers: &HeaderMap) -> Result<AuthState, Response> {
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

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == SESSION_COOKIE).then(|| value.to_owned())
    })
}

fn page(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{}</title>
</head>
<body>
  <main>{}</main>
</body>
</html>"#,
        escape_html(title),
        body
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(page("Error", "<p>Internal server error.</p>")),
    )
        .into_response()
}
