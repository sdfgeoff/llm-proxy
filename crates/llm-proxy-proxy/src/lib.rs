use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use llm_proxy_core::Config;
use llm_proxy_db::Database;
use serde_json::json;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
pub struct ProxyState {
    config: Arc<Config>,
    database: Database,
}

impl ProxyState {
    pub fn new(config: Arc<Config>, database: Database) -> Self {
        Self { config, database }
    }
}

pub fn router(state: ProxyState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: ProxyState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "proxy server listening");
    axum::serve(listener, router(state)).await
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn models(State(state): State<ProxyState>) -> impl IntoResponse {
    let _ = state.database.pool();
    let models = state
        .config
        .models
        .keys()
        .map(|id| {
            json!({
                "id": id,
                "object": "model",
                "created": 0,
                "owned_by": "llm-proxy"
            })
        })
        .collect::<Vec<_>>();

    Json(json!({
        "object": "list",
        "data": models
    }))
}
