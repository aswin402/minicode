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

    // 3. Root config files fallback:
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
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("tailwind.config.") {
                return Some(BlockFramework::Tailwind);
            }
            if name_str.starts_with("svelte.config.") {
                return Some(BlockFramework::Svelte);
            }
        }
    }

    None
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
}
