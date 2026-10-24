//! Dev server port auto-discovery and socket probing utilities.

use std::collections::HashSet;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const OUTPUT_PORT_MARKERS: &[&str] = &[
    "localhost:",
    "127.0.0.1:",
    "0.0.0.0:",
    "[::1]:",
    "[::]:",
    "port ",
    "port:",
];
const CLI_PORT_MARKERS: &[&str] = &["--port ", "--port=", "-p ", "-p=", "PORT="];

/// Strips ANSI escape sequences (e.g. `\x1b[1m5173\x1b[22m` emitted by Vite with `FORCE_COLOR=1`)
/// so port number markers like `localhost:5173` are cleanly recognized.
fn strip_ansi_codes(input: &str) -> String {
    if !input.contains('\x1b') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn scan_ports_after_markers(haystack: &str, markers: &[&str]) -> Vec<u16> {
    let mut ports = Vec::new();
    for marker in markers {
        let mut search_slice = haystack;
        while let Some(idx) = search_slice.find(marker) {
            let remainder = &search_slice[idx + marker.len()..];
            let digits_len = remainder.bytes().take_while(|b| b.is_ascii_digit()).count();
            if digits_len > 0 {
                if let Ok(port) = remainder[..digits_len].parse::<u16>() {
                    if (1024..=65535).contains(&port) {
                        ports.push(port);
                    }
                }
            }
            if remainder.is_empty() {
                break;
            }
            search_slice = &remainder[digits_len.max(1)..];
        }
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// Scans a line of process stdout/stderr output for exposed localhost port numbers and URLs.
pub fn scan_ports_from_output(line: &str) -> Vec<u16> {
    let clean = strip_ansi_codes(line);
    let lower = clean.to_lowercase();
    scan_ports_after_markers(&lower, OUTPUT_PORT_MARKERS)
}

/// Probes which loopback host (`"localhost"` or `"127.0.0.1"`)
/// is actively accepting TCP connections on `port`.
pub fn resolve_listening_loopback_host(port: u16) -> Option<&'static str> {
    let timeout = Duration::from_millis(crate::constants::DEFAULT_PORT_CONNECT_TIMEOUT_MS);
    let v4 = SocketAddr::from(([127, 0, 0, 1], port));
    if TcpStream::connect_timeout(&v4, timeout).is_ok() {
        return Some("127.0.0.1");
    }
    let v6 = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port));
    if TcpStream::connect_timeout(&v6, timeout).is_ok() {
        return Some("localhost");
    }
    None
}

/// Probes whether an HTTP server on `port` is not merely bound in the kernel TCP backlog,
/// but actively accepting connections in user-space and responding to an HTTP request.
/// Also wakes any job-control stopped (`T` / `Tl`) listener process group with `SIGCONT`.
pub fn resolve_http_responsive_loopback_host(port: u16) -> Option<&'static str> {
    use std::io::{Read, Write};

    // If the owning process is stopped by SIGTTIN/SIGTTOU ('T'), wake its process group immediately
    if let Some(pid) = find_pid_by_port(port) {
        let _ = wake_if_stopped(pid);
    }

    let connect_timeout = Duration::from_millis(250);
    let io_timeout = Duration::from_millis(500);
    let candidates: [(SocketAddr, &'static str); 2] = [
        (SocketAddr::from(([127, 0, 0, 1], port)), "127.0.0.1"),
        (
            SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port)),
            "localhost",
        ),
    ];

    for (addr, host_label) in candidates {
        if let Ok(mut stream) = TcpStream::connect_timeout(&addr, connect_timeout) {
            let _ = stream.set_write_timeout(Some(io_timeout));
            let _ = stream.set_read_timeout(Some(io_timeout));
            let req = format!(
                "GET / HTTP/1.0\r\nHost: localhost:{}\r\nUser-Agent: minicode-probe/1.0\r\nConnection: close\r\n\r\n",
                port
            );
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut buf = [0u8; 16];
                if let Ok(n) = stream.read(&mut buf) {
                    if n > 0 {
                        return Some(host_label);
                    }
                }
            }
        }
    }
    None
}

/// Checks if a process (or any of its descendants) is in job-control stopped state (`T` / `t`)
/// on Linux and sends `SIGCONT` to resume it.
pub fn wake_if_stopped(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        if pid <= 1 {
            return false;
        }
        let mut woke_any = false;
        let mut all_pids = find_all_descendants(pid);
        all_pids.insert(pid);
        for target_pid in all_pids {
            if let Ok(content) = std::fs::read_to_string(format!("/proc/{}/stat", target_pid)) {
                if let Some(rparen) = content.rfind(')') {
                    let remainder = content[rparen + 1..].trim_start();
                    if let Some(state_char) = remainder.chars().next() {
                        if state_char == 'T' || state_char == 't' {
                            unsafe {
                                let _ = libc::kill(-(target_pid as i32), libc::SIGCONT);
                                let _ = libc::kill(target_pid as i32, libc::SIGCONT);
                            }
                            woke_any = true;
                        }
                    }
                }
            }
        }
        woke_any
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}

/// Checks whether a TCP port is currently open and accepting connections on localhost (IPv4 or IPv6).
pub fn is_port_listening(port: u16) -> bool {
    resolve_listening_loopback_host(port).is_some()
}

#[cfg(target_os = "linux")]
fn read_pid_socket_inodes(pid: u32) -> HashSet<u64> {
    let mut socket_inodes = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(format!("/proc/{}/fd", pid)) {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path()) {
                let target_str = target.to_string_lossy();
                if let Some(inode_str) = target_str
                    .strip_prefix("socket:[")
                    .and_then(|s| s.strip_suffix(']'))
                {
                    if let Ok(inode) = inode_str.parse::<u64>() {
                        socket_inodes.insert(inode);
                    }
                }
            }
        }
    }
    socket_inodes
}

#[cfg(target_os = "linux")]
fn iter_listening_tcp_sockets(mut f: impl FnMut(u16, u64)) {
    // Format: sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode ...
    // st == "0A" means TCP_LISTEN
    for net_file in &["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = std::fs::read_to_string(net_file) {
            for line in content.lines().skip(1) {
                let mut cols = line.split_whitespace();
                let Some(_sl) = cols.next() else { continue };
                let Some(local_addr) = cols.next() else {
                    continue;
                };
                let Some(_rem_addr) = cols.next() else {
                    continue;
                };
                let Some(state) = cols.next() else { continue };
                if state != "0A" {
                    continue;
                }
                let Some(inode_str) = cols.nth(5) else {
                    continue;
                };
                let Some(port_hex) = local_addr.split(':').nth(1) else {
                    continue;
                };
                if let (Ok(port), Ok(inode)) =
                    (u16::from_str_radix(port_hex, 16), inode_str.parse::<u64>())
                {
                    f(port, inode);
                }
            }
        }
    }
}

/// Discovers open listening ports associated with a process ID (and its descendant processes)
/// by inspecting active sockets in `/proc/<pid>/fd` against `/proc/net/tcp` and `/proc/net/tcp6`.
#[cfg(target_os = "linux")]
pub fn find_open_process_ports(pid: u32) -> Vec<u16> {
    let mut socket_inodes = read_pid_socket_inodes(pid);
    for desc in find_all_descendants(pid) {
        socket_inodes.extend(read_pid_socket_inodes(desc));
    }
    if socket_inodes.is_empty() {
        return Vec::new();
    }

    let mut open_ports = Vec::new();
    iter_listening_tcp_sockets(|port, inode| {
        if (1024..=65535).contains(&port) && socket_inodes.contains(&inode) {
            open_ports.push(port);
        }
    });

    open_ports.sort_unstable();
    open_ports.dedup();
    open_ports
}

#[cfg(not(target_os = "linux"))]
pub fn find_open_process_ports(_pid: u32) -> Vec<u16> {
    Vec::new()
}

/// Identifies the operating system process ID listening on a given TCP port.
#[cfg(target_os = "linux")]
pub fn find_pid_by_port(port: u16) -> Option<u32> {
    let mut target_inodes = HashSet::new();
    iter_listening_tcp_sockets(|listen_port, inode| {
        if listen_port == port {
            target_inodes.insert(inode);
        }
    });

    if target_inodes.is_empty() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                if read_pid_socket_inodes(pid)
                    .iter()
                    .any(|inode| target_inodes.contains(inode))
                {
                    return Some(pid);
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

/// Recursively discovers all descendant process IDs of a given root process on Linux by traversing procfs.
#[cfg(target_os = "linux")]
pub fn find_all_descendants(root_pid: u32) -> HashSet<u32> {
    let mut parent_map: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();

    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            if let Ok(pid) = name_str.parse::<u32>() {
                let stat_file = format!("/proc/{}/stat", pid);
                if let Ok(content) = std::fs::read_to_string(stat_file) {
                    if let Some(rparen) = content.rfind(')') {
                        let remainder = &content[rparen + 1..];
                        // fields[0] is state, fields[1] is ppid
                        if let Some(ppid_str) = remainder.split_whitespace().nth(1) {
                            if let Ok(ppid) = ppid_str.parse::<u32>() {
                                parent_map.entry(ppid).or_default().push(pid);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut descendants = HashSet::new();
    let mut queue = vec![root_pid];
    while let Some(current) = queue.pop() {
        if let Some(children) = parent_map.get(&current) {
            for &child in children {
                if child != root_pid && descendants.insert(child) {
                    queue.push(child);
                }
            }
        }
    }

    descendants
}

#[cfg(not(target_os = "linux"))]
pub fn find_all_descendants(_root_pid: u32) -> HashSet<u32> {
    HashSet::new()
}

/// Checks if an operating system process is actively running (excluding reaped or zombie processes).
pub fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if pid == 0 {
            return false;
        }
        unsafe {
            if libc::kill(pid as i32, 0) != 0 {
                return false;
            }
        }
        // On Linux, inspect /proc/<pid>/stat to ensure process is not a zombie ('Z') or dead ('X')
        #[cfg(target_os = "linux")]
        {
            if let Ok(content) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
                if let Some(rparen) = content.rfind(')') {
                    let remainder = content[rparen + 1..].trim_start();
                    if let Some(state_char) = remainder.chars().next() {
                        if state_char == 'Z' || state_char == 'X' {
                            return false;
                        }
                        if state_char == 'T' || state_char == 't' {
                            let _ = wake_if_stopped(pid);
                        }
                    }
                }
            }
        }
        true
    }
    #[cfg(not(unix))]
    {
        false
    }
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
    if let Some(&p) = scan_ports_after_markers(cmd, CLI_PORT_MARKERS).first() {
        return Some(p);
    }

    // Also detect positional port in python3 -m http.server <port> or SimpleHTTPServer
    if cmd.contains("http.server") || cmd.contains("SimpleHTTPServer") {
        for word in cmd.split_whitespace() {
            if let Ok(p) = word.parse::<u16>() {
                if (1024..=65535).contains(&p) {
                    return Some(p);
                }
            }
        }
        return Some(8000); // Default python http.server port
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

    if (result.contains("http.server") || result.contains("SimpleHTTPServer"))
        && result.contains(&format!(" {}", old_s))
    {
        result = result.replace(&format!(" {}", old_s), &format!(" {}", new_s));
    }

    result
}

/// Scans process output for ports that failed to bind or were reported as busy/in-use.
pub fn scan_busy_ports_from_output(line: &str) -> Vec<u16> {
    let clean = strip_ansi_codes(line);
    let lower = clean.to_lowercase();
    if lower.contains("in use") || lower.contains("eaddrinuse") || lower.contains("already in use")
    {
        scan_ports_after_markers(&lower, OUTPUT_PORT_MARKERS)
    } else {
        Vec::new()
    }
}

/// Forcefully reclaims a port by terminating the conflicting process and its direct descendants.
#[cfg(unix)]
pub async fn force_reclaim_port(port: u16, initial_pid: Option<u32>) -> bool {
    let mut target_pids = std::collections::HashSet::new();
    let my_pid = std::process::id();

    if let Some(pid) = initial_pid {
        if pid > 1 && pid != my_pid {
            target_pids.insert(pid);
            for desc in find_all_descendants(pid) {
                if desc > 1 && desc != my_pid {
                    target_pids.insert(desc);
                }
            }
        }
    }

    if let Some(p) = find_pid_by_port(port) {
        if p > 1 && p != my_pid {
            target_pids.insert(p);
            for desc in find_all_descendants(p) {
                if desc > 1 && desc != my_pid {
                    target_pids.insert(desc);
                }
            }
        }
    }

    if target_pids.is_empty() {
        return !is_port_listening(port);
    }

    // Step 1: Send SIGTERM to all target PIDs directly (NEVER to external PGIDs)
    for &p in &target_pids {
        if p > 1 && p != my_pid {
            unsafe {
                let _ = libc::kill(p as i32, libc::SIGTERM);
            }
        }
    }

    // Poll up to 400ms in 50ms intervals
    for _ in 0..8 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if !is_port_listening(port) {
            return true;
        }
    }

    // Step 2: Send SIGKILL to all target PIDs directly (NEVER to external PGIDs)
    for &p in &target_pids {
        if p > 1 && p != my_pid {
            unsafe {
                let _ = libc::kill(p as i32, libc::SIGKILL);
            }
        }
    }

    // Poll up to 300ms in 50ms intervals
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if !is_port_listening(port) {
            return true;
        }
    }

    // Final sweep: check if any late process took over
    if let Some(lp) = find_pid_by_port(port) {
        if lp > 1 && lp != my_pid {
            unsafe {
                let _ = libc::kill(lp as i32, libc::SIGKILL);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    !is_port_listening(port)
}

#[cfg(not(unix))]
pub async fn force_reclaim_port(_port: u16, _initial_pid: Option<u32>) -> bool {
    false
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

    let suggested_fallback =
        find_next_available_port(requested_port.saturating_add(1), DEFAULT_PORT_SCAN_RANGE);

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
                    requested_port,
                    conflicting_pid,
                    requested_port.saturating_add(1),
                    requested_port.saturating_add(DEFAULT_PORT_SCAN_RANGE)
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
            #[cfg(unix)]
            {
                if force_reclaim_port(requested_port, conflicting_pid).await {
                    return Ok(PortResolution::Reclaimed {
                        port: requested_port,
                        killed_pid: conflicting_pid,
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

    #[test]
    fn test_scan_ports_from_ansi_colored_vite_output() {
        let vite_line = "  \x1b[32m➜\x1b[39m  \x1b[1mLocal\x1b[22m:   \x1b[36mhttp://localhost:\x1b[1m5173\x1b[22m/\x1b[39m";
        assert_eq!(scan_ports_from_output(vite_line), vec![5173]);
    }
}
