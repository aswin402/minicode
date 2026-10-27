use crate::blocks::models::{
    BlockComponent, BlockFramework, BlockGradient, BlockPalette, BlockTemplate,
};
use crate::blocks::store::BlockStore;
use crate::blocks::BlockError;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
struct SeedCatalog {
    components: Vec<BlockComponent>,
    palettes: Vec<BlockPalette>,
    gradients: Vec<BlockGradient>,
    templates: Vec<BlockTemplate>,
}

/// Seeds default components, palettes, gradients, and templates into the block store.
/// Skips any items that already exist in the store by ID to protect user modifications.
/// Returns the number of newly added components.
pub fn seed_default_blocks(store: &mut BlockStore) -> Result<usize, BlockError> {
    let catalog: SeedCatalog = serde_json::from_str(include_str!("seed_catalog.json"))?;

    // Temporarily detach persistence path during batch insertion to prevent cascading disk writes
    let prev_path = store.take_persistence_path();

    let insert_res = (|| -> Result<usize, BlockError> {
        let mut seeded_count = 0;

        for comp in catalog.components {
            if store.get_component(&comp.id).is_none() {
                store.insert_component(comp)?;
                seeded_count += 1;
            }
        }

        for palette in catalog.palettes {
            if store.get_palette(&palette.id).is_none() {
                store.insert_palette(palette)?;
            }
        }

        let extra_palettes: [(&str, &str, [&str; 4], &[&str]); 4] = [
            (
                "00000000-0000-4000-8000-000000000901",
                "Obsidian Terminal Cyan",
                ["#090A0F", "#12151E", "#06B6D4", "#F8FAFC"],
                &[
                    "dark",
                    "terminal",
                    "developer",
                    "cli",
                    "obsidian",
                    "cyan",
                    "tech",
                ],
            ),
            (
                "00000000-0000-4000-8000-000000000902",
                "Carbon Emerald Matrix",
                ["#0A0D0C", "#111715", "#10B981", "#F1F5F9"],
                &[
                    "dark",
                    "terminal",
                    "developer",
                    "carbon",
                    "emerald",
                    "code",
                    "minimal",
                ],
            ),
            (
                "00000000-0000-4000-8000-000000000903",
                "Graphite Amber Telemetry",
                ["#0B0C10", "#14171F", "#F59E0B", "#F8FAFC"],
                &[
                    "dark",
                    "developer",
                    "telemetry",
                    "industrial",
                    "amber",
                    "dashboard",
                ],
            ),
            (
                "00000000-0000-4000-8000-000000000904",
                "Monochrome Editorial Slate",
                ["#09090B", "#18181B", "#38BDF8", "#FAFAFA"],
                &["dark", "minimal", "editorial", "slate", "saas", "modern"],
            ),
        ];
        for (id_str, name, colors, tags) in extra_palettes {
            if let Ok(id) = uuid::Uuid::parse_str(id_str) {
                if store.get_palette(&id).is_none() {
                    store.insert_palette(BlockPalette {
                        id,
                        name: name.to_string(),
                        colors: [
                            colors[0].to_string(),
                            colors[1].to_string(),
                            colors[2].to_string(),
                            colors[3].to_string(),
                        ],
                        tags: tags.iter().map(|t| (*t).to_string()).collect(),
                    })?;
                }
            }
        }

        for gradient in catalog.gradients {
            if store.get_gradient(&gradient.id).is_none() {
                store.insert_gradient(gradient)?;
            }
        }

        for template in catalog.templates {
            if store.get_template(&template.id).is_none() {
                store.insert_template(template)?;
            }
        }

        Ok(seeded_count)
    })();

    // Restore persistence path
    store.set_persistence_path(prev_path);

    let seeded_count = insert_res?;

    // Persist once if configured
    store.persist_if_configured()?;

    Ok(seeded_count)
}

/// Inspects the project root to detect the active frontend framework / stack.
pub fn detect_project_framework(project_root: &Path) -> Option<BlockFramework> {
    if !project_root.exists() {
        return None;
    }

    // 1. Root components.json immediately signals Shadcn UI
    if project_root.join("components.json").is_file() {
        return Some(BlockFramework::Shadcn);
    }

    // 2. Inspect package.json if present
    let pkg_json_path = project_root.join("package.json");
    if pkg_json_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut dep_keys = Vec::new();

                if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                    dep_keys.extend(deps.keys().map(|k| k.to_lowercase()));
                }
                if let Some(dev_deps) = json.get("devDependencies").and_then(|d| d.as_object()) {
                    dep_keys.extend(dev_deps.keys().map(|k| k.to_lowercase()));
                }
                if let Some(peer_deps) = json.get("peerDependencies").and_then(|d| d.as_object()) {
                    dep_keys.extend(peer_deps.keys().map(|k| k.to_lowercase()));
                }

                // Check dependencies in prioritized order:
                // a. Shadcn / Radix UI
                if dep_keys.iter().any(|k| {
                    k.contains("shadcn") || k.starts_with("@shadcn/") || k.contains("@radix-ui/")
                }) {
                    return Some(BlockFramework::Shadcn);
                }

                // b. React / Next.js
                if dep_keys.iter().any(|k| {
                    k == "react"
                        || k == "react-dom"
                        || k == "next"
                        || k.starts_with("react-")
                        || k.starts_with("@types/react")
                        || k.starts_with("next/")
                        || k.starts_with("@next/")
                }) {
                    return Some(BlockFramework::React);
                }

                // c. Svelte / SvelteKit
                if dep_keys
                    .iter()
                    .any(|k| k == "svelte" || k.starts_with("@sveltejs/") || k.contains("svelte"))
                {
                    return Some(BlockFramework::Svelte);
                }

                // d. Tailwind CSS
                if dep_keys
                    .iter()
                    .any(|k| k == "tailwindcss" || k.contains("tailwindcss"))
                {
                    return Some(BlockFramework::Tailwind);
                }

                // e. SCSS / Sass
                if dep_keys
                    .iter()
                    .any(|k| k == "sass" || k == "scss" || k.contains("sass") || k.contains("scss"))
                {
                    return Some(BlockFramework::Scss);
                }
            }
        }
    }

    // 3. Inspect minikit.json / onpkg.json if present
    for manifest_name in &["minikit.json", "minicode.json", "onpkg.json"] {
        let manifest_path = project_root.join(manifest_name);
        if manifest_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(stack) = json.get("stack").and_then(|s| s.as_str()) {
                        let s_lower = stack.to_ascii_lowercase();
                        if s_lower.contains("shadcn") {
                            return Some(BlockFramework::Shadcn);
                        }
                        if s_lower.contains("react")
                            || s_lower.contains("next")
                            || s_lower == "mern"
                            || s_lower == "pern"
                        {
                            return Some(BlockFramework::React);
                        }
                        if s_lower.contains("svelte") {
                            return Some(BlockFramework::Svelte);
                        }
                        if s_lower.contains("tailwind") {
                            return Some(BlockFramework::Tailwind);
                        }
                        if s_lower.contains("static") || s_lower.contains("html") {
                            return Some(BlockFramework::Css);
                        }
                    }
                }
            }
        }
    }

    // 4. Root config files fallback:
    // Svelte config
    if project_root.join("svelte.config.js").is_file()
        || project_root.join("svelte.config.ts").is_file()
        || project_root.join("svelte.config.cjs").is_file()
        || project_root.join("svelte.config.mjs").is_file()
    {
        return Some(BlockFramework::Svelte);
    }

    // Tailwind config
    if project_root.join("tailwind.config.js").is_file()
        || project_root.join("tailwind.config.ts").is_file()
        || project_root.join("tailwind.config.cjs").is_file()
        || project_root.join("tailwind.config.mjs").is_file()
    {
        return Some(BlockFramework::Tailwind);
    }

    // Check if any file in root starts with "tailwind.config." or "svelte.config."
    if let Ok(entries) = std::fs::read_dir(project_root) {
        let mut has_svelte = false;
        let mut has_tailwind = false;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("svelte.config.") {
                has_svelte = true;
            } else if name_str.starts_with("tailwind.config.") {
                has_tailwind = true;
            }
        }
        if has_svelte {
            return Some(BlockFramework::Svelte);
        }
        if has_tailwind {
            return Some(BlockFramework::Tailwind);
        }
    }

    if project_root.join("index.html").is_file()
        || project_root.join("styles.css").is_file()
        || project_root.join("style.css").is_file()
    {
        return Some(BlockFramework::Css);
    }

    None
}

/// Comprehensive project stack analysis for component framework and styling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStackInfo {
    pub framework: Option<BlockFramework>,
    pub has_tailwind: bool,
    pub has_scss: bool,
}

impl ProjectStackInfo {
    /// Detects the stack information from a given project root.
    pub fn detect(project_root: &Path) -> Self {
        if !project_root.exists() {
            return Self {
                framework: None,
                has_tailwind: false,
                has_scss: false,
            };
        }

        let mut has_tailwind = false;
        let mut has_scss = false;

        // Check package.json for tailwind or sass/scss
        let pkg_json_path = project_root.join("package.json");
        if pkg_json_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let mut dep_keys = Vec::new();
                    if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                        dep_keys.extend(deps.keys().map(|k| k.to_lowercase()));
                    }
                    if let Some(dev_deps) = json.get("devDependencies").and_then(|d| d.as_object())
                    {
                        dep_keys.extend(dev_deps.keys().map(|k| k.to_lowercase()));
                    }
                    if let Some(peer_deps) =
                        json.get("peerDependencies").and_then(|d| d.as_object())
                    {
                        dep_keys.extend(peer_deps.keys().map(|k| k.to_lowercase()));
                    }

                    if dep_keys
                        .iter()
                        .any(|k| k == "tailwindcss" || k.contains("tailwindcss"))
                    {
                        has_tailwind = true;
                    }
                    if dep_keys.iter().any(|k| {
                        k == "sass" || k == "scss" || k.contains("sass") || k.contains("scss")
                    }) {
                        has_scss = true;
                    }
                }
            }
        }

        // Check minikit.json / onpkg.json for tailwind packages or active_skills
        for manifest_name in &["minikit.json", "minicode.json", "onpkg.json"] {
            let manifest_path = project_root.join(manifest_name);
            if manifest_path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        for key in &["packages", "dev_packages", "active_skills"] {
                            if let Some(arr) = json.get(*key).and_then(|v| v.as_array()) {
                                if arr.iter().filter_map(|v| v.as_str()).any(|s| {
                                    let lower = s.to_ascii_lowercase();
                                    lower == "tailwind" || lower.contains("tailwindcss")
                                }) {
                                    has_tailwind = true;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check root directory files for tailwind config
        if project_root.join("tailwind.config.js").is_file()
            || project_root.join("tailwind.config.ts").is_file()
            || project_root.join("tailwind.config.cjs").is_file()
            || project_root.join("tailwind.config.mjs").is_file()
        {
            has_tailwind = true;
        }

        if let Ok(entries) = std::fs::read_dir(project_root) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("tailwind.config.") {
                    has_tailwind = true;
                }
            }
        }

        let framework = detect_project_framework(project_root);

        Self {
            framework,
            has_tailwind,
            has_scss,
        }
    }

    /// Formats the prompt directive string to guide the AI agent.
    pub fn format_prompt_directive(&self) -> String {
        match self.framework {
            Some(BlockFramework::Shadcn) => {
                "Detected Project Stack: Shadcn UI + React (Tailwind CSS).\n    Guideline: Default `block_search` queries with framework=\"shadcn\" or framework=\"react\".".to_string()
            }
            Some(BlockFramework::React) => {
                if self.has_tailwind {
                    "Detected Project Stack: React with Tailwind CSS.\n    Guideline: Default `block_search` queries with framework=\"react\" or framework=\"tailwind\".".to_string()
                } else if self.has_scss {
                    "Detected Project Stack: React with SCSS.\n    Guideline: Default `block_search` queries with framework=\"react\" or framework=\"scss\".".to_string()
                } else {
                    "Detected Project Stack: React.\n    Guideline: Default `block_search` queries with framework=\"react\".".to_string()
                }
            }
            Some(BlockFramework::Svelte) => {
                if self.has_tailwind {
                    "Detected Project Stack: Svelte with Tailwind CSS.\n    Guideline: Default `block_search` queries with framework=\"svelte\" or framework=\"tailwind\".".to_string()
                } else {
                    "Detected Project Stack: Svelte.\n    Guideline: Default `block_search` queries with framework=\"svelte\".".to_string()
                }
            }
            Some(BlockFramework::Tailwind) => {
                "Detected Project Stack: Tailwind CSS.\n    Guideline: Default `block_search` queries with framework=\"tailwind\".".to_string()
            }
            Some(BlockFramework::Scss) => {
                "Detected Project Stack: SCSS / SASS.\n    Guideline: Default `block_search` queries with framework=\"scss\".".to_string()
            }
            Some(BlockFramework::Css) => {
                if self.has_tailwind {
                    "Detected Project Stack: Tailwind CSS.\n    Guideline: Default `block_search` queries with framework=\"tailwind\".".to_string()
                } else {
                    "Detected Project Stack: Standard Web (HTML/CSS).\n    Guideline: Default `block_search` queries with framework=\"html\" or framework=\"css\".".to_string()
                }
            }
            None => {
                if self.has_tailwind {
                    "Detected Project Stack: Tailwind CSS.\n    Guideline: Default `block_search` queries with framework=\"tailwind\".".to_string()
                } else {
                    "Detected Project Stack: None yet (uninitialized workspace — no stack or framework established yet).\n    Guideline: Resolve the project stack first (via `ask_user` if the user did not specify one, then scaffold with `kit_stack_add`), and then filter `block_search` by the chosen framework (`react`, `tailwind`, `shadcn`, `html`, or `css`).".to_string()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_seed_catalog_population() {
        let mut store = BlockStore::new();
        assert_eq!(store.stats().total_components, 0);

        let seeded = seed_default_blocks(&mut store).unwrap();
        assert!(
            seeded > 1000,
            "Expected >1000 components seeded, got {seeded}"
        );

        let stats = store.stats();
        assert!(
            stats.total_components > 1000,
            "Expected >1000 components, got {}",
            stats.total_components
        );
        assert!(
            stats.total_palettes >= 100,
            "Expected >=100 palettes, got {}",
            stats.total_palettes
        );
        assert!(
            stats.total_gradients >= 200,
            "Expected >=200 gradients, got {}",
            stats.total_gradients
        );
        assert!(
            stats.total_templates >= 3,
            "Expected >=3 templates, got {}",
            stats.total_templates
        );

        // Verify idempotency: seeding again should seed 0 components and preserve counts
        let reseeded = seed_default_blocks(&mut store).unwrap();
        assert_eq!(reseeded, 0);
        let re_stats = store.stats();
        assert_eq!(re_stats.total_components, stats.total_components);
        assert_eq!(re_stats.total_palettes, stats.total_palettes);
        assert_eq!(re_stats.total_gradients, stats.total_gradients);
        assert_eq!(re_stats.total_templates, stats.total_templates);
    }

    #[test]
    fn test_detect_project_framework_react() {
        let temp = tempdir().unwrap();
        let pkg = r#"{
            "name": "my-react-app",
            "dependencies": {
                "react": "^18.2.0",
                "react-dom": "^18.2.0"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();

        assert_eq!(
            detect_project_framework(temp.path()),
            Some(BlockFramework::React)
        );
    }

    #[test]
    fn test_detect_project_framework_tailwind() {
        let temp = tempdir().unwrap();
        let pkg = r#"{
            "name": "my-tw-app",
            "devDependencies": {
                "tailwindcss": "^3.4.0"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();

        assert_eq!(
            detect_project_framework(temp.path()),
            Some(BlockFramework::Tailwind)
        );

        // Also test root config file detection
        let temp2 = tempdir().unwrap();
        std::fs::write(
            temp2.path().join("tailwind.config.js"),
            "module.exports = {};",
        )
        .unwrap();
        assert_eq!(
            detect_project_framework(temp2.path()),
            Some(BlockFramework::Tailwind)
        );
    }

    #[test]
    fn test_detect_project_framework_svelte() {
        let temp = tempdir().unwrap();
        let pkg = r#"{
            "name": "my-svelte-app",
            "devDependencies": {
                "@sveltejs/kit": "^2.0.0",
                "svelte": "^4.0.0"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();

        assert_eq!(
            detect_project_framework(temp.path()),
            Some(BlockFramework::Svelte)
        );

        // Also test root config file detection
        let temp2 = tempdir().unwrap();
        std::fs::write(temp2.path().join("svelte.config.js"), "export default {};").unwrap();
        assert_eq!(
            detect_project_framework(temp2.path()),
            Some(BlockFramework::Svelte)
        );
    }

    #[test]
    fn test_detect_project_framework_shadcn() {
        // Test via components.json
        let temp = tempdir().unwrap();
        std::fs::write(temp.path().join("components.json"), "{}").unwrap();
        assert_eq!(
            detect_project_framework(temp.path()),
            Some(BlockFramework::Shadcn)
        );

        // Test via package.json dependencies
        let temp2 = tempdir().unwrap();
        let pkg = r#"{
            "name": "my-shadcn-app",
            "dependencies": {
                "react": "^18.2.0",
                "@radix-ui/react-dialog": "^1.0.5"
            }
        }"#;
        std::fs::write(temp2.path().join("package.json"), pkg).unwrap();
        assert_eq!(
            detect_project_framework(temp2.path()),
            Some(BlockFramework::Shadcn)
        );
    }

    #[test]
    fn test_detect_project_framework_none() {
        let temp = tempdir().unwrap();
        assert_eq!(detect_project_framework(temp.path()), None);

        // Empty package.json without frontend framework deps
        let temp2 = tempdir().unwrap();
        let pkg = r#"{
            "name": "backend-service",
            "dependencies": {
                "express": "^4.18.0",
                "pg": "^8.11.0"
            }
        }"#;
        std::fs::write(temp2.path().join("package.json"), pkg).unwrap();
        assert_eq!(detect_project_framework(temp2.path()), None);
    }

    #[test]
    fn test_detect_project_framework_scss() {
        let temp = tempdir().unwrap();
        let pkg = r#"{
            "name": "style-service",
            "dependencies": {
                "sass": "^1.69.0"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();
        assert_eq!(
            detect_project_framework(temp.path()),
            Some(BlockFramework::Scss)
        );
    }

    #[test]
    fn test_project_stack_info_detect_and_directives() {
        let temp = tempdir().unwrap();

        // 1. React + Tailwind
        let pkg = r#"{
            "name": "full-stack-web",
            "dependencies": {
                "react": "^18.2.0",
                "tailwindcss": "^3.4.0"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();

        let info = ProjectStackInfo::detect(temp.path());
        assert_eq!(info.framework, Some(BlockFramework::React));
        assert!(info.has_tailwind);
        assert!(!info.has_scss);

        let directive = info.format_prompt_directive();
        assert!(directive.contains("React with Tailwind CSS"));
        assert!(directive.contains("framework=\"react\" or framework=\"tailwind\""));

        // 2. Empty directory fallback (uninitialized workspace)
        let temp_empty = tempdir().unwrap();
        let empty_info = ProjectStackInfo::detect(temp_empty.path());
        assert_eq!(empty_info.framework, None);
        assert!(!empty_info.has_tailwind);
        let empty_directive = empty_info.format_prompt_directive();
        assert!(empty_directive.contains("None yet (uninitialized workspace"));
        assert!(empty_directive.contains("ask_user"));

        // 3. Static HTML directory
        std::fs::write(temp_empty.path().join("index.html"), "<!doctype html>").unwrap();
        let html_info = ProjectStackInfo::detect(temp_empty.path());
        assert_eq!(html_info.framework, Some(BlockFramework::Css));
        let html_directive = html_info.format_prompt_directive();
        assert!(html_directive.contains("Standard Web (HTML/CSS)"));
        assert!(html_directive.contains("framework=\"html\" or framework=\"css\""));

        // 4. minikit.json manifest detection
        let temp_kit = tempdir().unwrap();
        std::fs::write(
            temp_kit.path().join("minikit.json"),
            r#"{"name":"demo","stack":"react-vite","packages":["lucide-react"],"dev_packages":["tailwindcss"]}"#,
        )
        .unwrap();
        let kit_info = ProjectStackInfo::detect(temp_kit.path());
        assert_eq!(kit_info.framework, Some(BlockFramework::React));
        assert!(kit_info.has_tailwind);
    }
}
