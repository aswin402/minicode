//! Dev server port auto-discovery and socket probing utilities.

use std::collections::HashSet;
use std::net::{SocketAddr, TcpListener, TcpStream};
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

/// Checks whether a TCP port is currently open and accepting connections on localhost (IPv4 or IPv6).
pub fn is_port_listening(port: u16) -> bool {
    let timeout = Duration::from_millis(crate::constants::DEFAULT_PORT_CONNECT_TIMEOUT_MS);
    let v4 = SocketAddr::from(([127, 0, 0, 1], port));
    if TcpStream::connect_timeout(&v4, timeout).is_ok() {
        return true;
    }
    let v6 = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port));
    TcpStream::connect_timeout(&v6, timeout).is_ok()
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

/// Identifies the operating system process ID listening on a given TCP port.
#[cfg(target_os = "linux")]
pub fn find_pid_by_port(port: u16) -> Option<u32> {
    let target_hex = format!("{:04X}", port);
    let mut target_inodes = HashSet::new();

    for net_file in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = std::fs::read_to_string(net_file) {
            for line in content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // parts[1] is local_address (e.g. 0100007F:1435), parts[3] is state ("0A" == TCP_LISTEN), parts[9] is inode
                if parts.len() > 9 && parts[3] == "0A" {
                    if let Some(port_part) = parts[1].split(':').nth(1) {
                        if port_part.eq_ignore_ascii_case(&target_hex) {
                            if let Ok(inode) = parts[9].parse::<u64>() {
                                target_inodes.insert(inode);
                            }
                        }
                    }
                }
            }
        }
    }

    if target_inodes.is_empty() {
        return None;
    }

    // Scan /proc to find which PID owns this socket inode
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if let Ok(pid) = name_str.parse::<u32>() {
                let fd_dir = format!("/proc/{}/fd", pid);
                if let Ok(fd_entries) = std::fs::read_dir(fd_dir) {
                    for fd_entry in fd_entries.flatten() {
                        if let Ok(target) = std::fs::read_link(fd_entry.path()) {
                            let target_str = target.to_string_lossy();
                            if target_str.starts_with("socket:[") && target_str.ends_with(']') {
                                let inode_str = &target_str[8..target_str.len() - 1];
                                if let Ok(inode) = inode_str.parse::<u64>() {
                                    if target_inodes.contains(&inode) {
                                        return Some(pid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
pub fn find_pid_by_port(_port: u16) -> Option<u32> {
    None
}

/// Retrieves the command name and command line arguments of a process by PID.
#[cfg(target_os = "linux")]
pub fn get_process_info(pid: u32) -> (Option<String>, Option<String>) {
    let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string());

    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", pid))
        .ok()
        .map(|s| s.replace('\0', " ").trim().to_string());

    (comm, cmdline)
}

#[cfg(not(target_os = "linux"))]
pub fn get_process_info(_pid: u32) -> (Option<String>, Option<String>) {
    (None, None)
}

/// Finds the next available TCP port on localhost starting from `start_port`.
pub fn find_next_available_port(start_port: u16, max_tries: u16) -> Option<u16> {
    for offset in 0..max_tries {
        let Some(port) = start_port.checked_add(offset) else {
            break;
        };
        if port == 0 {
            continue;
        }
        if !is_port_listening(port) {
            // Confirm port is genuinely bindable on IPv4
            if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
                drop(listener);
                // Also verify IPv6 is not already occupied
                if let Err(e) = TcpListener::bind(("::1", port)) {
                    if e.kind() == std::io::ErrorKind::AddrInUse {
                        continue;
                    }
                }
                return Some(port);
            }
        }
    }
    None
}

/// Detects if a spawn request has an explicit or inferred target network port.
pub fn detect_requested_port(req: &crate::dev::models::SpawnDevRequest) -> Option<u16> {
    if let Some(hint) = req.port_hint {
        return Some(hint);
    }

    if let Some(port_val) = req.extra_env.get("PORT") {
        if let Ok(p) = port_val.parse::<u16>() {
            return Some(p);
        }
    }

    let cmd = &req.command;
    let markers = ["--port ", "--port=", "-p ", "-p=", "PORT="];
    for marker in &markers {
        let mut search_slice = cmd.as_str();
        while let Some(idx) = search_slice.find(marker) {
            let remainder = &search_slice[idx + marker.len()..];
            let num_str: String = remainder
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(p) = num_str.parse::<u16>() {
                if (1024..=65535).contains(&p) {
                    return Some(p);
                }
            }
            if remainder.is_empty() {
                break;
            }
            search_slice = remainder;
        }
    }

    None
}

/// Rewrites explicit port parameters in a command string from an old port to a new port.
pub fn rewrite_command_port(command: &str, old_port: u16, new_port: u16) -> String {
    let old_s = old_port.to_string();
    let new_s = new_port.to_string();

    let patterns = [
        format!("--port {}", old_s),
        format!("--port={}", old_s),
        format!("-p {}", old_s),
        format!("-p={}", old_s),
        format!("PORT={}", old_s),
        format!("port:{}", old_s),
    ];

    let replacements = [
        format!("--port {}", new_s),
        format!("--port={}", new_s),
        format!("-p {}", new_s),
        format!("-p={}", new_s),
        format!("PORT={}", new_s),
        format!("port:{}", new_s),
    ];

    let mut result = command.to_string();
    for (pat, rep) in patterns.iter().zip(replacements.iter()) {
        if result.contains(pat) {
            result = result.replace(pat, rep);
        }
    }
    result
}

/// Evaluates requested port availability against the selected conflict policy.
pub async fn arbitrate_port(
    requested_port: u16,
    policy: crate::dev::models::PortConflictPolicy,
) -> std::result::Result<crate::dev::models::PortResolution, crate::error::DevError> {
    use crate::constants::DEFAULT_PORT_SCAN_RANGE;
    use crate::dev::models::{PortConflict, PortConflictPolicy, PortResolution};

    if !is_port_listening(requested_port) {
        return Ok(PortResolution::Unchanged {
            port: requested_port,
        });
    }

    let conflicting_pid = find_pid_by_port(requested_port);
    let (process_name, command_line) = if let Some(pid) = conflicting_pid {
        get_process_info(pid)
    } else {
        (None, None)
    };

    let suggested_fallback = find_next_available_port(requested_port + 1, DEFAULT_PORT_SCAN_RANGE);

    let conflict = PortConflict {
        port: requested_port,
        conflicting_pid,
        process_name: process_name.clone(),
        command_line,
        suggested_fallback,
    };

    match policy {
        PortConflictPolicy::Fallback => {
            if let Some(fallback_port) = suggested_fallback {
                Ok(PortResolution::Shifted {
                    requested: requested_port,
                    resolved: fallback_port,
                    conflict,
                })
            } else {
                Err(crate::error::DevError::Port(format!(
                    "Port {} is occupied by PID {:?} and no available fallback port was found in range {}..{}",
                    requested_port, conflicting_pid, requested_port + 1, requested_port + DEFAULT_PORT_SCAN_RANGE
                )))
            }
        }
        PortConflictPolicy::Error => Err(crate::error::DevError::PortConflict {
            port: requested_port,
            conflicting_pid,
            process_name,
            suggested: suggested_fallback,
        }),
        PortConflictPolicy::Kill => {
            if let Some(pid) = conflicting_pid {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(pid as i32, libc::SIGTERM);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
                if is_port_listening(requested_port) {
                    #[cfg(unix)]
                    unsafe {
                        let _ = libc::kill(pid as i32, libc::SIGKILL);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if !is_port_listening(requested_port) {
                    return Ok(PortResolution::Reclaimed {
                        port: requested_port,
                        killed_pid: Some(pid),
                    });
                }
            }
            if let Some(fallback_port) = suggested_fallback {
                Ok(PortResolution::Shifted {
                    requested: requested_port,
                    resolved: fallback_port,
                    conflict,
                })
            } else {
                Err(crate::error::DevError::PortConflict {
                    port: requested_port,
                    conflicting_pid,
                    process_name,
                    suggested: suggested_fallback,
                })
            }
        }
        PortConflictPolicy::Ignore => Ok(PortResolution::Ignored {
            port: requested_port,
        }),
    }
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

    #[test]
    fn test_detect_requested_port() {
        use crate::dev::models::{DevProcessType, SpawnDevRequest};
        use std::collections::HashMap;

        // Port hint
        let req1 = SpawnDevRequest {
            command: "npm run dev".into(),
            name: None,
            process_type: DevProcessType::Frontend,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: Some(3000),
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };
        assert_eq!(detect_requested_port(&req1), Some(3000));

        // PORT env var
        let mut env = HashMap::new();
        env.insert("PORT".into(), "8080".into());
        let req2 = SpawnDevRequest {
            command: "python app.py".into(),
            name: None,
            process_type: DevProcessType::Backend,
            working_dir: None,
            extra_env: env,
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };
        assert_eq!(detect_requested_port(&req2), Some(8080));

        // --port CLI flag
        let req3 = SpawnDevRequest {
            command: "vite --port 5173".into(),
            name: None,
            process_type: DevProcessType::Frontend,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };
        assert_eq!(detect_requested_port(&req3), Some(5173));

        // -p= CLI flag
        let req4 = SpawnDevRequest {
            command: "server -p=4000".into(),
            name: None,
            process_type: DevProcessType::Backend,
            working_dir: None,
            extra_env: HashMap::new(),
            port_hint: None,
            max_memory_mb: None,
            port_policy: None,
            restart_policy: None,
        };
        assert_eq!(detect_requested_port(&req4), Some(4000));
    }

    #[test]
    fn test_rewrite_command_port() {
        let cmd1 = "vite --port 3000 --host";
        assert_eq!(
            rewrite_command_port(cmd1, 3000, 3001),
            "vite --port 3001 --host"
        );

        let cmd2 = "server --port=8080";
        assert_eq!(rewrite_command_port(cmd2, 8080, 8081), "server --port=8081");

        let cmd3 = "app -p 4000";
        assert_eq!(rewrite_command_port(cmd3, 4000, 4005), "app -p 4005");

        let cmd4 = "PORT=5000 node index.js";
        assert_eq!(
            rewrite_command_port(cmd4, 5000, 5001),
            "PORT=5001 node index.js"
        );
    }

    #[test]
    fn test_find_next_available_port() {
        // Find port starting above 30000
        let port = find_next_available_port(39900, 50);
        assert!(port.is_some());
        let p = port.unwrap();
        assert!(p >= 39900 && p < 39950);
    }

    #[tokio::test]
    async fn test_arbitrate_port_lifecycle() {
        use crate::dev::models::{PortConflictPolicy, PortResolution};

        // Pick high port that should be free
        let free_port = 49123;
        if !is_port_listening(free_port) {
            let res = arbitrate_port(free_port, PortConflictPolicy::Fallback)
                .await
                .unwrap();
            assert_eq!(res, PortResolution::Unchanged { port: free_port });
            assert_eq!(res.resolved_port(), free_port);
        }

        // Test with simulated bound port
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let bound_port = listener.local_addr().unwrap().port();

        // Fallback policy: should shift to next available port
        let res = arbitrate_port(bound_port, PortConflictPolicy::Fallback)
            .await
            .unwrap();
        match res {
            PortResolution::Shifted {
                requested,
                resolved,
                ..
            } => {
                assert_eq!(requested, bound_port);
                assert_ne!(resolved, bound_port);
            }
            _ => panic!("Expected Shifted resolution"),
        }

        // Error policy: should return error
        let err_res = arbitrate_port(bound_port, PortConflictPolicy::Error).await;
        assert!(err_res.is_err());

        // Ignore policy: should return ignored
        let ign_res = arbitrate_port(bound_port, PortConflictPolicy::Ignore)
            .await
            .unwrap();
        assert_eq!(ign_res, PortResolution::Ignored { port: bound_port });
    }
}
