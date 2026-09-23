use crate::error::{Result, ToolError};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Specification for a remote git / GitHub stack template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStackSpec {
    /// Original raw input string provided by user (e.g. `gh:shadcn-ui/ui/apps/www#main`)
    pub raw: String,
    /// Canonical clone URL (e.g. `https://github.com/shadcn-ui/ui.git`)
    pub clone_url: String,
    /// Repository owner / organization (e.g. `shadcn-ui`)
    pub owner: String,
    /// Repository name (e.g. `ui`)
    pub repo: String,
    /// Optional subdirectory within the repository (e.g. `apps/www`)
    pub subpath: Option<String>,
    /// Optional git branch, tag, or commit reference (e.g. `main`, `v1.2.0`)
    pub git_ref: Option<String>,
}

impl RemoteStackSpec {
    /// Determines whether an input string is a remote repository specification.
    pub fn is_remote(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with("gh:")
            || trimmed.starts_with("github:")
            || trimmed.starts_with("https://github.com/")
            || trimmed.starts_with("http://github.com/")
            || trimmed.starts_with("git@github.com:")
            || trimmed.starts_with("gitlab:")
            || trimmed.starts_with("https://gitlab.com/")
            || trimmed.starts_with("file://")
            || trimmed.ends_with(".git")
            || trimmed.contains(".git#")
    }

    /// Parses a remote specification string into structured components.
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if !Self::is_remote(trimmed) {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "'{}' is not a recognized remote stack repository specifier",
                    input
                ),
            }
            .into());
        }

        let raw = trimmed.to_string();

        if let Some(rest) = trimmed
            .strip_prefix("gh:")
            .or_else(|| trimmed.strip_prefix("github:"))
        {
            return Self::parse_shorthand(rest, &raw);
        }

        if let Some(rest) = trimmed.strip_prefix("gitlab:") {
            return Self::parse_gitlab_shorthand(rest, &raw);
        }

        if let Some(rest) = trimmed
            .strip_prefix("https://github.com/")
            .or_else(|| trimmed.strip_prefix("http://github.com/"))
        {
            return Self::parse_github_url(rest, &raw);
        }

        if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
            return Self::parse_git_ssh(rest, &raw);
        }

        if let Some(rest) = trimmed.strip_prefix("file://") {
            return Self::parse_file_url(rest, &raw);
        }

        if trimmed.ends_with(".git") || trimmed.contains(".git#") {
            return Self::parse_generic_git(trimmed, &raw);
        }

        Err(ToolError::InvalidArguments {
            name: "kit_stack_add".to_string(),
            reason: format!("Failed to parse remote specifier: {}", input),
        }
        .into())
    }

    fn parse_shorthand(rest: &str, raw: &str) -> Result<Self> {
        let (body, git_ref) = match rest.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (rest, None),
        };

        let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Invalid shorthand specifier '{}': expected 'gh:owner/repo[/subpath]'",
                    rest
                ),
            }
            .into());
        }

        let owner = parts[0].to_string();
        let repo = parts[1]
            .strip_suffix(".git")
            .unwrap_or(parts[1])
            .to_string();

        Self::validate_ident("owner", &owner)?;
        Self::validate_ident("repo", &repo)?;

        let subpath = if parts.len() > 2 {
            let sub = parts[2..].join("/");
            Self::validate_subpath(&sub)?;
            Some(sub)
        } else {
            None
        };

        let clone_url = format!("https://github.com/{}/{}.git", owner, repo);

        Ok(Self {
            raw: raw.to_string(),
            clone_url,
            owner,
            repo,
            subpath,
            git_ref,
        })
    }

    fn parse_gitlab_shorthand(rest: &str, raw: &str) -> Result<Self> {
        let (body, git_ref) = match rest.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (rest, None),
        };

        let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Invalid GitLab shorthand specifier '{}': expected 'gitlab:owner/repo[/subpath]'",
                    rest
                ),
            }
            .into());
        }

        let owner = parts[0].to_string();
        let repo = parts[1]
            .strip_suffix(".git")
            .unwrap_or(parts[1])
            .to_string();

        Self::validate_ident("owner", &owner)?;
        Self::validate_ident("repo", &repo)?;

        let subpath = if parts.len() > 2 {
            let sub = parts[2..].join("/");
            Self::validate_subpath(&sub)?;
            Some(sub)
        } else {
            None
        };

        let clone_url = format!("https://gitlab.com/{}/{}.git", owner, repo);

        Ok(Self {
            raw: raw.to_string(),
            clone_url,
            owner,
            repo,
            subpath,
            git_ref,
        })
    }

    fn parse_github_url(rest: &str, raw: &str) -> Result<Self> {
        let (body, hash_ref) = match rest.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (rest, None),
        };

        let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Invalid GitHub URL '{}': expected 'https://github.com/owner/repo'",
                    raw
                ),
            }
            .into());
        }

        let owner = parts[0].to_string();
        let repo = parts[1]
            .strip_suffix(".git")
            .unwrap_or(parts[1])
            .to_string();

        Self::validate_ident("owner", &owner)?;
        Self::validate_ident("repo", &repo)?;

        // Support web URL format: https://github.com/owner/repo/tree/<ref>/<subpath...>
        let (git_ref, subpath) = if parts.len() >= 4 && parts[2] == "tree" {
            let branch = parts[3].to_string();
            let sub = if parts.len() > 4 {
                let s = parts[4..].join("/");
                Self::validate_subpath(&s)?;
                Some(s)
            } else {
                None
            };
            (Some(branch), sub)
        } else {
            let sub = if parts.len() > 2 {
                let s = parts[2..].join("/");
                Self::validate_subpath(&s)?;
                Some(s)
            } else {
                None
            };
            (hash_ref, sub)
        };

        let clone_url = format!("https://github.com/{}/{}.git", owner, repo);

        Ok(Self {
            raw: raw.to_string(),
            clone_url,
            owner,
            repo,
            subpath,
            git_ref,
        })
    }

    fn parse_git_ssh(rest: &str, raw: &str) -> Result<Self> {
        let (body, git_ref) = match rest.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (rest, None),
        };

        let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!("Invalid SSH specifier: {}", raw),
            }
            .into());
        }

        let owner = parts[0].to_string();
        let repo = parts[1]
            .strip_suffix(".git")
            .unwrap_or(parts[1])
            .to_string();

        Self::validate_ident("owner", &owner)?;
        Self::validate_ident("repo", &repo)?;

        let subpath = if parts.len() > 2 {
            let s = parts[2..].join("/");
            Self::validate_subpath(&s)?;
            Some(s)
        } else {
            None
        };

        let clone_url = format!("https://github.com/{}/{}.git", owner, repo);

        Ok(Self {
            raw: raw.to_string(),
            clone_url,
            owner,
            repo,
            subpath,
            git_ref,
        })
    }

    fn parse_file_url(rest: &str, raw: &str) -> Result<Self> {
        let (body, git_ref) = match rest.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (rest, None),
        };

        let path = Path::new(body);
        let repo = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("local-repo")
            .strip_suffix(".git")
            .unwrap_or("local-repo")
            .to_string();
        let owner = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("local")
            .to_string();

        Ok(Self {
            raw: raw.to_string(),
            clone_url: body.to_string(),
            owner,
            repo,
            subpath: None,
            git_ref,
        })
    }

    fn parse_generic_git(trimmed: &str, raw: &str) -> Result<Self> {
        let (body, git_ref) = match trimmed.split_once('#') {
            Some((b, r)) => {
                let r_trim = r.trim();
                (
                    b,
                    if r_trim.is_empty() {
                        None
                    } else {
                        Some(r_trim.to_string())
                    },
                )
            }
            None => (trimmed, None),
        };

        let stripped = body.strip_suffix(".git").unwrap_or(body);
        let parts: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!("Invalid git repository specifier: {}", raw),
            }
            .into());
        }

        let repo = parts.last().copied().unwrap_or("repo").to_string();
        let owner = if parts.len() >= 2 {
            parts[parts.len() - 2].to_string()
        } else {
            "remote".to_string()
        };

        Ok(Self {
            raw: raw.to_string(),
            clone_url: body.to_string(),
            owner,
            repo,
            subpath: None,
            git_ref,
        })
    }

    fn validate_ident(kind: &str, s: &str) -> Result<()> {
        if s.is_empty()
            || !s
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Invalid characters in {} identifier '{}': alphanumeric, dash, and underscore allowed",
                    kind, s
                ),
            }
            .into());
        }
        Ok(())
    }

    fn validate_subpath(sub: &str) -> Result<()> {
        for seg in sub.split('/') {
            if seg == ".." || seg == "." || seg.is_empty() {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_add".to_string(),
                    reason: format!(
                        "Subpath '{}' contains invalid traversal or empty segments",
                        sub
                    ),
                }
                .into());
            }
        }
        Ok(())
    }
}

/// Native manager for fetching, caching, inspecting, and scaffolding remote git stacks.
pub struct MiniKitRemoteManager;

impl MiniKitRemoteManager {
    /// Returns the global cache directory for remote stacks (`~/.config/minicode/cache/remote_stacks`).
    pub fn get_cache_dir() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".config")
                .join("minicode")
                .join("cache")
                .join("remote_stacks")
        } else {
            std::env::temp_dir().join("minicode_remote_stacks")
        }
    }

    /// Clears the global remote stacks cache.
    pub fn clear_cache() -> Result<()> {
        let dir = Self::get_cache_dir();
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| ToolError::FileOp {
                path: dir.display().to_string(),
                source: e,
            })?;
        }
        Ok(())
    }

    /// Computes the dedicated cache path for a specific remote stack repository and ref.
    pub fn cache_dir_for_spec(spec: &RemoteStackSpec) -> PathBuf {
        let sanitized_ref = spec.git_ref.as_deref().unwrap_or("default");
        let safe_ref: String = sanitized_ref
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let folder_name = format!("{}_{}_{}", spec.owner, spec.repo, safe_ref);
        Self::get_cache_dir().join(folder_name)
    }

    /// Fetches or updates a remote stack repository via shallow clone into the cache.
    pub fn fetch_or_update(spec: &RemoteStackSpec, force_refresh: bool) -> Result<PathBuf> {
        let cache_dir = Self::cache_dir_for_spec(spec);
        let parent = cache_dir
            .parent()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: "Invalid cache directory path".to_string(),
            })?;
        fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
            path: parent.display().to_string(),
            source: e,
        })?;

        // Re-use cached copy if present and not force-refreshing
        if cache_dir.exists() && !force_refresh {
            let has_git = cache_dir.join(".git").exists();
            let has_files = fs::read_dir(&cache_dir)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
            if has_git || has_files {
                tracing::info!(
                    cache_dir = %cache_dir.display(),
                    url = %spec.clone_url,
                    "Using cached remote stack repository"
                );
                return Ok(cache_dir);
            }
        }

        if cache_dir.exists() {
            let _ = fs::remove_dir_all(&cache_dir);
        }

        let mut cmd = Command::new("git");
        cmd.arg("clone").arg("--depth").arg("1");
        if let Some(r) = &spec.git_ref {
            cmd.arg("--branch").arg(r);
        }
        cmd.arg(&spec.clone_url);
        cmd.arg(&cache_dir);

        tracing::info!(
            url = %spec.clone_url,
            git_ref = ?spec.git_ref,
            dest = %cache_dir.display(),
            "Cloning remote stack repository shallowly"
        );

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::InvalidArguments {
                    name: "kit_stack_add".to_string(),
                    reason: "Git is required to scaffold remote stacks but was not found on PATH. Please install git.".to_string(),
                }
            } else {
                ToolError::CommandExec(format!("git clone --depth 1 {}: {}", spec.clone_url, e))
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If branch clone failed, retry without --branch and then checkout
            if spec.git_ref.is_some() {
                tracing::warn!(
                    err = %stderr,
                    "Branch-specific shallow clone failed; retrying default shallow clone followed by checkout"
                );
                let _ = fs::remove_dir_all(&cache_dir);
                let fallback = Command::new("git")
                    .arg("clone")
                    .arg("--depth")
                    .arg("1")
                    .arg(&spec.clone_url)
                    .arg(&cache_dir)
                    .output()
                    .map_err(|e| {
                        ToolError::CommandExec(format!(
                            "git clone --depth 1 {}: {}",
                            spec.clone_url, e
                        ))
                    })?;

                if !fallback.status.success() {
                    let fb_err = String::from_utf8_lossy(&fallback.stderr);
                    return Err(ToolError::InvalidArguments {
                        name: "kit_stack_add".to_string(),
                        reason: format!(
                            "Failed to clone remote repository '{}': {}",
                            spec.clone_url,
                            fb_err.trim()
                        ),
                    }
                    .into());
                }

                if let Some(r) = &spec.git_ref {
                    let _ = Command::new("git")
                        .arg("checkout")
                        .arg(r)
                        .current_dir(&cache_dir)
                        .output();
                }
            } else {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_add".to_string(),
                    reason: format!(
                        "Failed to clone remote repository '{}': {}",
                        spec.clone_url,
                        stderr.trim()
                    ),
                }
                .into());
            }
        }

        Ok(cache_dir)
    }

    /// Recursively copies source directory files to destination, excluding git and dependency build artifacts.
    pub fn copy_dir_clean(source: &Path, dest: &Path) -> Result<usize> {
        let mut files_copied = 0;
        fs::create_dir_all(dest).map_err(|e| ToolError::FileOp {
            path: dest.display().to_string(),
            source: e,
        })?;

        let walker = ignore::WalkBuilder::new(source)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let rel = match path.strip_prefix(source) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if rel.as_os_str().is_empty() {
                continue;
            }

            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if rel_str == ".git"
                || rel_str.starts_with(".git/")
                || rel_str == "node_modules"
                || rel_str.starts_with("node_modules/")
                || rel_str == "target"
                || rel_str.starts_with("target/")
                || rel_str == "dist"
                || rel_str.starts_with("dist/")
                || rel_str == "build"
                || rel_str.starts_with("build/")
                || rel_str == ".next"
                || rel_str.starts_with(".next/")
                || rel_str == ".turbo"
                || rel_str.starts_with(".turbo/")
                || rel_str == ".venv"
                || rel_str.starts_with(".venv/")
                || rel_str == "venv"
                || rel_str.starts_with("venv/")
                || rel_str == "__pycache__"
                || rel_str.starts_with("__pycache__/")
                || rel_str == ".DS_Store"
                || rel_str.ends_with("/.DS_Store")
            {
                continue;
            }

            let target_path = dest.join(rel);

            if path.is_dir() {
                fs::create_dir_all(&target_path).map_err(|e| ToolError::FileOp {
                    path: target_path.display().to_string(),
                    source: e,
                })?;
            } else if path.is_file() {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                        path: parent.display().to_string(),
                        source: e,
                    })?;
                }
                fs::copy(path, &target_path).map_err(|e| ToolError::FileOp {
                    path: target_path.display().to_string(),
                    source: e,
                })?;
                files_copied += 1;
            }
        }

        Ok(files_copied)
    }

    /// Scaffolds a remote repository stack into the workspace or subproject.
    pub async fn scaffold_remote(
        workspace_root: &Path,
        spec_str: &str,
        target_dir_opt: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        let spec = RemoteStackSpec::parse(spec_str)?;

        let raw_dest: PathBuf = match target_dir_opt {
            Some(rel) if !rel.trim().is_empty() => {
                let p = Path::new(rel.trim());
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    workspace_root.join(p)
                }
            }
            _ => workspace_root.to_path_buf(),
        };

        let dest_dir = crate::sandbox::path::validate_path_in_workspace(workspace_root, &raw_dest)?;
        fs::create_dir_all(&dest_dir).map_err(|e| ToolError::FileOp {
            path: dest_dir.display().to_string(),
            source: e,
        })?;

        let cache_dir = Self::fetch_or_update(&spec, false)?;

        let source_dir = if let Some(sub) = &spec.subpath {
            let target = cache_dir.join(sub);
            if !target.exists() {
                let entries_summary = list_dir_summary(&cache_dir);
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_add".to_string(),
                    reason: format!(
                        "Subpath '{}' not found in remote repository '{}'. Available entries: {}",
                        sub, spec.clone_url, entries_summary
                    ),
                }
                .into());
            }

            let canon_target = target.canonicalize().map_err(|e| ToolError::FileOp {
                path: target.display().to_string(),
                source: e,
            })?;
            let canon_cache = cache_dir.canonicalize().map_err(|e| ToolError::FileOp {
                path: cache_dir.display().to_string(),
                source: e,
            })?;

            if !canon_target.starts_with(&canon_cache) {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_add".to_string(),
                    reason: "Subpath traversal attempts escape outside repository".to_string(),
                }
                .into());
            }

            canon_target
        } else {
            cache_dir.clone()
        };

        let files_count = Self::copy_dir_clean(&source_dir, &dest_dir)?;

        let project_name = dest_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string();

        let (runtime, package_manager) = super::sync::MiniKitSyncEngine::detect_runtime(&dest_dir);
        let (packages, dev_packages) =
            super::scaffolder::MiniKitScaffolder::extract_dependencies_from_workspace(&dest_dir);

        // Generate minikit.json and onpkg.json if not present in the remote template
        let manifest_path = dest_dir.join(crate::constants::MINIKIT_MANIFEST_FILE);
        if !manifest_path.exists() {
            let manifest = serde_json::json!({
                "name": project_name,
                "version": "0.1.0",
                "runtime": runtime,
                "package_manager": package_manager,
                "stack": spec.raw,
                "description": format!("Remote stack scaffolded from {}", spec.raw),
                "packages": packages,
                "dev_packages": dev_packages,
                "active_skills": [runtime]
            });
            let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
            let _ = fs::write(&manifest_path, &manifest_json);
            let _ = fs::write(
                dest_dir.join(crate::constants::ONPKG_MANIFEST_FILE),
                &manifest_json,
            );
        }

        // Generate AGENTS.md instructions if not present
        let agents_md_path = dest_dir.join("AGENTS.md");
        if !agents_md_path.exists() {
            let tech_pkgs = if packages.is_empty() {
                "none detected".to_string()
            } else {
                packages.join(", ")
            };
            let agents_md = format!(
                "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
                > Scaffolded from remote repository `{}` with `minicode` + `MiniKit`.\n\n\
                ## Project Summary\n\
                - **Name:** `{}`\n\
                - **Source:** `{}`\n\
                - **Runtime / Package Manager:** `{}`\n\
                - **Detected Packages:** {}\n\n\
                ## Architecture & Conventions\n\
                1. All project specifications and task tracking live under `minikit_docs/`.\n\
                2. Use `{}` as the package manager.\n\
                3. Follow standard {} best practices.\n",
                project_name,
                spec.raw,
                project_name,
                spec.raw,
                package_manager,
                tech_pkgs,
                package_manager,
                runtime
            );
            let _ = fs::write(&agents_md_path, agents_md);
        }

        // Generate standard workflow docs
        let docs_dir = dest_dir.join(crate::constants::MINIKIT_DOCS_DIR);
        super::sync::MiniKitSyncEngine::ensure_workflow_docs(&docs_dir, &project_name, runtime);
        let onpkg_docs = dest_dir.join(crate::constants::ONPKG_DOCS_DIR);
        super::sync::MiniKitSyncEngine::ensure_workflow_docs(&onpkg_docs, &project_name, runtime);

        let mut install_msg = String::new();
        if !no_install {
            install_msg = super::scaffolder::MiniKitScaffolder::run_package_installer(
                package_manager,
                &dest_dir,
            );
        }

        let ref_info = spec.git_ref.as_deref().unwrap_or("default");
        let subpath_info = spec.subpath.as_deref().unwrap_or("/");

        Ok(format!(
            "✔ Successfully scaffolded remote stack `{}` in `{}`\n\
            • Remote URL: {}\n\
            • Branch / Ref: {}\n\
            • Subpath: {}\n\
            • Files created: {} files\n\
            • Runtime: {} ({})\n\
            • Manifest: minikit.json, AGENTS.md, minikit_docs/\n{}",
            spec.raw,
            dest_dir.display(),
            spec.clone_url,
            ref_info,
            subpath_info,
            files_count,
            runtime,
            package_manager,
            install_msg
        ))
    }

    /// Inspects and displays information about a remote repository stack template.
    pub async fn show_remote(_workspace_root: &Path, spec_str: &str) -> Result<String> {
        let spec = RemoteStackSpec::parse(spec_str)?;
        let cache_dir = Self::fetch_or_update(&spec, false)?;

        let source_dir = if let Some(sub) = &spec.subpath {
            let target = cache_dir.join(sub);
            if !target.exists() {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_show".to_string(),
                    reason: format!(
                        "Subpath '{}' not found in remote repository '{}'. Available entries: {}",
                        sub,
                        spec.clone_url,
                        list_dir_summary(&cache_dir)
                    ),
                }
                .into());
            }
            target
        } else {
            cache_dir.clone()
        };

        let (runtime, package_manager) =
            super::sync::MiniKitSyncEngine::detect_runtime(&source_dir);
        let (packages, dev_packages) =
            super::scaffolder::MiniKitScaffolder::extract_dependencies_from_workspace(&source_dir);

        let mut sample_files = Vec::new();
        let mut total_files = 0;

        let walker = ignore::WalkBuilder::new(&source_dir)
            .hidden(false)
            .git_ignore(false)
            .build();

        for e in walker.flatten() {
            let p = e.path();
            if p.is_file() {
                if let Ok(rel) = p.strip_prefix(&source_dir) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if !rel_str.starts_with(".git/") && rel_str != ".git" {
                        total_files += 1;
                        if sample_files.len() < 25 {
                            sample_files.push(rel_str);
                        }
                    }
                }
            }
        }
        sample_files.sort();

        let ref_info = spec.git_ref.as_deref().unwrap_or("default");
        let subpath_info = spec.subpath.as_deref().unwrap_or("/");

        let mut out = format!(
            "📦 **Remote Stack: `{}`**\n\
            • **Repository:** `{}`\n\
            • **Branch / Tag:** `{}`\n\
            • **Subpath:** `{}`\n\
            • **Runtime / Package Manager:** `{} ({})`\n\
            • **Total Files:** {} files\n\
            • **Packages:** {}\n\
            • **Dev Packages:** {}\n\n\
            ### File Structure:\n",
            spec.raw,
            spec.clone_url,
            ref_info,
            subpath_info,
            runtime,
            package_manager,
            total_files,
            if packages.is_empty() {
                "none".to_string()
            } else {
                packages.join(", ")
            },
            if dev_packages.is_empty() {
                "none".to_string()
            } else {
                dev_packages.join(", ")
            },
        );

        for f in &sample_files {
            out.push_str(&format!("  ├── {}\n", f));
        }
        if total_files > sample_files.len() {
            out.push_str(&format!(
                "  ╰── ... and {} more files\n",
                total_files - sample_files.len()
            ));
        }

        Ok(out)
    }
}

fn list_dir_summary(dir: &Path) -> String {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                names.push(name);
            }
        }
    }
    names.sort();
    if names.is_empty() {
        "(empty)".to_string()
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_remote_recognition() {
        assert!(RemoteStackSpec::is_remote("gh:shadcn-ui/ui"));
        assert!(RemoteStackSpec::is_remote("github:owner/repo#branch"));
        assert!(RemoteStackSpec::is_remote("https://github.com/owner/repo"));
        assert!(RemoteStackSpec::is_remote(
            "https://github.com/owner/repo/tree/main/apps/www"
        ));
        assert!(RemoteStackSpec::is_remote("git@github.com:owner/repo.git"));
        assert!(RemoteStackSpec::is_remote("gitlab:owner/repo"));
        assert!(RemoteStackSpec::is_remote("file:///path/to/repo.git"));
        assert!(RemoteStackSpec::is_remote(
            "https://gitlab.com/group/repo.git"
        ));

        assert!(!RemoteStackSpec::is_remote("fastapi"));
        assert!(!RemoteStackSpec::is_remote("react-vite"));
        assert!(!RemoteStackSpec::is_remote("rust-cli"));
    }

    #[test]
    fn test_parse_github_shorthand() {
        let spec = RemoteStackSpec::parse("gh:owner/repo").unwrap();
        assert_eq!(spec.owner, "owner");
        assert_eq!(spec.repo, "repo");
        assert_eq!(spec.clone_url, "https://github.com/owner/repo.git");
        assert_eq!(spec.subpath, None);
        assert_eq!(spec.git_ref, None);

        let spec2 = RemoteStackSpec::parse("gh:owner/repo/apps/web#canary").unwrap();
        assert_eq!(spec2.owner, "owner");
        assert_eq!(spec2.repo, "repo");
        assert_eq!(spec2.subpath, Some("apps/web".to_string()));
        assert_eq!(spec2.git_ref, Some("canary".to_string()));

        let spec3 = RemoteStackSpec::parse("github:shadcn-ui/ui/apps/www").unwrap();
        assert_eq!(spec3.owner, "shadcn-ui");
        assert_eq!(spec3.repo, "ui");
        assert_eq!(spec3.subpath, Some("apps/www".to_string()));
    }

    #[test]
    fn test_parse_github_urls() {
        let spec = RemoteStackSpec::parse("https://github.com/vercel/next.js").unwrap();
        assert_eq!(spec.owner, "vercel");
        assert_eq!(spec.repo, "next.js");
        assert_eq!(spec.clone_url, "https://github.com/vercel/next.js.git");
        assert_eq!(spec.subpath, None);

        let spec_tree =
            RemoteStackSpec::parse("https://github.com/shadcn-ui/ui/tree/main/apps/www").unwrap();
        assert_eq!(spec_tree.owner, "shadcn-ui");
        assert_eq!(spec_tree.repo, "ui");
        assert_eq!(spec_tree.git_ref, Some("main".to_string()));
        assert_eq!(spec_tree.subpath, Some("apps/www".to_string()));

        let spec_hash = RemoteStackSpec::parse("https://github.com/owner/repo#v1.0.0").unwrap();
        assert_eq!(spec_hash.owner, "owner");
        assert_eq!(spec_hash.repo, "repo");
        assert_eq!(spec_hash.git_ref, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_parse_security_checks() {
        assert!(RemoteStackSpec::parse("gh:owner/repo/../etc").is_err());
        assert!(RemoteStackSpec::parse("gh:owner/repo/apps/../../root").is_err());
        assert!(RemoteStackSpec::parse("gh:invalid;name/repo").is_err());
        assert!(RemoteStackSpec::parse("gh:owner/invalid|repo").is_err());
    }

    #[test]
    fn test_copy_dir_clean_and_manifest_generation() {
        let temp_src = tempfile::tempdir().unwrap();
        let src_path = temp_src.path();
        let temp_dst = tempfile::tempdir().unwrap();
        let dst_path = temp_dst.path();

        // Create test source repo files
        fs::write(src_path.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        fs::create_dir_all(src_path.join("src")).unwrap();
        fs::write(src_path.join("src").join("main.rs"), "fn main() {}\n").unwrap();
        fs::create_dir_all(src_path.join(".git")).unwrap();
        fs::write(src_path.join(".git").join("config"), "git-config").unwrap();
        fs::create_dir_all(src_path.join("node_modules")).unwrap();
        fs::write(src_path.join("node_modules").join("dummy.js"), "// dummy").unwrap();

        let count = MiniKitRemoteManager::copy_dir_clean(src_path, dst_path).unwrap();
        assert_eq!(count, 2);
        assert!(dst_path.join("Cargo.toml").exists());
        assert!(dst_path.join("src").join("main.rs").exists());
        assert!(!dst_path.join(".git").exists());
        assert!(!dst_path.join("node_modules").exists());
    }
}
