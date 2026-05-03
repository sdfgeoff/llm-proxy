use axum::response::IntoResponse;

pub(crate) async fn health() -> impl IntoResponse {
    "ok"
}
