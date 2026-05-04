mod auth;
mod dashboard_charts;
mod dashboard_page;
mod pages;
mod payloads;
mod render;
mod request_routes;
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
        .route("/requests", get(request_routes::requests_page))
        .route("/requests/{id}", get(request_routes::request_detail_page))
        .route(
            "/requests/{id}/payload/{kind}",
            get(request_routes::download_payload),
        )
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use llm_proxy_core::{Config, MasterKey};
    use llm_proxy_db::Database;

    use super::*;

    #[tokio::test]
    async fn dashboard_router_builds_with_current_axum_route_syntax() {
        let dir = tempfile::tempdir().expect("tempdir");
        let database = Database::connect(&dir.path().join("dashboard.sqlite"))
            .await
            .expect("database");
        let master_key = MasterKey::load_or_create(&dir.path().join("master.key")).expect("key");
        let config = Config {
            payload_dir: dir.path().join("payloads"),
            ..Config::default()
        };

        let state = DashboardState::new(Arc::new(config), database, master_key, None);

        let _ = router(state);
    }
}
