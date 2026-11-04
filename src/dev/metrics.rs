//! Pure-Rust process resource monitoring (CPU % and physical RAM in MB).

use crate::dev::models::{DevProcessResourceUsage, RuntimeResourceSummary};
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

/// Reads a single process's physical memory footprint in kB on Linux using O(1) kernel
/// `mm->rss_stat` atomic counters from `/proc/<pid>/status` (avoiding expensive `/proc/<pid>/smaps_rollup`
/// kernel VMA page-table walks that stall the runtime for 10-40ms per process).
///
/// When `share_divisor > 1` (multi-process tree), divides shared file-backed pages (`RssFile:`)
/// across tree members so shared binary/library pages are not multiplied across every child.
#[cfg(target_os = "linux")]
fn read_process_memory_kb(pid: u32, share_divisor: usize) -> f32 {
    let status_path = format!("/proc/{}/status", pid);
    if let Ok(content) = std::fs::read_to_string(&status_path) {
        let anon = parse_proc_kb_key(&content, "RssAnon:").unwrap_or(0.0);
        let shmem = parse_proc_kb_key(&content, "RssShmem:").unwrap_or(0.0);
        let file = parse_proc_kb_key(&content, "RssFile:").unwrap_or(0.0);
        if anon > 0.0 || file > 0.0 || shmem > 0.0 {
            let div = (share_divisor.max(1)) as f32;
            return anon + (shmem / div) + (file / div);
        }
        if let Some(vm_rss) = parse_proc_kb_key(&content, "VmRSS:") {
            return vm_rss;
        }
    }
    0.0
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
        open_ports: Vec::new(),
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
        let mut alive_pids = Vec::with_capacity(target_pids.len());
        let tree_len = target_pids.len().max(1);

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

            alive_pids.push(p);
            total_kb += read_process_memory_kb(p, tree_len);
        }

        usage.memory_rss_mb = (total_kb / 1024.0 * 10.0).round() / 10.0;
        usage.cpu_percent = ((total_cpu * 10.0).round() / 10.0).clamp(0.0, 100.0);
        if !is_self && !alive_pids.is_empty() {
            usage.open_ports = crate::dev::ports::find_open_ports_for_pids(&alive_pids);
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if !is_self {
            usage.open_ports = find_open_process_ports(pid);
        }
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
