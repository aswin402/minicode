use crate::error::{Result, SecurityError};
use std::path::Path;

/// Applies Linux Landlock security restrictions to the current process/thread
/// before spawning a child process.
///
/// Restricts filesystem access to the workspace root, `/tmp`, and essential system paths.
/// Blocks TCP network operations if supported.
#[cfg(target_os = "linux")]
pub fn apply_landlock_sandbox(workspace_root: &Path, allow_network: bool) -> Result<()> {
    use landlock::{
        Access, AccessFs, AccessNet, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        ABI,
    };

    // If Landlock is not available on this kernel, return early without failing
    if landlock::Ruleset::default()
        .handle_access(AccessFs::from_all(ABI::V1))
        .is_err()
    {
        tracing::warn!(
            "Landlock not supported on this Linux kernel; proceeding without Landlock kernel-level filesystem enforcement"
        );
        return Ok(());
    }

    let ruleset = if !allow_network {
        match Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .and_then(|rs| rs.handle_access(AccessNet::ConnectTcp))
        {
            Ok(rs) => rs,
            Err(_) => Ruleset::default()
                .handle_access(AccessFs::from_all(ABI::V1))
                .map_err(|e| {
                    SecurityError::Landlock(format!("Failed to configure FS ruleset: {}", e))
                })?,
        }
    } else {
        Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .map_err(|e| {
                SecurityError::Landlock(format!("Failed to configure FS ruleset: {}", e))
            })?
    };

    let mut ruleset_created = ruleset.create().map_err(|e| {
        SecurityError::Landlock(format!("Failed to create Landlock ruleset: {}", e))
    })?;

    // Allow read/write within workspace root
    if let Ok(workspace_fd) = PathFd::new(workspace_root) {
        ruleset_created = ruleset_created
            .add_rule(PathBeneath::new(workspace_fd, AccessFs::from_all(ABI::V1)))
            .map_err(|e| SecurityError::Landlock(format!("Failed to add workspace rule: {}", e)))?;
    }

    // Allow read/write access to /tmp for compilers, package managers, and lockfiles
    let tmp_path = Path::new("/tmp");
    if tmp_path.exists() {
        if let Ok(fd) = PathFd::new(tmp_path) {
            ruleset_created = ruleset_created
                .add_rule(PathBeneath::new(fd, AccessFs::from_all(ABI::V1)))
                .map_err(|e| SecurityError::Landlock(format!("Failed to add /tmp rule: {}", e)))?;
        }
    }

    // Allow read-only access to standard system directories (/usr, /lib, /etc, /bin)
    for sys_path in &["/usr", "/lib", "/lib64", "/etc", "/bin"] {
        let p = Path::new(sys_path);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(fd, AccessFs::from_read(ABI::V1)))
                    .map_err(|e| {
                        SecurityError::Landlock(format!(
                            "Failed to add system path rule {}: {}",
                            sys_path, e
                        ))
                    })?;
            }
        }
    }

    // Restrict current thread (inherited by subsequent child processes)
    ruleset_created.restrict_self().map_err(|e| {
        SecurityError::Landlock(format!("Failed to enforce Landlock restrictions: {}", e))
    })?;

    tracing::debug!(workspace = %workspace_root.display(), "Landlock sandbox enforced");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_landlock_sandbox(_workspace_root: &Path, _allow_network: bool) -> Result<()> {
    tracing::debug!("Landlock is Linux-only; skipping on this platform");
    Ok(())
}
