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
            Err(_) => {
                tracing::warn!(
                    "Landlock ABI V4 not supported on kernel — network restriction unavailable, process will have full network access"
                );
                Ruleset::default()
                    .handle_access(AccessFs::from_all(ABI::V1))
                    .map_err(|e| {
                        SecurityError::Landlock(format!("Failed to configure FS ruleset: {}", e))
                    })?
            }
        }
    } else {
        Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .map_err(|e| {
                SecurityError::Landlock(format!("Failed to configure FS ruleset: {}", e))
            })?
    };

    let mut ruleset_created = match ruleset.create() {
        Ok(rc) => rc,
        Err(e) => {
            let err_msg = format!("{}", e);
            if err_msg.contains("ENOSYS")
                || err_msg.contains("EOPNOTSUPP")
                || err_msg.contains("Function not implemented")
                || err_msg.contains("Operation not supported")
                || err_msg.to_lowercase().contains("not supported")
            {
                tracing::warn!(
                    error = %e,
                    "Landlock not supported on this host kernel; proceeding without Landlock sandbox"
                );
                return Ok(());
            }
            return Err(SecurityError::Landlock(format!(
                "Failed to create Landlock ruleset: {}",
                e
            ))
            .into());
        }
    };

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

    let system_paths = [
        "/usr",
        "/lib",
        "/lib64",
        "/etc",
        "/bin",
        "/dev",
        "/proc",
        "/opt",
        "/usr/local",
    ];

    let home_dir = std::env::var("HOME").ok();
    let cargo_home = home_dir.as_ref().map(|h| format!("{}/.cargo", h));
    let rustup_home = home_dir.as_ref().map(|h| format!("{}/.rustup", h));
    let nvm_home = home_dir.as_ref().map(|h| format!("{}/.nvm", h));
    let local_home = home_dir.as_ref().map(|h| format!("{}/.local", h));

    for p_str in [cargo_home, rustup_home, nvm_home, local_home]
        .into_iter()
        .flatten()
    {
        let p = Path::new(&p_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(fd, AccessFs::from_read(ABI::V1)))
                    .map_err(|e| {
                        SecurityError::Landlock(format!(
                            "Failed to add path rule for {}: {}",
                            p.display(),
                            e
                        ))
                    })?;
            }
        }
    }

    for sys_path in &system_paths {
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
    if let Err(e) = ruleset_created.restrict_self() {
        let err_msg = format!("{}", e);
        if err_msg.contains("ENOSYS")
            || err_msg.contains("EOPNOTSUPP")
            || err_msg.contains("Function not implemented")
            || err_msg.contains("Operation not supported")
        {
            tracing::warn!(
                error = %e,
                "Landlock restrict_self unsupported on host kernel; proceeding without Landlock sandbox"
            );
            return Ok(());
        }
        return Err(SecurityError::Landlock(format!(
            "Failed to enforce Landlock restrictions: {}",
            e
        ))
        .into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_landlock_sandbox(_workspace_root: &Path, _allow_network: bool) -> Result<()> {
    tracing::debug!("Landlock is Linux-only; skipping on this platform");
    Ok(())
}

/// Returns true if the Linux kernel supports Landlock (>= 5.13).
/// The handle_access() call probes kernel support; if it succeeds, Landlock is available.
#[cfg(target_os = "linux")]
pub fn is_landlock_supported() -> bool {
    use landlock::{Access, AccessFs, Ruleset, RulesetAttr, ABI};
    Ruleset::default()
        .handle_access(AccessFs::from_all(ABI::V1))
        .is_ok()
}

/// Returns false on non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn is_landlock_supported() -> bool {
    false
}
