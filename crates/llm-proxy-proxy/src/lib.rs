use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use llm_proxy_core::{auth::hash_lookup_token, routing::resolve_route, Config, MasterKey};
use llm_proxy_db::{Database, NewRequestLog, ProxyApiKey, RequestLogUpdate};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tracing::{debug, info};
use url::Url;

#[derive(Clone)]
pub struct ProxyState {
    config: Arc<Config>,
    database: Database,
    master_key: MasterKey,
    client: Client,
}

impl ProxyState {
    pub fn new(config: Arc<Config>, database: Database, master_key: MasterKey) -> Self {
        Self {
            config,
            database,
            master_key,
            client: Client::new(),
        }
    }
}

pub fn router(state: ProxyState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, state: ProxyState) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(%addr, "proxy server listening");
    axum::serve(listener, router(state)).await
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn models(State(state): State<ProxyState>, headers: HeaderMap) -> Response {
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

    models.extend(
        state
            .config
            .models
            .keys()
            .map(|id| {
                json!({
                    "id": id,
                    "object": "model",
                    "created": 0,
                    "owned_by": "llm-proxy"
                })
            })
            .collect::<Vec<_>>(),
    );

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

async fn chat_completions(
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

    let upstream_url = match route_url(&route.base_url, "/v1/chat/completions") {
        Ok(url) => url,
        Err(()) => return internal_error().into_response(),
    };

    debug!(%upstream_url, "forwarding chat completions request");
    let mut request = state.client.post(upstream_url).json(&payload);
    if let Some(api_key) = upstream_api_key {
        request = request.bearer_auth(api_key);
    }
    let upstream_result = request.send().await;
    let upstream_response = match upstream_result {
        Ok(response) => response,
        Err(_) => {
            update_request_log_best_effort(
                &state,
                request_log_id.as_deref(),
                None,
                Some("upstream_request".to_owned()),
                start,
                None,
            )
            .await;
            return upstream_unavailable().into_response();
        }
    };

    let status = upstream_response.status();
    let content_type = upstream_response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));
    let body = match upstream_response.bytes().await {
        Ok(body) => body,
        Err(_) => {
            update_request_log_best_effort(
                &state,
                request_log_id.as_deref(),
                Some(status.as_u16()),
                Some("upstream_body".to_owned()),
                start,
                None,
            )
            .await;
            return upstream_unavailable().into_response();
        }
    };

    let provider_usage_json = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|value| value.get("usage").cloned())
        .map(|usage| usage.to_string());

    update_request_log_best_effort(
        &state,
        request_log_id.as_deref(),
        Some(status.as_u16()),
        None,
        start,
        provider_usage_json,
    )
    .await;

    let mut response = Response::new(axum::body::Body::from(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

enum AuthFailure {
    Unauthorized,
    Internal,
}

async fn authenticate_proxy_key(
    state: &ProxyState,
    headers: &HeaderMap,
) -> Result<ProxyApiKey, AuthFailure> {
    let Some(token) = bearer_token(headers) else {
        return Err(AuthFailure::Unauthorized);
    };

    match state
        .database
        .proxy_api_key_by_hash(&hash_lookup_token(token))
        .await
    {
        Ok(Some(key)) => Ok(key),
        Ok(None) => Err(AuthFailure::Unauthorized),
        Err(_) => Err(AuthFailure::Internal),
    }
}

async fn fetch_upstream_models(state: &ProxyState, url: Url) -> Result<Vec<Value>, ()> {
    let response = state.client.get(url).send().await.map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let body = response.json::<Value>().await.map_err(|_| ())?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

enum SecretLoadError {
    Missing,
    Internal,
}

async fn load_upstream_secret(state: &ProxyState, name: &str) -> Result<String, SecretLoadError> {
    let Some(secret) = state
        .database
        .upstream_secret(name)
        .await
        .map_err(|_| SecretLoadError::Internal)?
    else {
        return Err(SecretLoadError::Missing);
    };

    state
        .master_key
        .decrypt(&secret.encrypted_value, &secret.nonce)
        .map_err(|_| SecretLoadError::Internal)
}

fn route_url(base_url: &Url, path: &str) -> Result<Url, ()> {
    base_url.join(path.trim_start_matches('/')).map_err(|_| ())
}

async fn update_request_log_best_effort(
    state: &ProxyState,
    request_log_id: Option<&str>,
    http_status: Option<u16>,
    error_category: Option<String>,
    start: Instant,
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
                provider_usage_json,
            },
        )
        .await;
}

fn unauthorized() -> impl IntoResponse {
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

fn internal_error() -> impl IntoResponse {
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

fn bad_request(message: &'static str) -> impl IntoResponse {
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

fn not_implemented(message: &'static str) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": {
                "message": message,
                "type": "proxy_not_implemented"
            }
        })),
    )
}

fn upstream_unavailable() -> impl IntoResponse {
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

fn upstream_secret_missing(secret_name: &str) -> impl IntoResponse {
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::post,
    };
    use llm_proxy_core::{
        auth::{generate_proxy_api_key, hash_lookup_token},
        config::{Config, ModelRoute, RouteConfig},
    };
    use tower::ServiceExt;
    use url::Url;

    use super::*;

    async fn state_with_key(config: Config) -> (ProxyState, String, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let database = Database::connect(&dir.path().join("test.sqlite"))
            .await
            .expect("database");
        let token = generate_proxy_api_key();
        database
            .create_proxy_api_key("test", &hash_lookup_token(&token))
            .await
            .expect("create key");
        let master_key = MasterKey::load_or_create(&dir.path().join("master.key")).expect("key");
        (
            ProxyState::new(Arc::new(config), database, master_key),
            token,
            dir,
        )
    }

    #[tokio::test]
    async fn models_requires_proxy_key() {
        let (state, _, _dir) = state_with_key(Config::default()).await;
        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn models_returns_configured_models_when_upstream_unavailable() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "local".to_owned(),
            RouteConfig {
                base_url: Url::parse("http://127.0.0.1:9").expect("url"),
                upstream_api_key: None,
            },
        );
        let mut models = BTreeMap::new();
        models.insert(
            "fast-local".to_owned(),
            ModelRoute {
                route: "local".to_owned(),
                upstream_model: Some("llama".to_owned()),
            },
        );
        let config = Config {
            routes,
            models,
            ..Config::default()
        };
        let (state, token, _dir) = state_with_key(config).await;

        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("x-llm-proxy-models-partial"),
            Some(&http::HeaderValue::from_static("true"))
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let json: Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["data"][0]["id"], "fast-local");
    }

    #[tokio::test]
    async fn chat_completions_forwards_configured_upstream_secret() {
        let upstream = axum::Router::new().route(
            "/v1/chat/completions",
            post(|headers: HeaderMap| async move {
                assert_eq!(
                    headers.get(header::AUTHORIZATION),
                    Some(&http::HeaderValue::from_static("Bearer upstream-secret"))
                );
                Json(json!({
                    "id": "chatcmpl-test",
                    "object": "chat.completion",
                    "usage": { "prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3 }
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream");
        let upstream_addr = listener.local_addr().expect("upstream addr");
        tokio::spawn(async move {
            axum::serve(listener, upstream)
                .await
                .expect("serve upstream");
        });

        let mut routes = BTreeMap::new();
        routes.insert(
            "openai".to_owned(),
            RouteConfig {
                base_url: Url::parse(&format!("http://{upstream_addr}")).expect("url"),
                upstream_api_key: Some("openai-prod".to_owned()),
            },
        );
        let config = Config {
            default_route: "openai".to_owned(),
            routes,
            ..Config::default()
        };
        let (state, token, _dir) = state_with_key(config).await;
        let encrypted = state
            .master_key
            .encrypt("upstream-secret")
            .expect("encrypt");
        state
            .database
            .upsert_upstream_secret("openai-prod", &encrypted.ciphertext, &encrypted.nonce)
            .await
            .expect("store secret");

        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"gpt-5.5","messages":[{"role":"user","content":"hi"}]}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
