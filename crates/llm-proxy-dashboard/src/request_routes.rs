use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};

use crate::{
    auth::{require_admin, AuthState},
    pages::{render_request_detail_page, render_requests_page},
    payloads::read_payload,
    render::{internal_error, page},
    DashboardState,
};

pub(crate) async fn requests_page(
    State(state): State<DashboardState>,
    headers: HeaderMap,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => render_requests_page(&state).await,
        Ok(AuthState::NeedsSetup) => Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => Redirect::to("/login").into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn request_detail_page(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    match state.database.request_detail(&id).await {
        Ok(Some(request)) => render_request_detail_page(&request),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Html(page("Not Found", "<p>Request not found.</p>")),
        )
            .into_response(),
        Err(_) => internal_error(),
    }
}

pub(crate) async fn download_payload(
    State(state): State<DashboardState>,
    headers: HeaderMap,
    Path((id, kind)): Path<(String, String)>,
) -> Response {
    match require_admin(&state, &headers).await {
        Ok(AuthState::Authenticated) => {}
        Ok(AuthState::NeedsSetup) => return Redirect::to("/setup").into_response(),
        Ok(AuthState::Unauthenticated) => return Redirect::to("/login").into_response(),
        Err(response) => return response,
    }

    let detail = match state.database.request_detail(&id).await {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Html(page("Not Found", "<p>Request not found.</p>")),
            )
                .into_response();
        }
        Err(_) => return internal_error(),
    };
    let path = match kind.as_str() {
        "request" => detail.request_payload_path.as_deref(),
        "response" => detail.response_payload_path.as_deref(),
        _ => None,
    };
    let Some(path) = path else {
        return (
            StatusCode::NOT_FOUND,
            Html(page("Not Found", "<p>Payload not found.</p>")),
        )
            .into_response();
    };

    match read_payload(&state.config.payload_dir, &state.master_key, path) {
        Ok(bytes) => {
            let mut response = Response::new(axum::body::Body::from(bytes));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{id}_{kind}.json\""))
                    .expect("content disposition should be header-safe"),
            );
            response
        }
        Err(_) => internal_error(),
    }
}
