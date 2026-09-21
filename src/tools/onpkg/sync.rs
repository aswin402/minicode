use crate::error::Result;
use std::fs;
use std::path::Path;

/// Native engine for scanning workspace dependencies, updating `onpkg.json`, and refreshing `AGENTS.md`.
pub struct OnpkgSyncEngine;

impl OnpkgSyncEngine {
    /// Detects project runtime and primary configuration files.
    pub fn detect_runtime(workspace_root: &Path) -> (&'static str, &'static str) {
        if workspace_root.join("Cargo.toml").exists() {
            ("rust", "cargo")
        } else if workspace_root.join("pubspec.yaml").exists() {
            ("flutter", "flutter")
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            ("python", "uv")
        } else if workspace_root.join("bun.lockb").exists()
            || workspace_root.join("bun.lock").exists()
        {
            ("bun", "bun")
        } else if workspace_root.join("pnpm-lock.yaml").exists() {
            ("node", "pnpm")
        } else if workspace_root.join("yarn.lock").exists() {
            ("node", "yarn")
        } else if workspace_root.join("package.json").exists() {
            ("node", "npm")
        } else {
            ("generic", "custom")
        }
    }

    /// Performs native spec-driven synchronization.
    pub fn sync(workspace_root: &Path) -> Result<String> {
        let (runtime, package_manager) = Self::detect_runtime(workspace_root);
        let project_name = workspace_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        let manifest_path = super::default_manifest_path(workspace_root);
        let manifest_filename = manifest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minikit.json");

        let mut manifest: serde_json::Value = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if manifest.get("name").is_none() {
            manifest["name"] = serde_json::json!(project_name);
        }
        manifest["runtime"] = serde_json::json!(runtime);
        manifest["package_manager"] = serde_json::json!(package_manager);

        if manifest.get("active_skills").is_none() {
            manifest["active_skills"] = serde_json::json!([runtime]);
        }

        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
        )
        .ok();

        // 2. Refresh AGENTS.md if missing
        let docs_dir = super::resolve_docs_dir(workspace_root);
        let docs_dir_name = docs_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minikit_docs");

        let agents_md_path = workspace_root.join("AGENTS.md");
        if !agents_md_path.exists() {
            let agents_md = format!(
                "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
                > Synchronized with `minicode` + `MiniKit`.\n\n\
                ## Project Summary\n\
                - **Name:** `{}`\n\
                - **Runtime:** `{}`\n\
                - **Package Manager:** `{}`\n\n\
                ## Documentation & Guidelines\n\
                - Follow active rules under `{}/`.\n",
                project_name, project_name, runtime, package_manager, docs_dir_name
            );
            fs::write(&agents_md_path, agents_md).ok();
        }

        // 3. Ensure docs directory exists and synchronize OKF v0.2 index and log
        fs::create_dir_all(&docs_dir).ok();

        crate::context::okf::OkfManager::generate_index_md(&docs_dir).ok();
        crate::context::okf::OkfManager::append_log_entry(
            &docs_dir,
            &format!("minicode/v{}", env!("CARGO_PKG_VERSION")),
            "SYNC",
            manifest_filename,
            &format!("Synchronized project manifest and OKF knowledge catalog (Runtime: {}, Package Manager: {})", runtime, package_manager),
        ).ok();

        Ok(format!(
            "✔ Synchronized `{}` project manifest:\n\
            • Manifest: {} (Runtime: `{}`, Package Manager: `{}`)\n\
            • Instructions: AGENTS.md\n\
            • Documentation & OKF Catalog: {}/ (index.md & log.md updated)",
            project_name, manifest_filename, runtime, package_manager, docs_dir_name
        ))
    }
}
