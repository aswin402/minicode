//! Pure-Rust process resource monitoring (CPU % and physical RAM in MB).

use crate::dev::models::{DevProcessResourceUsage, RuntimeResourceSummary};
use crate::dev::ports::find_open_process_ports;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "linux")]
static CPU_DELTA_CACHE: OnceLock<Mutex<HashMap<u32, (f32, f32)>>> = OnceLock::new();

#[cfg(target_os = "linux")]
fn parse_proc_kb_key(content: &str, key: &str) -> Option<f32> {
    content
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|tok| tok.parse::<f32>().ok())
        .filter(|&val| val > 0.0)
}

/// Reads the host machine's total physical RAM in megabytes from `/proc/meminfo`.
pub fn sample_host_memory_total_mb() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb = parse_proc_kb_key(&content, "MemTotal:")?;
        Some((kb / 1024.0 * 10.0).round() / 10.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Reads a single process's physical memory footprint in kB on Linux.
///
/// Prefers `Pss:` (Proportional Set Size) from `/proc/<pid>/smaps_rollup` so shared library
/// and binary pages (`RssFile` / `Shared_Clean`) are divided proportionally across sharing
/// processes instead of being multiplied across every forked child in a process tree.
/// Falls back to `Rss:` or `VmRSS:` in `/proc/<pid>/status` when `smaps_rollup` is unavailable.
#[cfg(target_os = "linux")]
fn read_process_memory_kb(pid: u32) -> f32 {
    let smaps_path = format!("/proc/{}/smaps_rollup", pid);
    if let Ok(content) = std::fs::read_to_string(&smaps_path) {
        if let Some(kb) =
            parse_proc_kb_key(&content, "Pss:").or_else(|| parse_proc_kb_key(&content, "Rss:"))
        {
            return kb;
        }
    }

    let status_path = format!("/proc/{}/status", pid);
    std::fs::read_to_string(&status_path)
        .ok()
        .and_then(|content| parse_proc_kb_key(&content, "VmRSS:"))
        .unwrap_or(0.0)
}

#[cfg(target_os = "linux")]
fn clock_ticks_per_sec() -> f32 {
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    (ticks > 0).then_some(ticks as f32).unwrap_or(100.0)
}

#[cfg(target_os = "linux")]
fn compute_process_cpu_percent(
    pid: u32,
    total_jiffies: f32,
    starttime: f32,
    sys_uptime_secs: Option<f32>,
    clk_tck: f32,
) -> f32 {
    let Some(sys_up) = sys_uptime_secs else {
        return (total_jiffies / clk_tck).clamp(0.0, 100.0);
    };
    let sys_ticks = sys_up * clk_tck;
    let cache = CPU_DELTA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = cache.lock() {
        let prev = map.insert(pid, (total_jiffies, sys_ticks));
        if let Some((prev_jiffies, prev_sys_ticks)) = prev {
            let delta_sys = sys_ticks - prev_sys_ticks;
            if delta_sys >= 1.0 && total_jiffies >= prev_jiffies {
                return (((total_jiffies - prev_jiffies) / delta_sys) * 100.0).clamp(0.0, 100.0);
            }
        }
    }
    let process_ticks = (sys_ticks - starttime).max(clk_tck);
    ((total_jiffies / process_ticks) * 100.0).clamp(0.0, 100.0)
}

/// Samples resource usage (CPU%, RAM MB, active ports) for an active process PID and all its descendants.
pub fn sample_process_metrics(pid: u32) -> DevProcessResourceUsage {
    if pid == 0 {
        return DevProcessResourceUsage::default();
    }
    let is_self = pid == std::process::id();
    let mut usage = DevProcessResourceUsage {
        pid: Some(pid),
        cpu_percent: 0.0,
        memory_rss_mb: 0.0,
        open_ports: if is_self {
            Vec::new()
        } else {
            find_open_process_ports(pid)
        },
    };

    #[cfg(target_os = "linux")]
    {
        let mut target_pids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if seen.insert(pid) {
            target_pids.push(pid);
        }
        if !is_self {
            for desc in crate::dev::ports::find_all_descendants(pid) {
                if seen.insert(desc) {
                    target_pids.push(desc);
                }
            }
        }

        let sys_uptime_secs = std::fs::read_to_string("/proc/uptime").ok().and_then(|u| {
            u.split_whitespace()
                .next()
                .and_then(|s| s.parse::<f32>().ok())
        });
        let clk_tck = clock_ticks_per_sec();

        let mut total_kb: f32 = 0.0;
        let mut total_cpu: f32 = 0.0;
        let mut ports_set = std::collections::HashSet::new();
        for &p in usage.open_ports.iter() {
            ports_set.insert(p);
        }

        for &p in &target_pids {
            // Read state & CPU time from /proc/<p>/stat first so dead/zombie processes are skipped
            let stat_path = format!("/proc/{}/stat", p);
            let Ok(content) = std::fs::read_to_string(&stat_path) else {
                continue;
            };
            if let Some(r_paren) = content.rfind(')') {
                let remainder = &content[r_paren + 1..];
                let fields: Vec<&str> = remainder.split_whitespace().collect();
                if fields
                    .first()
                    .is_some_and(|&state| matches!(state, "Z" | "X" | "x"))
                {
                    continue;
                }
                // fields[11] is utime, fields[12] is stime, fields[19] is starttime
                if fields.len() > 19 {
                    let utime: f32 = fields[11].parse().unwrap_or(0.0);
                    let stime: f32 = fields[12].parse().unwrap_or(0.0);
                    let starttime: f32 = fields[19].parse().unwrap_or(0.0);
                    total_cpu += compute_process_cpu_percent(
                        p,
                        utime + stime,
                        starttime,
                        sys_uptime_secs,
                        clk_tck,
                    );
                }
            }

            // Read Proportional Set Size (PSS) / RSS memory in kB
            total_kb += read_process_memory_kb(p);

            // Also check open ports for descendants
            if p != pid {
                for port in find_open_process_ports(p) {
                    ports_set.insert(port);
                }
            }
        }

        usage.memory_rss_mb = (total_kb / 1024.0 * 10.0).round() / 10.0;
        usage.cpu_percent = ((total_cpu * 10.0).round() / 10.0).clamp(0.0, 100.0);
        let mut sorted_ports: Vec<u16> = ports_set.into_iter().collect();
        sorted_ports.sort_unstable();
        usage.open_ports = sorted_ports;
    }

    usage
}

/// Computes cumulative resource usage across a slice of process PIDs.
pub fn sample_aggregate_metrics(pids: &[u32]) -> RuntimeResourceSummary {
    let mut total_cpu = 0.0;
    let mut total_rss = 0.0;
    let mut ports_set = std::collections::HashSet::new();
    let mut unique_pids = std::collections::HashSet::new();

    for &pid in pids {
        if pid == 0 || !unique_pids.insert(pid) {
            continue;
        }
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
        total_active_processes: unique_pids.len(),
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
        let summary = sample_aggregate_metrics(&[my_pid, my_pid]);
        assert_eq!(summary.total_active_processes, 1);
        #[cfg(target_os = "linux")]
        {
            assert!(summary.total_memory_rss_mb > 0.0);
        }
    }

    #[test]
    fn test_sample_host_memory_total_mb() {
        #[cfg(target_os = "linux")]
        {
            let total = sample_host_memory_total_mb().expect("should read /proc/meminfo on Linux");
            assert!(total > 100.0);
        }
    }
}
