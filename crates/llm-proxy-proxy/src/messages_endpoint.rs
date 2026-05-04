use axum::{body::Bytes, extract::State, http::HeaderMap, response::Response};

use crate::{chat::proxy_completion_endpoint, ProxyState};

pub(crate) async fn messages(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_completion_endpoint(state, headers, body, "/v1/messages").await
}
