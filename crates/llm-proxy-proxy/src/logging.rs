use std::time::Instant;

use llm_proxy_core::tokens::TokenUsage;
use llm_proxy_db::{PayloadCaptureUpdate, RequestLogUpdate};

use crate::{
    payload::{archive_payload, ArchivedPayload, PayloadKind},
    ProxyState,
};

pub(crate) async fn update_request_log_best_effort(
    state: &ProxyState,
    request_log_id: Option<&str>,
    http_status: Option<u16>,
    error_category: Option<String>,
    start: Instant,
    token_usage: Option<TokenUsage>,
    provider_usage_json: Option<String>,
) {
    update_request_log_with_timing(
        state,
        request_log_id,
        RequestLogCompletion {
            http_status,
            error_category,
            start,
            timing: RequestTiming::default(),
            token_usage,
            provider_usage_json,
        },
    )
    .await;
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RequestTiming {
    pub(crate) upstream_first_byte_ms: Option<u64>,
    pub(crate) time_to_first_token_ms: Option<u64>,
    pub(crate) generation_ms: Option<u64>,
}

pub(crate) struct RequestLogCompletion {
    pub(crate) http_status: Option<u16>,
    pub(crate) error_category: Option<String>,
    pub(crate) start: Instant,
    pub(crate) timing: RequestTiming,
    pub(crate) token_usage: Option<TokenUsage>,
    pub(crate) provider_usage_json: Option<String>,
}

pub(crate) async fn update_request_log_with_timing(
    state: &ProxyState,
    request_log_id: Option<&str>,
    completion: RequestLogCompletion,
) {
    let Some(id) = request_log_id else {
        return;
    };
    let token_usage = completion.token_usage;
    let _ = state
        .database
        .update_request_log(
            id,
            RequestLogUpdate {
                http_status: completion.http_status,
                error_category: completion.error_category,
                duration_ms: Some(completion.start.elapsed().as_millis() as u64),
                upstream_first_byte_ms: completion.timing.upstream_first_byte_ms,
                time_to_first_token_ms: completion.timing.time_to_first_token_ms,
                generation_ms: completion.timing.generation_ms,
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
                provider_usage_json: completion.provider_usage_json,
            },
        )
        .await;
}

pub(crate) async fn capture_payload(
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

pub(crate) async fn update_payload_after_failure(
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

pub(crate) async fn update_payload_capture_best_effort(
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
