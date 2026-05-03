mod auth;
mod chat;
mod handlers;
mod logging;
mod models;
mod payload;
mod responses;
mod responses_endpoint;
mod streaming;
mod upstream;
mod usage;

use std::{net::SocketAddr, sync::Arc};

use axum::{routing::get, Router};
use llm_proxy_core::{Config, MasterKey};
use llm_proxy_db::Database;
use reqwest::Client;
use tokio::net::TcpListener;
use tracing::info;

#[derive(Clone)]
pub struct ProxyState {
    pub(crate) config: Arc<Config>,
    pub(crate) database: Database,
    pub(crate) master_key: MasterKey,
    pub(crate) client: Client,
}

impl ProxyState {
    pub fn new(config: Arc<Config>, database: Database, master_key: MasterKey) -> Self {
        Self {
            config,
            database,
            master_key,
            client: Client::new(),
        }
    }
}

pub fn router(state: ProxyState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/v1/models", get(models::models))
        .route(
            "/v1/chat/completions",
            axum::routing::post(chat::chat_completions),
        )
        .route(
            "/v1/responses",
            axum::routing::post(responses_endpoint::responses),
        )
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: ProxyState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "proxy server listening");
    axum::serve(listener, router(state)).await
}

#[cfg(test)]
mod tests;
