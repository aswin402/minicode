use crate::error::{Result, ToolError};
use crate::tools::minikit::stacks::{builtin::builtin_stacks, Stack};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

static BUILTIN_STACKS_CACHE: LazyLock<Vec<Stack>> = LazyLock::new(builtin_stacks);

#[derive(Clone, Debug)]
struct DirectoryStackCache {
    last_mtime: SystemTime,
    stacks: Vec<Stack>,
}

static STACK_DIR_CACHE: LazyLock<RwLock<HashMap<PathBuf, DirectoryStackCache>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Native engine for scaffolding application stacks, generating manifests, and auto-installing packages.
pub struct MiniKitScaffolder;

#[allow(dead_code)]
pub type OnpkgScaffolder = MiniKitScaffolder;

impl MiniKitScaffolder {
    /// Clears the directory stack cache (useful for testing and post-mutation invalidation).
    pub fn clear_cache() {
        if let Ok(mut cache) = STACK_DIR_CACHE.write() {
            cache.clear();
        }
    }

    /// Returns all natively embedded built-in stacks plus any custom workspace or user stacks.
    pub fn get_all_stacks() -> Vec<Stack> {
        Self::get_all_stacks_for(None)
    }

    /// Returns all natively embedded built-in stacks plus custom stacks in the specified workspace or user directories.
    pub fn get_all_stacks_for(workspace_root: Option<&Path>) -> Vec<Stack> {
        let mut stacks = BUILTIN_STACKS_CACHE.clone();

        let mut search_dirs = Vec::new();
        if let Some(ws) = workspace_root {
            let s1 = ws.join(".minicode").join("stacks");
            let s2 = ws.join(".minikit").join("stacks");
            if !search_dirs.contains(&s1) {
                search_dirs.push(s1);
            }
            if !search_dirs.contains(&s2) {
                search_dirs.push(s2);
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            let s1 = cwd.join(".minicode").join("stacks");
            let s2 = cwd.join(".minikit").join("stacks");
            if !search_dirs.contains(&s1) {
                search_dirs.push(s1);
            }
            if !search_dirs.contains(&s2) {
                search_dirs.push(s2);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let s1 = home.join(".config").join("minicode").join("stacks");
            let s2 = home.join(".minikit").join("stacks");
            let s3 = home.join(".onpkg").join("stacks");
            if !search_dirs.contains(&s1) {
                search_dirs.push(s1);
            }
            if !search_dirs.contains(&s2) {
                search_dirs.push(s2);
            }
            if !search_dirs.contains(&s3) {
                search_dirs.push(s3);
            }
        }

        for dir in search_dirs {
            let meta = match fs::metadata(&dir) {
                Ok(m) if m.is_dir() => m,
                _ => {
                    if let Ok(mut cache) = STACK_DIR_CACHE.write() {
                        cache.remove(&dir);
                    }
                    continue;
                }
            };

            let current_mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);

            // Fast path: check read cache
            let cached_stacks = {
                let cache = STACK_DIR_CACHE.read().ok();
                cache.and_then(|c| {
                    c.get(&dir).and_then(|entry| {
                        if entry.last_mtime == current_mtime {
                            Some(entry.stacks.clone())
                        } else {
                            None
                        }
                    })
                })
            };

            let dir_stacks = if let Some(hit) = cached_stacks {
                hit
            } else {
                let mut loaded = Vec::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(stack) = serde_json::from_str::<Stack>(&content) {
                                    loaded.push(stack);
                                }
                            }
                        }
                    }
                }
                if let Ok(mut cache) = STACK_DIR_CACHE.write() {
                    cache.insert(
                        dir.clone(),
                        DirectoryStackCache {
                            last_mtime: current_mtime,
                            stacks: loaded.clone(),
                        },
                    );
                }
                loaded
            };

            for stack in dir_stacks {
                if !stacks
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(&stack.name))
                {
                    stacks.push(stack);
                }
            }
        }

        stacks
    }

    /// Finds a stack by name across built-in and custom templates with alias resolution.
    #[allow(dead_code)]
    pub fn find_stack(name: &str) -> Option<Stack> {
        Self::find_stack_in_workspace(&PathBuf::new(), name)
    }

    /// Creates a starter custom stack template JSON in `.minicode/stacks/<name>.json` (or globally in `~/.config/minicode/stacks/<name>.json`).
    pub fn create_custom_stack(
        workspace_root: &Path,
        name: &str,
        runtime: &str,
        global: bool,
    ) -> Result<PathBuf> {
        let norm = name.trim().to_lowercase();
        if norm.is_empty() || norm.contains('/') || norm.contains('\\') || norm.contains("..") {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: format!(
                    "Invalid stack name '{}': must not contain path separators or traversal",
                    name
                ),
            }
            .into());
        }

        let target_dir = if global {
            let home = dirs::home_dir().ok_or_else(|| ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: "Could not determine home directory".to_string(),
            })?;
            home.join(".config").join("minicode").join("stacks")
        } else {
            workspace_root.join(".minicode").join("stacks")
        };

        fs::create_dir_all(&target_dir).map_err(|e| ToolError::FileOp {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let file_path = target_dir.join(format!("{}.json", norm));
        if file_path.exists() {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: format!(
                    "Custom stack template `{}` already exists at `{}`",
                    norm,
                    file_path.display()
                ),
            }
            .into());
        }

        let starter_stack = Stack {
            name: norm.clone(),
            runtime: runtime.trim().to_lowercase(),
            description: format!("Custom {} stack template for {}", runtime, norm),
            packages: vec![],
            dev_packages: vec![],
            transitive_packages: vec![],
            files: vec![crate::tools::minikit::stacks::StackFile {
                path: "README.md".to_string(),
                content: format!(
                    "# {}\n\nCustom architecture stack created with minicode.\n",
                    norm
                ),
                binary_content: None,
            }],
            hooks: vec![],
        };

        let json = serde_json::to_string_pretty(&starter_stack).map_err(|e| {
            ToolError::InvalidArguments {
                name: "kit_stack_new".to_string(),
                reason: format!("Failed to serialize stack JSON: {}", e),
            }
        })?;

        fs::write(&file_path, json).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Self::clear_cache();

        Ok(file_path)
    }

    /// Snapshots the current workspace or subproject into a reusable MiniKit stack template JSON
    /// in `.minicode/stacks/<name>.json` (or globally in `~/.config/minicode/stacks/<name>.json`).
    pub fn snapshot_workspace_to_stack(
        workspace_root: &Path,
        name: &str,
        description: Option<&str>,
        global: bool,
    ) -> Result<(PathBuf, usize, usize)> {
        let norm = name.trim().to_lowercase();
        if norm.is_empty() || norm.contains('/') || norm.contains('\\') || norm.contains("..") {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_snapshot".to_string(),
                reason: format!(
                    "Invalid stack name '{}': must not contain path separators or traversal",
                    name
                ),
            }
            .into());
        }

        let target_dir = if global {
            let home = dirs::home_dir().ok_or_else(|| ToolError::InvalidArguments {
                name: "kit_stack_snapshot".to_string(),
                reason: "Could not determine home directory".to_string(),
            })?;
            home.join(".config").join("minicode").join("stacks")
        } else {
            workspace_root.join(".minicode").join("stacks")
        };

        fs::create_dir_all(&target_dir).map_err(|e| ToolError::FileOp {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let file_path = target_dir.join(format!("{}.json", norm));

        // 1. Detect runtime and package manager
        let (runtime, _) = super::sync::MiniKitSyncEngine::detect_runtime(workspace_root);

        // 2. Extract dependencies from manifest or project config files
        let (packages, dev_packages) = Self::extract_dependencies_from_workspace(workspace_root);

        // 3. Collect files using ignore::WalkBuilder (respects .gitignore)
        let files = Self::collect_files_for_snapshot(workspace_root)?;
        let files_count = files.len();
        let packages_count = packages.len() + dev_packages.len();

        let desc = match description {
            Some(d) if !d.trim().is_empty() => d.trim().to_string(),
            _ => format!(
                "Snapshot template of {} ({} files, {} packages)",
                norm, files_count, packages_count
            ),
        };

        let stack = Stack {
            name: norm.clone(),
            runtime: runtime.to_string(),
            description: desc,
            packages,
            dev_packages,
            transitive_packages: vec![],
            files,
            hooks: vec![],
        };

        let json =
            serde_json::to_string_pretty(&stack).map_err(|e| ToolError::InvalidArguments {
                name: "kit_stack_snapshot".to_string(),
                reason: format!("Failed to serialize stack JSON: {}", e),
            })?;

        fs::write(&file_path, json).map_err(|e| ToolError::FileOp {
            path: file_path.display().to_string(),
            source: e,
        })?;

        Self::clear_cache();

        Ok((file_path, files_count, packages_count))
    }

    fn extract_dependencies_from_workspace(workspace_root: &Path) -> (Vec<String>, Vec<String>) {
        let mut packages = Vec::new();
        let mut dev_packages = Vec::new();

        // 1. Check minikit.json or onpkg.json
        for manifest_name in &[
            crate::constants::MINIKIT_MANIFEST_FILE,
            crate::constants::MINICODE_MANIFEST_FILE,
            crate::constants::ONPKG_MANIFEST_FILE,
        ] {
            let manifest_path = workspace_root.join(manifest_name);
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(pkgs) = val.get("packages").and_then(|p| p.as_array()) {
                            for p in pkgs {
                                if let Some(s) = p.as_str() {
                                    if !packages.contains(&s.to_string()) {
                                        packages.push(s.to_string());
                                    }
                                }
                            }
                        }
                        if let Some(dev) = val.get("dev_packages").and_then(|d| d.as_array()) {
                            for d in dev {
                                if let Some(s) = d.as_str() {
                                    if !dev_packages.contains(&s.to_string()) {
                                        dev_packages.push(s.to_string());
                                    }
                                }
                            }
                        }
                        if !packages.is_empty() || !dev_packages.is_empty() {
                            return (packages, dev_packages);
                        }
                    }
                }
            }
        }

        // 2. Check package.json (Node/Bun/Deno)
        let pkg_json = workspace_root.join("package.json");
        if pkg_json.exists() {
            if let Ok(content) = fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(deps) = val.get("dependencies").and_then(|d| d.as_object()) {
                        for k in deps.keys() {
                            if !packages.contains(k) {
                                packages.push(k.clone());
                            }
                        }
                    }
                    if let Some(devs) = val.get("devDependencies").and_then(|d| d.as_object()) {
                        for k in devs.keys() {
                            if !dev_packages.contains(k) {
                                dev_packages.push(k.clone());
                            }
                        }
                    }
                }
            }
        }

        // 3. Check Cargo.toml (Rust)
        let cargo_toml = workspace_root.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if let Ok(val) = toml::from_str::<toml::Value>(&content) {
                    if let Some(deps) = val.get("dependencies").and_then(|d| d.as_table()) {
                        for k in deps.keys() {
                            if !packages.contains(k) {
                                packages.push(k.clone());
                            }
                        }
                    }
                    if let Some(devs) = val.get("dev-dependencies").and_then(|d| d.as_table()) {
                        for k in devs.keys() {
                            if !dev_packages.contains(k) {
                                dev_packages.push(k.clone());
                            }
                        }
                    }
                }
            }
        }

        // 4. Check requirements.txt (Python)
        let req_txt = workspace_root.join("requirements.txt");
        if req_txt.exists() {
            if let Ok(content) = fs::read_to_string(&req_txt) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
                        continue;
                    }
                    let pkg_name = trimmed
                        .split(&['=', '<', '>', '~', '!'][..])
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !pkg_name.is_empty() && !packages.contains(&pkg_name.to_string()) {
                        packages.push(pkg_name.to_string());
                    }
                }
            }
        }

        // 5. Check pubspec.yaml (Flutter/Dart)
        let pubspec = workspace_root.join("pubspec.yaml");
        if pubspec.exists() {
            if let Ok(content) = fs::read_to_string(&pubspec) {
                let mut in_deps = false;
                let mut in_dev_deps = false;
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("dependencies:") {
                        in_deps = true;
                        in_dev_deps = false;
                        continue;
                    } else if trimmed.starts_with("dev_dependencies:") {
                        in_deps = false;
                        in_dev_deps = true;
                        continue;
                    } else if !line.starts_with(' ')
                        && !line.starts_with('\t')
                        && line.contains(':')
                    {
                        in_deps = false;
                        in_dev_deps = false;
                    }

                    if (in_deps || in_dev_deps) && !trimmed.is_empty() && !trimmed.starts_with('#')
                    {
                        if let Some((k, _)) = trimmed.split_once(':') {
                            let k = k.trim();
                            if k != "flutter" && k != "sdk" {
                                if in_deps && !packages.contains(&k.to_string()) {
                                    packages.push(k.to_string());
                                } else if in_dev_deps && !dev_packages.contains(&k.to_string()) {
                                    dev_packages.push(k.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        packages.sort();
        dev_packages.sort();
        (packages, dev_packages)
    }

    fn collect_files_for_snapshot(
        workspace_root: &Path,
    ) -> Result<Vec<crate::tools::minikit::stacks::StackFile>> {
        let mut files = Vec::new();
        const MAX_FILE_SIZE: u64 = 512 * 1024; // 512 KB
        const MAX_TOTAL_FILES: usize = 500;

        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let rel = match path.strip_prefix(workspace_root) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let rel_str = rel.to_string_lossy().replace('\\', "/");

            // Exclude directories and meta-files
            let should_exclude = rel_str.starts_with(".git/")
                || rel_str == ".git"
                || rel_str.starts_with(".minicode/")
                || rel_str == ".minicode"
                || rel_str.starts_with(".onpkg/")
                || rel_str == ".onpkg"
                || rel_str.starts_with("node_modules/")
                || rel_str.starts_with("target/")
                || rel_str.starts_with("dist/")
                || rel_str.starts_with("build/")
                || rel_str.starts_with(".next/")
                || rel_str.starts_with("__pycache__/")
                || rel_str.starts_with(".venv/")
                || rel_str.starts_with("venv/")
                || rel_str.starts_with(".turbo/")
                || rel_str.starts_with(".cache/")
                || rel_str.ends_with(".lock")
                || rel_str == "package-lock.json"
                || rel_str == "pnpm-lock.yaml"
                || rel_str == "yarn.lock"
                || rel_str == "bun.lockb"
                || rel_str == "bun.lock"
                || rel_str.ends_with(".sqlite")
                || rel_str.ends_with(".sqlite3")
                || rel_str.ends_with(".db")
                || rel_str.ends_with(".pyc")
                || rel_str.ends_with(".DS_Store");

            if should_exclude {
                continue;
            }

            if let Ok(metadata) = path.metadata() {
                if metadata.len() > MAX_FILE_SIZE {
                    continue;
                }
            }

            if let Ok(bytes) = fs::read(path) {
                match String::from_utf8(bytes.clone()) {
                    Ok(text) => {
                        files.push(crate::tools::minikit::stacks::StackFile {
                            path: rel_str,
                            content: text,
                            binary_content: None,
                        });
                    }
                    Err(_) => {
                        files.push(crate::tools::minikit::stacks::StackFile {
                            path: rel_str,
                            content: String::new(),
                            binary_content: Some(bytes),
                        });
                    }
                }
            }

            if files.len() >= MAX_TOTAL_FILES {
                break;
            }
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Deletes a custom stack template from `.minicode/stacks/<name>.json` (or globally `~/.config/minicode/stacks/`).
    pub fn delete_custom_stack(
        workspace_root: &Path,
        stack_name: &str,
        global: bool,
    ) -> Result<String> {
        let norm = stack_name.trim().to_lowercase();
        if norm.is_empty() || norm.contains('/') || norm.contains('\\') || norm.contains("..") {
            return Err(ToolError::InvalidArguments {
                name: "kit_stack_remove".to_string(),
                reason: format!(
                    "Invalid stack name '{}': must not contain path separators or traversal",
                    stack_name
                ),
            }
            .into());
        }

        let file_path = if global {
            if let Some(home) = dirs::home_dir() {
                home.join(".config")
                    .join("minicode")
                    .join("stacks")
                    .join(format!("{}.json", norm))
            } else {
                return Err(ToolError::InvalidArguments {
                    name: "kit_stack_remove".to_string(),
                    reason: "Home directory could not be determined".to_string(),
                }
                .into());
            }
        } else {
            workspace_root
                .join(".minicode")
                .join("stacks")
                .join(format!("{}.json", norm))
        };

        if file_path.exists() {
            fs::remove_file(&file_path).map_err(|e| ToolError::FileOp {
                path: file_path.display().to_string(),
                source: e,
            })?;
            Self::clear_cache();
            Ok(format!(
                "✔ Successfully removed custom stack template `{}` at `{}`",
                norm,
                file_path.display()
            ))
        } else {
            Err(ToolError::InvalidArguments {
                name: "kit_stack_remove".to_string(),
                reason: format!(
                    "Custom stack template `{}` not found at `{}`",
                    norm,
                    file_path.display()
                ),
            }
            .into())
        }
    }

    /// Finds a stack by name, prioritizing workspace-specific stacks in `.minicode/stacks`, with alias resolution.
    pub fn find_stack_in_workspace(workspace_root: &Path, name: &str) -> Option<Stack> {
        let norm = name.trim().to_lowercase();
        let all = Self::get_all_stacks_for(if workspace_root.as_os_str().is_empty() {
            None
        } else {
            Some(workspace_root)
        });

        // 1. Exact match
        if let Some(s) = all.iter().find(|s| s.name.to_lowercase() == norm) {
            return Some(s.clone());
        }

        // 2. Canonical technology aliases
        let alias = match norm.as_str() {
            "rust" | "cargo" => "rust-cli",
            "express" => "express-api",
            "static" | "html" | "web" => "static-website",
            "flutter" | "flutter-riverpod" | "riverpod" => "flutter-riverpod-my_app",
            "react" => "react-vite",
            "next" | "nextjs" => "next-template",
            "hono" => "hono-full",
            "python" | "py" => "fastapi",
            _ => norm.as_str(),
        };
        if let Some(s) = all.iter().find(|s| s.name.to_lowercase() == alias) {
            return Some(s.clone());
        }

        // 3. Prefix match
        all.into_iter().find(|s| {
            let sn = s.name.to_lowercase();
            sn.starts_with(&norm) || norm.starts_with(&sn)
        })
    }

    /// Scaffolds a stack into `target_dir`.
    pub async fn scaffold(
        workspace_root: &Path,
        stack_name: &str,
        target_dir_opt: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        let stack = Self::find_stack_in_workspace(workspace_root, stack_name).ok_or_else(|| {
            let available: Vec<String> =
                Self::get_all_stacks().into_iter().map(|s| s.name).collect();
            ToolError::InvalidArguments {
                name: "kit_stack_add".to_string(),
                reason: format!(
                    "Stack `{}` not found. Available built-in and custom stacks: {}",
                    stack_name,
                    available.join(", ")
                ),
            }
        })?;

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

        let files_count = stack.files.len();

        // 1. Write all template files
        for f in &stack.files {
            let file_path = dest_dir.join(&f.path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ToolError::FileOp {
                    path: parent.display().to_string(),
                    source: e,
                })?;
            }

            if let Some(bin) = &f.binary_content {
                fs::write(&file_path, bin).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;
            } else {
                fs::write(&file_path, &f.content).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;
            }

            // If the template file belongs to onpkg_docs/, mirror it into minikit_docs/
            if f.path.starts_with(crate::constants::ONPKG_DOCS_DIR) {
                let minikit_rel = f.path.replacen(
                    crate::constants::ONPKG_DOCS_DIR,
                    crate::constants::MINIKIT_DOCS_DIR,
                    1,
                );
                let minikit_file_path = dest_dir.join(&minikit_rel);
                if let Some(parent) = minikit_file_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Some(bin) = &f.binary_content {
                    let _ = fs::write(&minikit_file_path, bin);
                } else {
                    let _ = fs::write(&minikit_file_path, &f.content);
                }
            }
        }

        // 2. Generate onpkg.json manifest
        let project_name = dest_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string();

        let manifest = serde_json::json!({
            "name": project_name,
            "version": "0.1.0",
            "runtime": stack.runtime,
            "package_manager": stack.runtime,
            "stack": stack.name,
            "description": stack.description,
            "packages": stack.packages,
            "dev_packages": stack.dev_packages,
            "active_skills": [stack.runtime, stack.name]
        });

        let manifest_path = dest_dir.join(crate::constants::MINIKIT_MANIFEST_FILE);
        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        fs::write(&manifest_path, &manifest_json).ok();
        fs::write(
            dest_dir.join(crate::constants::ONPKG_MANIFEST_FILE),
            &manifest_json,
        )
        .ok();

        // 3. Generate AGENTS.md instructions
        let agents_md = format!(
            "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
            > Scaffolded with `minicode` + `MiniKit` native stack engine.\n\n\
            ## Project Summary\n\
            - **Name:** `{}`\n\
            - **Stack:** `{}`\n\
            - **Runtime / Package Manager:** `{}`\n\
            - **Description:** {}\n\n\
            ## Architecture & Conventions\n\
            1. All project specifications and task tracking live under `minikit_docs/`.\n\
            2. Use `{}` as the package manager.\n\
            3. Follow standard {} best practices.\n",
            project_name,
            project_name,
            stack.name,
            stack.runtime,
            stack.description,
            stack.runtime,
            stack.runtime
        );
        fs::write(dest_dir.join("AGENTS.md"), agents_md).ok();

        // 4. Generate initial workflow docs under minikit_docs and onpkg_docs
        let docs_dir = dest_dir.join(crate::constants::MINIKIT_DOCS_DIR);
        super::sync::MiniKitSyncEngine::ensure_workflow_docs(
            &docs_dir,
            &project_name,
            &stack.runtime,
        );
        let onpkg_docs = dest_dir.join(crate::constants::ONPKG_DOCS_DIR);
        super::sync::MiniKitSyncEngine::ensure_workflow_docs(
            &onpkg_docs,
            &project_name,
            &stack.runtime,
        );

        // 5. Post-scaffold install hooks
        let mut install_msg = String::new();
        if !no_install {
            install_msg = Self::run_package_installer(&stack.runtime, &dest_dir);
        }

        Ok(format!(
            "✔ Successfully scaffolded stack `{}` in `{}`\n\
            • Files created: {} files\n\
            • Manifest: minikit.json, AGENTS.md, minikit_docs/\n\
            • Runtime: {}\n{}",
            stack.name,
            dest_dir.display(),
            files_count,
            stack.runtime,
            install_msg
        ))
    }

    /// Automatically runs the best package installer for the runtime.
    fn run_package_installer(runtime: &str, dest_dir: &Path) -> String {
        let (cmd, args) = match runtime {
            "bun" => ("bun", vec!["install"]),
            "uv" => ("uv", vec!["sync"]),
            "cargo" => ("cargo", vec!["check"]),
            "flutter" => ("flutter", vec!["pub", "get"]),
            "npm" => ("npm", vec!["install"]),
            "pnpm" => ("pnpm", vec!["install"]),
            "yarn" => ("yarn", vec!["install"]),
            _ => return "\nℹ Skipped auto-install (unknown runtime)".to_string(),
        };

        match Command::new(cmd).args(&args).current_dir(dest_dir).output() {
            Ok(output) if output.status.success() => {
                format!("• Package install: ✔ `{}` completed successfully.", cmd)
            }
            Ok(output) => {
                let err = String::from_utf8_lossy(&output.stderr);
                format!(
                    "• Package install: ⚠ `{} {}` exited with error: {}",
                    cmd,
                    args.join(" "),
                    err.lines().next().unwrap_or("failed")
                )
            }
            Err(_) => {
                format!(
                    "• Package install: ℹ `{}` CLI not found on system. Run `{} {}` manually.",
                    cmd,
                    cmd,
                    args.join(" ")
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_custom_workspace_stack_scaffolding() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // 1. Create a custom stack template in .minicode/stacks/my-custom.json
        let stacks_dir = ws.join(".minicode").join("stacks");
        fs::create_dir_all(&stacks_dir).unwrap();

        let custom_stack = serde_json::json!({
            "name": "my-custom",
            "runtime": "bun",
            "description": "My custom microservice template",
            "packages": ["hono"],
            "dev_packages": ["typescript"],
            "files": [
                {
                    "path": "src/index.ts",
                    "content": "import { Hono } from 'hono';\nconst app = new Hono();\nexport default app;"
                }
            ]
        });
        fs::write(
            stacks_dir.join("my-custom.json"),
            serde_json::to_string_pretty(&custom_stack).unwrap(),
        )
        .unwrap();

        // 2. Discover stack
        let found = OnpkgScaffolder::find_stack_in_workspace(ws, "my-custom");
        assert!(found.is_some());
        let stack = found.unwrap();
        assert_eq!(stack.name, "my-custom");
        assert_eq!(stack.files.len(), 1);

        // 3. Scaffold stack
        let target = ws.join("service-output");
        let res = OnpkgScaffolder::scaffold(ws, "my-custom", Some(target.to_str().unwrap()), true)
            .await
            .unwrap();
        assert!(res.contains("Successfully scaffolded stack `my-custom`"));
        assert!(target.join("src/index.ts").exists());
        assert!(target.join("minikit.json").exists());
        assert!(target.join("AGENTS.md").exists());
    }

    #[test]
    fn test_create_custom_stack() {
        let temp = TempDir::new().unwrap();
        let path =
            OnpkgScaffolder::create_custom_stack(temp.path(), "my-starter", "bun", false).unwrap();
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"name\": \"my-starter\""));
        assert!(content.contains("\"runtime\": \"bun\""));

        let found = OnpkgScaffolder::find_stack_in_workspace(temp.path(), "my-starter");
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_snapshot_workspace_to_stack_and_scaffold_lifecycle() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // 1. Create realistic project files
        let src = ws.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("index.ts"),
            "import { Hono } from 'hono';\nexport const app = new Hono();\n",
        )
        .unwrap();
        fs::write(
            ws.join("README.md"),
            "# Sample Project\nSnapshot test project\n",
        )
        .unwrap();

        let pkg_json = serde_json::json!({
            "name": "sample-project",
            "dependencies": {
                "hono": "^4.0.0",
                "@hono/node-server": "^1.0.0"
            },
            "devDependencies": {
                "typescript": "^5.0.0",
                "@types/node": "^20.0.0"
            }
        });
        fs::write(
            ws.join("package.json"),
            serde_json::to_string_pretty(&pkg_json).unwrap(),
        )
        .unwrap();

        // 2. Perform snapshot
        let (stack_path, files_count, pkgs_count) = MiniKitScaffolder::snapshot_workspace_to_stack(
            ws,
            "snap-api",
            Some("Snapshot of API"),
            false,
        )
        .unwrap();

        assert!(stack_path.exists());
        assert_eq!(files_count, 3); // src/index.ts, README.md, package.json
        assert_eq!(pkgs_count, 4); // 2 deps + 2 devDeps

        // 3. Find and verify stack
        let found = MiniKitScaffolder::find_stack_in_workspace(ws, "snap-api");
        assert!(found.is_some());
        let stack = found.unwrap();
        assert_eq!(stack.name, "snap-api");
        assert_eq!(stack.description, "Snapshot of API");
        assert!(stack.packages.contains(&"hono".to_string()));
        assert!(stack.dev_packages.contains(&"typescript".to_string()));

        // 4. Scaffold from this snapshot into a new directory
        let out_dir = temp.path().join("generated-app");
        let scaffold_res =
            MiniKitScaffolder::scaffold(ws, "snap-api", Some(out_dir.to_str().unwrap()), true)
                .await
                .unwrap();

        assert!(scaffold_res.contains("Successfully scaffolded stack `snap-api`"));
        assert!(out_dir.join("src/index.ts").exists());
        assert!(out_dir.join("package.json").exists());
        assert!(out_dir.join("README.md").exists());
        assert!(out_dir.join("minikit.json").exists());
    }

    #[test]
    fn test_snapshot_excludes_vcs_and_build_artifacts() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        // Create normal file
        fs::create_dir_all(ws.join("src")).unwrap();
        fs::write(ws.join("src/main.rs"), "fn main() {}").unwrap();

        // Create git and build directories
        fs::create_dir_all(ws.join(".git/objects")).unwrap();
        fs::write(ws.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        fs::create_dir_all(ws.join("target/debug")).unwrap();
        fs::write(ws.join("target/debug/bin"), "binary").unwrap();
        fs::create_dir_all(ws.join("node_modules/pkg")).unwrap();
        fs::write(ws.join("node_modules/pkg/index.js"), "module").unwrap();
        fs::write(ws.join("Cargo.lock"), "# lockfile").unwrap();

        let (stack_path, files_count, _) =
            MiniKitScaffolder::snapshot_workspace_to_stack(ws, "clean-snap", None, false).unwrap();

        assert!(stack_path.exists());
        // Only src/main.rs should be included
        assert_eq!(files_count, 1);
        let stack = MiniKitScaffolder::find_stack_in_workspace(ws, "clean-snap").unwrap();
        assert_eq!(stack.files.len(), 1);
        assert_eq!(stack.files[0].path, "src/main.rs");
    }

    #[test]
    fn test_snapshot_invalid_names() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        assert!(MiniKitScaffolder::snapshot_workspace_to_stack(ws, "", None, false).is_err());
        assert!(
            MiniKitScaffolder::snapshot_workspace_to_stack(ws, "../traversal", None, false)
                .is_err()
        );
        assert!(
            MiniKitScaffolder::snapshot_workspace_to_stack(ws, "sub/dir", None, false).is_err()
        );
    }

    #[test]
    fn test_stack_cache_mtime_invalidation_lifecycle() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        MiniKitScaffolder::clear_cache();

        // 1. Initial count
        let initial_count = MiniKitScaffolder::get_all_stacks_for(Some(ws)).len();

        // 2. Create custom stack
        MiniKitScaffolder::create_custom_stack(ws, "cached-test", "bun", false).unwrap();

        // 3. get_all_stacks_for should immediately see it (cache cleared on create)
        let with_custom = MiniKitScaffolder::get_all_stacks_for(Some(ws));
        assert_eq!(with_custom.len(), initial_count + 1);
        assert!(with_custom.iter().any(|s| s.name == "cached-test"));

        // 4. Repeated call hits cache
        let cached_hit = MiniKitScaffolder::get_all_stacks_for(Some(ws));
        assert_eq!(cached_hit.len(), initial_count + 1);

        // 5. Delete stack
        MiniKitScaffolder::delete_custom_stack(ws, "cached-test", false).unwrap();
        let after_delete = MiniKitScaffolder::get_all_stacks_for(Some(ws));
        assert_eq!(after_delete.len(), initial_count);
        assert!(!after_delete.iter().any(|s| s.name == "cached-test"));
    }
}
