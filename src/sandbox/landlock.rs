use crate::error::{Result, SecurityError};
use std::path::Path;

/// Applies Linux Landlock security restrictions to the current process/thread
/// before spawning a child process.
///
/// Restricts filesystem access to the workspace root, `/tmp`, and essential system paths.
/// Blocks TCP network operations if supported.
#[cfg(target_os = "linux")]
pub fn apply_landlock_sandbox(workspace_root: &Path, allow_network: bool) -> Result<()> {
    apply_landlock_sandbox_with_opts(workspace_root, allow_network, false)
}

/// Applies Linux Landlock security restrictions with configurable read-only workspace option.
#[cfg(target_os = "linux")]
pub fn apply_landlock_sandbox_with_opts(
    workspace_root: &Path,
    allow_network: bool,
    read_only: bool,
) -> Result<()> {
    use landlock::{
        Access, AccessFs, AccessNet, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
        ABI,
    };

    // Determine the highest available FS ABI supported by the running kernel (V4 -> V3 -> V2 -> V1)
    let fs_abi = [ABI::V4, ABI::V3, ABI::V2, ABI::V1]
        .into_iter()
        .find(|&abi| {
            Ruleset::default()
                .handle_access(AccessFs::from_all(abi))
                .and_then(|r| r.create())
                .is_ok()
        });

    let fs_abi = match fs_abi {
        Some(abi) => abi,
        None => {
            tracing::warn!(
                "Landlock not supported on this Linux kernel; proceeding without Landlock kernel-level filesystem enforcement"
            );
            return Ok(());
        }
    };

    let mut ruleset_created = if !allow_network {
        match Ruleset::default()
            .handle_access(AccessFs::from_all(fs_abi))
            .and_then(|rs| rs.handle_access(AccessNet::ConnectTcp))
            .and_then(|rs| rs.create())
        {
            Ok(rc) => rc,
            Err(_) => {
                tracing::warn!(
                    "Landlock network restriction unavailable on this kernel, process will have network access"
                );
                Ruleset::default()
                    .handle_access(AccessFs::from_all(fs_abi))
                    .and_then(|rs| rs.create())
                    .map_err(|e| {
                        SecurityError::Landlock(format!("Failed to create FS ruleset: {}", e))
                    })?
            }
        }
    } else {
        Ruleset::default()
            .handle_access(AccessFs::from_all(fs_abi))
            .and_then(|rs| rs.create())
            .map_err(|e| SecurityError::Landlock(format!("Failed to create FS ruleset: {}", e)))?
    };

    // Allow read/write or read-only within workspace root
    if let Ok(workspace_fd) = PathFd::new(workspace_root) {
        let access = if read_only {
            AccessFs::from_read(fs_abi)
        } else {
            AccessFs::from_all(fs_abi)
        };
        ruleset_created = ruleset_created
            .add_rule(PathBeneath::new(workspace_fd, access))
            .map_err(|e| SecurityError::Landlock(format!("Failed to add workspace rule: {}", e)))?;
    }

    // Allow read/traversal on ancestor directories of workspace root so getcwd() and path resolution succeed
    let mut current_ancestor = workspace_root.parent();
    while let Some(parent) = current_ancestor {
        if parent.exists() {
            if let Ok(parent_fd) = PathFd::new(parent) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(parent_fd, AccessFs::from_read(fs_abi)))
                    .map_err(|e| {
                        SecurityError::Landlock(format!("Failed to add ancestor rule: {}", e))
                    })?;
            }
        }
        current_ancestor = parent.parent();
    }

    // Allow read/write access to /tmp and shared memory/terminals for compilers, package managers, and lockfiles
    for rw_path_str in &["/tmp", "/dev/shm", "/dev/pts"] {
        let p = Path::new(rw_path_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(fd, AccessFs::from_all(fs_abi)))
                    .map_err(|e| {
                        SecurityError::Landlock(format!(
                            "Failed to add rw path rule {}: {}",
                            rw_path_str, e
                        ))
                    })?;
            }
        }
    }

    // Allow read/write access to standard device sink and random streams
    for dev_file in &[
        "/dev/null",
        "/dev/zero",
        "/dev/urandom",
        "/dev/random",
        "/dev/tty",
    ] {
        let p = Path::new(dev_file);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(fd, AccessFs::from_all(fs_abi)))
                    .map_err(|e| {
                        SecurityError::Landlock(format!(
                            "Failed to add dev file rule {}: {}",
                            dev_file, e
                        ))
                    })?;
            }
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
        "/run",
    ];

    let home_dir = std::env::var("HOME").ok();

    // Allow read/write access to package manager caches (~/.npm, ~/.cache, ~/.cargo, ~/.pnpm-store, ~/.bun) for builds, dependency downloads, and locks
    if let Some(ref h) = home_dir {
        let npm_home = format!("{}/.npm", h);
        let user_cache = format!("{}/.cache", h);
        let cargo_home = format!("{}/.cargo", h);
        let pnpm_store = format!("{}/.pnpm-store", h);
        let bun_home = format!("{}/.bun", h);
        let pnpm_local = format!("{}/.local/share/pnpm", h);

        for rw_home_str in [
            npm_home, user_cache, cargo_home, pnpm_store, bun_home, pnpm_local,
        ] {
            let p = Path::new(&rw_home_str);
            if !p.exists() {
                let _ = std::fs::create_dir_all(p);
            }
            if p.exists() {
                if let Ok(fd) = PathFd::new(p) {
                    let access = if read_only {
                        AccessFs::from_read(fs_abi)
                    } else {
                        AccessFs::from_all(fs_abi)
                    };
                    ruleset_created = ruleset_created
                        .add_rule(PathBeneath::new(fd, access))
                        .map_err(|e| {
                            SecurityError::Landlock(format!(
                                "Failed to add rw home path rule {}: {}",
                                rw_home_str, e
                            ))
                        })?;
                }
            }
        }
    }

    let rustup_home = home_dir.as_ref().map(|h| format!("{}/.rustup", h));
    let nvm_home = home_dir.as_ref().map(|h| format!("{}/.nvm", h));
    let local_home = home_dir.as_ref().map(|h| format!("{}/.local", h));
    let config_home = home_dir.as_ref().map(|h| format!("{}/.config", h));
    let gitconfig = home_dir.as_ref().map(|h| format!("{}/.gitconfig", h));

    for p_str in [rustup_home, nvm_home, local_home, config_home, gitconfig]
        .into_iter()
        .flatten()
    {
        let p = Path::new(&p_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                ruleset_created = ruleset_created
                    .add_rule(PathBeneath::new(fd, AccessFs::from_read(fs_abi)))
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
                    .add_rule(PathBeneath::new(fd, AccessFs::from_read(fs_abi)))
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

#[cfg(not(target_os = "linux"))]
pub fn apply_landlock_sandbox_with_opts(
    _workspace_root: &Path,
    _allow_network: bool,
    _read_only: bool,
) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use landlock::{
        Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
    };

    #[test]
    #[cfg(target_os = "linux")]
    fn test_landlock_dev_null_support() {
        if !is_landlock_supported() {
            return;
        }
        let ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(ABI::V1))
            .unwrap();
        let mut rc = ruleset.create().unwrap();

        for path in &["/tmp", "/dev/shm"] {
            let p = Path::new(path);
            if p.exists() {
                if let Ok(fd) = PathFd::new(p) {
                    rc = rc
                        .add_rule(PathBeneath::new(fd, AccessFs::from_all(ABI::V1)))
                        .unwrap();
                }
            }
        }

        // Test character device
        let p = Path::new("/dev/null");
        if let Ok(fd) = PathFd::new(p) {
            match rc.add_rule(PathBeneath::new(fd, AccessFs::from_all(ABI::V1))) {
                Ok(_) => {
                    println!("Direct /dev/null PathBeneath succeeded!");
                }
                Err(e) => {
                    println!("Direct /dev/null PathBeneath error: {}", e);
                }
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_landlock_npm_and_cache_whitelisted() {
        if !is_landlock_supported() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        // Test that apply_landlock_sandbox_with_opts runs cleanly with the whitelisted paths
        let res = apply_landlock_sandbox_with_opts(temp.path(), true, false);
        // Note: restrict_self affects the current thread, so in test we expect either Ok or unsupported
        assert!(res.is_ok());
    }
}
