mod collector;
pub mod db;
pub mod types;

use std::time::Duration;

use llm_proxy_db::Database;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::collector::{collect, PreviousSnapshot};
use crate::types::Snapshot;

/// Handle to send snapshots to WebSocket clients.
/// Clone this and pass it to the dashboard's WebSocket handler.
#[derive(Clone)]
pub struct MonitorHandle {
    tx: broadcast::Sender<Snapshot>,
}

impl MonitorHandle {
    fn new(tx: broadcast::Sender<Snapshot>) -> Self {
        Self { tx }
    }

    /// Create a receiver for snapshots. Each call gets an independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Snapshot> {
        self.tx.subscribe()
    }
}

/// Configuration for the monitor task.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// How often to collect metrics (default: 5 seconds).
    pub interval: Duration,
    /// How long to retain metrics in the database (default: 24 hours).
    pub retention: Duration,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            retention: Duration::from_secs(24 * 3600),
        }
    }
}

/// Spawn the background monitor task.
///
/// The task collects system metrics at the configured interval, stores them in the database,
/// and broadcasts them to subscribers via the returned [`MonitorHandle`].
pub fn spawn_monitor_task(db: Database, config: MonitorConfig) -> MonitorHandle {
    let (tx, _rx) = broadcast::channel::<Snapshot>(64);
    let handle = MonitorHandle::new(tx);

    tokio::spawn(monitor_loop(db, config, handle.clone()));

    handle
}

async fn monitor_loop(db: Database, config: MonitorConfig, handle: MonitorHandle) {
    let mut prev: Option<PreviousSnapshot> = None;
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(3600)); // cleanup once per hour

    info!("monitor task started (interval={:?})", config.interval);

    // Check nvidia-smi availability
    let has_gpu = std::process::Command::new("nvidia-smi")
        .arg("--query-gpu=index")
        .arg("--format=csv,noheader")
        .output()
        .is_ok();

    if !has_gpu {
        warn!("nvidia-smi not available — GPU metrics will not be collected");
    }

    // Check CPU thermal zones
    let has_thermal = std::fs::read_dir("/sys/class/thermal").is_ok();
    if !has_thermal {
        warn!("No /sys/class/thermal directory — CPU temperature will not be available");
    }

    let mut collect_interval = tokio::time::interval(config.interval);

    loop {
        tokio::select! {
            _ = collect_interval.tick() => {
                let (snapshot, prev_snapshot) = collect(prev.as_ref());
                prev = Some(prev_snapshot);

                // Store in database
                if let Err(e) = db::insert_snapshot(&db, &snapshot).await {
                    warn!(error = %e, "failed to insert snapshot");
                }

                // Broadcast to WebSocket clients (best-effort, drop if no listeners)
                let _ = handle.tx.send(snapshot);
            }
            _ = cleanup_interval.tick() => {
                // Cleanup old snapshots
                let std_retention = config.retention;
                let cutoff = time::OffsetDateTime::now_utc() - time::Duration::hours(std_retention.as_secs() as i64 / 3600);
                let cutoff_str = cutoff.format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_else(|_| cutoff.to_string());

                if let Ok(count) = db::cleanup_old_snapshots(&db, &cutoff_str).await {
                    if count > 0 {
                        info!(removed = count, "cleaned up old system metrics");
                    }
                }
            }
        }
    }
}
