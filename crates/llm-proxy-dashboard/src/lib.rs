use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use llm_proxy_core::Config;
use llm_proxy_db::Database;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
pub struct DashboardState {
    config: Arc<Config>,
    database: Database,
}

impl DashboardState {
    pub fn new(config: Arc<Config>, database: Database) -> Self {
        Self { config, database }
    }
}

pub fn router(state: DashboardState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: DashboardState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "admin server listening");
    axum::serve(listener, router(state)).await
}

async fn index(State(state): State<DashboardState>) -> impl IntoResponse {
    let _ = state.database.pool();
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
  </main>
</body>
</html>"#,
        state.config.proxy_listen, state.config.admin_listen, state.config.default_route
    ))
}

async fn health() -> impl IntoResponse {
    "ok"
}
