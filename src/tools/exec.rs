use crate::error::{Result, ToolError};
use crate::sandbox::env::build_sanitized_command;
use crate::sandbox::landlock::apply_landlock_sandbox;
use std::path::Path;
use std::time::Duration;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 50 * 1024; // 50 KB cap

/// Executes a shell command inside the sandboxed workspace environment.
pub async fn exec_cmd(
    workspace_root: &Path,
    command_str: &str,
    timeout_secs: Option<u64>,
) -> Result<String> {
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    // Optional Landlock enforcement
    apply_landlock_sandbox(workspace_root, true).ok();

    let mut std_cmd = build_sanitized_command("sh", workspace_root);
    std_cmd.arg("-c").arg(command_str);

    let mut tokio_cmd = tokio::process::Command::from(std_cmd);

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

    if combined.len() > MAX_OUTPUT_BYTES {
        let truncated = &combined[..MAX_OUTPUT_BYTES];
        combined = format!(
            "{}\n\n[... Output truncated: exceeded 50KB limit ...]",
            truncated
        );
    }

    let status_code = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Ok(format!(
            "Command exited with non-zero status ({status_code}):\n{combined}"
        ));
    }

    if combined.trim().is_empty() {
        Ok("Command executed successfully (no output).".to_string())
    } else {
        Ok(combined)
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
