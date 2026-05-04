use serde::Serialize;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{Database, DbError};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProxyApiKey {
    pub id: String,
    pub label: String,
    pub created_at: String,
}

impl Database {
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
}

fn proxy_api_key_from_row(row: (String, String, String)) -> ProxyApiKey {
    ProxyApiKey {
        id: row.0,
        label: row.1,
        created_at: row.2,
    }
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting should not fail")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
