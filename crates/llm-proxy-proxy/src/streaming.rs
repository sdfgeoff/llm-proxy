use std::{io, pin::Pin, time::Instant};

use axum::{
    body::{Body, Bytes},
    http::header,
    response::Response,
};
use futures_util::{stream, Stream, StreamExt};
use llm_proxy_core::tokens::{estimate_token_usage, token_usage_from_provider, TokenUsage};
use serde_json::Value;

use crate::{
    logging::{
        capture_payload, update_payload_capture_best_effort, update_request_log_best_effort,
    },
    payload::{ArchivedPayload, PayloadKind},
    ProxyState,
};

type UpstreamByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

pub(crate) struct StreamingResponseInput<S> {
    pub(crate) state: ProxyState,
    pub(crate) request_log_id: Option<String>,
    pub(crate) start: Instant,
    pub(crate) request_payload: Value,
    pub(crate) status: reqwest::StatusCode,
    pub(crate) content_type: header::HeaderValue,
    pub(crate) upstream_stream: S,
    pub(crate) request_archive: Option<ArchivedPayload>,
    pub(crate) payload_capture_error: Option<String>,
}

struct StreamingState {
    proxy_state: ProxyState,
    request_log_id: Option<String>,
    start: Instant,
    request_payload: Value,
    status: u16,
    upstream_stream: UpstreamByteStream,
    request_archive: Option<ArchivedPayload>,
    response_body: Vec<u8>,
    payload_capture_error: Option<String>,
}

pub(crate) fn streaming_response<S>(input: StreamingResponseInput<S>) -> Response
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let status = input.status;
    let content_type = input.content_type;
    let stream_state = StreamingState {
        proxy_state: input.state,
        request_log_id: input.request_log_id,
        start: input.start,
        request_payload: input.request_payload,
        status: status.as_u16(),
        upstream_stream: Box::pin(input.upstream_stream),
        request_archive: input.request_archive,
        response_body: Vec::new(),
        payload_capture_error: input.payload_capture_error,
    };
    let response_stream = stream::try_unfold(stream_state, |mut state| async move {
        match state.upstream_stream.next().await {
            Some(Ok(chunk)) => {
                state.response_body.extend_from_slice(&chunk);
                Ok(Some((chunk, state)))
            }
            Some(Err(_)) => {
                finalize_streaming_request(state, Some("upstream_stream".to_owned())).await;
                Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "upstream stream failed",
                ))
            }
            None => {
                finalize_streaming_request(state, None).await;
                Ok(None)
            }
        }
    });

    let mut response = Response::new(Body::from_stream(response_stream));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response
}

async fn finalize_streaming_request(mut state: StreamingState, error_category: Option<String>) {
    let response_archive = match state.request_log_id.as_deref() {
        Some(id) => match capture_payload(
            &state.proxy_state,
            id,
            PayloadKind::Response,
            &state.response_body,
        )
        .await
        {
            Ok(archive) => Some(archive),
            Err(error) => {
                state.payload_capture_error = Some(error);
                None
            }
        },
        None => None,
    };

    if let Some(id) = state.request_log_id.as_deref() {
        update_payload_capture_best_effort(
            &state.proxy_state,
            id,
            state.request_archive,
            response_archive,
            state.payload_capture_error,
        )
        .await;
    }

    let (token_usage, provider_usage_json) =
        token_usage_for_stream_response(&state.request_payload, &state.response_body);
    update_request_log_best_effort(
        &state.proxy_state,
        state.request_log_id.as_deref(),
        Some(state.status),
        error_category,
        state.start,
        token_usage,
        provider_usage_json,
    )
    .await;
}

fn token_usage_for_stream_response(
    request: &Value,
    response_body: &[u8],
) -> (Option<TokenUsage>, Option<String>) {
    let body = String::from_utf8_lossy(response_body);
    let provider_usage = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "[DONE]")
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|chunk| chunk.get("usage").cloned())
        .find(|usage| !usage.is_null());
    let provider_usage_json = provider_usage.as_ref().map(Value::to_string);
    let token_usage = provider_usage
        .as_ref()
        .and_then(token_usage_from_provider)
        .or_else(|| {
            Some(estimate_token_usage(
                request,
                &Value::String(body.into_owned()),
            ))
        });

    (token_usage, provider_usage_json)
}
