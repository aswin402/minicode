use crate::error::{Result, ToolError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Metadata for a resolved package across npm, PyPI, crates.io, or pub.dev
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub runtime: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
}

/// Upgraded multi-ecosystem package registry client and manifest updater.
pub struct PkgRegistry {
    client: reqwest::Client,
}

impl Default for PkgRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PkgRegistry {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("minicode/0.3.39 (https://github.com/aswin402/minicode)")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }

    /// Auto-detects the project's primary package ecosystem from workspace files.
    pub fn detect_runtime(workspace_root: &Path) -> String {
        let onpkg_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
        if let Ok(content) = fs::read_to_string(&onpkg_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(r) = val.get("runtime").and_then(|v| v.as_str()) {
                    match r {
                        "bun" | "node" | "npm" | "pnpm" | "yarn" => return "npm".to_string(),
                        "uv" | "python" | "pip" => return "pypi".to_string(),
                        "cargo" | "rust" => return "cargo".to_string(),
                        "flutter" | "dart" => return "pub".to_string(),
                        _ => {}
                    }
                }
            }
        }

        if workspace_root.join("package.json").exists() {
            "npm".to_string()
        } else if workspace_root.join("Cargo.toml").exists() {
            "cargo".to_string()
        } else if workspace_root.join("pyproject.toml").exists()
            || workspace_root.join("requirements.txt").exists()
        {
            "pypi".to_string()
        } else if workspace_root.join("pubspec.yaml").exists() {
            "pub".to_string()
        } else {
            "npm".to_string()
        }
    }

    /// Fetches verified package metadata and latest version from the upstream registry.
    pub async fn fetch_info(
        &self,
        name: &str,
        runtime_opt: Option<&str>,
        workspace_root: &Path,
    ) -> Result<PkgInfo> {
        let clean_name = name.trim();
        if clean_name.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: "Package name cannot be empty".to_string(),
            }
            .into());
        }

        let runtime = runtime_opt
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| Self::detect_runtime(workspace_root));

        match runtime.as_str() {
            "npm" | "bun" | "node" | "pnpm" | "yarn" => self.fetch_npm(clean_name).await,
            "pypi" | "pip" | "python" | "uv" => self.fetch_pypi(clean_name).await,
            "cargo" | "rust" => self.fetch_cargo(clean_name).await,
            "pub" | "dart" | "flutter" => self.fetch_pub(clean_name).await,
            _ => Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: format!(
                    "Unsupported runtime '{}'. Supported: npm, pypi, cargo, pub",
                    runtime
                ),
            }
            .into()),
        }
    }

    /// Adds a package to the workspace's manifest file and synchronizes onpkg.json.
    pub async fn add_to_project(
        &self,
        workspace_root: &Path,
        name: &str,
        version_opt: Option<&str>,
        runtime_opt: Option<&str>,
        is_dev: bool,
    ) -> Result<String> {
        let clean_name = name.trim();
        let runtime = runtime_opt
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| Self::detect_runtime(workspace_root));

        // If version is omitted, fetch latest verified version from registry
        let (version, pkg_info) = match version_opt {
            Some(v) if !v.trim().is_empty() => (v.trim().to_string(), None),
            _ => {
                let info = self
                    .fetch_info(clean_name, Some(&runtime), workspace_root)
                    .await?;
                (info.version.clone(), Some(info))
            }
        };

        // 1. Inject into project native manifest
        match runtime.as_str() {
            "npm" | "bun" | "node" | "pnpm" | "yarn" => {
                Self::add_to_npm_manifest(workspace_root, clean_name, &version, is_dev)?;
            }
            "cargo" | "rust" => {
                Self::add_to_cargo_manifest(workspace_root, clean_name, &version, is_dev)?;
            }
            "pypi" | "python" | "uv" | "pip" => {
                Self::add_to_python_manifest(workspace_root, clean_name, &version, is_dev)?;
            }
            "pub" | "flutter" | "dart" => {
                Self::add_to_flutter_manifest(workspace_root, clean_name, &version, is_dev)?;
            }
            _ => {}
        }

        // 2. Update onpkg.json manifest
        Self::add_to_onpkg_manifest(workspace_root, clean_name, is_dev)?;

        // 3. Trigger onpkg sync engine
        crate::tools::onpkg::sync::OnpkgSyncEngine::sync(workspace_root).ok();

        let desc = pkg_info
            .map(|i| format!("\nDescription: {}", i.description))
            .unwrap_or_default();

        Ok(format!(
            "✔ Successfully added `{}` v{} ({}) to project dependencies.{}",
            clean_name,
            version,
            if is_dev { "dev" } else { "production" },
            desc
        ))
    }

    async fn fetch_npm(&self, name: &str) -> Result<PkgInfo> {
        let url = format!("https://registry.npmjs.org/{}", name);
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                ToolError::CommandExec(format!("Failed to reach npm registry: {}", e))
            })?;

        if !resp.status().is_success() {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: format!("npm package '{}' not found (HTTP {})", name, resp.status()),
            }
            .into());
        }

        let data: NpmResponse = resp.json().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to parse npm registry JSON: {}", e))
        })?;

        let latest_tag = data.dist_tags.get("latest").cloned().unwrap_or_default();
        let pkg = data.versions.get(&latest_tag).cloned().unwrap_or_default();

        Ok(PkgInfo {
            name: name.to_string(),
            version: pkg.version.unwrap_or(latest_tag),
            description: pkg.description.unwrap_or_default(),
            runtime: "npm".to_string(),
            homepage: pkg.homepage,
            repository: pkg.repository.and_then(|r| r.url),
            license: pkg.license,
        })
    }

    async fn fetch_pypi(&self, name: &str) -> Result<PkgInfo> {
        let url = format!("https://pypi.org/pypi/{}/json", name);
        let resp =
            self.client.get(&url).send().await.map_err(|e| {
                ToolError::CommandExec(format!("Failed to reach PyPI registry: {}", e))
            })?;

        if !resp.status().is_success() {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: format!("PyPI package '{}' not found (HTTP {})", name, resp.status()),
            }
            .into());
        }

        let data: PyPIResponse = resp.json().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to parse PyPI registry JSON: {}", e))
        })?;

        let info = data.info;
        Ok(PkgInfo {
            name: info.name,
            version: info.version,
            description: info.summary.unwrap_or_default(),
            runtime: "pypi".to_string(),
            homepage: info.home_page,
            repository: info.project_urls.and_then(|u| u.get("Source").cloned()),
            license: info.license,
        })
    }

    async fn fetch_cargo(&self, name: &str) -> Result<PkgInfo> {
        let url = format!("https://crates.io/api/v1/crates/{}", name);
        let resp = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                "minicode/0.3.39 (https://github.com/aswin402/minicode)",
            )
            .send()
            .await
            .map_err(|e| {
                ToolError::CommandExec(format!("Failed to reach crates.io registry: {}", e))
            })?;

        if !resp.status().is_success() {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: format!(
                    "crates.io crate '{}' not found (HTTP {})",
                    name,
                    resp.status()
                ),
            }
            .into());
        }

        let data: CargoResponse = resp.json().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to parse crates.io registry JSON: {}", e))
        })?;

        let crate_data = data.crate_data;
        Ok(PkgInfo {
            name: crate_data.name,
            version: crate_data.max_version,
            description: crate_data.description.unwrap_or_default(),
            runtime: "cargo".to_string(),
            homepage: crate_data.homepage,
            repository: crate_data.repository,
            license: crate_data.license,
        })
    }

    async fn fetch_pub(&self, name: &str) -> Result<PkgInfo> {
        let url = format!("https://pub.dev/api/packages/{}", name);
        let resp = self.client.get(&url).send().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to reach pub.dev registry: {}", e))
        })?;

        if !resp.status().is_success() {
            return Err(ToolError::InvalidArguments {
                name: "onpkg_pkg_info".to_string(),
                reason: format!(
                    "pub.dev package '{}' not found (HTTP {})",
                    name,
                    resp.status()
                ),
            }
            .into());
        }

        let data: PubResponse = resp.json().await.map_err(|e| {
            ToolError::CommandExec(format!("Failed to parse pub.dev registry JSON: {}", e))
        })?;

        let pubspec = data.latest.pubspec;
        Ok(PkgInfo {
            name: pubspec.name,
            version: pubspec.version,
            description: pubspec.description.unwrap_or_default(),
            runtime: "pub".to_string(),
            homepage: pubspec.homepage,
            repository: None,
            license: None,
        })
    }

    fn add_to_npm_manifest(
        workspace_root: &Path,
        name: &str,
        version: &str,
        is_dev: bool,
    ) -> Result<()> {
        let pkg_path = workspace_root.join("package.json");
        let mut root: serde_json::Value = if pkg_path.exists() {
            let content = fs::read_to_string(&pkg_path).map_err(|e| ToolError::FileOp {
                path: pkg_path.display().to_string(),
                source: e,
            })?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({
                "name": workspace_root.file_name().and_then(|n| n.to_str()).unwrap_or("app"),
                "version": "0.1.0",
            })
        };

        let target_key = if is_dev {
            "devDependencies"
        } else {
            "dependencies"
        };
        if root.get(target_key).is_none() {
            root[target_key] = serde_json::json!({});
        }

        let ver_spec = if version.starts_with('^')
            || version.starts_with('~')
            || version.starts_with('>')
            || version.starts_with('=')
        {
            version.to_string()
        } else {
            format!("^{}", version)
        };

        if let Some(deps) = root.get_mut(target_key).and_then(|d| d.as_object_mut()) {
            deps.insert(name.to_string(), serde_json::Value::String(ver_spec));
        }

        let out_json = serde_json::to_string_pretty(&root).map_err(|e| {
            ToolError::CommandExec(format!("Failed to serialize package.json: {}", e))
        })?;
        fs::write(&pkg_path, out_json).map_err(|e| ToolError::FileOp {
            path: pkg_path.display().to_string(),
            source: e,
        })?;

        Ok(())
    }

    fn add_to_cargo_manifest(
        workspace_root: &Path,
        name: &str,
        version: &str,
        is_dev: bool,
    ) -> Result<()> {
        let cargo_path = workspace_root.join("Cargo.toml");
        if !cargo_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&cargo_path).map_err(|e| ToolError::FileOp {
            path: cargo_path.display().to_string(),
            source: e,
        })?;

        let target_section = if is_dev {
            "[dev-dependencies]"
        } else {
            "[dependencies]"
        };
        let dep_line = format!("{} = \"{}\"", name, version);

        if content.contains(&format!("{} =", name)) {
            return Ok(());
        }

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut inserted = false;

        for (i, line) in lines.iter().enumerate() {
            if line.trim() == target_section {
                lines.insert(i + 1, dep_line.clone());
                inserted = true;
                break;
            }
        }

        if !inserted {
            lines.push(String::new());
            lines.push(target_section.to_string());
            lines.push(dep_line);
        }

        fs::write(&cargo_path, lines.join("\n") + "\n").map_err(|e| ToolError::FileOp {
            path: cargo_path.display().to_string(),
            source: e,
        })?;

        Ok(())
    }

    fn add_to_python_manifest(
        workspace_root: &Path,
        name: &str,
        version: &str,
        _is_dev: bool,
    ) -> Result<()> {
        let req_path = workspace_root.join("requirements.txt");
        let dep_line = format!("{}=={}", name, version);

        if req_path.exists() {
            let content = fs::read_to_string(&req_path).unwrap_or_default();
            if !content.lines().any(|l| l.starts_with(name)) {
                let updated = format!("{}\n{}\n", content.trim_end(), dep_line);
                fs::write(&req_path, updated).ok();
            }
        } else {
            fs::write(&req_path, format!("{}\n", dep_line)).ok();
        }

        Ok(())
    }

    fn add_to_flutter_manifest(
        workspace_root: &Path,
        name: &str,
        version: &str,
        is_dev: bool,
    ) -> Result<()> {
        let pubspec_path = workspace_root.join("pubspec.yaml");
        if !pubspec_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&pubspec_path).unwrap_or_default();
        let target_section = if is_dev {
            "dev_dependencies:"
        } else {
            "dependencies:"
        };
        let dep_line = format!("  {}: ^{}", name, version);

        if content.contains(&format!("{}:", name)) {
            return Ok(());
        }

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut inserted = false;

        for (i, line) in lines.iter().enumerate() {
            if line.trim() == target_section {
                lines.insert(i + 1, dep_line.clone());
                inserted = true;
                break;
            }
        }

        if !inserted {
            lines.push(target_section.to_string());
            lines.push(dep_line);
        }

        fs::write(&pubspec_path, lines.join("\n") + "\n").ok();
        Ok(())
    }

    fn add_to_onpkg_manifest(workspace_root: &Path, name: &str, is_dev: bool) -> Result<()> {
        let onpkg_path = workspace_root.join(crate::constants::ONPKG_MANIFEST_FILE);
        if !onpkg_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&onpkg_path).unwrap_or_default();
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&content) {
            let key = if is_dev { "dev_packages" } else { "packages" };
            if val.get(key).is_none() {
                val[key] = serde_json::json!([]);
            }

            if let Some(arr) = val.get_mut(key).and_then(|a| a.as_array_mut()) {
                let name_val = serde_json::Value::String(name.to_string());
                if !arr.contains(&name_val) {
                    arr.push(name_val);
                    if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                        fs::write(&onpkg_path, pretty).ok();
                    }
                }
            }
        }

        Ok(())
    }
}

// ── JSON Response Deserialization Models ─────────────────────────────────

#[derive(Deserialize, Default)]
struct NpmResponse {
    #[serde(rename = "dist-tags")]
    dist_tags: HashMap<String, String>,
    versions: HashMap<String, NpmVersion>,
}

#[derive(Deserialize, Default, Clone)]
struct NpmVersion {
    version: Option<String>,
    description: Option<String>,
    homepage: Option<String>,
    license: Option<String>,
    repository: Option<NpmRepo>,
}

#[derive(Deserialize, Default, Clone)]
struct NpmRepo {
    url: Option<String>,
}

#[derive(Deserialize)]
struct PyPIResponse {
    info: PyPIInfo,
}

#[derive(Deserialize)]
struct PyPIInfo {
    name: String,
    version: String,
    summary: Option<String>,
    home_page: Option<String>,
    license: Option<String>,
    project_urls: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct CargoResponse {
    #[serde(rename = "crate")]
    crate_data: CargoCrate,
}

#[derive(Deserialize)]
struct CargoCrate {
    name: String,
    max_version: String,
    description: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    license: Option<String>,
}

#[derive(Deserialize)]
struct PubResponse {
    latest: PubVersion,
}

#[derive(Deserialize)]
struct PubVersion {
    pubspec: PubPubspec,
}

#[derive(Deserialize)]
struct PubPubspec {
    name: String,
    version: String,
    description: Option<String>,
    homepage: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_runtime_defaults() {
        let temp = TempDir::new().unwrap();
        assert_eq!(PkgRegistry::detect_runtime(temp.path()), "npm");

        fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(PkgRegistry::detect_runtime(temp.path()), "cargo");
    }

    #[test]
    fn test_add_to_npm_manifest() {
        let temp = TempDir::new().unwrap();
        let pkg_json = temp.path().join("package.json");
        fs::write(&pkg_json, "{\"dependencies\": {}}").unwrap();

        PkgRegistry::add_to_npm_manifest(temp.path(), "hono", "4.13.8", false).unwrap();
        let content = fs::read_to_string(&pkg_json).unwrap();
        assert!(content.contains("\"hono\": \"^4.13.8\""));

        PkgRegistry::add_to_npm_manifest(temp.path(), "typescript", "5.9.3", true).unwrap();
        let content2 = fs::read_to_string(&pkg_json).unwrap();
        assert!(content2.contains("\"typescript\": \"^5.9.3\""));
    }

    #[test]
    fn test_add_to_cargo_manifest() {
        let temp = TempDir::new().unwrap();
        let cargo_toml = temp.path().join("Cargo.toml");
        fs::write(&cargo_toml, "[package]\nname = \"foo\"\n\n[dependencies]\n").unwrap();

        PkgRegistry::add_to_cargo_manifest(temp.path(), "tokio", "1.40.0", false).unwrap();
        let content = fs::read_to_string(&cargo_toml).unwrap();
        assert!(content.contains("tokio = \"1.40.0\""));
    }
}
