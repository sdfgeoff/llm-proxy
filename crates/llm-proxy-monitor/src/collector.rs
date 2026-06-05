use std::collections::HashMap;
use std::fs;
use std::process::Command;

use time::OffsetDateTime;

use crate::types::{CpuCore, CpuInfo, CpuTemp, DiskInfo, GpuInfo, LoadAverage, NetworkInfo, RamInfo, Snapshot};

/// Parse a single /proc/stat CPU line: "cpu0 1234 567 ..."
fn parse_cpu_line(line: &str) -> Option<CpuCore> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 9 {
        return None;
    }
    let nums: Vec<u64> = parts[1..].iter().filter_map(|s| s.parse().ok()).collect();
    if nums.len() < 8 {
        return None;
    }
    Some(CpuCore {
        usage_percent: 0.0,
        user: nums[0],
        nice: nums[1],
        system: nums[2],
        idle: nums[3],
        iowait: nums[4],
        irq: nums[5],
        softirq: nums[6],
        steal: nums[7],
    })
}

/// Parse /proc/stat for total CPU and per-core CPU times.
fn parse_proc_stat(raw: &str) -> Option<(CpuCore, Vec<CpuCore>)> {
    let mut total_core = None;
    let mut cores = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("cpu") {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let parsed = parse_cpu_line(trimmed)?;
        if parts[0] == "cpu" {
            total_core = Some(parsed);
        } else {
            cores.push(parsed);
        }
    }

    total_core.map(|t| (t, cores))
}

/// Parse /proc/meminfo into RamInfo.
fn parse_proc_meminfo(raw: &str) -> RamInfo {
    let mut vals: HashMap<&str, u64> = HashMap::new();

    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let key = parts[0].trim_end_matches(':');
        if let Ok(val_kb) = parts[1].parse::<u64>() {
            vals.insert(key, val_kb / 1024); // Convert KB to MB
        }
    }

    let total = vals.get("MemTotal").copied().unwrap_or(0);
    let free = vals.get("MemFree").copied().unwrap_or(0);
    let available = vals.get("MemAvailable").copied().unwrap_or(free);
    let buffers = vals.get("Buffers").copied().unwrap_or(0);
    let cached = vals.get("Cached").copied().unwrap_or(0);
    let used = total.saturating_sub(free).saturating_sub(buffers).saturating_sub(cached);
    let usage_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    RamInfo {
        total_mb: total,
        free_mb: free,
        available_mb: available,
        used_mb: used,
        buffers_mb: buffers,
        cached_mb: cached,
        usage_percent,
    }
}

/// Parse /proc/loadavg into LoadAverage.
fn parse_proc_loadavg(raw: &str) -> LoadAverage {
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.len() < 3 {
        return LoadAverage { load1: 0.0, load5: 0.0, load15: 0.0 };
    }
    LoadAverage {
        load1: parts[0].parse().unwrap_or(0.0),
        load5: parts[1].parse().unwrap_or(0.0),
        load15: parts[2].parse().unwrap_or(0.0),
    }
}

/// Parse /proc/net/dev into per-interface stats.
fn parse_proc_net_dev(raw: &str) -> Vec<NetworkInfo> {
    let mut result = Vec::new();

    for line in raw.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let iface = parts[0].trim();
        let nums: Vec<u64> = parts[1].split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if nums.len() < 16 {
            continue;
        }

        result.push(NetworkInfo {
            interface: iface.to_string(),
            bytes_received: nums[0],
            bytes_transmitted: nums[8],
            packets_received: nums[1],
            packets_transmitted: nums[9],
            receive_errors: nums[2],
            transmit_errors: nums[10],
        });
    }

    result
}

/// Parse /proc/diskstats into per-device stats.
fn parse_proc_diskstats(raw: &str) -> Vec<DiskInfo> {
    let mut result = Vec::new();

    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 14 {
            continue;
        }
        let reads_completed = parts[3].parse::<u64>().unwrap_or(0);
        let reads_sectors = parts[5].parse::<u64>().unwrap_or(0);
        let writes_completed = parts[7].parse::<u64>().unwrap_or(0);
        let writes_sectors = parts[9].parse::<u64>().unwrap_or(0);

        result.push(DiskInfo {
            device: parts[2].to_string(),
            mount: String::new(),
            total_mb: 0,
            used_mb: 0,
            available_mb: 0,
            usage_percent: 0.0,
            bytes_read: reads_sectors * 512,
            bytes_written: writes_sectors * 512,
            reads: reads_completed,
            writes: writes_completed,
        });
    }

    result
}

/// Get disk usage per mount point using `df`.
fn get_disk_usage() -> Vec<DiskInfo> {
    let output = Command::new("df")
        .args(["-B1", "--output=target,size,used,avail,pcent"])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();

    for (i, line) in stdout.lines().enumerate() {
        if i == 0 {
            continue; // skip header
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let mount = parts[0];
        let size = parts[1].parse::<u64>().unwrap_or(0) / (1024 * 1024);
        let used = parts[2].parse::<u64>().unwrap_or(0) / (1024 * 1024);
        let avail = parts[3].parse::<u64>().unwrap_or(0) / (1024 * 1024);
        let pct_str = parts[4].trim_end_matches('%');
        let usage_percent = pct_str.parse::<f64>().unwrap_or(0.0);

        result.push(DiskInfo {
            device: String::new(),
            mount: mount.to_string(),
            total_mb: size,
            used_mb: used,
            available_mb: avail,
            usage_percent,
            bytes_read: 0,
            bytes_written: 0,
            reads: 0,
            writes: 0,
        });
    }

    result
}

/// Parse nvidia-smi CSV output (with header row).
fn parse_nvidia_smi(raw: &str) -> Option<(Vec<String>, Vec<GpuInfo>)> {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.len() < 2 {
        return None;
    }

    let mut name_idx = None;
    let mut util_idx = None;
    let mut vram_total_idx = None;
    let mut vram_used_idx = None;
    let mut temp_idx = None;
    let mut power_idx = None;

    let header = lines[0];
    let headers: Vec<String> = header.split(',').map(|s| s.trim().to_lowercase()).collect();

    for (i, h) in headers.iter().enumerate() {
        if h.contains("name") && !h.contains("vram") {
            name_idx = Some(i);
        } else if h.contains("utilization") && h.contains("gpu") {
            util_idx = Some(i);
        } else if h.contains("used") && h.contains("memory") {
            vram_used_idx = Some(i);
        } else if h.contains("total") && h.contains("memory") {
            vram_total_idx = Some(i);
        } else if h.contains("temperature") {
            temp_idx = Some(i);
        } else if h.contains("power.draw") || (h.contains("power") && !h.contains("limit")) {
            power_idx = Some(i);
        }
    }

    let mut names = Vec::new();
    let mut gpus = Vec::new();

    for (idx, line) in lines[1..].iter().enumerate() {
        let fields: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if fields.is_empty() {
            continue;
        }

        let name = name_idx
            .and_then(|i| fields.get(i))
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("GPU {idx}"));

        names.push(name.clone());

        let get_float = |i: Option<usize>| -> f64 {
            i.and_then(|idx| fields.get(idx))
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0)
        };

        let get_u64 = |i: Option<usize>| -> u64 {
            i.and_then(|idx| fields.get(idx))
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0)
        };

        let utilization = get_float(util_idx).round();
        let vram_total = get_u64(vram_total_idx);
        let vram_used = get_u64(vram_used_idx);
        let temperature = get_float(temp_idx);
        let power = get_float(power_idx);

        gpus.push(GpuInfo {
            name,
            index: idx as u32,
            utilization_percent: utilization,
            vram_total_mb: vram_total,
            vram_used_mb: vram_used,
            vram_free_mb: vram_total.saturating_sub(vram_used),
            temperature_c: temperature,
            power_watts: power,
        });
    }

    if gpus.is_empty() {
        return None;
    }

    Some((names, gpus))
}

/// Run nvidia-smi and parse the output.
fn get_gpu_info() -> Option<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,utilization.gpu,memory.total,memory.used,memory.free,temperature.gpu,power.draw",
            "--format=csv,nounits",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return None,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (_, gpus) = parse_nvidia_smi(&stdout)?;
    Some(gpus)
}

/// Parse CPU temperatures from /sys/class/thermal/.
fn get_cpu_temps() -> Vec<CpuTemp> {
    let mut temps = Vec::new();
    let base = "/sys/class/thermal";

    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let type_file = path.join("type");
            let temp_file = path.join("temp");

            if let Ok(type_name) = fs::read_to_string(&type_file) {
                if let Ok(temp_str) = fs::read_to_string(&temp_file) {
                    if let Ok(temp_milli) = temp_str.trim().parse::<i64>() {
                        temps.push(CpuTemp {
                            name: type_name.trim().to_string(),
                            temperature_c: temp_milli as f64 / 1000.0,
                        });
                    }
                }
            }
        }
    }

    temps
}

/// Compute CPU usage percentages from two snapshots.
fn compute_cpu_usage(prev: &Option<CpuCore>, curr: &CpuCore) -> f64 {
    if let Some(prev) = prev {
        let prev_total = prev.user + prev.nice + prev.system + prev.idle + prev.iowait + prev.irq + prev.softirq + prev.steal;
        let curr_total = curr.user + curr.nice + curr.system + curr.idle + curr.iowait + curr.irq + curr.softirq + curr.steal;
        let total_diff = (curr_total as i64 - prev_total as i64) as f64;
        if total_diff > 0.0 {
            let busy_diff = (curr.user as i64 + curr.nice as i64 + curr.system as i64
                - prev.user as i64 - prev.nice as i64 - prev.system as i64) as f64;
            let busy_diff = busy_diff + (curr.irq as i64 + curr.softirq as i64 - prev.irq as i64 - prev.softirq as i64) as f64;
            (busy_diff / total_diff) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    }
}

/// Compute per-core usage percentages from two snapshots.
fn compute_per_core_usage(prev: &[CpuCore], curr: &[CpuCore]) -> Vec<f64> {
    prev.iter().zip(curr.iter())
        .map(|(p, c)| compute_cpu_usage(&Some(p.clone()), c))
        .collect()
}

/// A previous snapshot used for computing deltas.
#[derive(Clone, Debug)]
pub struct PreviousSnapshot {
    pub cpu_total: CpuCore,
    pub cpu_cores: Vec<CpuCore>,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
}

/// Collect a single snapshot of all system metrics.
///
/// Returns (snapshot, previous_snapshot_for_deltas).
/// The previous snapshot is returned so the caller can pass it to the next call.
pub fn collect(prev: Option<&PreviousSnapshot>) -> (Snapshot, PreviousSnapshot) {
    let now = OffsetDateTime::now_utc();

    // Parse /proc/stat
    let stat_raw = fs::read_to_string("/proc/stat").unwrap_or_default();
    let (cpu_raw, cores_raw) = parse_proc_stat(&stat_raw).unwrap_or_else(|| {
        (CpuCore {
            usage_percent: 0.0, user: 0, nice: 0, system: 0, idle: 0,
            iowait: 0, irq: 0, softirq: 0, steal: 0,
        }, vec![])
    });

    // Parse /proc/meminfo
    let mem_raw = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let ram = parse_proc_meminfo(&mem_raw);

    // Parse /proc/loadavg
    let load_raw = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load_average = parse_proc_loadavg(&load_raw);

    // Parse /proc/net/dev (raw cumulative counters)
    let net_raw = fs::read_to_string("/proc/net/dev").unwrap_or_default();
    let networks_raw = parse_proc_net_dev(&net_raw);

    // Parse /proc/diskstats (raw cumulative counters)
    let disk_raw = fs::read_to_string("/proc/diskstats").unwrap_or_default();
    let diskstats_raw = parse_proc_diskstats(&disk_raw);

    // Get disk usage from df
    let disk_usage = get_disk_usage();

    // Merge disk usage into diskstats by mount
    let mut disks_for_snapshot = diskstats_raw.clone();
    for usage in &disk_usage {
        for raw in &mut disks_for_snapshot {
            if raw.device != "loop" && raw.device != "sr0"
                && raw.mount.is_empty() {
                    raw.mount = usage.mount.clone();
                    raw.total_mb = usage.total_mb;
                    raw.used_mb = usage.used_mb;
                    raw.available_mb = usage.available_mb;
                    raw.usage_percent = usage.usage_percent;
                }
        }
    }
    for usage in &disk_usage {
        let already_exists = disks_for_snapshot.iter().any(|d| d.mount == usage.mount);
        if !already_exists {
            disks_for_snapshot.push(DiskInfo {
                device: String::new(),
                mount: usage.mount.clone(),
                total_mb: usage.total_mb,
                used_mb: usage.used_mb,
                available_mb: usage.available_mb,
                usage_percent: usage.usage_percent,
                bytes_read: 0,
                bytes_written: 0,
                reads: 0,
                writes: 0,
            });
        }
    }

    // Get GPU info
    let gpus = get_gpu_info().unwrap_or_default();

    // Get CPU temps
    let cpu_temps = get_cpu_temps();

    // Compute CPU deltas
    let (usage_percent, per_core_usage) = if let Some(prev) = prev {
        (
            compute_cpu_usage(&Some(prev.cpu_total.clone()), &cpu_raw),
            compute_per_core_usage(&prev.cpu_cores, &cores_raw),
        )
    } else {
        (0.0, vec![0.0; cores_raw.len()])
    };

    // Compute network deltas from raw cumulative counters
    let networks_delta: Vec<NetworkInfo> = if let Some(prev) = prev {
        networks_raw
            .iter()
            .zip(prev.networks.iter())
            .map(|(curr, prev_net)| NetworkInfo {
                interface: curr.interface.clone(),
                bytes_received: curr.bytes_received.saturating_sub(prev_net.bytes_received),
                bytes_transmitted: curr.bytes_transmitted.saturating_sub(prev_net.bytes_transmitted),
                packets_received: curr.packets_received.saturating_sub(prev_net.packets_received),
                packets_transmitted: curr.packets_transmitted.saturating_sub(prev_net.packets_transmitted),
                receive_errors: curr.receive_errors.saturating_sub(prev_net.receive_errors),
                transmit_errors: curr.transmit_errors.saturating_sub(prev_net.transmit_errors),
            })
            .collect()
    } else {
        networks_raw.clone()
    };

    // Compute disk deltas from raw cumulative counters
    let disks_delta: Vec<DiskInfo> = if let Some(prev) = prev {
        disks_for_snapshot
            .iter()
            .zip(prev.disks.iter())
            .map(|(curr, prev_disk)| DiskInfo {
                device: curr.device.clone(),
                mount: curr.mount.clone(),
                total_mb: curr.total_mb,
                used_mb: curr.used_mb,
                available_mb: curr.available_mb,
                usage_percent: curr.usage_percent,
                bytes_read: curr.bytes_read.saturating_sub(prev_disk.bytes_read),
                bytes_written: curr.bytes_written.saturating_sub(prev_disk.bytes_written),
                reads: curr.reads.saturating_sub(prev_disk.reads),
                writes: curr.writes.saturating_sub(prev_disk.writes),
            })
            .collect()
    } else {
        disks_for_snapshot.clone()
    };

    let per_core: Vec<CpuCore> = cores_raw
        .iter()
        .zip(per_core_usage.iter())
        .map(|(core, usage)| CpuCore {
            usage_percent: *usage,
            user: core.user,
            nice: core.nice,
            system: core.system,
            idle: core.idle,
            iowait: core.iowait,
            irq: core.irq,
            softirq: core.softirq,
            steal: core.steal,
        })
        .collect();

    let cpu = CpuInfo {
        usage_percent,
        per_core_usage,
        user: cpu_raw.user,
        nice: cpu_raw.nice,
        system: cpu_raw.system,
        idle: cpu_raw.idle,
        iowait: cpu_raw.iowait,
        irq: cpu_raw.irq,
        softirq: cpu_raw.softirq,
        steal: cpu_raw.steal,
        per_core,
    };

    let prev = PreviousSnapshot {
        cpu_total: cpu_raw,
        cpu_cores: cores_raw,
        disks: disks_for_snapshot,
        networks: networks_raw,
    };

    let snapshot = Snapshot {
        timestamp: now,
        cpu,
        ram,
        gpus,
        disks: disks_delta,
        networks: networks_delta,
        load_average,
        cpu_temps,
    };

    (snapshot, prev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_proc_stat() {
        let raw = "cpu  100 20 30 500 10 5 3 2\ncpu0  80 15 25 400 8 4 2 1\ncpu1  20 5 5 100 2 1 1 1";
        let (total, cores) = parse_proc_stat(raw).unwrap();
        assert_eq!(total.user, 100);
        assert_eq!(total.nice, 20);
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[0].user, 80);
        assert_eq!(cores[1].user, 20);
    }

    #[test]
    fn test_parse_proc_meminfo() {
        let raw = "MemTotal:       16384000 kB\nMemFree:         4096000 kB\nMemAvailable:    8192000 kB\nBuffers:          512000 kB\nCached:          4096000 kB\nSwapCached:            0 kB\nActive:          8192000 kB\nInactive:        4096000 kB";
        let ram = parse_proc_meminfo(raw);
        assert_eq!(ram.total_mb, 16000);
        assert_eq!(ram.free_mb, 4000);
        assert_eq!(ram.available_mb, 8000);
        assert_eq!(ram.buffers_mb, 500);
        assert_eq!(ram.cached_mb, 4000);
    }

    #[test]
    fn test_parse_proc_loadavg() {
        let raw = "0.52 0.48 0.45 2/350 12345";
        let load = parse_proc_loadavg(raw);
        assert_eq!(load.load1, 0.52);
        assert_eq!(load.load5, 0.48);
        assert_eq!(load.load15, 0.45);
    }

    #[test]
    fn test_parse_proc_net_dev() {
        let raw = r#"Inter-|   Receive                                                |  Transmit
face |bytes packets errs drop fifo frame compressed compressed|bytes packets errs drop fifo colls carrier compressed
eth0:    1234567 8901 0 0 0 0 0 0 7654321 2345 0 0 0 0 0 0
lo:      100000 1000 0 0 0 0 0 0   100000 1000 0 0 0 0 0 0"#;
        let nets = parse_proc_net_dev(raw);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].interface, "eth0");
        assert_eq!(nets[1].interface, "lo");
    }

    #[test]
    fn test_compute_cpu_usage() {
        let prev = CpuCore {
            usage_percent: 0.0, user: 100, nice: 10, system: 50, idle: 800, iowait: 5, irq: 2, softirq: 3, steal: 1,
        };
        let curr = CpuCore {
            usage_percent: 0.0, user: 200, nice: 20, system: 100, idle: 850, iowait: 10, irq: 4, softirq: 5, steal: 2,
        };
        let usage = compute_cpu_usage(&Some(prev), &curr);
        assert!(usage > 74.0 && usage < 75.0);
    }

    #[test]
    fn test_parse_nvidia_smi() {
        let raw = r#"index, name, utilization.gpu [%], memory.total [MiB], memory.used [MiB], memory.free [MiB], temperature.gpu, power.draw [W]
0, Tesla V100-SXM2-16GB, 45, 16384, 8192, 8192, 65, 150.5
1, Tesla V100-SXM2-16GB, 80, 16384, 12000, 4384, 72, 250.0
"#;
        let (names, gpus) = parse_nvidia_smi(raw).unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].utilization_percent, 45.0);
        assert_eq!(gpus[0].vram_total_mb, 16384);
        assert_eq!(gpus[0].vram_used_mb, 8192);
        assert_eq!(gpus[0].temperature_c, 65.0);
        assert_eq!(gpus[0].power_watts, 150.5);
    }

    #[test]
    fn test_parse_nvidia_smi_empty() {
        let result = parse_nvidia_smi("no data here");
        assert!(result.is_none());
    }
}
