mod auth;
mod pages;
mod render;
mod routes;
mod state;

use std::net::SocketAddr;

use axum::{
    routing::{get, post},
    Router,
};
pub use state::DashboardState;
use tokio::net::TcpListener;
use tracing::info;

pub fn router(state: DashboardState) -> Router {
    Router::new()
        .route("/", get(routes::index))
        .route("/health", get(routes::health))
        .route("/setup", get(routes::setup_page).post(routes::setup))
        .route("/login", get(routes::login_page).post(routes::login))
        .route("/logout", post(routes::logout))
        .route("/keys", get(routes::keys_page).post(routes::create_key))
        .route(
            "/upstream-secrets",
            get(routes::upstream_secrets_page).post(routes::upsert_upstream_secret),
        )
        .route(
            "/upstream-secrets/delete",
            post(routes::delete_upstream_secret),
        )
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: DashboardState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "admin server listening");
    axum::serve(listener, router(state)).await
}
