use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::json;

pub(crate) fn unauthorized() -> impl IntoResponse {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "message": "Missing or invalid proxy API key",
                "type": "authentication_error"
            }
        })),
    )
}

pub(crate) fn internal_error() -> impl IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": {
                "message": "Internal proxy error",
                "type": "proxy_error"
            }
        })),
    )
}

pub(crate) fn bad_request(message: &'static str) -> impl IntoResponse {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": {
                "message": message,
                "type": "invalid_request_error"
            }
        })),
    )
}

pub(crate) fn upstream_unavailable() -> impl IntoResponse {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": {
                "message": "Upstream request failed",
                "type": "upstream_error"
            }
        })),
    )
}

pub(crate) fn upstream_secret_missing(secret_name: &str) -> impl IntoResponse {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({
            "error": {
                "message": format!("Upstream secret '{secret_name}' is not configured"),
                "type": "upstream_secret_missing"
            }
        })),
    )
}
