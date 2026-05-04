use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
    response::Json,
    routing::post,
};
use llm_proxy_core::{
    auth::{generate_proxy_api_key, hash_lookup_token},
    config::{Config, RouteConfig},
    MasterKey,
};
use llm_proxy_db::Database;
use serde_json::{json, Value};
use tower::ServiceExt;
use url::Url;

use crate::{router, ProxyState};

#[tokio::test]
async fn messages_accepts_x_api_key_and_forwards_anthropic_headers() {
    let upstream = axum::Router::new().route(
        "/v1/messages",
        post(|headers: HeaderMap| async move {
            assert_eq!(
                headers.get("x-api-key"),
                Some(&http::HeaderValue::from_static("upstream-anthropic"))
            );
            assert_eq!(
                headers.get("anthropic-version"),
                Some(&http::HeaderValue::from_static("2023-06-01"))
            );
            Json(json!({
                "id": "msg-test",
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "text", "text": "ok" }],
                "usage": { "input_tokens": 5, "output_tokens": 6 }
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
        "anthropic".to_owned(),
        RouteConfig {
            base_url: Url::parse(&format!("http://{upstream_addr}")).expect("url"),
            upstream_api_key: Some("anthropic-prod".to_owned()),
        },
    );
    let config = Config {
        default_route: "anthropic".to_owned(),
        routes,
        ..Config::default()
    };
    let (state, token, _dir) = state_with_key(config).await;
    let database = state.database.clone();
    let encrypted = state
        .master_key
        .encrypt("upstream-anthropic")
        .expect("encrypt");
    state
        .database
        .upsert_upstream_secret("anthropic-prod", &encrypted.ciphertext, &encrypted.nonce)
        .await
        .expect("store secret");

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/messages")
                .header("x-api-key", token)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"model":"claude-test","max_tokens":16,"messages":[{"role":"user","content":"hi"}]}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["id"], "msg-test");

    let requests = database.recent_requests(1).await.expect("requests");
    let detail = database
        .request_detail(&requests[0].id)
        .await
        .expect("request detail")
        .expect("detail");
    assert_eq!(detail.endpoint, "/v1/messages");
    assert_eq!(detail.total_tokens, Some(11));
}

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
