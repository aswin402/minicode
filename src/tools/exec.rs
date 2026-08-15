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

    let execution_future = tokio_cmd.output();

    let output = tokio::time::timeout(timeout, execution_future)
        .await
        .map_err(|_| ToolError::CommandTimeout {
            timeout_secs: timeout.as_secs(),
        })?
        .map_err(|e| ToolError::CommandExec(format!("Process spawn error: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str("[stderr]: ");
        combined.push_str(&stderr);
    }

    if combined.len() > crate::constants::EXEC_MAX_OUTPUT_BYTES {
        let valid_end = combined.floor_char_boundary(crate::constants::EXEC_MAX_OUTPUT_BYTES);
        let truncated = &combined[..valid_end];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded max limit ...]",
            truncated
        );
    }

    let status_code = output.status.code().unwrap_or(-1);
    let exit_code = output.status.code();
    let compacted = super::compactor::compact_tool_output(command_str, &combined, exit_code);

    if !output.status.success() {
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
