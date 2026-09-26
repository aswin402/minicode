//! Pure-Rust process resource monitoring (CPU % and RSS memory in MB).

use crate::dev::models::{DevProcessResourceUsage, RuntimeResourceSummary};
use crate::dev::ports::find_open_process_ports;

/// Samples resource usage (CPU%, RSS Memory MB, active ports) for an active process PID.
pub fn sample_process_metrics(pid: u32) -> DevProcessResourceUsage {
    let mut usage = DevProcessResourceUsage {
        pid: Some(pid),
        cpu_percent: 0.0,
        memory_rss_mb: 0.0,
        open_ports: find_open_process_ports(pid),
    };

    #[cfg(target_os = "linux")]
    {
        // 1. Read Resident Set Size (RSS) memory from /proc/<pid>/status
        let status_path = format!("/proc/{}/status", pid);
        if let Ok(content) = std::fs::read_to_string(&status_path) {
            for line in content.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<f32>() {
                            usage.memory_rss_mb = (kb / 1024.0 * 10.0).round() / 10.0;
                        }
                    }
                    break;
                }
            }
        }

        // 2. Read CPU time from /proc/<pid>/stat and calculate real utilization
        let stat_path = format!("/proc/{}/stat", pid);
        if let Ok(content) = std::fs::read_to_string(&stat_path) {
            if let Some(r_paren) = content.rfind(')') {
                let remainder = &content[r_paren + 1..];
                let fields: Vec<&str> = remainder.split_whitespace().collect();
                // fields[11] is utime, fields[12] is stime, fields[19] is starttime
                if fields.len() > 19 {
                    let utime: f32 = fields[11].parse().unwrap_or(0.0);
                    let stime: f32 = fields[12].parse().unwrap_or(0.0);
                    let starttime: f32 = fields[19].parse().unwrap_or(0.0);
                    let total_jiffies = utime + stime;

                    let sys_uptime_secs =
                        std::fs::read_to_string("/proc/uptime").ok().and_then(|u| {
                            u.split_whitespace()
                                .next()
                                .and_then(|s| s.parse::<f32>().ok())
                        });

                    let est_cpu = if let Some(sys_up) = sys_uptime_secs {
                        let sys_ticks = sys_up * 100.0;
                        let process_ticks = (sys_ticks - starttime).max(1.0);
                        ((total_jiffies / process_ticks) * 100.0).clamp(0.0, 100.0)
                    } else {
                        (total_jiffies / 100.0).clamp(0.0, 100.0)
                    };

                    usage.cpu_percent = (est_cpu * 10.0).round() / 10.0;
                }
            }
        }
    }

    usage
}

/// Computes cumulative resource usage across a slice of process PIDs.
pub fn sample_aggregate_metrics(pids: &[u32]) -> RuntimeResourceSummary {
    let mut total_cpu = 0.0;
    let mut total_rss = 0.0;
    let mut ports_set = std::collections::HashSet::new();

    for &pid in pids {
        let u = sample_process_metrics(pid);
        total_cpu += u.cpu_percent;
        total_rss += u.memory_rss_mb;
        for p in u.open_ports {
            ports_set.insert(p);
        }
    }

    let mut active_ports: Vec<u16> = ports_set.into_iter().collect();
    active_ports.sort_unstable();

    RuntimeResourceSummary {
        total_active_processes: pids.len(),
        total_cpu_percent: (total_cpu * 10.0).round() / 10.0,
        total_memory_rss_mb: (total_rss * 10.0).round() / 10.0,
        active_ports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_current_process_metrics() {
        let my_pid = std::process::id();
        let usage = sample_process_metrics(my_pid);
        assert_eq!(usage.pid, Some(my_pid));
        #[cfg(target_os = "linux")]
        {
            assert!(usage.memory_rss_mb > 0.0);
        }
    }

    #[test]
    fn test_sample_aggregate_metrics() {
        let my_pid = std::process::id();
        let summary = sample_aggregate_metrics(&[my_pid]);
        assert_eq!(summary.total_active_processes, 1);
        #[cfg(target_os = "linux")]
        {
            assert!(summary.total_memory_rss_mb > 0.0);
        }
    }
}
