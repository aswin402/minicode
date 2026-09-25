//! Dev server port auto-discovery and socket probing utilities.

use std::collections::HashSet;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Scans a line of process stdout/stderr output for exposed localhost port numbers and URLs.
pub fn scan_ports_from_output(line: &str) -> Vec<u16> {
    let mut ports = HashSet::new();
    let lower = line.to_lowercase();

    // Look for standard URL patterns (http://localhost:5173, http://127.0.0.1:8000, etc.)
    let markers = ["localhost:", "127.0.0.1:", "0.0.0.0:", "port ", "port:"];
    for marker in &markers {
        let mut search_slice = lower.as_str();
        while let Some(idx) = search_slice.find(marker) {
            let remainder = &search_slice[idx + marker.len()..];
            let num_str: String = remainder
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(port) = num_str.parse::<u16>() {
                if (1024..=65535).contains(&port) {
                    ports.insert(port);
                }
            }
            if remainder.is_empty() {
                break;
            }
            search_slice = remainder;
        }
    }

    let mut result: Vec<u16> = ports.into_iter().collect();
    result.sort_unstable();
    result
}

/// Checks whether a TCP port is currently open and accepting connections on localhost.
pub fn is_port_listening(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok()
}

/// Discovers open listening ports associated with a process ID by inspecting active sockets.
#[cfg(target_os = "linux")]
pub fn find_open_process_ports(pid: u32) -> Vec<u16> {
    // Read sockets from /proc/<pid>/fd and correlate with /proc/net/tcp
    let fd_dir = format!("/proc/{}/fd", pid);
    let mut socket_inodes = HashSet::new();

    if let Ok(entries) = std::fs::read_dir(fd_dir) {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let target_str = target.to_string_lossy();
                if target_str.starts_with("socket:[") && target_str.ends_with(']') {
                    let inode_str = &target_str[8..target_str.len() - 1];
                    if let Ok(inode) = inode_str.parse::<u64>() {
                        socket_inodes.insert(inode);
                    }
                }
            }
        }
    }

    if socket_inodes.is_empty() {
        return Vec::new();
    }

    let mut open_ports = HashSet::new();
    for net_file in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = std::fs::read_to_string(net_file) {
            for line in content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // Format: sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode ...
                // st == 0A means TCP_LISTEN
                if parts.len() > 9 && parts[3] == "0A" {
                    if let Ok(inode) = parts[9].parse::<u64>() {
                        if socket_inodes.contains(&inode) {
                            if let Some(port_hex) = parts[1].split(':').nth(1) {
                                if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                                    if (1024..=65535).contains(&port) {
                                        open_ports.insert(port);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<u16> = open_ports.into_iter().collect();
    result.sort_unstable();
    result
}

#[cfg(not(target_os = "linux"))]
pub fn find_open_process_ports(_pid: u32) -> Vec<u16> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_ports_from_output() {
        let line1 = "  ➜  Local:   http://localhost:5173/";
        assert_eq!(scan_ports_from_output(line1), vec![5173]);

        let line2 = "ready on http://127.0.0.1:3000 and network http://0.0.0.0:3000";
        assert_eq!(scan_ports_from_output(line2), vec![3000]);

        let line3 = "Server listening on port 8080 successfully";
        assert_eq!(scan_ports_from_output(line3), vec![8080]);

        let line4 = "Nothing here to see";
        assert!(scan_ports_from_output(line4).is_empty());
    }
}
