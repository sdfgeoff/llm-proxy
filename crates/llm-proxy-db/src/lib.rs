mod admin;
mod metrics;
#[cfg(test)]
mod metrics_tests;
mod migrations;
mod request_rows;
#[cfg(test)]
mod request_tests;
mod requests;
mod secrets;

use std::path::Path;

/// Convert a SQLite timestamp string (e.g. `"2024-01-15 14:30:00"`) to ISO 8601 (`"2024-01-15T14:30:00Z"`).
pub(crate) fn to_iso8601(s: &str) -> String {
    match s.len() {
        10 => format!("{s}T00:00:00Z"),
        16 => format!("{}:00Z", s.replace(' ', "T")),
        _ => format!("{}Z", s.replace(' ', "T")),
    }
}

pub use admin::ProxyApiKey;
pub use metrics::{
    DashboardMetrics, DashboardPeriod, DimensionMetric, HourlyMetric, MetricsOverview, StatusMetric,
};
pub use requests::{
    NewRequestLog, PayloadCaptureUpdate, RequestDetail, RequestLogUpdate, RequestSummary,
};
pub use secrets::{UpstreamSecret, UpstreamSecretInfo};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, SqlitePool};
use thiserror::Error;
use tracing::log::LevelFilter;

#[derive(Debug, Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
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

    async fn initialize(&self) -> Result<(), DbError> {
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&self.pool)
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&self.pool)
            .await?;
        for statement in migrations::MIGRATION_0001
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            sqlx::query(statement).execute(&self.pool).await?;
        }
        Ok(())
    }
}

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
}
