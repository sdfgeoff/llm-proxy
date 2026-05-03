use crate::{RequestDetail, RequestSummary};

#[derive(sqlx::FromRow)]
pub(crate) struct RequestSummaryRow {
    pub(crate) id: String,
    pub(crate) started_at: String,
    pub(crate) label: Option<String>,
    pub(crate) endpoint: String,
    pub(crate) requested_model: Option<String>,
    pub(crate) route_name: Option<String>,
    pub(crate) http_status: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) payload_capture_status: String,
}

impl From<RequestSummaryRow> for RequestSummary {
    fn from(row: RequestSummaryRow) -> Self {
        Self {
            id: row.id,
            started_at: row.started_at,
            proxy_key_label: row.label,
            endpoint: row.endpoint,
            requested_model: row.requested_model,
            route_name: row.route_name,
            http_status: row.http_status,
            duration_ms: row.duration_ms,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            payload_capture_status: row.payload_capture_status,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(crate) struct RequestDetailRow {
    pub(crate) id: String,
    pub(crate) started_at: String,
    pub(crate) label: Option<String>,
    pub(crate) endpoint: String,
    pub(crate) requested_model: Option<String>,
    pub(crate) upstream_model: Option<String>,
    pub(crate) route_name: Option<String>,
    pub(crate) routing_match: Option<String>,
    pub(crate) stream: i64,
    pub(crate) http_status: Option<i64>,
    pub(crate) error_category: Option<String>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) input_tokens: Option<i64>,
    pub(crate) output_tokens: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) cached_input_tokens: Option<i64>,
    pub(crate) reasoning_tokens: Option<i64>,
    pub(crate) token_source: Option<String>,
    pub(crate) provider_usage_json: Option<String>,
    pub(crate) payload_capture_status: String,
    pub(crate) payload_capture_error: Option<String>,
    pub(crate) request_payload_path: Option<String>,
    pub(crate) response_payload_path: Option<String>,
    pub(crate) request_payload_bytes: Option<i64>,
    pub(crate) response_payload_bytes: Option<i64>,
    pub(crate) request_payload_hash: Option<String>,
    pub(crate) response_payload_hash: Option<String>,
}

impl From<RequestDetailRow> for RequestDetail {
    fn from(row: RequestDetailRow) -> Self {
        Self {
            id: row.id,
            started_at: row.started_at,
            proxy_key_label: row.label,
            endpoint: row.endpoint,
            requested_model: row.requested_model,
            upstream_model: row.upstream_model,
            route_name: row.route_name,
            routing_match: row.routing_match,
            stream: row.stream != 0,
            http_status: row.http_status,
            error_category: row.error_category,
            duration_ms: row.duration_ms,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            cached_input_tokens: row.cached_input_tokens,
            reasoning_tokens: row.reasoning_tokens,
            token_source: row.token_source,
            provider_usage_json: row.provider_usage_json,
            payload_capture_status: row.payload_capture_status,
            payload_capture_error: row.payload_capture_error,
            request_payload_path: row.request_payload_path,
            response_payload_path: row.response_payload_path,
            request_payload_bytes: row.request_payload_bytes,
            response_payload_bytes: row.response_payload_bytes,
            request_payload_hash: row.request_payload_hash,
            response_payload_hash: row.response_payload_hash,
        }
    }
}
