use std::time::Instant;

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
};
use llm_proxy_core::routing::resolve_route;
use llm_proxy_db::NewRequestLog;
use serde_json::Value;
use tracing::debug;

use crate::{
    auth::{authenticate_proxy_key, AuthFailure},
    logging::{
        capture_payload, update_payload_after_failure, update_payload_capture_best_effort,
        update_request_log_best_effort,
    },
    payload::PayloadKind,
    responses::{
        bad_request, internal_error, unauthorized, upstream_secret_missing, upstream_unavailable,
    },
    streaming::{streaming_response, StreamingResponseInput},
    upstream::{content_type_or_json, load_upstream_secret, route_url, SecretLoadError},
    usage::token_usage_for_response,
    ProxyState,
};

pub(crate) async fn chat_completions(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    proxy_completion_endpoint(state, headers, body, "/v1/chat/completions").await
}

pub(crate) async fn proxy_completion_endpoint(
    state: ProxyState,
    headers: HeaderMap,
    body: Bytes,
    endpoint: &'static str,
) -> Response {
    let proxy_key = match authenticate_proxy_key(&state, &headers).await {
        Ok(proxy_key) => proxy_key,
        Err(AuthFailure::Unauthorized) => return unauthorized().into_response(),
        Err(AuthFailure::Internal) => return internal_error().into_response(),
    };

    let start = Instant::now();
    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(_) => return bad_request("Request body must be valid JSON").into_response(),
    };

    let requested_model = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let stream = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(requested_model_value) = requested_model.as_deref() else {
        return bad_request("Request body must include a string model field").into_response();
    };

    let resolved = resolve_route(&state.config, requested_model_value);
    let Some(route) = state.config.routes.get(&resolved.route_name) else {
        return internal_error().into_response();
    };

    let upstream_api_key = match route.upstream_api_key.as_deref() {
        Some(secret_name) => match load_upstream_secret(&state, secret_name).await {
            Ok(secret) => Some(secret),
            Err(SecretLoadError::Missing) => {
                return upstream_secret_missing(secret_name).into_response();
            }
            Err(SecretLoadError::Internal) => return internal_error().into_response(),
        },
        None => None,
    };

    if let Some(model) = payload.get_mut("model") {
        *model = Value::String(resolved.upstream_model.clone());
    }

    let request_log_id = state
        .database
        .insert_request_log(NewRequestLog {
            proxy_key_id: proxy_key.id,
            endpoint: endpoint.to_owned(),
            requested_model,
            upstream_model: Some(resolved.upstream_model),
            route_name: Some(resolved.route_name),
            routing_match: Some(resolved.routing_match.as_str().to_owned()),
            stream,
        })
        .await
        .ok();

    let mut request_archive = None;
    let mut response_archive = None;
    let mut payload_capture_error = None;
    if let Some(id) = request_log_id.as_deref() {
        match capture_payload(&state, id, PayloadKind::Request, &body).await {
            Ok(archive) => request_archive = Some(archive),
            Err(error) => payload_capture_error = Some(error),
        }
    }

    let upstream_url = match route_url(&route.base_url, endpoint) {
        Ok(url) => url,
        Err(()) => return internal_error().into_response(),
    };

    debug!(%upstream_url, %endpoint, "forwarding proxy request");
    let mut request = state.client.post(upstream_url).json(&payload);
    if let Some(api_key) = upstream_api_key {
        request = request.bearer_auth(api_key);
    }
    let upstream_response = match request.send().await {
        Ok(response) => response,
        Err(_) => {
            update_payload_after_failure(
                &state,
                request_log_id.as_deref(),
                request_archive,
                payload_capture_error,
            )
            .await;
            update_request_log_best_effort(
                &state,
                request_log_id.as_deref(),
                None,
                Some("upstream_request".to_owned()),
                start,
                None,
                None,
            )
            .await;
            return upstream_unavailable().into_response();
        }
    };

    let status = upstream_response.status();
    let content_type = content_type_or_json(upstream_response.headers());
    if stream {
        return streaming_response(StreamingResponseInput {
            state,
            request_log_id,
            start,
            request_payload: payload,
            status,
            content_type,
            upstream_stream: upstream_response.bytes_stream(),
            request_archive,
            payload_capture_error,
        });
    }

    let response_body = match upstream_response.bytes().await {
        Ok(body) => body,
        Err(_) => {
            update_payload_after_failure(
                &state,
                request_log_id.as_deref(),
                request_archive,
                payload_capture_error,
            )
            .await;
            update_request_log_best_effort(
                &state,
                request_log_id.as_deref(),
                Some(status.as_u16()),
                Some("upstream_body".to_owned()),
                start,
                None,
                None,
            )
            .await;
            return upstream_unavailable().into_response();
        }
    };

    if let Some(id) = request_log_id.as_deref() {
        match capture_payload(&state, id, PayloadKind::Response, &response_body).await {
            Ok(archive) => response_archive = Some(archive),
            Err(error) => payload_capture_error = Some(error),
        }
        update_payload_capture_best_effort(
            &state,
            id,
            request_archive,
            response_archive,
            payload_capture_error,
        )
        .await;
    }

    let (token_usage, provider_usage_json) = token_usage_for_response(&payload, &response_body);

    update_request_log_best_effort(
        &state,
        request_log_id.as_deref(),
        Some(status.as_u16()),
        None,
        start,
        token_usage,
        provider_usage_json,
    )
    .await;

    let mut response = Response::new(axum::body::Body::from(response_body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response
}
