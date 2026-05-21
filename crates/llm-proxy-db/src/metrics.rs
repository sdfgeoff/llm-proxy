use crate::{Database, DbError};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DashboardPeriod {
    Last24Hours,
    Last7Days,
    Last30Days,
}

impl DashboardPeriod {
    pub fn from_query(value: Option<&str>) -> Self {
        match value {
            Some("7d") => Self::Last7Days,
            Some("30d") => Self::Last30Days,
            _ => Self::Last24Hours,
        }
    }

    pub fn as_query(self) -> &'static str {
        match self {
            Self::Last24Hours => "24h",
            Self::Last7Days => "7d",
            Self::Last30Days => "30d",
        }
    }

    fn sqlite_range(self) -> &'static str {
        match self {
            Self::Last24Hours => "-24 hours",
            Self::Last7Days => "-7 days",
            Self::Last30Days => "-30 days",
        }
    }

    fn bucket_expr(self) -> &'static str {
        match self {
            Self::Last24Hours => {
                "strftime('%Y-%m-%d %H:', started_at) || printf('%02d', (CAST(strftime('%M', started_at) AS INTEGER) / 15) * 15)"
            }
            Self::Last7Days | Self::Last30Days => "strftime('%Y-%m-%d', started_at)",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DashboardMetrics {
    pub period: DashboardPeriod,
    pub overview: MetricsOverview,
    pub hourly: Vec<HourlyMetric>,
    pub by_model: Vec<DimensionMetric>,
    pub by_key: Vec<DimensionMetric>,
    pub by_status: Vec<StatusMetric>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MetricsOverview {
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub avg_duration_ms: Option<f64>,
    pub avg_tokens_per_second: Option<f64>,
    pub avg_time_to_first_token_ms: Option<f64>,
    pub error_count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HourlyMetric {
    pub bucket: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub avg_tokens_per_second: Option<f64>,
    pub avg_time_to_first_token_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DimensionMetric {
    pub label: String,
    pub request_count: i64,
    pub total_tokens: i64,
    pub avg_tokens_per_second: Option<f64>,
    pub avg_time_to_first_token_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatusMetric {
    pub label: String,
    pub request_count: i64,
}

impl Database {
    pub async fn dashboard_metrics(
        &self,
        period: DashboardPeriod,
    ) -> Result<DashboardMetrics, DbError> {
        let overview = self.metrics_overview(period).await?;
        let hourly = self.hourly_metrics(period).await?;
        let by_model = self.dimension_metrics(period, "requested_model").await?;
        let by_key = self
            .dimension_metrics(period, "proxy_api_key.label")
            .await?;
        let by_status = self.status_metrics(period).await?;

        Ok(DashboardMetrics {
            period,
            overview,
            hourly,
            by_model,
            by_key,
            by_status,
        })
    }

    async fn metrics_overview(&self, period: DashboardPeriod) -> Result<MetricsOverview, DbError> {
        let row = sqlx::query_as::<_, MetricsOverviewRow>(
            r#"
            SELECT
                COUNT(*) AS request_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                AVG(duration_ms) AS avg_duration_ms,
                AVG(CASE
                    WHEN duration_ms > 0 AND output_tokens IS NOT NULL
                    THEN output_tokens * 1000.0 / duration_ms
                END) AS avg_tokens_per_second,
                AVG(time_to_first_token_ms) AS avg_time_to_first_token_ms,
                COALESCE(SUM(CASE
                    WHEN http_status >= 400 OR error_category IS NOT NULL
                    THEN 1 ELSE 0
                END), 0) AS error_count
            FROM request_log
            WHERE started_at >= datetime('now', ?)
            "#,
        )
        .bind(period.sqlite_range())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn hourly_metrics(&self, period: DashboardPeriod) -> Result<Vec<HourlyMetric>, DbError> {
        let sql = format!(
            r#"
            SELECT
                {bucket} AS bucket,
                COUNT(*) AS request_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                AVG(CASE
                    WHEN duration_ms > 0 AND output_tokens IS NOT NULL
                    THEN output_tokens * 1000.0 / duration_ms
                END) AS avg_tokens_per_second,
                AVG(time_to_first_token_ms) AS avg_time_to_first_token_ms
            FROM request_log
            WHERE started_at >= datetime('now', ?)
            GROUP BY bucket
            ORDER BY bucket
            "#,
            bucket = period.bucket_expr()
        );
        let rows = sqlx::query_as::<_, HourlyMetricRow>(&sql)
            .bind(period.sqlite_range())
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn dimension_metrics(
        &self,
        period: DashboardPeriod,
        dimension: &str,
    ) -> Result<Vec<DimensionMetric>, DbError> {
        let sql = format!(
            r#"
            SELECT
                COALESCE({dimension}, 'unknown') AS label,
                COUNT(*) AS request_count,
                COALESCE(SUM(request_log.total_tokens), 0) AS total_tokens,
                AVG(CASE
                    WHEN request_log.duration_ms > 0 AND request_log.output_tokens IS NOT NULL
                    THEN request_log.output_tokens * 1000.0 / request_log.duration_ms
                END) AS avg_tokens_per_second,
                AVG(request_log.time_to_first_token_ms) AS avg_time_to_first_token_ms
            FROM request_log
            LEFT JOIN proxy_api_key ON proxy_api_key.id = request_log.proxy_key_id
            WHERE request_log.started_at >= datetime('now', ?)
            GROUP BY 1
            ORDER BY total_tokens DESC, request_count DESC
            LIMIT 10
            "#
        );
        let rows = sqlx::query_as::<_, DimensionMetricRow>(&sql)
            .bind(period.sqlite_range())
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn status_metrics(&self, period: DashboardPeriod) -> Result<Vec<StatusMetric>, DbError> {
        let rows = sqlx::query_as::<_, StatusMetricRow>(
            r#"
            SELECT
                CASE
                    WHEN http_status IS NULL THEN 'pending'
                    WHEN http_status >= 500 THEN '5xx'
                    WHEN http_status >= 400 THEN '4xx'
                    WHEN http_status >= 300 THEN '3xx'
                    WHEN http_status >= 200 THEN '2xx'
                    ELSE 'other'
                END AS label,
                COUNT(*) AS request_count
            FROM request_log
            WHERE started_at >= datetime('now', ?)
            GROUP BY label
            ORDER BY label
            "#,
        )
        .bind(period.sqlite_range())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[derive(sqlx::FromRow)]
struct MetricsOverviewRow {
    request_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    avg_duration_ms: Option<f64>,
    avg_tokens_per_second: Option<f64>,
    avg_time_to_first_token_ms: Option<f64>,
    error_count: i64,
}

impl From<MetricsOverviewRow> for MetricsOverview {
    fn from(row: MetricsOverviewRow) -> Self {
        Self {
            request_count: row.request_count,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            avg_duration_ms: row.avg_duration_ms,
            avg_tokens_per_second: row.avg_tokens_per_second,
            avg_time_to_first_token_ms: row.avg_time_to_first_token_ms,
            error_count: row.error_count,
        }
    }
}

#[derive(sqlx::FromRow)]
struct HourlyMetricRow {
    bucket: String,
    request_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    avg_tokens_per_second: Option<f64>,
    avg_time_to_first_token_ms: Option<f64>,
}

impl From<HourlyMetricRow> for HourlyMetric {
    fn from(row: HourlyMetricRow) -> Self {
        Self {
            bucket: row.bucket,
            request_count: row.request_count,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            total_tokens: row.total_tokens,
            avg_tokens_per_second: row.avg_tokens_per_second,
            avg_time_to_first_token_ms: row.avg_time_to_first_token_ms,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DimensionMetricRow {
    label: String,
    request_count: i64,
    total_tokens: i64,
    avg_tokens_per_second: Option<f64>,
    avg_time_to_first_token_ms: Option<f64>,
}

impl From<DimensionMetricRow> for DimensionMetric {
    fn from(row: DimensionMetricRow) -> Self {
        Self {
            label: row.label,
            request_count: row.request_count,
            total_tokens: row.total_tokens,
            avg_tokens_per_second: row.avg_tokens_per_second,
            avg_time_to_first_token_ms: row.avg_time_to_first_token_ms,
        }
    }
}

#[derive(sqlx::FromRow)]
struct StatusMetricRow {
    label: String,
    request_count: i64,
}

impl From<StatusMetricRow> for StatusMetric {
    fn from(row: StatusMetricRow) -> Self {
        Self {
            label: row.label,
            request_count: row.request_count,
        }
    }
}
