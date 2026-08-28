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

        let onpkg_json_path = workspace_root.join("onpkg.json");
        let mut manifest: serde_json::Value = if onpkg_json_path.exists() {
            let content = fs::read_to_string(&onpkg_json_path).unwrap_or_default();
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
            &onpkg_json_path,
            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
        )
        .ok();

        // 2. Refresh AGENTS.md if missing
        let agents_md_path = workspace_root.join("AGENTS.md");
        if !agents_md_path.exists() {
            let agents_md = format!(
                "# {} — Agent Guidelines & Repository Instructions 🧠\n\n\
                > Synchronized with `minicode` + `onpkg`.\n\n\
                ## Project Summary\n\
                - **Name:** `{}`\n\
                - **Runtime:** `{}`\n\
                - **Package Manager:** `{}`\n\n\
                ## Documentation & Guidelines\n\
                - Follow active rules under `onpkg_docs/`.\n",
                project_name, project_name, runtime, package_manager
            );
            fs::write(&agents_md_path, agents_md).ok();
        }

        // 3. Ensure onpkg_docs/ directory exists
        let docs_dir = workspace_root.join("onpkg_docs");
        fs::create_dir_all(&docs_dir).ok();

        Ok(format!(
            "✔ Synchronized `{}` project manifest:\n\
            • Manifest: onpkg.json (Runtime: `{}`, Package Manager: `{}`)\n\
            • Instructions: AGENTS.md\n\
            • Documentation: onpkg_docs/",
            project_name, runtime, package_manager
        ))
    }
}
