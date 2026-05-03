use crate::{Database, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamSecret {
    pub name: String,
    pub encrypted_value: Vec<u8>,
    pub nonce: Vec<u8>,
    pub created_at: String,
    pub updated_at: String,
}

impl Database {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
