use llm_proxy_db::Database;
use sqlx::FromRow;
use time::OffsetDateTime;

use crate::types::Snapshot;

/// Insert a snapshot into the database.
pub async fn insert_snapshot(db: &Database, snapshot: &Snapshot) -> Result<(), llm_proxy_db::DbError> {
    let ts = snapshot.timestamp.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| snapshot.timestamp.to_string());

    let gpu_json = if snapshot.gpus.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&snapshot.gpus).unwrap_or_default())
    };
    let disk_json = if snapshot.disks.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&snapshot.disks).unwrap_or_default())
    };
    let network_json = if snapshot.networks.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&snapshot.networks).unwrap_or_default())
    };
    let cpu_temps_json = if snapshot.cpu_temps.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&snapshot.cpu_temps).unwrap_or_default())
    };

    sqlx::query(
        r#"
        INSERT INTO system_metrics (
            timestamp, cpu_usage_percent, cpu_cores,
            ram_total_mb, ram_used_mb, ram_available_mb, ram_usage_percent,
            load_avg_1, load_avg_5, load_avg_15,
            gpu_json, disk_json, network_json, cpu_temps_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&ts)
    .bind(snapshot.cpu.usage_percent)
    .bind(snapshot.cpu.per_core.len() as i64)
    .bind(snapshot.ram.total_mb as i64)
    .bind(snapshot.ram.used_mb as i64)
    .bind(snapshot.ram.available_mb as i64)
    .bind(snapshot.ram.usage_percent)
    .bind(snapshot.load_average.load1)
    .bind(snapshot.load_average.load5)
    .bind(snapshot.load_average.load15)
    .bind(gpu_json)
    .bind(disk_json)
    .bind(network_json)
    .bind(cpu_temps_json)
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Delete snapshots older than the given cutoff time.
pub async fn cleanup_old_snapshots(
    db: &Database,
    cutoff: &str,
) -> Result<u64, llm_proxy_db::DbError> {
    let result = sqlx::query("DELETE FROM system_metrics WHERE timestamp < ?")
        .bind(cutoff)
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected())
}

/// Row from the system_metrics table.
#[derive(FromRow)]
struct MetricRow {
    timestamp: String,
    cpu_usage_percent: f64,
    cpu_cores: i64,
    ram_total_mb: i64,
    ram_used_mb: i64,
    ram_available_mb: i64,
    ram_usage_percent: f64,
    load_avg_1: f64,
    load_avg_5: f64,
    load_avg_15: f64,
    gpu_json: Option<String>,
    disk_json: Option<String>,
    network_json: Option<String>,
    cpu_temps_json: Option<String>,
}

impl From<MetricRow> for Snapshot {
    fn from(row: MetricRow) -> Self {
        use crate::types::*;

        let timestamp = OffsetDateTime::parse(&row.timestamp, &time::format_description::well_known::Rfc3339)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH);

        let gpus: Vec<GpuInfo> = row.gpu_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        let disks: Vec<DiskInfo> = row.disk_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        let networks: Vec<NetworkInfo> = row.network_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        let cpu_temps: Vec<CpuTemp> = row.cpu_temps_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();

        Snapshot {
            timestamp,
            cpu: CpuInfo {
                usage_percent: row.cpu_usage_percent,
                per_core_usage: vec![0.0; row.cpu_cores as usize],
                user: 0, nice: 0, system: 0, idle: 0,
                iowait: 0, irq: 0, softirq: 0, steal: 0,
                per_core: vec![],
            },
            ram: RamInfo {
                total_mb: row.ram_total_mb as u64,
                free_mb: 0,
                available_mb: row.ram_available_mb as u64,
                used_mb: row.ram_used_mb as u64,
                buffers_mb: 0,
                cached_mb: 0,
                usage_percent: row.ram_usage_percent,
            },
            gpus,
            disks,
            networks,
            load_average: LoadAverage {
                load1: row.load_avg_1,
                load5: row.load_avg_5,
                load15: row.load_avg_15,
            },
            cpu_temps,
        }
    }
}

/// Query snapshots in a time range.
pub async fn query_snapshots(
    db: &Database,
    start: &str,
    end: &str,
) -> Result<Vec<Snapshot>, llm_proxy_db::DbError> {
    let rows = sqlx::query_as::<_, MetricRow>(
        r#"
        SELECT timestamp, cpu_usage_percent, cpu_cores,
            ram_total_mb, ram_used_mb, ram_available_mb, ram_usage_percent,
            load_avg_1, load_avg_5, load_avg_15,
            gpu_json, disk_json, network_json, cpu_temps_json
        FROM system_metrics
        WHERE timestamp >= ? AND timestamp <= ?
        ORDER BY timestamp
        "#,
    )
    .bind(start)
    .bind(end)
    .fetch_all(db.pool())
    .await?;

    Ok(rows.into_iter().map(|row| row.into()).collect())
}
