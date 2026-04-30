use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use llm_proxy_core::{auth::hash_lookup_token, Config};
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

async fn models(State(state): State<ProxyState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = bearer_token(&headers) else {
        return unauthorized().into_response();
    };

    match state
        .database
        .proxy_api_key_by_hash(&hash_lookup_token(token))
        .await
    {
        Ok(Some(_)) => {}
        Ok(None) => return unauthorized().into_response(),
        Err(_) => return internal_error().into_response(),
    }

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
    .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

fn unauthorized() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "message": "Missing or invalid proxy API key",
                "type": "authentication_error"
            }
        })),
    )
}

fn internal_error() -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": {
                "message": "Internal proxy error",
                "type": "proxy_error"
            }
        })),
    )
}
