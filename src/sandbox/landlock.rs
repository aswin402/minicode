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

    let add_path_rule = |rs: landlock::RulesetCreated,
                         p: &Path,
                         access: landlock::BitFlags<AccessFs>,
                         ctx: &str|
     -> Result<landlock::RulesetCreated> {
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                return rs.add_rule(PathBeneath::new(fd, access)).map_err(|e| {
                    SecurityError::Landlock(format!(
                        "Failed to add {} rule for {}: {}",
                        ctx,
                        p.display(),
                        e
                    ))
                    .into()
                });
            }
        }
        Ok(rs)
    };

    let rw_access = AccessFs::from_all(fs_abi);
    let ro_access = AccessFs::from_read(fs_abi);
    let ws_access = if read_only { ro_access } else { rw_access };

    // Allow read/write or read-only within workspace root
    ruleset_created = add_path_rule(ruleset_created, workspace_root, ws_access, "workspace")?;

    // Allow read/traversal on ancestor directories of workspace root so getcwd() and path resolution succeed
    let mut current_ancestor = workspace_root.parent();
    while let Some(parent) = current_ancestor {
        ruleset_created = add_path_rule(ruleset_created, parent, ro_access, "ancestor")?;
        current_ancestor = parent.parent();
    }

    // If workspace_root is an isolated `.minicode/worktrees/<id>` directory and not read-only,
    // allow read/write access to the parent repo's `.git` (for worktree index/refs) and `target` (warm build cache).
    if !read_only {
        if let Some(repo_root) =
            crate::sandbox::env::detect_parent_repo_from_worktree(workspace_root)
        {
            for shared_sub in &[".git", "target", "node_modules"] {
                let shared_dir = repo_root.join(shared_sub);
                ruleset_created =
                    add_path_rule(ruleset_created, &shared_dir, rw_access, "shared worktree")?;
            }
        }
    }

    // Allow read/write access to /tmp, shared memory/terminals, and standard device streams
    for rw_path_str in &[
        "/tmp",
        "/dev/shm",
        "/dev/pts",
        "/dev/null",
        "/dev/zero",
        "/dev/urandom",
        "/dev/random",
        "/dev/tty",
    ] {
        ruleset_created = add_path_rule(ruleset_created, Path::new(rw_path_str), rw_access, "dev")?;
    }

    let home_dir = std::env::var("HOME").ok();

    // Allow read/write access to package manager caches (~/.npm, ~/.cache, ~/.cargo, ~/.pnpm-store, ~/.bun)
    if let Some(ref h) = home_dir {
        let user_cache = format!("{}/.cache", h);
        if !read_only && !Path::new(&user_cache).exists() {
            let _ = std::fs::create_dir_all(&user_cache);
        }

        for rw_sub in [
            ".npm",
            ".npm-global",
            ".cache",
            ".cargo",
            ".pnpm-store",
            ".bun",
            ".local/share/pnpm",
            ".yarn",
            ".deno",
            ".fnm",
        ] {
            let p_str = format!("{}/{}", h, rw_sub);
            ruleset_created =
                add_path_rule(ruleset_created, Path::new(&p_str), ws_access, "home cache")?;
        }

        for ro_sub in [".rustup", ".nvm", ".local", ".config", ".gitconfig"] {
            let p_str = format!("{}/{}", h, ro_sub);
            ruleset_created =
                add_path_rule(ruleset_created, Path::new(&p_str), ro_access, "home ro")?;
        }
    }

    for sys_path in &[
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
    ] {
        ruleset_created = add_path_rule(ruleset_created, Path::new(sys_path), ro_access, "system")?;
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
            let _ = rc.add_rule(PathBeneath::new(fd, AccessFs::from_all(ABI::V1)));
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_landlock_npm_and_cache_whitelisted() {
        if !is_landlock_supported() {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        // Run restrict_self on a dedicated thread so it doesn't restrict the cargo test worker thread pool
        let res = std::thread::spawn(move || apply_landlock_sandbox_with_opts(&path, true, false))
            .join()
            .unwrap();
        assert!(res.is_ok());
    }
}
