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
                provider_usage_json = ?
            WHERE id = ?
            "#,
        )
        .bind(update.http_status.map(i64::from))
        .bind(update.error_category)
        .bind(update.duration_ms.map(|value| value as i64))
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

        let row: (i64, i64, String, String) = sqlx::query_as(
            "SELECT http_status, duration_ms, payload_capture_status, request_payload_path FROM request_log WHERE id = ?",
        )
        .bind(id)
        .fetch_one(db.pool())
        .await
        .expect("fetch log");
        assert_eq!(
            row,
            (200, 123, "complete".to_owned(), "req.zst.enc".to_owned())
        );
    }
}
