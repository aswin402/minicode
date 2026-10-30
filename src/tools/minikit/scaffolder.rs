use crate::error::{Result, ToolError};
use crate::tools::minikit::stacks::{builtin::builtin_stacks, Stack};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;

static BUILTIN_STACKS_CACHE: LazyLock<Vec<Stack>> = LazyLock::new(builtin_stacks);
static GLOBAL_DB_SEEDED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
struct DirectoryStackCache {
    last_mtime: SystemTime,
    json_count: usize,
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
        let _ = crate::tools::minikit::remote::MiniKitRemoteManager::clear_cache();
    }

    /// Returns the path to the user-level global stack template database (`~/.config/minicode/stacks`).
    pub fn global_stacks_db_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config").join("minicode").join("stacks"))
    }

    /// Computes a lightweight `(latest_mtime, json_file_count)` fingerprint for a stack directory
    /// so in-place edits to `<stack>.json` files as well as file additions/removals invalidate the cache.
    fn compute_dir_fingerprint(dir: &Path) -> Option<(SystemTime, usize)> {
        let meta = fs::metadata(dir).ok()?;
        if !meta.is_dir() {
            return None;
        }
        let mut latest = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let mut json_count = 0usize;
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(emeta) = entry.metadata() {
                        if emeta.is_file() {
                            json_count += 1;
                            if let Ok(mtime) = emeta.modified() {
                                if mtime > latest {
                                    latest = mtime;
                                }
                            }
                        }
                    }
                }
            }
        }
        Some((latest, json_count))
    }

    /// Resolves a stack name or shorthand alias (e.g. `react` -> `react-vite`) against built-in stacks.
    fn resolve_builtin_stack_name(name: &str) -> Option<&'static str> {
        let norm = name.trim().to_lowercase();
        let alias = match norm.as_str() {
            "rust" | "cargo" => "rust-cli",
            "express" => "express-api",
            "static" | "html" | "web" => "static-website",
            "flutter" | "flutter-riverpod" | "riverpod" => "flutter-riverpod-my_app",
            "react" => "react-vite",
            "next" | "nextjs" => "next-template",
            "hono" => "hono-full",
            "python" | "py" => "fastapi",
            other => other,
        };
        BUILTIN_STACKS_CACHE
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(alias))
            .map(|s| s.name.as_str())
    }

    /// Seeds built-in stack templates as standalone `<stack.name>.json` files into `target_dir`.
    /// When `overwrite` is `false`, existing JSON files on disk are preserved so user customizations are never lost.
    /// When `overwrite` is `true`, built-in stack JSON files are restored to their canonical binary definitions.
    pub fn seed_builtin_stacks_to_dir(
        target_dir: &Path,
        stack_filter: Option<&str>,
        overwrite: bool,
    ) -> Result<Vec<PathBuf>> {
        fs::create_dir_all(target_dir).map_err(|e| ToolError::FileOp {
            path: target_dir.display().to_string(),
            source: e,
        })?;

        let resolved_filter = match stack_filter.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                let Some(canonical) = Self::resolve_builtin_stack_name(raw) else {
                    let available: Vec<&str> = BUILTIN_STACKS_CACHE
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect();
                    return Err(ToolError::InvalidArguments {
                        name: "kit_stack_reset".to_string(),
                        reason: format!(
                            "Built-in stack `{}` not found. Available built-in stacks: {}",
                            raw,
                            available.join(", ")
                        ),
                    }
                    .into());
                };
                Some(canonical)
            }
            None => None,
        };

        let mut written = Vec::new();
        for stack in BUILTIN_STACKS_CACHE.iter() {
            if let Some(target_name) = resolved_filter {
                if !stack.name.eq_ignore_ascii_case(target_name) {
                    continue;
                }
            }
            let file_path = target_dir.join(format!("{}.json", stack.name));
            if !overwrite && file_path.exists() {
                continue;
            }
            let json =
                serde_json::to_string_pretty(stack).map_err(|e| ToolError::InvalidArguments {
                    name: "kit_stack_reset".to_string(),
                    reason: format!("Failed to serialize built-in stack `{}`: {}", stack.name, e),
                })?;
            fs::write(&file_path, json).map_err(|e| ToolError::FileOp {
                path: file_path.display().to_string(),
                source: e,
            })?;
            written.push(file_path);
        }

        let version_stamp = target_dir.join(".seed_version");
        let _ = fs::write(&version_stamp, env!("CARGO_PKG_VERSION"));

        if !written.is_empty() {
            Self::clear_cache();
        }

        Ok(written)
    }

    /// Ensures the global stack database (`~/.config/minicode/stacks/*.json`) is seeded with all
    /// built-in templates at least once per process, refreshing built-in templates when the
    /// `minicode` binary version advances.
    pub fn ensure_global_stack_db_seeded() {
        if GLOBAL_DB_SEEDED.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(global_dir) = Self::global_stacks_db_dir() else {
            return;
        };
        let all_present = BUILTIN_STACKS_CACHE
            .iter()
            .all(|s| global_dir.join(format!("{}.json", s.name)).exists());
        let version_matches = fs::read_to_string(global_dir.join(".seed_version"))
            .map(|v| v.trim() == env!("CARGO_PKG_VERSION"))
            .unwrap_or(false);
        if !all_present || !version_matches {
            let _ = Self::seed_builtin_stacks_to_dir(&global_dir, None, !version_matches);
        }
    }

    /// Resets one or all built-in stack templates in `~/.config/minicode/stacks/` back to their
    /// canonical embedded definitions.
    pub fn reset_builtin_stacks(stack_name: Option<&str>) -> Result<Vec<PathBuf>> {
        let global_dir =
            Self::global_stacks_db_dir().ok_or_else(|| ToolError::InvalidArguments {
                name: "kit_stack_reset".to_string(),
                reason: "Could not determine user home directory for ~/.config/minicode/stacks"
                    .to_string(),
            })?;
        let paths = Self::seed_builtin_stacks_to_dir(&global_dir, stack_name, true)?;
        GLOBAL_DB_SEEDED.store(true, Ordering::SeqCst);
        Ok(paths)
    }

    /// Resolves the active on-disk JSON database path for a stack (`<workspace>/.minicode/stacks/<name>.json`
    /// or `~/.config/minicode/stacks/<name>.json`), if present on disk.
    pub fn resolve_stack_db_path(
        workspace_root: Option<&Path>,
        stack_name: &str,
    ) -> Option<PathBuf> {
        Self::ensure_global_stack_db_seeded();
        let filename = format!("{}.json", stack_name.trim().to_lowercase());
        if let Some(ws) = workspace_root {
            for sub in &[".minicode", ".minikit"] {
                let candidate = ws.join(sub).join("stacks").join(&filename);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        if let Some(global_dir) = Self::global_stacks_db_dir() {
            let candidate = global_dir.join(&filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Returns the count of built-in stacks embedded in the binary.
    pub fn builtin_stack_count() -> usize {
        BUILTIN_STACKS_CACHE.len()
    }

    /// Returns all available stacks, prioritizing workspace-local (`.minicode/stacks/`) and
    /// user-global (`~/.config/minicode/stacks/`) database JSONs over embedded fallback definitions.
    pub fn get_all_stacks() -> Vec<Stack> {
        Self::get_all_stacks_for(None)
    }

    /// Zero-allocation check for whether `(rel_path, content_trimmed)` matches an unmodified starter file
    /// in any built-in stack or loaded custom/database stack.
    /// Avoids deep-cloning `BUILTIN_STACKS_CACHE` on every `write_file`.
    pub fn is_scaffold_file_content(
        workspace_root: Option<&Path>,
        rel_path: &str,
        content_trimmed: &str,
    ) -> bool {
        let clean_rel = rel_path.trim_start_matches("./");
        let matches_scaffold_entry = |sf_path: &str, sf_content: &str| -> bool {
            if sf_path.trim_start_matches("./") != clean_rel {
                return false;
            }
            let sf_trimmed = sf_content.trim();
            if sf_trimmed == content_trimmed {
                return true;
            }
            // Also match `index.html` when `kit_stack_add` substituted `<title>minikit ...</title>` with `<title><project_name></title>`
            if clean_rel.ends_with("index.html") && sf_trimmed.contains("<title>minikit ") {
                if let (Some(s_start), Some(s_end), Some(c_start), Some(c_end)) = (
                    sf_trimmed.find("<title>"),
                    sf_trimmed.find("</title>"),
                    content_trimmed.find("<title>"),
                    content_trimmed.find("</title>"),
                ) {
                    if s_start < s_end && c_start < c_end {
                        return sf_trimmed[..s_start] == content_trimmed[..c_start]
                            && sf_trimmed[s_end..] == content_trimmed[c_end..];
                    }
                }
            }
            false
        };
        for stack in BUILTIN_STACKS_CACHE.iter() {
            for sf in &stack.files {
                if matches_scaffold_entry(&sf.path, &sf.content) {
                    return true;
                }
            }
        }
        // Check any already-cached disk stack directories without triggering disk I/O
        if let Ok(cache) = STACK_DIR_CACHE.read() {
            for entry in cache.values() {
                for stack in &entry.stacks {
                    for sf in &stack.files {
                        if matches_scaffold_entry(&sf.path, &sf.content) {
                            return true;
                        }
                    }
                }
            }
        }
        // Only inspect workspace custom stack directories if they exist on disk
        if let Some(ws) = workspace_root {
            let custom1 = ws.join(".minicode").join("stacks");
            let custom2 = ws.join(".minikit").join("stacks");
            if custom1.is_dir() || custom2.is_dir() {
                for stack in Self::get_all_stacks_for(Some(ws)) {
                    for sf in &stack.files {
                        if matches_scaffold_entry(&sf.path, &sf.content) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Returns all available stacks, loading from the local stack database in priority order:
    /// 1. Workspace `.minicode/stacks/*.json` & `.minikit/stacks/*.json`
    /// 2. Global user DB `~/.config/minicode/stacks/*.json` (auto-seeded on first run)
    /// 3. Embedded binary fallback (`BUILTIN_STACKS_CACHE`)
    pub fn get_all_stacks_for(workspace_root: Option<&Path>) -> Vec<Stack> {
        Self::ensure_global_stack_db_seeded();

        let mut stacks = BUILTIN_STACKS_CACHE.clone();
        let mut overridden_names: HashSet<String> = HashSet::new();

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
            let Some((current_mtime, current_count)) = Self::compute_dir_fingerprint(&dir) else {
                if let Ok(mut cache) = STACK_DIR_CACHE.write() {
                    cache.remove(&dir);
                }
                continue;
            };

            // Fast path: check read cache
            let cached_stacks = {
                let cache = STACK_DIR_CACHE.read().ok();
                cache.and_then(|c| {
                    c.get(&dir).and_then(|entry| {
                        if entry.last_mtime == current_mtime && entry.json_count == current_count {
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
                loaded.sort_by(|a, b| a.name.cmp(&b.name));
                if let Ok(mut cache) = STACK_DIR_CACHE.write() {
                    cache.insert(
                        dir.clone(),
                        DirectoryStackCache {
                            last_mtime: current_mtime,
                            json_count: current_count,
                            stacks: loaded.clone(),
                        },
                    );
                }
                loaded
            };

            for stack in dir_stacks {
                let lower_name = stack.name.to_lowercase();
                if overridden_names.contains(&lower_name) {
                    continue;
                }
                overridden_names.insert(lower_name);

                if let Some(existing) = stacks
                    .iter_mut()
                    .find(|s| s.name.eq_ignore_ascii_case(&stack.name))
                {
                    *existing = stack;
                } else {
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

    fn resolve_custom_stack_file(
        workspace_root: &Path,
        name: &str,
        global: bool,
        tool_name: &str,
        create_dir: bool,
    ) -> Result<(String, PathBuf)> {
        let norm = name.trim().to_lowercase();
        if norm.is_empty() || norm.contains('/') || norm.contains('\\') || norm.contains("..") {
            return Err(ToolError::InvalidArguments {
                name: tool_name.to_string(),
                reason: format!(
                    "Invalid stack name '{}': must not contain path separators or traversal",
                    name
                ),
            }
            .into());
        }

        let target_dir = if global {
            let home = dirs::home_dir().ok_or_else(|| ToolError::InvalidArguments {
                name: tool_name.to_string(),
                reason: "Could not determine home directory".to_string(),
            })?;
            home.join(".config").join("minicode").join("stacks")
        } else {
            workspace_root.join(".minicode").join("stacks")
        };

        if create_dir {
            fs::create_dir_all(&target_dir).map_err(|e| ToolError::FileOp {
                path: target_dir.display().to_string(),
                source: e,
            })?;
        }

        let file_path = target_dir.join(format!("{}.json", norm));
        Ok((norm, file_path))
    }

    /// Creates a starter custom stack template JSON in `.minicode/stacks/<name>.json` (or globally in `~/.config/minicode/stacks/<name>.json`).
    pub fn create_custom_stack(
        workspace_root: &Path,
        name: &str,
        runtime: &str,
        global: bool,
    ) -> Result<PathBuf> {
        let (norm, file_path) =
            Self::resolve_custom_stack_file(workspace_root, name, global, "kit_stack_new", true)?;
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
        let (norm, file_path) = Self::resolve_custom_stack_file(
            workspace_root,
            name,
            global,
            "kit_stack_snapshot",
            true,
        )?;

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

    pub fn extract_dependencies_from_workspace(
        workspace_root: &Path,
    ) -> (Vec<String>, Vec<String>) {
        let mut packages = Vec::new();
        let mut dev_packages = Vec::new();
        let push_unique = |vec: &mut Vec<String>, item: &str| {
            if !vec.iter().any(|existing| existing == item) {
                vec.push(item.to_string());
            }
        };

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
                            for p in pkgs.iter().filter_map(|p| p.as_str()) {
                                push_unique(&mut packages, p);
                            }
                        }
                        if let Some(dev) = val.get("dev_packages").and_then(|d| d.as_array()) {
                            for d in dev.iter().filter_map(|d| d.as_str()) {
                                push_unique(&mut dev_packages, d);
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
                        packages.extend(deps.keys().cloned());
                    }
                    if let Some(devs) = val.get("devDependencies").and_then(|d| d.as_object()) {
                        dev_packages.extend(devs.keys().cloned());
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
                        packages.extend(deps.keys().cloned());
                    }
                    if let Some(devs) = val.get("dev-dependencies").and_then(|d| d.as_table()) {
                        dev_packages.extend(devs.keys().cloned());
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
                    if !pkg_name.is_empty() {
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
                                if in_deps {
                                    packages.push(k.to_string());
                                } else if in_dev_deps {
                                    dev_packages.push(k.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        packages.sort();
        packages.dedup();
        dev_packages.sort();
        dev_packages.dedup();
        (packages, dev_packages)
    }

    fn collect_files_for_snapshot(
        workspace_root: &Path,
    ) -> Result<Vec<crate::tools::minikit::stacks::StackFile>> {
        let mut files = Vec::new();
        const MAX_FILE_SIZE: u64 = 512 * 1024; // 512 KB
        const MAX_TOTAL_FILES: usize = 500;
        const EXCLUDED_DIRS: &[&str] = &[
            ".git",
            ".minicode",
            ".onpkg",
            "node_modules",
            "target",
            "dist",
            "build",
            ".next",
            "__pycache__",
            ".venv",
            "venv",
            ".turbo",
            ".cache",
        ];

        let walker = ignore::WalkBuilder::new(workspace_root)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|entry| {
                if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) {
                    if let Some(name) = entry.file_name().to_str() {
                        return !EXCLUDED_DIRS.contains(&name);
                    }
                }
                true
            })
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

            // Exclude meta-files and lockfiles
            let should_exclude = rel_str.ends_with(".lock")
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
                match String::from_utf8(bytes) {
                    Ok(text) => {
                        files.push(crate::tools::minikit::stacks::StackFile {
                            path: rel_str,
                            content: text,
                            binary_content: None,
                        });
                    }
                    Err(e) => {
                        files.push(crate::tools::minikit::stacks::StackFile {
                            path: rel_str,
                            content: String::new(),
                            binary_content: Some(e.into_bytes()),
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
        let (norm, file_path) = Self::resolve_custom_stack_file(
            workspace_root,
            stack_name,
            global,
            "kit_stack_remove",
            false,
        )?;

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

    /// Checks if a stack name or path is a remote repository specification.
    pub fn is_remote_spec(name: &str) -> bool {
        crate::tools::minikit::remote::RemoteStackSpec::is_remote(name)
    }

    /// Inspects and displays information about a remote repository stack template.
    pub async fn show_remote(workspace_root: &Path, stack_name: &str) -> Result<String> {
        crate::tools::minikit::remote::MiniKitRemoteManager::show_remote(workspace_root, stack_name)
            .await
    }

    /// Scaffolds a stack into `target_dir`.
    pub async fn scaffold(
        workspace_root: &Path,
        stack_name: &str,
        target_dir_opt: Option<&str>,
        no_install: bool,
    ) -> Result<String> {
        if Self::is_remote_spec(stack_name) {
            return crate::tools::minikit::remote::MiniKitRemoteManager::scaffold_remote(
                workspace_root,
                stack_name,
                target_dir_opt,
                no_install,
            )
            .await;
        }

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

        let project_name = dest_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string();

        // 1. Write all template files
        for f in &stack.files {
            // Remap any legacy onpkg_docs/ path to minikit_docs/ so onpkg_docs is never created
            let clean_rel_path = if f.path.starts_with(crate::constants::ONPKG_DOCS_DIR) {
                f.path.replacen(
                    crate::constants::ONPKG_DOCS_DIR,
                    crate::constants::MINIKIT_DOCS_DIR,
                    1,
                )
            } else {
                f.path.clone()
            };
            // Normalize package documentation: documentation must always live in packages/ not skills/
            let clean_rel_path = if clean_rel_path.contains("/skills/") {
                clean_rel_path.replacen("/skills/", "/packages/", 1)
            } else if clean_rel_path.starts_with("skills/") {
                clean_rel_path.replacen("skills/", "packages/", 1)
            } else {
                clean_rel_path
            };
            let is_doc = clean_rel_path.starts_with(crate::constants::MINIKIT_DOCS_DIR);
            let file_path = if is_doc {
                workspace_root.join(&clean_rel_path)
            } else {
                dest_dir.join(&clean_rel_path)
            };

            // If it is a documentation file and already exists in workspace_root, preserve existing specs
            if is_doc && file_path.exists() {
                continue;
            }

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
                let content_to_write = if clean_rel_path.ends_with("index.html")
                    && f.content.contains("<title>minikit ")
                {
                    if let (Some(t_start), Some(t_end)) = (
                        f.content.find("<title>minikit "),
                        f.content.find("</title>"),
                    ) {
                        if t_start < t_end {
                            format!(
                                "{}<title>{}</title>{}",
                                &f.content[..t_start],
                                project_name,
                                &f.content[t_end + "</title>".len()..]
                            )
                        } else {
                            f.content.clone()
                        }
                    } else {
                        f.content.clone()
                    }
                } else {
                    f.content.clone()
                };
                fs::write(&file_path, &content_to_write).map_err(|e| ToolError::FileOp {
                    path: file_path.display().to_string(),
                    source: e,
                })?;
            }
        }

        // 2. Generate minikit.json manifest

        // 2. Dynamically resolve matching skills from the catalog using stack files & package dependencies
        let stack_file_paths: Vec<String> = stack.files.iter().map(|f| f.path.clone()).collect();
        let mut stack_packages = stack.packages.clone();
        stack_packages.extend(stack.dev_packages.clone());
        let mut active_skills = super::skills::MiniKitSkillsManager::resolve_skills_for_stack(
            &dest_dir,
            &stack_file_paths,
            &stack_packages,
        );
        active_skills.retain(|s| !s.starts_with("minikit-") && !s.starts_with("pkg-"));

        // Install resolved active workflow skills into target workspace .minicode/skills/
        let vault_store = crate::vault::store::VaultStore::new(&dest_dir);
        let skills_dir = dest_dir
            .join(crate::constants::WORKSPACE_DIR_NAME)
            .join("skills");
        for skill_name in &active_skills {
            if vault_store.load_to_project(skill_name).is_err() {
                if let Some(builtin) = super::builtin_skills::find_builtin_skill(skill_name) {
                    let target_skill_dir = skills_dir.join(builtin.name);
                    let _ = fs::create_dir_all(&target_skill_dir);
                    let target_file = target_skill_dir.join(crate::constants::SKILL_MD_FILE);
                    if !target_file.exists() {
                        let _ = fs::write(&target_file, builtin.content);
                    }
                }
            }
        }

        let manifest = serde_json::json!({
            "name": project_name,
            "version": "0.1.0",
            "runtime": stack.runtime,
            "package_manager": stack.runtime,
            "stack": stack.name,
            "description": stack.description,
            "packages": stack.packages,
            "dev_packages": stack.dev_packages,
            "active_skills": active_skills.clone()
        });

        let manifest_path = dest_dir.join(crate::constants::MINIKIT_MANIFEST_FILE);
        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        fs::write(&manifest_path, &manifest_json).ok();
        if dest_dir != workspace_root {
            let root_manifest = workspace_root.join(crate::constants::MINIKIT_MANIFEST_FILE);
            if !root_manifest.exists() {
                fs::write(&root_manifest, &manifest_json).ok();
            }
        }

        let has_lucide = stack
            .files
            .iter()
            .any(|f| f.content.contains("lucide-react"))
            || stack.packages.iter().any(|d| d == "lucide-react")
            || stack.dev_packages.iter().any(|d| d == "lucide-react");
        let has_frontend = stack.files.iter().any(|f| {
            f.path.ends_with(".tsx")
                || f.path.ends_with(".jsx")
                || f.path.ends_with(".vue")
                || f.path.ends_with(".html")
        });
        let has_react_components = stack
            .files
            .iter()
            .any(|f| f.path.ends_with(".tsx") || f.path.ends_with(".jsx"));
        let is_static_web =
            stack.files.iter().any(|f| f.path == "index.html") && !has_react_components;
        let starter_code_files: Vec<String> = stack
            .files
            .iter()
            .filter(|f| {
                f.binary_content.is_none()
                    && !f.path.starts_with("minikit_docs/")
                    && !f.path.starts_with("onpkg_docs/")
                    && !f.path.starts_with(".minicode/")
            })
            .take(24)
            .map(|f| format!("`{}`", f.path))
            .collect();

        // 3. Generate .minicode/minicode.md canonical workspace instructions
        let mut convention_rules = vec![
            "All project specifications and task tracking live under `minikit_docs/`.".to_string(),
            format!("Use `{}` as the runtime / package manager.", stack.runtime),
            format!("Follow standard {} best practices.", stack.runtime),
            "Code Modification: Use `patch_file` for targeted edits to single files. Use `replace_in_files` ONLY when applying the exact same pattern replacement across 2 or more files simultaneously.".to_string(),
        ];
        if has_frontend {
            convention_rules.push("Lighting & Glows: Use CSS radial gradients instead of massive blur filters (`blur-[128px]`) to ensure clean rendering in headless browsers.".to_string());
            convention_rules.push("UI & Layout: Fixed navbars require adequate hero top-padding (`pt-28`+ or equivalent CSS top padding) to avoid title overlap. Animations must initialize with visible content.".to_string());
        }
        if has_lucide {
            convention_rules.push("Brand Icons: Modern `lucide-react` does NOT export brand icons (Github, Twitter, Discord, Google, LinkedIn). Render brand icons as inline `<svg>` elements with `viewBox=\"0 0 24 24\"` or standard UI icons (`Globe`, `Terminal`, `Share2`).".to_string());
        }
        if has_react_components {
            convention_rules.push("Modular Architecture: Avoid monolithic single-file pages. Split landing pages into modular section components (<200 lines each) under `src/components/`.".to_string());
            convention_rules.push("React Hooks v7 Lint (`react-hooks/set-state-in-effect`): Never call `setState` synchronously inside a `useEffect` body (e.g. `if (cond) { setDone(true); return; }`). Initialize state lazily via `useState(() => ...)` or derive values during render, and only call `setState` inside async/timer/observer/event callbacks.".to_string());
        } else if is_static_web {
            convention_rules.push("Starter Files & Modular ES6: Update or overwrite the scaffolded starter files (`index.html`, `style.css`, `app.js`) directly instead of creating duplicate filenames (do not create `styles.css` alongside `style.css` or leave an unused root `app.js` when adding ES modules under `js/`).".to_string());
        }
        let numbered_conventions = convention_rules
            .iter()
            .enumerate()
            .map(|(i, rule)| format!("{}. {}", i + 1, rule))
            .collect::<Vec<_>>()
            .join("\n");

        let minicode_md = format!(
            "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
            > Scaffolded with `minicode` + `MiniKit` native stack engine.\n\n\
            ## Project Summary\n\
            - **Name:** `{}`\n\
            - **Stack:** `{}`\n\
            - **Runtime / Package Manager:** `{}`\n\
            - **Description:** {}\n\n\
            ## Architecture & Conventions\n\
            {}\n",
            project_name,
            project_name,
            stack.name,
            stack.runtime,
            stack.description,
            numbered_conventions
        );
        let dest_minicode_dir = dest_dir.join(crate::constants::WORKSPACE_DIR_NAME);
        let _ = fs::create_dir_all(&dest_minicode_dir);
        fs::write(
            dest_dir.join(crate::constants::MINICODE_MD_FILE),
            &minicode_md,
        )
        .ok();
        if dest_dir != workspace_root {
            let root_minicode_md = workspace_root.join(crate::constants::MINICODE_MD_FILE);
            if !root_minicode_md.exists() {
                let _ =
                    fs::create_dir_all(workspace_root.join(crate::constants::WORKSPACE_DIR_NAME));
                fs::write(&root_minicode_md, &minicode_md).ok();
            }
        }

        // 4. Generate initial workflow docs under dest_dir
        let docs_dir = dest_dir.join(crate::constants::MINIKIT_DOCS_DIR);
        super::sync::MiniKitSyncEngine::ensure_workflow_docs(
            &docs_dir,
            &project_name,
            &stack.runtime,
        );
        if dest_dir != workspace_root {
            let root_docs = workspace_root.join(crate::constants::MINIKIT_DOCS_DIR);
            if !root_docs.exists() {
                super::sync::MiniKitSyncEngine::ensure_workflow_docs(
                    &root_docs,
                    &project_name,
                    &stack.runtime,
                );
            }
        }

        // 5. Post-scaffold install hooks
        let mut install_msg = String::new();
        if !no_install {
            install_msg = Self::run_package_installer(&stack.runtime, &dest_dir).await;
        }

        let has_starter_layout = stack.files.iter().any(|f| {
            f.path.contains("RootLayout") || f.path.contains("Navbar") || f.path.contains("Footer")
        });
        let mut guidance_lines = Vec::new();
        if has_starter_layout {
            guidance_lines.push("• ⚡ Layout Architecture: This starter template already includes a global layout wrapper (e.g. `src/layouts/RootLayout.tsx` with `<Navbar />` and `<Footer />`).\n  DO NOT duplicate `<Navbar />` or `<Footer />` inside page components (`HomePage.tsx`). Modify `RootLayout.tsx` or the existing components directly to customize navigation.".to_string());
        }
        let starter_files_display = if starter_code_files.is_empty() {
            "`index.html`, `src/main.rs`".to_string()
        } else {
            starter_code_files.join(", ")
        };
        if is_static_web {
            guidance_lines.push(format!(
                "• 📝 Starter Files & Overwrite:\n  - Existing files: {}\n  - Read multiple starter files in one turn with `batch_read(paths=[...])` (never use `exec_cmd(\"cat/find ...\")`).\n  - Update or overwrite these exact files (`overwrite: true` in `write_file` or `patch_file`) instead of creating duplicate filenames.",
                starter_files_display
            ));
        } else {
            guidance_lines.push(format!(
                "• 📝 Starter Files & Overwrite:\n  - Existing files: {}\n  - Read multiple starter files in one turn with `batch_read(paths=[...])` (never use `exec_cmd(\"cat/find ...\")`).\n  - When replacing starter files, pass `overwrite: true` in `write_file` or use `patch_file`.",
                starter_files_display
            ));
        }

        if has_lucide {
            guidance_lines.push("• 🎨 Brand Icons: `lucide-react` does NOT export brand icons (`Github`, `Twitter`, `Discord`).\n  Always render brand icons as inline `<svg>` elements or use standard UI icons (`Code`, `Globe`, `Share2`, `Terminal`).".to_string());
        }

        if has_frontend {
            guidance_lines.push("• 💡 Lighting & Ambient Glows: In headless browser / CI environments, extreme CSS blur filters (e.g. `blur-[128px]`) are dropped by software rasterizers (SwiftShader).\n  Build ambient background glows with CSS radial gradients (e.g. `radial-gradient(circle, rgba(...) 0%, transparent 70%)`), which render crisply in both hardware GPU and headless capture.".to_string());
            guidance_lines.push("• 📐 Layout & Fixed Navbar Spacing: When using fixed or floating top navbars (`fixed top-0`, `fixed top-4`), add adequate top padding to the page or hero section (`pt-28` or `pt-32`) so headings are never occluded.".to_string());
            if has_react_components {
                guidance_lines.push("• 🎬 Animation & GSAP Invariant: Register plugins globally ONCE in the entrypoint (`main.tsx` or `App.tsx`) with `gsap.registerPlugin(ScrollTrigger)`.\n  In child components, only import what is actively used (avoid TS6133 unused imports) and keep initial element states visible for headless screenshots.".to_string());
            }
        }

        // Handoff: point at the natural next subsystems so one tool leads to the next.
        let framework_hint = if stack
            .files
            .iter()
            .any(|f| f.path.ends_with(".tsx") || f.path.ends_with(".jsx"))
        {
            Some("react")
        } else if stack.files.iter().any(|f| f.path.ends_with(".html")) {
            Some("css")
        } else {
            None
        };
        // Ensure .gitignore in dest_dir excludes .minicode/, minikit_docs/, and minikit.json so CSS/bundler scanners
        // (such as Tailwind v4 oxide) and git status do not index agent working memory, manifests, or skill docs.
        let gitignore_path = dest_dir.join(".gitignore");
        if gitignore_path.exists() {
            if let Ok(mut gi) = std::fs::read_to_string(&gitignore_path) {
                let mut changed = false;
                for entry in [".minicode/", "minikit_docs/", "minikit.json"] {
                    if !gi
                        .lines()
                        .any(|l| l.trim() == entry || l.trim() == entry.trim_end_matches('/'))
                    {
                        if !gi.ends_with('\n') {
                            gi.push('\n');
                        }
                        gi.push_str(entry);
                        gi.push('\n');
                        changed = true;
                    }
                }
                if changed {
                    let _ = std::fs::write(&gitignore_path, gi);
                }
            }
        }

        // Automatically initialize a Git repository if neither dest_dir nor workspace_root has one yet,
        // so `power_review`, `git diff`, and auto-commits work out of the box on newly scaffolded projects.
        if !dest_dir.join(".git").exists() && !workspace_root.join(".git").exists() {
            let _ = std::process::Command::new("git")
                .arg("init")
                .current_dir(&dest_dir)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }

        guidance_lines.push("Next steps:".to_string());
        guidance_lines.push(
            "1. Read .minicode/minicode.md and the generated entry files before editing; extend the template instead of rewriting it.".to_string(),
        );
        guidance_lines.push(
            "2. Right-sized planning: call create_plan (2-3 steps for small tasks, 4-8+ verifiable steps for medium/large tasks), then update_progress as each step finishes.".to_string(),
        );
        if let Some(fw) = framework_hint {
            guidance_lines.push(format!(
                "3. UI: block_palettes for colour tokens and block_search(query=\"hero\", framework=\"{}\", include_code=true) for section components.",
                fw
            ));
            if fw == "react" {
                guidance_lines.push(
                    "• Brand Icons: lucide-react does NOT export brand icons (Github, Twitter, Discord). Use inline <svg> or UI icons (Globe, Terminal).".to_string()
                );
                guidance_lines.push(
                    "• Modular Sections: Split page sections into separate components under src/components/ (<200 lines each).".to_string()
                );
            }
        }
        guidance_lines.push(
            "4. Run it with minitask(action=\"start\", ...) using the template's dev script and verify with browser_navigate or tests.".to_string(),
        );

        let layout_notice = if guidance_lines.is_empty() {
            String::new()
        } else {
            format!("\n{}", guidance_lines.join("\n"))
        };

        let skills_summary = if active_skills.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{} (loaded in `.minicode/skills/` & active in session)",
                active_skills.join(", ")
            )
        };

        Ok(format!(
            "✔ Successfully scaffolded stack `{}` in `{}`\n\
            • Files created: {} files\n\
            • Manifest: minikit.json, .minicode/minicode.md, minikit_docs/\n\
            • Active Skills: {}\n\
            • Runtime: {}\n{}{}",
            stack.name,
            dest_dir.display(),
            files_count,
            skills_summary,
            stack.runtime,
            install_msg,
            layout_notice
        ))
    }

    /// Automatically runs the best package installer for the runtime asynchronously with a timeout.
    pub async fn run_package_installer(runtime: &str, dest_dir: &Path) -> String {
        let (cmd, args) = match runtime {
            "bun" => ("bun", vec!["install", "--no-progress"]),
            "uv" => ("uv", vec!["sync"]),
            "cargo" => ("cargo", vec!["check"]),
            "flutter" => ("flutter", vec!["pub", "get"]),
            "npm" => ("npm", vec!["install", "--no-audit", "--no-fund"]),
            "pnpm" => ("pnpm", vec!["install"]),
            "yarn" => ("yarn", vec!["install"]),
            _ => return "\nℹ Skipped auto-install (unknown runtime)".to_string(),
        };

        let mut command = tokio::process::Command::new(cmd);
        command
            .args(&args)
            .current_dir(dest_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        match tokio::time::timeout(std::time::Duration::from_secs(60), command.output()).await {
            Ok(Ok(output)) if output.status.success() => {
                format!("• Package install: ✔ `{}` completed successfully.", cmd)
            }
            Ok(Ok(output)) => {
                let err = String::from_utf8_lossy(&output.stderr);
                format!(
                    "• Package install: ⚠ `{} {}` exited with error: {}",
                    cmd,
                    args.join(" "),
                    err.lines().next().unwrap_or("failed")
                )
            }
            Ok(Err(_)) => {
                format!(
                    "• Package install: ℹ `{}` CLI not found on system. Run `{} {}` manually.",
                    cmd,
                    cmd,
                    args.join(" ")
                )
            }
            Err(_) => {
                format!(
                    "• Package install: ⚠ `{} {}` timed out after 60s. Run `{}` manually if needed.",
                    cmd,
                    args.join(" "),
                    cmd
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
        assert!(target.join(crate::constants::MINICODE_MD_FILE).exists());
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

    #[tokio::test]
    async fn test_scaffold_static_website_tailors_minicode_md_and_package_docs() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();

        let res = MiniKitScaffolder::scaffold(ws, "static-website", None, true)
            .await
            .unwrap();
        assert!(res.contains("Successfully scaffolded stack `static-website`"));
        assert!(res.contains("`index.html`, `style.css`, `app.js`"));
        assert!(!res.contains("lucide-react"));

        let minicode_md = fs::read_to_string(ws.join(crate::constants::MINICODE_MD_FILE)).unwrap();
        assert!(minicode_md.contains("Starter Files & Modular ES6"));
        assert!(!minicode_md.contains("lucide-react"));
        assert!(!minicode_md.contains("src/components/"));

        assert!(!ws.join("minikit_docs/packages/minikit_react.md").exists());
    }

    #[test]
    fn test_seed_builtin_stacks_and_database_first_override() {
        let temp = TempDir::new().unwrap();
        let ws = temp.path();
        let db_dir = ws.join(".minicode").join("stacks");

        // 1. Seed all built-in stacks into db_dir
        let seeded = MiniKitScaffolder::seed_builtin_stacks_to_dir(&db_dir, None, false).unwrap();
        assert_eq!(seeded.len(), MiniKitScaffolder::builtin_stack_count());
        assert!(db_dir.join("react-vite.json").exists());
        assert!(db_dir.join("react-vite-full.json").exists());

        // 2. Customize react-vite.json on disk
        let rv_path = db_dir.join("react-vite.json");
        let mut rv_stack: Stack =
            serde_json::from_str(&fs::read_to_string(&rv_path).unwrap()).unwrap();
        rv_stack.description = "Customized React Vite from Local Stack DB".to_string();
        fs::write(&rv_path, serde_json::to_string_pretty(&rv_stack).unwrap()).unwrap();
        MiniKitScaffolder::clear_cache();

        // 3. Re-seeding with overwrite=false must NOT overwrite user customizations
        let reseeded = MiniKitScaffolder::seed_builtin_stacks_to_dir(&db_dir, None, false).unwrap();
        assert!(reseeded.is_empty());

        // 4. Database-First priority: find_stack_in_workspace returns the customized disk stack
        let loaded = MiniKitScaffolder::find_stack_in_workspace(ws, "react-vite").unwrap();
        assert_eq!(
            loaded.description,
            "Customized React Vite from Local Stack DB"
        );

        // 5. Resetting with overwrite=true restores canonical built-in definition
        let reset =
            MiniKitScaffolder::seed_builtin_stacks_to_dir(&db_dir, Some("react"), true).unwrap();
        assert_eq!(reset.len(), 1);
        let restored = MiniKitScaffolder::find_stack_in_workspace(ws, "react-vite").unwrap();
        assert_ne!(
            restored.description,
            "Customized React Vite from Local Stack DB"
        );
    }

    #[test]
    fn test_react_vite_minimal_vs_react_vite_full_separation() {
        let minimal = BUILTIN_STACKS_CACHE
            .iter()
            .find(|s| s.name == "react-vite")
            .unwrap();
        let full = BUILTIN_STACKS_CACHE
            .iter()
            .find(|s| s.name == "react-vite-full")
            .unwrap();

        // Minimal react-vite must NOT include full-stack state/data libraries
        assert!(!minimal.packages.iter().any(|p| p == "zustand"));
        assert!(!minimal.packages.iter().any(|p| p == "axios"));
        assert!(!minimal
            .packages
            .iter()
            .any(|p| p == "@tanstack/react-query"));
        assert!(!minimal.packages.iter().any(|p| p == "zod"));
        assert!(minimal.packages.iter().any(|p| p == "react"));
        assert!(minimal.packages.iter().any(|p| p == "lucide-react"));
        assert!(minimal.packages.iter().any(|p| p == "react-router-dom"));
        assert!(minimal.packages.iter().any(|p| p == "tailwindcss"));

        // Minimal react-vite files must not contain QueryProvider or Zustand store files
        assert!(!minimal
            .files
            .iter()
            .any(|f| f.path.contains("QueryProvider")));
        assert!(!minimal
            .files
            .iter()
            .any(|f| f.path.starts_with("src/store/")));
        assert!(!minimal.files.iter().any(|f| f.path.starts_with("src/api/")));

        // Full react-vite-full MUST include them
        assert!(full.packages.iter().any(|p| p == "zustand"));
        assert!(full.packages.iter().any(|p| p == "axios"));
        assert!(full.packages.iter().any(|p| p == "@tanstack/react-query"));
        assert!(full.packages.iter().any(|p| p == "zod"));
        assert!(full.files.iter().any(|f| f.path.contains("QueryProvider")));
    }
}
