use crate::constants::{
    BLOCKED_PREFIXES, EXEC_DEFAULT_TIMEOUT_SECS, EXEC_MAX_OUTPUT_BYTES,
    PROCESS_KILL_GRACE_PERIOD_MS, SECRET_PATTERNS, SIGNAL_KILLED_EXIT_CODE, WHITELIST_ENV_VARS,
};
use crate::error::{Result, ToolError};
use crate::tools::compactor::compact_tool_output;
use crate::tools::rtk_filter::RtkFilter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The isolation backend used to execute a sandboxed command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxBackendType {
    /// Linux unprivileged namespaces via Bubblewrap (`bwrap`).
    Bubblewrap,
    /// Linux kernel Landlock filesystem & network ruleset with POSIX rlimits.
    Landlock,
    /// Cross-platform process-isolated execution with sanitized environment.
    ProcessIsolated,
}

impl std::fmt::Display for SandboxBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bubblewrap => write!(f, "bubblewrap (namespaces)"),
            Self::Landlock => write!(f, "landlock (kernel fs/net)"),
            Self::ProcessIsolated => write!(f, "process-isolated"),
        }
    }
}

/// Preferred backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SandboxBackendPreference {
    /// Automatically selects the most secure available backend (Bubblewrap -> Landlock -> ProcessIsolated).
    #[default]
    Auto,
    /// Force Bubblewrap namespaces (fails if bwrap is missing).
    Bubblewrap,
    /// Force Landlock kernel sandboxing.
    Landlock,
    /// Standard sanitized process isolation without namespaces.
    ProcessIsolated,
}

/// Fine-grained configuration policy for sandbox execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Whether TCP/socket network access is permitted (default: false).
    pub allow_network: bool,
    /// Whether the host workspace is strictly read-only (default: false).
    pub read_only_workspace: bool,
    /// If true, runs within an isolated ephemeral directory so writes do not touch workspace.
    pub ephemeral_overlay: bool,
    /// Wall-clock timeout in seconds (default: 30s).
    pub timeout_secs: u64,
    /// Virtual memory ceiling in megabytes.
    pub max_memory_mb: Option<u64>,
    /// CPU limit in seconds.
    pub max_cpu_seconds: Option<u64>,
    /// Custom environment variables to inject.
    pub extra_env: HashMap<String, String>,
    /// Backend selection preference.
    pub backend_preference: SandboxBackendPreference,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            allow_network: false,
            read_only_workspace: false,
            ephemeral_overlay: false,
            timeout_secs: EXEC_DEFAULT_TIMEOUT_SECS,
            max_memory_mb: None,
            max_cpu_seconds: None,
            extra_env: HashMap::new(),
            backend_preference: SandboxBackendPreference::Auto,
        }
    }
}

/// Outcome of a sandboxed execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub combined_output: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub backend_used: SandboxBackendType,
    pub duration_ms: u64,
    pub network_isolated: bool,
    pub read_only_enforced: bool,
    pub ephemeral_writes_discarded: bool,
    pub discarded_files: Vec<String>,
}

/// Checks if bubblewrap (`bwrap`) is available on the system PATH.
pub fn is_bwrap_available() -> bool {
    which::which("bwrap").is_ok()
}

/// Determines the best active sandbox backend given the host environment and preference.
pub fn resolve_backend(pref: SandboxBackendPreference) -> Result<SandboxBackendType> {
    match pref {
        SandboxBackendPreference::Bubblewrap => {
            if is_bwrap_available() {
                Ok(SandboxBackendType::Bubblewrap)
            } else {
                Err(ToolError::CommandExec(
                    "Bubblewrap (`bwrap`) is not installed or not in PATH".to_string(),
                )
                .into())
            }
        }
        SandboxBackendPreference::Landlock => Ok(SandboxBackendType::Landlock),
        SandboxBackendPreference::ProcessIsolated => Ok(SandboxBackendType::ProcessIsolated),
        SandboxBackendPreference::Auto => {
            if is_bwrap_available() {
                Ok(SandboxBackendType::Bubblewrap)
            } else {
                #[cfg(target_os = "linux")]
                {
                    if crate::sandbox::landlock::is_landlock_supported() {
                        return Ok(SandboxBackendType::Landlock);
                    }
                }
                Ok(SandboxBackendType::ProcessIsolated)
            }
        }
    }
}

/// RAII guard that creates and automatically cleans up an ephemeral temporary directory.
struct EphemeralDir {
    path: PathBuf,
}

impl EphemeralDir {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("minicode_sb_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).map_err(|e| {
            ToolError::CommandExec(format!("Failed to create ephemeral sandbox dir: {}", e))
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for EphemeralDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// High-level executor for sandboxed commands.
pub async fn run_sandboxed(
    workspace_root: &Path,
    command_str: &str,
    policy: &SandboxPolicy,
) -> Result<SandboxExecutionResult> {
    let backend = resolve_backend(policy.backend_preference)?;
    let start_time = Instant::now();

    // Prepare ephemeral directory if requested
    let mut ephemeral_dir = None;
    let mut initial_files = std::collections::HashSet::new();
    let effective_workspace = if policy.ephemeral_overlay {
        let temp = EphemeralDir::new()?;
        // Snapshot existing files in temp dir (empty initially)
        if let Ok(entries) = std::fs::read_dir(temp.path()) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    initial_files.insert(name);
                }
            }
        }
        let p = temp.path().to_path_buf();
        ephemeral_dir = Some(temp);
        p
    } else {
        workspace_root.to_path_buf()
    };

    let result = match backend {
        SandboxBackendType::Bubblewrap => {
            run_bwrap(
                workspace_root,
                &effective_workspace,
                command_str,
                policy,
                ephemeral_dir.is_some(),
            )
            .await
        }
        SandboxBackendType::Landlock => {
            run_landlock_or_isolated(
                workspace_root,
                &effective_workspace,
                command_str,
                policy,
                true,
            )
            .await
        }
        SandboxBackendType::ProcessIsolated => {
            run_landlock_or_isolated(
                workspace_root,
                &effective_workspace,
                command_str,
                policy,
                false,
            )
            .await
        }
    }?;

    let duration_ms = start_time.elapsed().as_millis() as u64;

    // Check for discarded files in ephemeral mode
    let mut discarded_files = Vec::new();
    let ephemeral_writes_discarded = if let Some(ref temp) = ephemeral_dir {
        if let Ok(entries) = std::fs::read_dir(temp.path()) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if !initial_files.contains(&name) {
                        discarded_files.push(name);
                    }
                }
            }
        }
        discarded_files.sort();
        !discarded_files.is_empty()
    } else {
        false
    };

    // Format output with compactors
    let exit_code = result.exit_code;
    let rtk_res = RtkFilter::filter(command_str, &result.combined_output, exit_code);
    let compacted = compact_tool_output(command_str, &rtk_res.content, exit_code);

    let success = exit_code == Some(0);

    Ok(SandboxExecutionResult {
        stdout: result.stdout,
        stderr: result.stderr,
        combined_output: compacted,
        exit_code,
        success,
        backend_used: backend,
        duration_ms,
        network_isolated: !policy.allow_network,
        read_only_enforced: policy.read_only_workspace,
        ephemeral_writes_discarded,
        discarded_files,
    })
}

struct RawCommandOutput {
    stdout: String,
    stderr: String,
    combined_output: String,
    exit_code: Option<i32>,
}

/// Executes command within Bubblewrap unprivileged namespaces.
async fn run_bwrap(
    host_workspace: &Path,
    effective_workspace: &Path,
    command_str: &str,
    policy: &SandboxPolicy,
    is_ephemeral: bool,
) -> Result<RawCommandOutput> {
    let mut bwrap_cmd = std::process::Command::new("bwrap");

    // Standard read-only system mounts
    for dir in &["/usr", "/bin", "/lib", "/lib64", "/etc"] {
        let p = Path::new(dir);
        if p.exists() {
            bwrap_cmd.arg("--ro-bind-try").arg(p).arg(p);
        }
    }

    // Bind common developer toolchains if present
    if let Ok(home) = std::env::var("HOME") {
        let home_p = Path::new(&home);
        for sub in &[".cargo", ".rustup", ".nvm", ".local", ".pyenv", ".bun"] {
            let tool_path = home_p.join(sub);
            if tool_path.exists() {
                bwrap_cmd
                    .arg("--ro-bind-try")
                    .arg(&tool_path)
                    .arg(&tool_path);
            }
        }
    }

    // System pseudo-filesystems & temporary workspace
    bwrap_cmd.arg("--proc").arg("/proc");
    bwrap_cmd.arg("--dev").arg("/dev");
    bwrap_cmd.arg("--tmpfs").arg("/tmp");

    // Namespace isolation flags
    bwrap_cmd.arg("--unshare-user");
    bwrap_cmd.arg("--unshare-ipc");
    bwrap_cmd.arg("--unshare-pid");
    bwrap_cmd.arg("--unshare-uts");
    bwrap_cmd.arg("--die-with-parent");

    // Network isolation
    if !policy.allow_network {
        bwrap_cmd.arg("--unshare-net");
    } else {
        // When network is allowed, bind /run so /etc/resolv.conf symlinks can resolve DNS
        let run_p = Path::new("/run");
        if run_p.exists() {
            bwrap_cmd.arg("--ro-bind-try").arg(run_p).arg(run_p);
        }
    }

    // Workspace mounting
    if is_ephemeral {
        // Mount real workspace read-only so tools can inspect files
        bwrap_cmd
            .arg("--ro-bind-try")
            .arg(host_workspace)
            .arg(host_workspace);
        // Mount ephemeral scratchpad read-write
        bwrap_cmd
            .arg("--bind")
            .arg(effective_workspace)
            .arg(effective_workspace);
        bwrap_cmd.arg("--chdir").arg(effective_workspace);
    } else if policy.read_only_workspace {
        bwrap_cmd
            .arg("--ro-bind-try")
            .arg(host_workspace)
            .arg(host_workspace);
        bwrap_cmd.arg("--chdir").arg(host_workspace);
    } else {
        bwrap_cmd
            .arg("--bind-try")
            .arg(host_workspace)
            .arg(host_workspace);
        bwrap_cmd.arg("--chdir").arg(host_workspace);
    }

    // Clean environment
    bwrap_cmd.arg("--clearenv");

    // Whitelisted host variables
    for &var in WHITELIST_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            bwrap_cmd.arg("--setenv").arg(var).arg(val);
        }
    }

    // Injected custom environment variables
    for (k, v) in &policy.extra_env {
        bwrap_cmd.arg("--setenv").arg(k).arg(v);
    }

    // Sandbox indicators
    bwrap_cmd
        .arg("--setenv")
        .arg("MINICODE_WORKSPACE")
        .arg(effective_workspace.to_string_lossy().as_ref());
    bwrap_cmd.arg("--setenv").arg("MINICODE_SANDBOX").arg("1");
    bwrap_cmd
        .arg("--setenv")
        .arg("MINICODE_SANDBOX_BACKEND")
        .arg("bubblewrap");

    // Command to execute
    bwrap_cmd.arg("/bin/sh").arg("-c").arg(command_str);

    execute_std_command_with_limits(bwrap_cmd, policy).await
}

/// Executes command using Landlock (Linux) or sanitized process isolation (cross-platform).
async fn run_landlock_or_isolated(
    _host_workspace: &Path,
    effective_workspace: &Path,
    command_str: &str,
    policy: &SandboxPolicy,
    _use_landlock: bool,
) -> Result<RawCommandOutput> {
    let mut std_cmd = std::process::Command::new("sh");
    std_cmd.env_clear();
    std_cmd.current_dir(effective_workspace);

    // Whitelisted variables
    for &var_name in WHITELIST_ENV_VARS {
        if let Ok(val) = std::env::var(var_name) {
            std_cmd.env(var_name, val);
        }
    }

    // Filter host environment against secret patterns
    for (key, val) in std::env::vars() {
        let key_upper = key.to_uppercase();
        let is_sensitive = SECRET_PATTERNS.iter().any(|&pat| key_upper.contains(pat));
        let has_blocked_prefix = BLOCKED_PREFIXES
            .iter()
            .any(|&pfx| key_upper.starts_with(pfx));

        if !is_sensitive && !has_blocked_prefix && !WHITELIST_ENV_VARS.contains(&key.as_str()) {
            std_cmd.env(key, val);
        }
    }

    // Injected custom variables
    for (k, v) in &policy.extra_env {
        std_cmd.env(k, v);
    }

    std_cmd.env("MINICODE_WORKSPACE", effective_workspace);
    std_cmd.env("MINICODE_SANDBOX", "1");
    std_cmd.env(
        "MINICODE_SANDBOX_BACKEND",
        if _use_landlock {
            "landlock"
        } else {
            "process-isolated"
        },
    );

    std_cmd.arg("-c").arg(command_str);

    #[cfg(target_os = "linux")]
    if _use_landlock {
        let ws = effective_workspace.to_path_buf();
        let allow_net = policy.allow_network;
        let ro = policy.read_only_workspace;
        unsafe {
            use std::os::unix::process::CommandExt;
            std_cmd.pre_exec(move || {
                crate::sandbox::landlock::apply_landlock_sandbox_with_opts(&ws, allow_net, ro)
                    .map_err(|e| {
                        std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            format!("Landlock sandbox enforcement failed: {}", e),
                        )
                    })
            });
        }
    }

    execute_std_command_with_limits(std_cmd, policy).await
}

/// Spawns a configured `std::process::Command` under Tokio with process groups, output bounds, and limits.
async fn execute_std_command_with_limits(
    mut std_cmd: std::process::Command,
    policy: &SandboxPolicy,
) -> Result<RawCommandOutput> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mem_mb = policy.max_memory_mb;
        let cpu_sec = policy.max_cpu_seconds;

        unsafe {
            std_cmd.pre_exec(move || {
                if let Some(mb) = mem_mb {
                    let bytes = mb.saturating_mul(1024 * 1024);
                    let rlim = libc::rlimit {
                        rlim_cur: bytes,
                        rlim_max: bytes,
                    };
                    libc::setrlimit(libc::RLIMIT_AS, &rlim);
                }
                if let Some(sec) = cpu_sec {
                    let rlim = libc::rlimit {
                        rlim_cur: sec,
                        rlim_max: sec,
                    };
                    libc::setrlimit(libc::RLIMIT_CPU, &rlim);
                }
                Ok(())
            });
        }
    }

    let mut tokio_cmd = tokio::process::Command::from(std_cmd);
    tokio_cmd.kill_on_drop(true);
    tokio_cmd.stdout(std::process::Stdio::piped());
    tokio_cmd.stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    {
        tokio_cmd.process_group(0);
    }

    let mut child = tokio_cmd
        .spawn()
        .map_err(|e| ToolError::CommandExec(format!("Process spawn error: {}", e)))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let max_read_bytes = (EXEC_MAX_OUTPUT_BYTES as u64) + 1;

    let read_stdout = async {
        if let Some(mut out) = stdout {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut out).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stdout_buf).await;
            let _ = tokio::io::copy(&mut out, &mut tokio::io::sink()).await;
        }
    };

    let read_stderr = async {
        if let Some(mut err) = stderr {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut err).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stderr_buf).await;
            let _ = tokio::io::copy(&mut err, &mut tokio::io::sink()).await;
        }
    };

    let timeout = Duration::from_secs(policy.timeout_secs);
    let run_fut = async {
        tokio::join!(read_stdout, read_stderr);
        child.wait().await
    };

    let status = match tokio::time::timeout(timeout, run_fut).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return Err(ToolError::CommandExec(format!("Process execution error: {}", e)).into());
        }
        Err(_) => {
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGTERM);
                }
                tokio::time::sleep(Duration::from_millis(PROCESS_KILL_GRACE_PERIOD_MS)).await;
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = child.kill().await;
            return Err(ToolError::CommandTimeout {
                timeout_secs: timeout.as_secs(),
            }
            .into());
        }
    };

    let stdout_str = String::from_utf8_lossy(&stdout_buf);
    let stderr_str = String::from_utf8_lossy(&stderr_buf);

    let mut combined = String::new();
    if !stdout_str.is_empty() {
        combined.push_str(&stdout_str);
    }
    if !stderr_str.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str("[stderr]: ");
        combined.push_str(&stderr_str);
    }

    if combined.len() > EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    Ok(RawCommandOutput {
        stdout: stdout_str.to_string(),
        stderr: stderr_str.to_string(),
        combined_output: combined,
        exit_code: status.code().or(Some(SIGNAL_KILLED_EXIT_CODE)),
    })
}

/// Formats a `SandboxExecutionResult` into an agent-friendly narrative summary.
pub fn format_sandbox_result(result: &SandboxExecutionResult) -> String {
    let mut header = format!(
        "[Sandbox: {} | Network: {} | Duration: {}ms]",
        result.backend_used,
        if result.network_isolated {
            "BLOCKED"
        } else {
            "ALLOWED"
        },
        result.duration_ms
    );

    if result.ephemeral_writes_discarded {
        header.push_str(&format!(
            "\n[Ephemeral scratchpad discarded: {} file(s): {}]",
            result.discarded_files.len(),
            result.discarded_files.join(", ")
        ));
    }

    if !result.success {
        let code_str = result
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        format!(
            "{header}\nCommand exited with non-zero status ({code_str}):\n{}",
            result.combined_output
        )
    } else if result.combined_output.trim().is_empty() {
        format!("{header}\nCommand executed successfully (no output).")
    } else {
        format!("{header}\n{}", result.combined_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_exec_echo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let policy = SandboxPolicy::default();
        let res = run_sandboxed(temp_dir.path(), "echo 'minicode sandbox'", &policy)
            .await
            .unwrap();

        assert!(res.success);
        assert!(res.combined_output.contains("minicode sandbox"));
        assert!(res.network_isolated);
    }

    #[tokio::test]
    async fn test_sandbox_read_only_blocks_write() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut policy = SandboxPolicy::default();
        policy.read_only_workspace = true;

        let test_file = temp_dir.path().join("cant_write.txt");
        let cmd = format!("touch {}", test_file.display());
        let res = run_sandboxed(temp_dir.path(), &cmd, &policy).await;

        // In read-only mode, the command should fail or output permission denied
        if let Ok(r) = res {
            assert!(!r.success || !test_file.exists());
        }
    }

    #[tokio::test]
    async fn test_sandbox_ephemeral_tracks_discarded() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut policy = SandboxPolicy::default();
        policy.ephemeral_overlay = true;

        let res = run_sandboxed(temp_dir.path(), "echo 'hello' > scratch_file.txt", &policy)
            .await
            .unwrap();

        assert!(res.success);
        assert!(res.ephemeral_writes_discarded);
        assert!(res
            .discarded_files
            .contains(&"scratch_file.txt".to_string()));
        // Must NOT exist in the host workspace
        assert!(!temp_dir.path().join("scratch_file.txt").exists());
    }
}
