use crate::error::{Result, SecurityError};
use std::path::{Path, PathBuf};

/// Lexically normalizes a path resolving `.`, `..`, and prefix/root components without filesystem access.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(..) => components.push(component),
            std::path::Component::RootDir => {
                components.clear();
                components.push(component);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if let Some(last) = components.last() {
                    match last {
                        std::path::Component::Normal(..) => {
                            components.pop();
                        }
                        _ => components.push(component),
                    }
                } else {
                    components.push(component);
                }
            }
            std::path::Component::Normal(..) => components.push(component),
        }
    }
    components.into_iter().collect()
}

/// Validates that a requested path does not escape the workspace root.
///
/// Handles relative paths, `..` traversals, and symbolic link dereferencing
/// by canonicalizing both the workspace root and the target path.
///
/// **Security Note**: This is a user-space validation check subject to TOCTOU races.
/// Linux Landlock kernel-level sandboxing (applied in `exec.rs`) is the primary
/// enforcement mechanism. This function serves as a defense-in-depth layer.
pub fn validate_path_in_workspace(workspace_root: &Path, user_path: &Path) -> Result<PathBuf> {
    let canonical_root =
        std::fs::canonicalize(workspace_root).map_err(|e| SecurityError::PathEscapesWorkspace {
            path: workspace_root.display().to_string(),
            workspace_root: format!("Failed to canonicalize workspace root: {}", e),
        })?;

    let raw_resolved = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        // Redundant workspace root folder prefix heuristic:
        // LLMs frequently prefix relative paths with the workspace folder's name
        // (e.g. "minitest10/index.html" when the workspace IS "minitest10").
        // If the path is relative, and its first component matches workspace_root.file_name(),
        // AND canonical_root.join(user_path) doesn't already exist, strip that redundant first component.
        let sanitized_user_path =
            if let Some(ws_name) = canonical_root.file_name().and_then(|n| n.to_str()) {
                if let Ok(rel) = user_path.strip_prefix(ws_name) {
                    if !canonical_root.join(user_path).exists() {
                        rel
                    } else {
                        user_path
                    }
                } else {
                    user_path
                }
            } else {
                user_path
            };

        canonical_root.join(sanitized_user_path)
    };

    let normalized = normalize_path(&raw_resolved);

    // Walk up to the nearest existing ancestor and resolve IT. Comparing
    // canonical forms on both sides keeps symlinked roots (e.g. /tmp ->
    // /private/tmp on macOS) working for existing AND not-yet-existing paths.
    let mut probe = normalized.clone();
    while !probe.exists() {
        match probe.parent() {
            Some(parent) => probe = parent.to_path_buf(),
            None => break,
        }
    }
    let canonical_probe =
        std::fs::canonicalize(&probe).map_err(|e| SecurityError::PathEscapesWorkspace {
            path: probe.display().to_string(),
            workspace_root: format!("Failed to canonicalize path: {}", e),
        })?;

    if !canonical_probe.starts_with(&canonical_root) {
        return Err(SecurityError::PathEscapesWorkspace {
            path: user_path.display().to_string(),
            workspace_root: canonical_root.display().to_string(),
        }
        .into());
    }

    if probe == normalized {
        // Path exists: return its fully resolved form.
        Ok(canonical_probe)
    } else {
        // Not-yet-existing path: re-anchor the non-existing tail onto the
        // canonical ancestor (e.g. for write_file / new file creation).
        let tail =
            normalized
                .strip_prefix(&probe)
                .map_err(|_| SecurityError::PathEscapesWorkspace {
                    path: user_path.display().to_string(),
                    workspace_root: canonical_root.display().to_string(),
                })?;
        Ok(canonical_probe.join(tail))
    }
}

/// Kernel-enforced workspace root handle that eliminates in-process symlink TOCTOU races
/// (`ckanthony/Chisel` `openat` + `O_NOFOLLOW` parity, zero external crates).
///
/// On Unix, holds an `O_DIRECTORY | O_CLOEXEC` file descriptor for the canonical workspace root
/// and walks every relative path component using `openat(..., O_NOFOLLOW)` so the kernel itself
/// atomically rejects symlink escapes (`ELOOP`) on reads, atomic writes (`renameat`), and deletes
/// (`unlinkat`). Falls back cleanly to `validate_path_in_workspace` on non-Unix targets.
pub struct ConfinedRoot {
    canonical_root: PathBuf,
    #[cfg(unix)]
    root_fd: std::os::fd::OwnedFd,
}

impl ConfinedRoot {
    /// Opens a kernel-anchored root handle for `workspace_root`.
    pub fn open(workspace_root: &Path) -> Result<Self> {
        let canonical_root = std::fs::canonicalize(workspace_root).map_err(|e| {
            SecurityError::PathEscapesWorkspace {
                path: workspace_root.display().to_string(),
                workspace_root: format!("Failed to canonicalize workspace root: {e}"),
            }
        })?;

        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::os::fd::FromRawFd;
            use std::os::unix::ffi::OsStrExt;

            let c_root = CString::new(canonical_root.as_os_str().as_bytes()).map_err(|_| {
                SecurityError::PathEscapesWorkspace {
                    path: canonical_root.display().to_string(),
                    workspace_root: "Workspace root contains interior NUL byte".to_string(),
                }
            })?;
            // SAFETY: `c_root` is a valid NUL-terminated path; return FD is checked for `< 0`
            // before transferring ownership to `OwnedFd`.
            let raw_fd = unsafe {
                libc::open(
                    c_root.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
                )
            };
            if raw_fd < 0 {
                let err = std::io::Error::last_os_error();
                return Err(SecurityError::PathEscapesWorkspace {
                    path: canonical_root.display().to_string(),
                    workspace_root: format!("Failed to open confined workspace root fd: {err}"),
                }
                .into());
            }
            // SAFETY: `raw_fd >= 0` is a newly opened owned file descriptor.
            let root_fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw_fd) };
            Ok(Self {
                canonical_root,
                root_fd,
            })
        }

        #[cfg(not(unix))]
        {
            Ok(Self { canonical_root })
        }
    }

    /// Resolves `user_path` into strictly relative normal path components guaranteed to stay
    /// inside `self.canonical_root`, optionally stripping a redundant leading workspace folder name.
    fn resolve_relative_components(
        &self,
        user_path: &Path,
        strip_redundant_root_prefix: bool,
    ) -> Result<Vec<String>> {
        let raw_resolved = if user_path.is_absolute() {
            user_path.to_path_buf()
        } else {
            let sanitized = if strip_redundant_root_prefix {
                if let Some(ws_name) = self.canonical_root.file_name().and_then(|n| n.to_str()) {
                    if let Ok(rel) = user_path.strip_prefix(ws_name) {
                        if !self.canonical_root.join(user_path).exists() {
                            rel
                        } else {
                            user_path
                        }
                    } else {
                        user_path
                    }
                } else {
                    user_path
                }
            } else {
                user_path
            };
            self.canonical_root.join(sanitized)
        };

        let normalized = normalize_path(&raw_resolved);
        let rel = normalized.strip_prefix(&self.canonical_root).map_err(|_| {
            SecurityError::PathEscapesWorkspace {
                path: user_path.display().to_string(),
                workspace_root: self.canonical_root.display().to_string(),
            }
        })?;

        let mut parts = Vec::new();
        for comp in rel.components() {
            match comp {
                std::path::Component::Normal(os) => {
                    let s = os.to_string_lossy().to_string();
                    if s.is_empty() || s.contains('\0') {
                        return Err(SecurityError::PathEscapesWorkspace {
                            path: user_path.display().to_string(),
                            workspace_root: self.canonical_root.display().to_string(),
                        }
                        .into());
                    }
                    parts.push(s);
                }
                std::path::Component::CurDir => {}
                _ => {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: user_path.display().to_string(),
                        workspace_root: self.canonical_root.display().to_string(),
                    }
                    .into());
                }
            }
        }
        if parts.is_empty() {
            return Err(SecurityError::PathEscapesWorkspace {
                path: user_path.display().to_string(),
                workspace_root: self.canonical_root.display().to_string(),
            }
            .into());
        }
        Ok(parts)
    }

    /// Reads raw bytes of `user_path` under kernel `openat` + `O_NOFOLLOW` confinement.
    /// Rejects any symlink escaping the workspace root with `SecurityError::PathEscapesWorkspace`.
    pub fn read_bytes(&self, user_path: &Path) -> Result<Vec<u8>> {
        let components = self.resolve_relative_components(user_path, true)?;
        #[cfg(unix)]
        {
            self.read_bytes_unix(&components, user_path, 0)
        }
        #[cfg(not(unix))]
        {
            let target = validate_path_in_workspace(&self.canonical_root, user_path)?;
            std::fs::read(&target).map_err(|e| {
                crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                }
                .into()
            })
        }
    }

    /// Reads a UTF-8 file under kernel `openat` + `O_NOFOLLOW` confinement.
    pub fn read_to_string(&self, user_path: &Path) -> Result<String> {
        let bytes = self.read_bytes(user_path)?;
        String::from_utf8(bytes).map_err(|e| {
            crate::error::ToolError::FileOp {
                path: user_path.display().to_string(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            }
            .into()
        })
    }

    /// Atomically writes `data` to `user_path` using kernel-confined `mkdirat`, `openat` with
    /// `O_NOFOLLOW | O_CREAT | O_EXCL` for the temp file, `fstatat(AT_SYMLINK_NOFOLLOW)` to block
    /// writing over a symlink, and `renameat` for atomic commit.
    pub fn atomic_write_bytes(&self, user_path: &Path, data: &[u8]) -> Result<()> {
        let components = self.resolve_relative_components(user_path, true)?;
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::io::Write;
            use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

            let (parent_parts, leaf) = components.split_at(components.len() - 1);
            let leaf_name = &leaf[0];
            let parent_fd = self.walk_parent_dirs_unix(parent_parts, true, user_path)?;
            let c_leaf = CString::new(leaf_name.as_bytes()).map_err(|_| {
                SecurityError::PathEscapesWorkspace {
                    path: user_path.display().to_string(),
                    workspace_root: self.canonical_root.display().to_string(),
                }
            })?;

            // Refuse overwriting a symlink target (blocks symlink-swap overwrite attacks)
            // and preserve existing file permissions (e.g. executable scripts) on atomic replace.
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: `parent_fd` is a valid directory FD and `c_leaf` is NUL-terminated.
            let stat_rc = unsafe {
                libc::fstatat(
                    parent_fd.as_raw_fd(),
                    c_leaf.as_ptr(),
                    &mut st,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            let existing_mode: Option<libc::mode_t> = if stat_rc == 0 {
                let file_type = st.st_mode & libc::S_IFMT;
                if file_type == libc::S_IFLNK {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: user_path.display().to_string(),
                        workspace_root: format!(
                            "Refusing confined write through symlink target inside {}",
                            self.canonical_root.display()
                        ),
                    }
                    .into());
                }
                if file_type == libc::S_IFDIR {
                    return Err(crate::error::ToolError::FileOp {
                        path: user_path.display().to_string(),
                        source: std::io::Error::from_raw_os_error(libc::EISDIR),
                    }
                    .into());
                }
                Some(st.st_mode & 0o777)
            } else {
                None
            };

            let tmp_name = format!(
                ".tmp_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            );
            let c_tmp = CString::new(tmp_name.as_bytes()).map_err(|_| {
                SecurityError::PathEscapesWorkspace {
                    path: user_path.display().to_string(),
                    workspace_root: self.canonical_root.display().to_string(),
                }
            })?;

            // SAFETY: `parent_fd` is an open directory FD anchored to `self.root_fd`.
            let tmp_raw = unsafe {
                libc::openat(
                    parent_fd.as_raw_fd(),
                    c_tmp.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o644 as libc::c_uint,
                )
            };
            if tmp_raw < 0 {
                return Err(crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: std::io::Error::last_os_error(),
                }
                .into());
            }

            if let Some(mode) = existing_mode {
                // SAFETY: `tmp_raw >= 0` is our newly created exclusive file descriptor.
                unsafe {
                    let _ = libc::fchmod(tmp_raw, mode);
                }
            }

            // SAFETY: `tmp_raw >= 0` is a newly created exclusive file descriptor.
            let mut file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(tmp_raw) });
            if let Err(e) = file.write_all(data).and_then(|()| file.flush()) {
                drop(file);
                unsafe {
                    let _ = libc::unlinkat(parent_fd.as_raw_fd(), c_tmp.as_ptr(), 0);
                }
                return Err(crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                }
                .into());
            }
            drop(file);

            // SAFETY: Both `c_tmp` and `c_leaf` live in the exact same verified `parent_fd`.
            let rename_rc = unsafe {
                libc::renameat(
                    parent_fd.as_raw_fd(),
                    c_tmp.as_ptr(),
                    parent_fd.as_raw_fd(),
                    c_leaf.as_ptr(),
                )
            };
            if rename_rc != 0 {
                let err = std::io::Error::last_os_error();
                unsafe {
                    let _ = libc::unlinkat(parent_fd.as_raw_fd(), c_tmp.as_ptr(), 0);
                }
                return Err(crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: err,
                }
                .into());
            }
            Ok(())
        }

        #[cfg(not(unix))]
        {
            use std::io::Write;
            let target = validate_path_in_workspace(&self.canonical_root, user_path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                })?;
            }
            let parent = target.parent().unwrap_or(&self.canonical_root);
            let tmp_path = parent.join(format!(
                ".tmp_{}_{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            ));
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp_path)
                .map_err(|e| crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                })?;
            f.write_all(data).and_then(|()| f.flush()).map_err(|e| {
                crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                }
            })?;
            drop(f);
            std::fs::rename(&tmp_path, &target).map_err(|e| {
                let _ = std::fs::remove_file(&tmp_path);
                crate::error::ToolError::FileOp {
                    path: user_path.display().to_string(),
                    source: e,
                }
            })?;
            Ok(())
        }
    }

    #[cfg(unix)]
    fn walk_parent_dirs_unix(
        &self,
        parent_parts: &[String],
        create_missing: bool,
        orig_path: &Path,
    ) -> Result<std::os::fd::OwnedFd> {
        use std::ffi::CString;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        // Start from a duplicate handle of `self.root_fd`
        let dot = CString::new(".").map_err(|_| SecurityError::PathEscapesWorkspace {
            path: orig_path.display().to_string(),
            workspace_root: self.canonical_root.display().to_string(),
        })?;
        // SAFETY: `self.root_fd` is a valid open directory FD.
        let mut cur_raw = unsafe {
            libc::openat(
                self.root_fd.as_raw_fd(),
                dot.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if cur_raw < 0 {
            return Err(crate::error::ToolError::FileOp {
                path: orig_path.display().to_string(),
                source: std::io::Error::last_os_error(),
            }
            .into());
        }
        // SAFETY: `cur_raw >= 0` is an owned FD.
        let mut cur_fd = unsafe { OwnedFd::from_raw_fd(cur_raw) };

        for seg in parent_parts {
            let c_seg =
                CString::new(seg.as_bytes()).map_err(|_| SecurityError::PathEscapesWorkspace {
                    path: orig_path.display().to_string(),
                    workspace_root: self.canonical_root.display().to_string(),
                })?;
            if create_missing {
                // SAFETY: `cur_fd` is an open directory FD anchored inside `self.root_fd`.
                let mk_rc = unsafe { libc::mkdirat(cur_fd.as_raw_fd(), c_seg.as_ptr(), 0o755) };
                if mk_rc != 0 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::EEXIST) {
                        return Err(crate::error::ToolError::FileOp {
                            path: orig_path.display().to_string(),
                            source: err,
                        }
                        .into());
                    }
                }
            }

            // SAFETY: `O_NOFOLLOW | O_DIRECTORY` enforces kernel rejection of symlink directory hops.
            cur_raw = unsafe {
                libc::openat(
                    cur_fd.as_raw_fd(),
                    c_seg.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if cur_raw < 0 {
                let err = std::io::Error::last_os_error();
                if matches!(err.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: orig_path.display().to_string(),
                        workspace_root: format!(
                            "Blocked symlink/non-directory hop at `{seg}` inside {}",
                            self.canonical_root.display()
                        ),
                    }
                    .into());
                }
                return Err(crate::error::ToolError::FileOp {
                    path: orig_path.display().to_string(),
                    source: err,
                }
                .into());
            }
            // SAFETY: `cur_raw >= 0` is an owned FD; previous `cur_fd` closes on reassignment.
            cur_fd = unsafe { OwnedFd::from_raw_fd(cur_raw) };
        }

        Ok(cur_fd)
    }

    #[cfg(unix)]
    fn read_bytes_unix(
        &self,
        components: &[String],
        orig_path: &Path,
        symlink_depth: usize,
    ) -> Result<Vec<u8>> {
        use std::ffi::CString;
        use std::io::Read;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        let (parent_parts, leaf) = components.split_at(components.len() - 1);
        let leaf_name = &leaf[0];
        let parent_fd = self.walk_parent_dirs_unix(parent_parts, false, orig_path)?;
        let c_leaf = CString::new(leaf_name.as_bytes()).map_err(|_| {
            SecurityError::PathEscapesWorkspace {
                path: orig_path.display().to_string(),
                workspace_root: self.canonical_root.display().to_string(),
            }
        })?;

        // SAFETY: `O_NOFOLLOW` atomically refuses following a symlink without checking confinement.
        let file_raw = unsafe {
            libc::openat(
                parent_fd.as_raw_fd(),
                c_leaf.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if file_raw < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ELOOP) {
                // Allow strictly intra-workspace relative symlinks (up to depth 4) while still
                // rejecting any symlink targeting paths outside `self.canonical_root`.
                if symlink_depth >= 4 {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: orig_path.display().to_string(),
                        workspace_root: self.canonical_root.display().to_string(),
                    }
                    .into());
                }
                let mut link_buf = vec![0u8; 4096];
                // SAFETY: `parent_fd` is valid and `link_buf` has `link_buf.len()` writable bytes.
                let n = unsafe {
                    libc::readlinkat(
                        parent_fd.as_raw_fd(),
                        c_leaf.as_ptr(),
                        link_buf.as_mut_ptr().cast::<libc::c_char>(),
                        link_buf.len(),
                    )
                };
                if n <= 0 || (n as usize) >= link_buf.len() {
                    return Err(SecurityError::PathEscapesWorkspace {
                        path: orig_path.display().to_string(),
                        workspace_root: self.canonical_root.display().to_string(),
                    }
                    .into());
                }
                let target_str = String::from_utf8_lossy(&link_buf[..n as usize]).to_string();
                let target_path = PathBuf::from(&target_str);
                let resolved_target = if target_path.is_absolute() {
                    target_path
                } else {
                    let mut base = PathBuf::new();
                    for p in parent_parts {
                        base.push(p);
                    }
                    base.push(target_path);
                    base
                };
                let next_parts = self.resolve_relative_components(&resolved_target, false)?;
                return self.read_bytes_unix(&next_parts, orig_path, symlink_depth + 1);
            }
            return Err(crate::error::ToolError::FileOp {
                path: orig_path.display().to_string(),
                source: err,
            }
            .into());
        }

        // SAFETY: `file_raw >= 0` is a newly opened owned file descriptor.
        let mut file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(file_raw) });
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| crate::error::ToolError::FileOp {
                path: orig_path.display().to_string(),
                source: e,
            })?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_path_inside_workspace() {
        let temp_dir = std::env::temp_dir();
        let valid_file = temp_dir.join("test_file.txt");
        let result = validate_path_in_workspace(&temp_dir, &valid_file);
        assert!(result.is_ok());
    }

    #[test]
    fn test_path_traversal_blocked() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_sandbox_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let malicious_path = PathBuf::from("../../etc/shadow");
        let result = validate_path_in_workspace(&temp_dir, &malicious_path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_redundant_workspace_prefix_stripped() {
        let temp_dir =
            std::env::temp_dir().join(format!("minicode_ws_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let ws_name = temp_dir.file_name().unwrap().to_str().unwrap();

        // Path like "minicode_ws_test_.../index.html" should resolve to "index.html" in the root
        let redundant_rel = PathBuf::from(format!("{}/index.html", ws_name));
        let resolved = validate_path_in_workspace(&temp_dir, &redundant_rel).unwrap();
        let canonical_root = std::fs::canonicalize(&temp_dir).unwrap();
        assert_eq!(resolved, canonical_root.join("index.html"));

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_confined_root_atomic_write_and_kernel_symlink_escape_rejection() {
        let ws = tempfile::tempdir().expect("workspace tempdir");
        let outside = tempfile::tempdir().expect("outside tempdir");
        let secret_file = outside.path().join("secret.txt");
        std::fs::write(&secret_file, "TOP_SECRET").expect("write secret");

        let root = ConfinedRoot::open(ws.path()).expect("open confined root");

        // 1. Atomic write into newly created nested subdirectory + confined read roundtrip
        root.atomic_write_bytes(Path::new("nested/dir/hello.txt"), b"hello chisel")
            .expect("atomic write succeeds");
        let content = root
            .read_to_string(Path::new("nested/dir/hello.txt"))
            .expect("confined read succeeds");
        assert_eq!(content, "hello chisel");

        // 2. Symlink pointing outside workspace must be rejected on BOTH read AND write
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret_file, ws.path().join("escape_link.txt"))
                .expect("symlink file");
            assert!(
                root.read_to_string(Path::new("escape_link.txt")).is_err(),
                "escaping file symlink must be blocked by ConfinedRoot"
            );
            assert!(
                root.atomic_write_bytes(Path::new("escape_link.txt"), b"pwned")
                    .is_err(),
                "writing through file symlink must be blocked by ConfinedRoot"
            );
            assert_eq!(std::fs::read_to_string(&secret_file).unwrap(), "TOP_SECRET");

            // Directory symlink pointing outside workspace must be rejected by O_NOFOLLOW
            std::os::unix::fs::symlink(outside.path(), ws.path().join("escape_dir"))
                .expect("symlink dir");
            assert!(
                root.atomic_write_bytes(Path::new("escape_dir/new.txt"), b"pwned")
                    .is_err(),
                "directory symlink hop must be blocked by O_NOFOLLOW"
            );
            assert!(!outside.path().join("new.txt").exists());

            // Intra-workspace relative symlink remains readable without escaping
            std::os::unix::fs::symlink("hello.txt", ws.path().join("nested/dir/alias.txt"))
                .expect("intra-workspace symlink");
            assert_eq!(
                root.read_to_string(Path::new("nested/dir/alias.txt"))
                    .expect("intra-workspace relative symlink read"),
                "hello chisel"
            );
        }
    }
}
