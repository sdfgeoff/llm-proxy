use uuid::Uuid;

use crate::{Database, DbError};

#[derive(Debug, Clone)]
pub struct NewRequestLog {
    pub proxy_key_id: String,
    pub endpoint: String,
    pub requested_model: Option<String>,
    pub upstream_model: Option<String>,
    pub route_name: Option<String>,
    pub routing_match: Option<String>,
    pub stream: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RequestLogUpdate {
    pub http_status: Option<u16>,
    pub error_category: Option<String>,
    pub duration_ms: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub accepted_prediction_tokens: Option<u64>,
    pub rejected_prediction_tokens: Option<u64>,
    pub token_source: Option<String>,
    pub provider_usage_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PayloadCaptureUpdate {
    pub status: String,
    pub error: Option<String>,
    pub request_path: Option<String>,
    pub response_path: Option<String>,
    pub request_bytes: Option<u64>,
    pub response_bytes: Option<u64>,
    pub request_hash: Option<String>,
    pub response_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestSummary {
    pub id: String,
    pub started_at: String,
    pub proxy_key_label: Option<String>,
    pub endpoint: String,
    pub requested_model: Option<String>,
    pub route_name: Option<String>,
    pub http_status: Option<i64>,
    pub duration_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub payload_capture_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDetail {
    pub id: String,
    pub started_at: String,
    pub proxy_key_label: Option<String>,
    pub endpoint: String,
    pub requested_model: Option<String>,
    pub upstream_model: Option<String>,
    pub route_name: Option<String>,
    pub routing_match: Option<String>,
    pub stream: bool,
    pub http_status: Option<i64>,
    pub error_category: Option<String>,
    pub duration_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub token_source: Option<String>,
    pub provider_usage_json: Option<String>,
    pub payload_capture_status: String,
    pub payload_capture_error: Option<String>,
    pub request_payload_path: Option<String>,
    pub response_payload_path: Option<String>,
    pub request_payload_bytes: Option<i64>,
    pub response_payload_bytes: Option<i64>,
    pub request_payload_hash: Option<String>,
    pub response_payload_hash: Option<String>,
}

impl Database {
    pub async fn insert_request_log(&self, log: NewRequestLog) -> Result<String, DbError> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO request_log (
                id,
                proxy_key_id,
                endpoint,
                requested_model,
                upstream_model,
                route_name,
                routing_match,
                stream
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(log.proxy_key_id)
        .bind(log.endpoint)
        .bind(log.requested_model)
        .bind(log.upstream_model)
        .bind(log.route_name)
        .bind(log.routing_match)
        .bind(i64::from(log.stream))
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn update_request_log(
        &self,
        id: &str,
        update: RequestLogUpdate,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE request_log
            SET http_status = ?,
                error_category = ?,
                duration_ms = ?,
                input_tokens = ?,
                output_tokens = ?,
                total_tokens = ?,
                cached_input_tokens = ?,
                reasoning_tokens = ?,
                accepted_prediction_tokens = ?,
                rejected_prediction_tokens = ?,
                token_source = ?,
                provider_usage_json = ?
            WHERE id = ?
            "#,
        )
        .bind(update.http_status.map(i64::from))
        .bind(update.error_category)
        .bind(update.duration_ms.map(|value| value as i64))
        .bind(update.input_tokens.map(|value| value as i64))
        .bind(update.output_tokens.map(|value| value as i64))
        .bind(update.total_tokens.map(|value| value as i64))
        .bind(update.cached_input_tokens.map(|value| value as i64))
        .bind(update.reasoning_tokens.map(|value| value as i64))
        .bind(update.accepted_prediction_tokens.map(|value| value as i64))
        .bind(update.rejected_prediction_tokens.map(|value| value as i64))
        .bind(update.token_source)
        .bind(update.provider_usage_json)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_payload_capture(
        &self,
        id: &str,
        update: PayloadCaptureUpdate,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE request_log
            SET payload_capture_status = ?,
                payload_capture_error = ?,
                request_payload_path = ?,
                response_payload_path = ?,
                request_payload_bytes = ?,
                response_payload_bytes = ?,
                request_payload_hash = ?,
                response_payload_hash = ?
            WHERE id = ?
            "#,
        )
        .bind(update.status)
        .bind(update.error)
        .bind(update.request_path)
        .bind(update.response_path)
        .bind(update.request_bytes.map(|value| value as i64))
        .bind(update.response_bytes.map(|value| value as i64))
        .bind(update.request_hash)
        .bind(update.response_hash)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn recent_requests(&self, limit: u32) -> Result<Vec<RequestSummary>, DbError> {
        let rows = sqlx::query_as::<_, RequestSummaryRow>(
            r#"
            SELECT
                request_log.id,
                request_log.started_at,
                proxy_api_key.label,
                request_log.endpoint,
                request_log.requested_model,
                request_log.route_name,
                request_log.http_status,
                request_log.duration_ms,
                request_log.input_tokens,
                request_log.output_tokens,
                request_log.total_tokens,
                request_log.payload_capture_status
            FROM request_log
            LEFT JOIN proxy_api_key ON proxy_api_key.id = request_log.proxy_key_id
            ORDER BY request_log.started_at DESC
            LIMIT ?
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(RequestSummary::from)
        .collect();

        Ok(rows)
    }

    pub async fn request_detail(&self, id: &str) -> Result<Option<RequestDetail>, DbError> {
        let detail = sqlx::query_as::<_, RequestDetailRow>(
            r#"
            SELECT
                request_log.id,
                request_log.started_at,
                proxy_api_key.label,
                request_log.endpoint,
                request_log.requested_model,
                request_log.upstream_model,
                request_log.route_name,
                request_log.routing_match,
                request_log.stream,
                request_log.http_status,
                request_log.error_category,
                request_log.duration_ms,
                request_log.input_tokens,
                request_log.output_tokens,
                request_log.total_tokens,
                request_log.cached_input_tokens,
                request_log.reasoning_tokens,
                request_log.token_source,
                request_log.provider_usage_json,
                request_log.payload_capture_status,
                request_log.payload_capture_error,
                request_log.request_payload_path,
                request_log.response_payload_path,
                request_log.request_payload_bytes,
                request_log.response_payload_bytes,
                request_log.request_payload_hash,
                request_log.response_payload_hash
            FROM request_log
            LEFT JOIN proxy_api_key ON proxy_api_key.id = request_log.proxy_key_id
            WHERE request_log.id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(RequestDetail::from);

        Ok(detail)
    }
}

#[derive(sqlx::FromRow)]
struct RequestSummaryRow {
    id: String,
    started_at: String,
    label: Option<String>,
    endpoint: String,
    requested_model: Option<String>,
    route_name: Option<String>,
    http_status: Option<i64>,
    duration_ms: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    payload_capture_status: String,
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
struct RequestDetailRow {
    id: String,
    started_at: String,
    label: Option<String>,
    endpoint: String,
    requested_model: Option<String>,
    upstream_model: Option<String>,
    route_name: Option<String>,
    routing_match: Option<String>,
    stream: i64,
    http_status: Option<i64>,
    error_category: Option<String>,
    duration_ms: Option<i64>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    token_source: Option<String>,
    provider_usage_json: Option<String>,
    payload_capture_status: String,
    payload_capture_error: Option<String>,
    request_payload_path: Option<String>,
    response_payload_path: Option<String>,
    request_payload_bytes: Option<i64>,
    response_payload_bytes: Option<i64>,
    request_payload_hash: Option<String>,
    response_payload_hash: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

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

        let row: (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
            String,
            String,
            String,
        ) = sqlx::query_as(
            "SELECT http_status, duration_ms, input_tokens, output_tokens, total_tokens, cached_input_tokens, reasoning_tokens, accepted_prediction_tokens, rejected_prediction_tokens, token_source, payload_capture_status, request_payload_path FROM request_log WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(db.pool())
        .await
        .expect("fetch log");
        assert_eq!(
            row,
            (
                200,
                123,
                10,
                5,
                15,
                4,
                2,
                3,
                1,
                "provider".to_owned(),
                "complete".to_owned(),
                "req.zst.enc".to_owned()
            )
        );

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
        assert_eq!(detail.total_tokens, Some(15));
        assert_eq!(detail.token_source, Some("provider".to_owned()));
    }
}
