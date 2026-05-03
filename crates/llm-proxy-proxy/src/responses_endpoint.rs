use axum::{body::Bytes, extract::State, http::HeaderMap, response::Response};

use crate::{chat::proxy_completion_endpoint, ProxyState};

pub(crate) async fn responses(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_completion_endpoint(state, headers, body, "/v1/responses").await
}
