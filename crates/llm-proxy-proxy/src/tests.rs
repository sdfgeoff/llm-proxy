use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, Request, StatusCode},
    response::Json,
    routing::post,
};
use llm_proxy_core::{
    auth::{generate_proxy_api_key, hash_lookup_token},
    config::{Config, ModelRoute, RouteConfig},
    MasterKey,
};
use llm_proxy_db::Database;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;

use crate::{router, ProxyState};

async fn state_with_key(mut config: Config) -> (ProxyState, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    config.payload_dir = dir.path().join("payloads");
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
