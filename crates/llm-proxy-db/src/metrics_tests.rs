use crate::{Database, NewRequestLog, RequestLogUpdate};

#[tokio::test]
async fn dashboard_metrics_group_requests_by_time_model_key_and_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::connect(&dir.path().join("metrics.sqlite"))
        .await
        .expect("connect database");
    let local_key = db
        .create_proxy_api_key("local", "local-hash")
        .await
        .expect("create local key");
    let ci_key = db
        .create_proxy_api_key("ci", "ci-hash")
        .await
        .expect("create ci key");

    let first = insert_metric_request(&db, &local_key.id, "gpt-5.5", 200, 1000, 25, 25)
        .await
        .expect("first request");
    let second = insert_metric_request(&db, &ci_key.id, "gpt-5.5", 500, 2000, 30, 40)
        .await
        .expect("second request");
    sqlx::query("UPDATE request_log SET time_to_first_token_ms = 120 WHERE id IN (?, ?)")
        .bind(first)
        .bind(second)
        .execute(db.pool())
        .await
        .expect("set ttft");

    let metrics = db.dashboard_metrics().await.expect("metrics");

    assert_eq!(metrics.overview.request_count, 2);
    assert_eq!(metrics.overview.total_tokens, 120);
    assert_eq!(metrics.overview.error_count, 1);
    assert_eq!(metrics.overview.avg_time_to_first_token_ms, Some(120.0));
    assert_eq!(metrics.hourly.len(), 1);
    assert_eq!(metrics.hourly[0].total_tokens, 120);
    assert_eq!(metrics.by_model[0].label, "gpt-5.5");
    assert_eq!(metrics.by_model[0].request_count, 2);
    assert_eq!(metrics.by_key.len(), 2);
    assert!(metrics.by_status.iter().any(|row| row.label == "2xx"));
    assert!(metrics.by_status.iter().any(|row| row.label == "5xx"));
}

async fn insert_metric_request(
    db: &Database,
    proxy_key_id: &str,
    model: &str,
    status: u16,
    duration_ms: u64,
    input_tokens: u64,
    output_tokens: u64,
) -> Result<String, crate::DbError> {
    let id = db
        .insert_request_log(NewRequestLog {
            proxy_key_id: proxy_key_id.to_owned(),
            endpoint: "/v1/chat/completions".to_owned(),
            requested_model: Some(model.to_owned()),
            upstream_model: Some(model.to_owned()),
            route_name: Some("default".to_owned()),
            routing_match: Some("default".to_owned()),
            stream: false,
        })
        .await?;
    db.update_request_log(
        &id,
        RequestLogUpdate {
            http_status: Some(status),
            duration_ms: Some(duration_ms),
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            total_tokens: Some(input_tokens + output_tokens),
            token_source: Some("provider".to_owned()),
            ..RequestLogUpdate::default()
        },
    )
    .await?;
    Ok(id)
}
