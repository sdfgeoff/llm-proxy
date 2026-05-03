use std::time::Instant;

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Json, Response},
};
use llm_proxy_db::{NewRequestLog, RequestLogUpdate};
use serde_json::json;

use crate::{
    auth::authenticate_proxy_key,
    responses::{internal_error, unauthorized},
    upstream::{fetch_upstream_models, route_url},
    ProxyState,
};

pub(crate) async fn models(State(state): State<ProxyState>, headers: HeaderMap) -> Response {
    let Ok(proxy_key) = authenticate_proxy_key(&state, &headers).await else {
        return unauthorized().into_response();
    };

    let start = Instant::now();
    let request_log_id = state
        .database
        .insert_request_log(NewRequestLog {
            proxy_key_id: proxy_key.id,
            endpoint: "/v1/models".to_owned(),
            requested_model: None,
            upstream_model: None,
            route_name: Some(state.config.default_route.clone()),
            routing_match: Some("default".to_owned()),
            stream: false,
        })
        .await
        .ok();

    let default_route = match state.config.routes.get(&state.config.default_route) {
        Some(route) => route,
        None => return internal_error().into_response(),
    };

    let mut partial = false;
    let mut models = match route_url(&default_route.base_url, "/v1/models") {
        Ok(url) => match fetch_upstream_models(&state, url).await {
            Ok(upstream_models) => upstream_models,
            Err(()) => {
                partial = true;
                Vec::new()
            }
        },
        Err(()) => {
            partial = true;
            Vec::new()
        }
    };

    models.extend(state.config.models.keys().map(|id| {
        json!({
            "id": id,
            "object": "model",
            "created": 0,
            "owned_by": "llm-proxy"
        })
    }));

    if let Some(id) = request_log_id {
        let _ = state
            .database
            .update_request_log(
                &id,
                RequestLogUpdate {
                    http_status: Some(200),
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    ..RequestLogUpdate::default()
                },
            )
            .await;
    }

    let mut response = Json(json!({
        "object": "list",
        "data": models
    }))
    .into_response();

    if partial {
        response.headers_mut().insert(
            "x-llm-proxy-models-partial",
            HeaderValue::from_static("true"),
        );
    }

    response
}
