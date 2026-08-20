use crate::error::{Result, ToolError};
use crate::sandbox::env::build_sanitized_command;
use crate::sandbox::landlock::apply_landlock_sandbox;
use std::path::Path;
use std::time::Duration;

/// Executes a shell command inside the sandboxed workspace environment.
pub async fn exec_cmd(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
) -> Result<String> {
    let timeout =
        Duration::from_secs(timeout_secs.unwrap_or(crate::constants::EXEC_DEFAULT_TIMEOUT_SECS));

    let mut std_cmd = build_sanitized_command("sh", workspace_root);
    std_cmd.arg("-c").arg(command_str);

    #[cfg(target_os = "linux")]
    {
        let ws = workspace_root.to_path_buf();
        unsafe {
            use std::os::unix::process::CommandExt;
            std_cmd.pre_exec(move || {
                apply_landlock_sandbox(&ws, true).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("Landlock sandbox failed: {}", e),
                    )
                })
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
    let max_read_bytes = (crate::constants::EXEC_MAX_OUTPUT_BYTES as u64) + 1;

    let read_stdout = async {
        if let Some(mut out) = stdout {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut out).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stdout_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut out, &mut tokio::io::sink()).await;
        }
    };

    let read_stderr = async {
        if let Some(mut err) = stderr {
            use tokio::io::AsyncReadExt;
            let mut limited = (&mut err).take(max_read_bytes);
            let _ = limited.read_to_end(&mut stderr_buf).await;
            // Drain any remaining output to sink so child process does not block on full OS pipe buffer
            let _ = tokio::io::copy(&mut err, &mut tokio::io::sink()).await;
        }
    };

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
                tokio::time::sleep(std::time::Duration::from_millis(
                    crate::constants::PROCESS_KILL_GRACE_PERIOD_MS,
                ))
                .await;
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

    if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    let status_code = status
        .code()
        .unwrap_or(crate::constants::SIGNAL_KILLED_EXIT_CODE);
    let exit_code = status.code();
    let rtk_res = super::rtk_filter::RtkFilter::filter(command_str, &combined, exit_code);
    let compacted = super::compactor::compact_tool_output(command_str, &rtk_res.content, exit_code);

    if !status.success() {
        return Ok(format!(
            "Command exited with non-zero status ({status_code}):\n{compacted}"
        ));
    }

    if compacted.trim().is_empty() {
        Ok("Command executed successfully (no output).".to_string())
    } else {
        Ok(compacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_echo() {
        let temp_dir = std::env::temp_dir();
        let out = exec_cmd(&temp_dir, "echo 'hello minicode'", Some(5))
            .await
            .unwrap();
        assert!(out.contains("hello minicode"));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let temp_dir = std::env::temp_dir();
        let res = exec_cmd(&temp_dir, "sleep 3", Some(1)).await;
        assert!(res.is_err());
    }
}
