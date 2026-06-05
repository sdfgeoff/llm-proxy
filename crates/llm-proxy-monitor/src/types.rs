use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A complete snapshot of all system metrics at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp: OffsetDateTime,
    pub cpu: CpuInfo,
    pub ram: RamInfo,
    pub gpus: Vec<GpuInfo>,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
    pub load_average: LoadAverage,
    pub cpu_temps: Vec<CpuTemp>,
}

/// CPU utilization since last boot, broken into time categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub usage_percent: f64,
    pub per_core_usage: Vec<f64>,
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
    pub per_core: Vec<CpuCore>,
}

/// Per-core CPU counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCore {
    pub usage_percent: f64,
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

/// Memory information from /proc/meminfo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamInfo {
    pub total_mb: u64,
    pub free_mb: u64,
    pub available_mb: u64,
    pub used_mb: u64,
    pub buffers_mb: u64,
    pub cached_mb: u64,
    pub usage_percent: f64,
}

/// GPU information from nvidia-smi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub index: u32,
    pub utilization_percent: f64,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub vram_free_mb: u64,
    pub temperature_c: f64,
    pub power_watts: f64,
}

/// Disk/mount point information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount: String,
    pub device: String,
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_percent: f64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub reads: u64,
    pub writes: u64,
}

/// Network interface information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub interface: String,
    pub bytes_received: u64,
    pub bytes_transmitted: u64,
    pub packets_received: u64,
    pub packets_transmitted: u64,
    pub receive_errors: u64,
    pub transmit_errors: u64,
}

/// System load averages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
}

/// CPU temperature from thermal zones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTemp {
    pub name: String,
    pub temperature_c: f64,
}
