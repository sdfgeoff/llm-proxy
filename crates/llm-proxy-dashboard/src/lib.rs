mod auth;
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
        // Auth (SPA — GET returns shell, POST handles action)
        .route("/setup", get(routes::spa).post(routes::setup))
        .route("/login", get(routes::spa).post(routes::login))
        .route("/logout", post(routes::logout))
        // SPA shell (served for all app routes, with auth check)
        .route("/", get(routes::spa))
        .route("/requests", get(routes::spa))
        .route("/requests/{id}", get(routes::spa))
        .route("/keys", get(routes::spa))
        .route("/secrets", get(routes::spa))
        // Payload download (file, not SPA)
        .route("/requests/{id}/payload/{kind}", get(routes::download_payload))
        // API endpoints
        .route("/api/auth/status", get(routes::api_auth_status))
        .route("/api/charts", get(routes::api_charts))
        .route("/api/requests", get(routes::api_requests))
        .route("/api/requests/{id}", get(routes::api_request_detail))
        .route("/api/keys", get(routes::api_keys).post(routes::api_create_key))
        .route("/api/secrets", get(routes::api_secrets).post(routes::api_upsert_secret))
        .route("/api/secrets/delete", post(routes::api_delete_secret))
        // Static files
        // Embedded Vite frontend assets
        .route("/static/style.css", get(routes::serve_css))
        .route("/style.css", get(routes::serve_css))
        .route("/index.js", get(routes::serve_js))
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
