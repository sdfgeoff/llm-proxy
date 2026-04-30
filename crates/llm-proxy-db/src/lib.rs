use std::path::Path;

use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, SqlitePool};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tracing::log::LevelFilter;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyApiKey {
    pub id: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamSecret {
    pub name: String,
    pub encrypted_value: Vec<u8>,
    pub nonce: Vec<u8>,
    pub created_at: String,
    pub updated_at: String,
}

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

impl Database {
    pub async fn connect(path: &Path) -> Result<Self, DbError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .log_statements(LevelFilter::Debug);

        let pool = SqlitePool::connect_with(options).await?;
        let database = Self { pool };
        database.initialize().await?;
        Ok(database)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn has_admin_account(&self) -> Result<bool, DbError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admin_account WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0 > 0)
    }

    pub async fn set_admin_password_hash(&self, password_hash: &str) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO admin_account (id, password_hash, updated_at)
            VALUES (1, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                password_hash = excluded.password_hash,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(password_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn admin_password_hash(&self) -> Result<Option<String>, DbError> {
        let hash = sqlx::query_scalar("SELECT password_hash FROM admin_account WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(hash)
    }

    pub async fn create_admin_session(
        &self,
        session_hash: &str,
        ttl: Duration,
    ) -> Result<String, DbError> {
        let id = Uuid::now_v7().to_string();
        let expires_at = OffsetDateTime::now_utc() + ttl;
        sqlx::query(
            r#"
            INSERT INTO admin_session (id, session_hash, expires_at)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(session_hash)
        .bind(format_timestamp(expires_at))
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn admin_session_exists(&self, session_hash: &str) -> Result<bool, DbError> {
        let row: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM admin_session
            WHERE session_hash = ?
              AND expires_at > CURRENT_TIMESTAMP
            "#,
        )
        .bind(session_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0 > 0)
    }

    pub async fn delete_admin_session(&self, session_hash: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM admin_session WHERE session_hash = ?")
            .bind(session_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_proxy_api_key(
        &self,
        label: &str,
        key_hash: &str,
    ) -> Result<ProxyApiKey, DbError> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"
            INSERT INTO proxy_api_key (id, label, key_hash)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(label)
        .bind(key_hash)
        .execute(&self.pool)
        .await?;

        Ok(self
            .proxy_api_key_by_id(&id)
            .await?
            .expect("created proxy key should be readable"))
    }

    pub async fn proxy_api_key_by_hash(
        &self,
        key_hash: &str,
    ) -> Result<Option<ProxyApiKey>, DbError> {
        let key = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, label, created_at
            FROM proxy_api_key
            WHERE key_hash = ?
              AND revoked_at IS NULL
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?
        .map(proxy_api_key_from_row);

        Ok(key)
    }

    pub async fn list_proxy_api_keys(&self) -> Result<Vec<ProxyApiKey>, DbError> {
        let keys = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, label, created_at
            FROM proxy_api_key
            WHERE revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(proxy_api_key_from_row)
        .collect();

        Ok(keys)
    }

    pub async fn upsert_upstream_secret(
        &self,
        name: &str,
        encrypted_value: &[u8],
        nonce: &[u8],
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO upstream_secret (name, encrypted_value, nonce, updated_at)
            VALUES (?, ?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(name) DO UPDATE SET
                encrypted_value = excluded.encrypted_value,
                nonce = excluded.nonce,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(name)
        .bind(encrypted_value)
        .bind(nonce)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upstream_secret(&self, name: &str) -> Result<Option<UpstreamSecret>, DbError> {
        let secret = sqlx::query_as::<_, (String, Vec<u8>, Vec<u8>, String, String)>(
            r#"
            SELECT name, encrypted_value, nonce, created_at, updated_at
            FROM upstream_secret
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .map(upstream_secret_from_row);

        Ok(secret)
    }

    pub async fn list_upstream_secrets(&self) -> Result<Vec<UpstreamSecret>, DbError> {
        let secrets = sqlx::query_as::<_, (String, Vec<u8>, Vec<u8>, String, String)>(
            r#"
            SELECT name, encrypted_value, nonce, created_at, updated_at
            FROM upstream_secret
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(upstream_secret_from_row)
        .collect();

        Ok(secrets)
    }

    pub async fn delete_upstream_secret(&self, name: &str) -> Result<(), DbError> {
        sqlx::query("DELETE FROM upstream_secret WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

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

    async fn proxy_api_key_by_id(&self, id: &str) -> Result<Option<ProxyApiKey>, DbError> {
        let key = sqlx::query_as::<_, (String, String, String)>(
            r#"
            SELECT id, label, created_at
            FROM proxy_api_key
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .map(proxy_api_key_from_row);

        Ok(key)
    }

    async fn initialize(&self) -> Result<(), DbError> {
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;
        for statement in MIGRATION_0001
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sqlx::query(statement).execute(&self.pool).await?;
        }
        Ok(())
    }
}

fn proxy_api_key_from_row(row: (String, String, String)) -> ProxyApiKey {
    ProxyApiKey {
        id: row.0,
        label: row.1,
        created_at: row.2,
    }
}

fn upstream_secret_from_row(row: (String, Vec<u8>, Vec<u8>, String, String)) -> UpstreamSecret {
    UpstreamSecret {
        name: row.0,
        encrypted_value: row.1,
        nonce: row.2,
        created_at: row.3,
        updated_at: row.4,
    }
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting should not fail")
}

const MIGRATION_0001: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);

CREATE TABLE IF NOT EXISTS admin_account (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS admin_session (
    id TEXT PRIMARY KEY,
    session_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS proxy_api_key (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    revoked_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS upstream_secret (
    name TEXT PRIMARY KEY,
    encrypted_value BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS request_log (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    proxy_key_id TEXT,
    endpoint TEXT NOT NULL,
    requested_model TEXT,
    upstream_model TEXT,
    route_name TEXT,
    routing_match TEXT,
    stream INTEGER NOT NULL DEFAULT 0,
    http_status INTEGER,
    error_category TEXT,
    client_disconnected INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER,
    upstream_first_byte_ms INTEGER,
    time_to_first_token_ms INTEGER,
    generation_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    total_tokens INTEGER,
    cached_input_tokens INTEGER,
    reasoning_tokens INTEGER,
    accepted_prediction_tokens INTEGER,
    rejected_prediction_tokens INTEGER,
    token_source TEXT,
    provider_usage_json TEXT,
    payload_capture_status TEXT NOT NULL DEFAULT 'not_started',
    payload_capture_error TEXT,
    request_payload_path TEXT,
    response_payload_path TEXT,
    request_payload_bytes INTEGER,
    response_payload_bytes INTEGER,
    request_payload_hash TEXT,
    response_payload_hash TEXT,
    FOREIGN KEY(proxy_key_id) REFERENCES proxy_api_key(id)
);

CREATE INDEX IF NOT EXISTS idx_request_log_started_at ON request_log(started_at);
CREATE INDEX IF NOT EXISTS idx_request_log_proxy_key_id ON request_log(proxy_key_id);
CREATE INDEX IF NOT EXISTS idx_request_log_requested_model ON request_log(requested_model);
CREATE INDEX IF NOT EXISTS idx_request_log_route_name ON request_log(route_name);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initializes_sqlite_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite");

        let db = Database::connect(&path).await.expect("connect database");
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(db.pool())
            .await
            .expect("count migrations");

        assert_eq!(row.0, 1);
    }

    #[tokio::test]
    async fn stores_admin_password_and_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite");
        let db = Database::connect(&path).await.expect("connect database");

        assert!(!db.has_admin_account().await.expect("admin state"));

        db.set_admin_password_hash("hash").await.expect("set hash");
        assert!(db.has_admin_account().await.expect("admin state"));
        assert_eq!(
            db.admin_password_hash().await.expect("get hash"),
            Some("hash".to_owned())
        );

        db.create_admin_session("session-hash", Duration::days(7))
            .await
            .expect("create session");
        assert!(db
            .admin_session_exists("session-hash")
            .await
            .expect("session exists"));

        db.delete_admin_session("session-hash")
            .await
            .expect("delete session");
        assert!(!db
            .admin_session_exists("session-hash")
            .await
            .expect("session removed"));
    }

    #[tokio::test]
    async fn creates_and_finds_proxy_api_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite");
        let db = Database::connect(&path).await.expect("connect database");

        let created = db
            .create_proxy_api_key("local dev", "key-hash")
            .await
            .expect("create key");
        let found = db
            .proxy_api_key_by_hash("key-hash")
            .await
            .expect("find key")
            .expect("key should exist");
        let all = db.list_proxy_api_keys().await.expect("list keys");

        assert_eq!(created, found);
        assert_eq!(all, vec![created]);
    }

    #[tokio::test]
    async fn upserts_lists_and_deletes_upstream_secrets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test.sqlite");
        let db = Database::connect(&path).await.expect("connect database");

        db.upsert_upstream_secret("openai-prod", b"ciphertext", b"nonce")
            .await
            .expect("upsert secret");
        let secret = db
            .upstream_secret("openai-prod")
            .await
            .expect("get secret")
            .expect("secret exists");

        assert_eq!(secret.name, "openai-prod");
        assert_eq!(secret.encrypted_value, b"ciphertext");
        assert_eq!(secret.nonce, b"nonce");
        assert_eq!(db.list_upstream_secrets().await.expect("list").len(), 1);

        db.delete_upstream_secret("openai-prod")
            .await
            .expect("delete secret");
        assert!(db
            .upstream_secret("openai-prod")
            .await
            .expect("get deleted")
            .is_none());
    }

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

        let row: (i64, i64) =
            sqlx::query_as("SELECT http_status, duration_ms FROM request_log WHERE id = ?")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("fetch log");
        assert_eq!(row, (200, 123));
    }
}
