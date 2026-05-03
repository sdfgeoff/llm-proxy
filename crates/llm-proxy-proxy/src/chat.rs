use std::time::Instant;

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
};
use llm_proxy_core::routing::resolve_route;
use llm_proxy_core::tokens::{estimate_token_usage, token_usage_from_provider, TokenUsage};
use llm_proxy_db::{NewRequestLog, PayloadCaptureUpdate, RequestLogUpdate};
use serde_json::Value;
use tracing::debug;

use crate::{
    auth::{authenticate_proxy_key, AuthFailure},
    payload::{archive_payload, ArchivedPayload, PayloadKind},
    responses::{
        bad_request, internal_error, not_implemented, unauthorized, upstream_secret_missing,
        upstream_unavailable,
    },
    upstream::{content_type_or_json, load_upstream_secret, route_url, SecretLoadError},
    ProxyState,
};

pub(crate) async fn chat_completions(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: Bytes,
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
    if stream {
        return not_implemented("Streaming proxy support is not implemented yet").into_response();
    }

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
            endpoint: "/v1/chat/completions".to_owned(),
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

    let upstream_url = match route_url(&route.base_url, "/v1/chat/completions") {
        Ok(url) => url,
        Err(()) => return internal_error().into_response(),
    };

    debug!(%upstream_url, "forwarding chat completions request");
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

async fn update_request_log_best_effort(
    state: &ProxyState,
    request_log_id: Option<&str>,
    http_status: Option<u16>,
    error_category: Option<String>,
    start: Instant,
    token_usage: Option<TokenUsage>,
    provider_usage_json: Option<String>,
) {
    let Some(id) = request_log_id else {
        return;
    };
    let _ = state
        .database
        .update_request_log(
            id,
            RequestLogUpdate {
                http_status,
                error_category,
                duration_ms: Some(start.elapsed().as_millis() as u64),
                input_tokens: token_usage.as_ref().and_then(|usage| usage.input_tokens),
                output_tokens: token_usage.as_ref().and_then(|usage| usage.output_tokens),
                total_tokens: token_usage.as_ref().and_then(|usage| usage.total_tokens),
                cached_input_tokens: token_usage
                    .as_ref()
                    .and_then(|usage| usage.cached_input_tokens),
                reasoning_tokens: token_usage
                    .as_ref()
                    .and_then(|usage| usage.reasoning_tokens),
                accepted_prediction_tokens: token_usage
                    .as_ref()
                    .and_then(|usage| usage.accepted_prediction_tokens),
                rejected_prediction_tokens: token_usage
                    .as_ref()
                    .and_then(|usage| usage.rejected_prediction_tokens),
                token_source: token_usage.map(|usage| usage.token_source),
                provider_usage_json,
            },
        )
        .await;
}

fn token_usage_for_response(
    request: &Value,
    response_body: &[u8],
) -> (Option<TokenUsage>, Option<String>) {
    let response_json = serde_json::from_slice::<Value>(response_body).ok();
    let provider_usage = response_json.as_ref().and_then(|value| value.get("usage"));
    let provider_usage_json = provider_usage.map(Value::to_string);
    let token_usage = provider_usage
        .and_then(token_usage_from_provider)
        .or_else(|| {
            let response_value = response_json.unwrap_or_else(|| {
                Value::String(String::from_utf8_lossy(response_body).into_owned())
            });
            Some(estimate_token_usage(request, &response_value))
        });

    (token_usage, provider_usage_json)
}

async fn capture_payload(
    state: &ProxyState,
    request_id: &str,
    kind: PayloadKind,
    payload: &[u8],
) -> Result<ArchivedPayload, String> {
    if !state.config.payload_capture.default_enabled {
        return Err("payload capture disabled".to_owned());
    }

    archive_payload(
        &state.config.payload_dir,
        &state.master_key,
        request_id,
        kind,
        payload,
    )
    .map_err(|error| error.to_string())
}

async fn update_payload_after_failure(
    state: &ProxyState,
    request_id: Option<&str>,
    request: Option<ArchivedPayload>,
    error: Option<String>,
) {
    let Some(id) = request_id else {
        return;
    };
    update_payload_capture_best_effort(state, id, request, None, error).await;
}

async fn update_payload_capture_best_effort(
    state: &ProxyState,
    request_id: &str,
    request: Option<ArchivedPayload>,
    response: Option<ArchivedPayload>,
    error: Option<String>,
) {
    let status = if error.is_some() {
        "failed"
    } else if request.is_some() || response.is_some() {
        "complete"
    } else {
        "disabled"
    };
    let _ = state
        .database
        .update_payload_capture(
            request_id,
            PayloadCaptureUpdate {
                status: status.to_owned(),
                error,
                request_path: request
                    .as_ref()
                    .map(|archive| archive.relative_path.clone()),
                response_path: response
                    .as_ref()
                    .map(|archive| archive.relative_path.clone()),
                request_bytes: request.as_ref().map(|archive| archive.raw_bytes),
                response_bytes: response.as_ref().map(|archive| archive.raw_bytes),
                request_hash: request.map(|archive| archive.raw_sha256),
                response_hash: response.map(|archive| archive.raw_sha256),
            },
        )
        .await;
}
