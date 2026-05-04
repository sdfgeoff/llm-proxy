use crate::{Database, NewRequestLog, PayloadCaptureUpdate, RequestLogUpdate};

#[tokio::test]
async fn inserts_and_updates_request_log() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.sqlite");
    let db = Database::connect(&path).await.expect("connect database");
    let key = db
        .create_proxy_api_key("local dev", "key-hash")
        .await
        .expect("create key");

    let id = db
        .insert_request_log(NewRequestLog {
            proxy_key_id: key.id,
            endpoint: "/v1/chat/completions".to_owned(),
            requested_model: Some("fast-local".to_owned()),
            upstream_model: Some("llama".to_owned()),
            route_name: Some("local".to_owned()),
            routing_match: Some("explicit".to_owned()),
            stream: false,
        })
        .await
        .expect("insert log");
    db.update_request_log(
        &id,
        RequestLogUpdate {
            http_status: Some(200),
            duration_ms: Some(123),
            upstream_first_byte_ms: Some(25),
            time_to_first_token_ms: Some(40),
            generation_ms: Some(83),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cached_input_tokens: Some(4),
            reasoning_tokens: Some(2),
            accepted_prediction_tokens: Some(3),
            rejected_prediction_tokens: Some(1),
            token_source: Some("provider".to_owned()),
            provider_usage_json: Some(r#"{"total_tokens":15}"#.to_owned()),
            ..RequestLogUpdate::default()
        },
    )
    .await
    .expect("update log");
    db.update_payload_capture(
        &id,
        PayloadCaptureUpdate {
            status: "complete".to_owned(),
            request_path: Some("req.zst.enc".to_owned()),
            response_path: Some("res.zst.enc".to_owned()),
            request_bytes: Some(10),
            response_bytes: Some(20),
            request_hash: Some("req-hash".to_owned()),
            response_hash: Some("res-hash".to_owned()),
            ..PayloadCaptureUpdate::default()
        },
    )
    .await
    .expect("update payload");

    let row: RequestLogTestRow =
        sqlx::query_as(
            "SELECT http_status, duration_ms, upstream_first_byte_ms, time_to_first_token_ms, generation_ms, input_tokens, output_tokens, total_tokens, cached_input_tokens, reasoning_tokens, accepted_prediction_tokens, rejected_prediction_tokens, token_source, payload_capture_status, request_payload_path FROM request_log WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(db.pool())
        .await
        .expect("fetch log");
    assert_eq!(row.http_status, 200);
    assert_eq!(row.duration_ms, 123);
    assert_eq!(row.upstream_first_byte_ms, 25);
    assert_eq!(row.time_to_first_token_ms, 40);
    assert_eq!(row.generation_ms, 83);
    assert_eq!(row.input_tokens, 10);
    assert_eq!(row.output_tokens, 5);
    assert_eq!(row.total_tokens, 15);
    assert_eq!(row.cached_input_tokens, 4);
    assert_eq!(row.reasoning_tokens, 2);
    assert_eq!(row.accepted_prediction_tokens, 3);
    assert_eq!(row.rejected_prediction_tokens, 1);
    assert_eq!(row.token_source, "provider");
    assert_eq!(row.payload_capture_status, "complete");
    assert_eq!(row.request_payload_path, "req.zst.enc");

    let recent = db.recent_requests(10).await.expect("recent requests");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, id);
    assert_eq!(recent[0].http_status, Some(200));

    let detail = db
        .request_detail(&recent[0].id)
        .await
        .expect("request detail")
        .expect("detail exists");
    assert_eq!(detail.payload_capture_status, "complete");
    assert_eq!(detail.request_payload_path, Some("req.zst.enc".to_owned()));
    assert_eq!(detail.time_to_first_token_ms, Some(40));
    assert_eq!(detail.generation_ms, Some(83));
    assert_eq!(detail.total_tokens, Some(15));
    assert_eq!(detail.token_source, Some("provider".to_owned()));
}

#[derive(sqlx::FromRow)]
struct RequestLogTestRow {
    http_status: i64,
    duration_ms: i64,
    upstream_first_byte_ms: i64,
    time_to_first_token_ms: i64,
    generation_ms: i64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cached_input_tokens: i64,
    reasoning_tokens: i64,
    accepted_prediction_tokens: i64,
    rejected_prediction_tokens: i64,
    token_source: String,
    payload_capture_status: String,
    request_payload_path: String,
}
