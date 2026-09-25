use crate::blocks::models::{BlockCategory, BlockFramework, BlockPalette};
use crate::blocks::seed::{detect_project_framework, ProjectStackInfo};
use crate::blocks::BlockError;
use std::path::{Path, PathBuf};

/// Detected project architecture conventions for components and module imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConventions {
    /// Detected UI framework
    pub framework: BlockFramework,
    /// Whether project is configured with TypeScript
    pub is_typescript: bool,
    /// Default file extension for UI components ("tsx", "jsx", "svelte")
    pub file_extension: String,
    /// Resolved target directory for UI components (e.g. "src/components")
    pub component_dir: PathBuf,
    /// Path alias prefix if configured (e.g. "@/" or "~/")
    pub path_alias_prefix: Option<String>,
    /// Path alias replacement base in repo (e.g. "src/" or "./")
    pub path_alias_base: Option<String>,
    /// Preferred component export style: true for default export, false for named export
    pub is_default_export: bool,
    /// Detected icon package name (e.g. "lucide-react", "react-icons", "@heroicons/react")
    pub icon_library: Option<String>,
    /// Whether Tailwind CSS is active
    pub has_tailwind: bool,
    /// Whether SCSS is active
    pub has_scss: bool,
}

impl ProjectConventions {
    /// Inspects the workspace root to detect project conventions.
    pub fn detect(workspace_root: &Path) -> Self {
        let stack = ProjectStackInfo::detect(workspace_root);
        let framework = stack
            .framework
            .or_else(|| detect_project_framework(workspace_root))
            .unwrap_or(BlockFramework::Tailwind);

        let mut is_typescript = false;
        let mut path_alias_prefix = None;
        let mut path_alias_base = None;

        // 1. Check tsconfig.json or jsconfig.json
        let tsconfig_path = workspace_root.join("tsconfig.json");
        let jsconfig_path = workspace_root.join("jsconfig.json");

        if tsconfig_path.is_file() {
            is_typescript = true;
            if let Ok(content) = std::fs::read_to_string(&tsconfig_path) {
                parse_path_alias(&content, &mut path_alias_prefix, &mut path_alias_base);
            }
        } else if jsconfig_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&jsconfig_path) {
                parse_path_alias(&content, &mut path_alias_prefix, &mut path_alias_base);
            }
        }

        // Check for root .ts configs (e.g. vite.config.ts, tailwind.config.ts)
        if !is_typescript
            && (workspace_root.join("vite.config.ts").is_file()
                || workspace_root.join("tailwind.config.ts").is_file())
        {
            is_typescript = true;
        }

        // Check for any .tsx or .ts files if tsconfig was not found
        if !is_typescript {
            let src_dir = workspace_root.join("src");
            if src_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&src_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            if ext == "tsx" || ext == "ts" {
                                is_typescript = true;
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Read package.json dependencies to detect TypeScript and icon libraries
        let pkg_path = workspace_root.join("package.json");
        let mut pkg_dep_keys = Vec::new();
        if pkg_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&pkg_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) {
                        pkg_dep_keys.extend(deps.keys().map(|k| k.to_lowercase()));
                    }
                    if let Some(dev_deps) = json.get("devDependencies").and_then(|d| d.as_object())
                    {
                        pkg_dep_keys.extend(dev_deps.keys().map(|k| k.to_lowercase()));
                    }
                }
            }
        }

        if !is_typescript && pkg_dep_keys.iter().any(|k| k == "typescript") {
            is_typescript = true;
        }

        // Determine file extension
        let file_extension = match framework {
            BlockFramework::Svelte => "svelte".to_string(),
            _ => {
                if is_typescript {
                    "tsx".to_string()
                } else {
                    "jsx".to_string()
                }
            }
        };

        // 2. Detect icon library from package.json
        let mut icon_library = None;
        if pkg_dep_keys.iter().any(|k| k == "lucide-react") {
            icon_library = Some("lucide-react".to_string());
        } else if pkg_dep_keys.iter().any(|k| k == "react-icons") {
            icon_library = Some("react-icons/lu".to_string());
        } else if pkg_dep_keys.iter().any(|k| k.contains("heroicons")) {
            icon_library = Some("@heroicons/react/24/outline".to_string());
        }

        // 3. Resolve preferred component directory
        let component_dir = if workspace_root.join("src/components").is_dir() {
            PathBuf::from("src/components")
        } else if workspace_root.join("components").is_dir() {
            PathBuf::from("components")
        } else if workspace_root.join("app/components").is_dir() {
            PathBuf::from("app/components")
        } else {
            PathBuf::from("src/components")
        };

        // 4. Detect export style convention (check existing component if present)
        let is_default_export = detect_export_style(workspace_root, &component_dir);

        let conventions = Self {
            framework,
            is_typescript,
            file_extension,
            component_dir,
            path_alias_prefix,
            path_alias_base,
            is_default_export,
            icon_library,
            has_tailwind: stack.has_tailwind,
            has_scss: stack.has_scss,
        };

        // If this workspace uses Vite + TypeScript, ensure src/vite-env.d.ts exists
        conventions.ensure_vite_env(workspace_root);

        conventions
    }

    /// Ensures that `src/vite-env.d.ts` exists in Vite + TypeScript projects
    /// to guarantee clean resolution of CSS side-effect imports and asset types.
    pub fn ensure_vite_env(&self, workspace_root: &Path) {
        let pkg_has_vite = workspace_root.join("package.json").is_file()
            && std::fs::read_to_string(workspace_root.join("package.json"))
                .map(|content| content.contains("\"vite\""))
                .unwrap_or(false);

        let is_vite = workspace_root.join("vite.config.ts").is_file()
            || workspace_root.join("vite.config.js").is_file()
            || workspace_root.join("vite.config.mjs").is_file()
            || pkg_has_vite;

        if self.is_typescript && is_vite {
            let src_dir = workspace_root.join("src");
            if let Ok(()) = std::fs::create_dir_all(&src_dir) {
                let vite_env = src_dir.join("vite-env.d.ts");
                if !vite_env.exists() {
                    let _ = std::fs::write(&vite_env, "/// <reference types=\"vite/client\" />\n");
                }
            }
        }
    }
}

/// Helper to parse compilerOptions.paths from a tsconfig/jsconfig JSON string.
fn parse_path_alias(
    json_str: &str,
    prefix_out: &mut Option<String>,
    base_out: &mut Option<String>,
) {
    // Basic comment stripping for json5/jsonc in tsconfig
    let cleaned_lines: Vec<String> = json_str
        .lines()
        .map(|l| {
            let trimmed = l.trim_start();
            if trimmed.starts_with("//") {
                String::new()
            } else {
                l.to_string()
            }
        })
        .collect();
    let cleaned = cleaned_lines.join("\n");

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        if let Some(paths) = v
            .get("compilerOptions")
            .and_then(|c| c.get("paths"))
            .and_then(|p| p.as_object())
        {
            for (key, val) in paths {
                if let Some(target_arr) = val.as_array() {
                    if let Some(first_target) = target_arr.first().and_then(|t| t.as_str()) {
                        let clean_key = key.trim_end_matches('*');
                        let clean_target = first_target
                            .trim_start_matches("./")
                            .trim_start_matches('/')
                            .trim_end_matches('*');

                        *prefix_out = Some(clean_key.to_string());
                        *base_out = Some(clean_target.to_string());
                        return;
                    }
                }
            }
        }
    }
}

/// Helper to detect export convention from existing component files in the directory.
fn detect_export_style(workspace_root: &Path, comp_dir: &Path) -> bool {
    let full_comp_dir = workspace_root.join(comp_dir);
    if full_comp_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&full_comp_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Ok(code) = std::fs::read_to_string(&p) {
                        if code.contains("export default function")
                            || code.contains("export default const")
                            || code.contains("export default ")
                        {
                            return true;
                        } else if code.contains("export function") || code.contains("export const")
                        {
                            return false;
                        }
                    }
                }
            }
        }
    }
    // Default to false (named exports) for modern modular code
    false
}

/// Resolves an optimal module import path for a component relative to a consumer file,
/// taking into account configured path aliases (e.g. `@/components/Navbar`) or relative paths (`./Navbar`).
pub fn resolve_import_path(
    workspace_root: &Path,
    component_file: &Path,
    consumer_file: &Path,
    conventions: &ProjectConventions,
) -> String {
    let comp_rel = if component_file.is_absolute() {
        component_file
            .strip_prefix(workspace_root)
            .unwrap_or(component_file)
    } else {
        component_file
    };

    let consumer_rel = if consumer_file.is_absolute() {
        consumer_file
            .strip_prefix(workspace_root)
            .unwrap_or(consumer_file)
    } else {
        consumer_file
    };

    // Strip extension (.tsx, .jsx, .ts, .js)
    let comp_without_ext = if let Some(parent) = comp_rel.parent() {
        if let Some(stem) = comp_rel.file_stem().and_then(|s| s.to_str()) {
            if parent.as_os_str().is_empty() {
                PathBuf::from(stem)
            } else {
                parent.join(stem)
            }
        } else {
            comp_rel.to_path_buf()
        }
    } else {
        comp_rel.to_path_buf()
    };

    let comp_str = comp_without_ext.to_string_lossy().replace('\\', "/");

    // 1. Check path alias
    if let (Some(prefix), Some(base)) =
        (&conventions.path_alias_prefix, &conventions.path_alias_base)
    {
        let clean_base = base.trim_end_matches('/');
        if comp_str.starts_with(clean_base) {
            let suffix = comp_str
                .trim_start_matches(clean_base)
                .trim_start_matches('/');
            return format!("{}{}", prefix, suffix);
        } else if clean_base.is_empty() {
            return format!("{}{}", prefix, comp_str.trim_start_matches('/'));
        }
    }

    // 2. Relative path calculation
    let consumer_dir = consumer_rel.parent().unwrap_or_else(|| Path::new(""));
    let rel_path = compute_relative_path(consumer_dir, &comp_without_ext);

    let mut rel_str = rel_path.to_string_lossy().replace('\\', "/");
    if !rel_str.starts_with('.') && !rel_str.starts_with('/') {
        rel_str = format!("./{}", rel_str);
    }
    rel_str
}

/// Computes a relative path from a directory to a target file in pure Rust.
fn compute_relative_path(from_dir: &Path, to_file: &Path) -> PathBuf {
    use std::path::Component;

    let from_components: Vec<_> = from_dir
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    let to_components: Vec<_> = to_file
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();

    let mut common_len = 0;
    while common_len < from_components.len()
        && common_len < to_components.len()
        && from_components[common_len] == to_components[common_len]
    {
        common_len += 1;
    }

    let mut result = PathBuf::new();
    for _ in common_len..from_components.len() {
        result.push("..");
    }
    for comp in &to_components[common_len..] {
        result.push(comp.as_os_str());
    }

    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

/// Generates an exact TypeScript/JavaScript import statement.
pub fn generate_import_statement(
    component_name: &str,
    import_path: &str,
    is_default_export: bool,
) -> String {
    let clean_name = component_name.trim();
    if is_default_export {
        format!("import {} from \"{}\";", clean_name, import_path)
    } else {
        format!("import {{ {} }} from \"{}\";", clean_name, import_path)
    }
}

/// Injects an import statement into a target source file without creating duplicates.
/// Returns Ok(true) if the import was inserted, or Ok(false) if the import was already present.
pub fn wire_import_into_file(
    target_file: &Path,
    import_statement: &str,
    component_name: &str,
) -> Result<bool, BlockError> {
    if !target_file.is_file() {
        return Err(BlockError::Storage(format!(
            "Target consumer file '{}' does not exist",
            target_file.display()
        )));
    }

    let content = std::fs::read_to_string(target_file).map_err(BlockError::Io)?;

    // Check if the component is already imported
    let check_named = format!("{{ {} }}", component_name);
    let check_named_spaced = format!("{{{}}}", component_name);
    let check_default = format!("import {} from", component_name);

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ")
            && (trimmed.contains(&check_named)
                || trimmed.contains(&check_named_spaced)
                || trimmed.contains(&check_default)
                || trimmed.contains(&format!(" {} ", component_name)))
        {
            return Ok(false); // Already imported
        }
    }

    // Locate the insertion index (right after the last import line)
    let lines: Vec<&str> = content.lines().collect();
    let mut last_import_idx = None;
    let mut first_code_idx = 0;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") || (trimmed.starts_with("from ") && idx > 0) {
            last_import_idx = Some(idx);
        } else if !trimmed.is_empty()
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && !trimmed.starts_with('*')
            && !trimmed.starts_with("\"use ")
            && !trimmed.starts_with("'use ")
            && last_import_idx.is_none()
            && first_code_idx == 0
        {
            first_code_idx = idx;
        }
    }

    let mut new_lines = Vec::with_capacity(lines.len() + 2);
    if let Some(last_imp) = last_import_idx {
        for (idx, line) in lines.iter().enumerate() {
            new_lines.push(*line);
            if idx == last_imp {
                new_lines.push(import_statement);
            }
        }
    } else {
        // No existing imports. Handle 'use client' or initial comments
        let mut inserted = false;
        for (idx, line) in lines.iter().enumerate() {
            if !inserted && idx >= first_code_idx {
                new_lines.push(import_statement);
                inserted = true;
            }
            new_lines.push(*line);
        }
        if !inserted {
            new_lines.push(import_statement);
        }
    }

    let final_content = new_lines.join("\n") + if content.ends_with('\n') { "\n" } else { "" };
    std::fs::write(target_file, final_content).map_err(BlockError::Io)?;

    Ok(true)
}

/// Generates a clean, convention-conforming custom UI component matching workspace conventions.
pub fn scaffold_custom_component(
    conventions: &ProjectConventions,
    name: &str,
    category: BlockCategory,
    description: &str,
    props: &[String],
    palette: Option<&BlockPalette>,
) -> (String, String) {
    let clean_name = to_pascal_case(name);
    let filename = format!("{}.{}", clean_name, conventions.file_extension);

    let is_ts = conventions.is_typescript;
    let has_tw = conventions.has_tailwind || conventions.framework == BlockFramework::Tailwind;

    let icon_import = match &conventions.icon_library {
        Some(lib) if lib == "lucide-react" => {
            "import { ArrowRight, Sparkles } from \"lucide-react\";\n"
        }
        Some(lib) if lib.contains("react-icons") => {
            "import { LuArrowRight, LuSparkles } from \"react-icons/lu\";\n"
        }
        Some(lib) if lib.contains("heroicons") => {
            "import { ArrowRightIcon, SparklesIcon } from \"@heroicons/react/24/outline\";\n"
        }
        _ => "",
    };

    let props_type_name = format!("{}Props", clean_name);
    let mut props_interface = String::new();
    let mut props_destructure = String::new();

    if !props.is_empty() {
        let mut fields = Vec::new();
        let mut destructure_fields = Vec::new();
        for p in props {
            let field_name = p.trim();
            if !field_name.is_empty() {
                fields.push(format!("  {}?: string;", field_name));
                destructure_fields.push(field_name.to_string());
            }
        }
        if is_ts && !fields.is_empty() {
            props_interface = format!(
                "export interface {} {{\n{}\n  className?: string;\n}}\n\n",
                props_type_name,
                fields.join("\n")
            );
            props_destructure = format!(
                "{{ {}, className = \"\" }}: {}",
                destructure_fields.join(", "),
                props_type_name
            );
        } else if !destructure_fields.is_empty() {
            props_destructure =
                format!("{{ {}, className = \"\" }}", destructure_fields.join(", "));
        }
    } else if is_ts {
        props_interface = format!(
            "export interface {} {{\n  title?: string;\n  className?: string;\n}}\n\n",
            props_type_name
        );
        props_destructure = format!(
            "{{ title = \"{}\", className = \"\" }}: {}",
            clean_name, props_type_name
        );
    } else {
        props_destructure = format!("{{ title = \"{}\", className = \"\" }}", clean_name);
    }

    let container_class = if has_tw {
        if let Some(pal) = palette {
            let bg = &pal.colors[0];
            let surface = &pal.colors[1];
            let accent = &pal.colors[2];
            match category {
                BlockCategory::Navbar => format!("flex items-center justify-between px-6 py-4 bg-[{}]/90 backdrop-blur border-b border-[{}]/30 text-white", surface, accent),
                BlockCategory::Hero => format!("flex flex-col items-center justify-center text-center py-20 px-4 max-w-4xl mx-auto space-y-6 bg-[{}] text-white", bg),
                BlockCategory::Card => format!("p-6 rounded-2xl bg-[{}]/80 border border-[{}]/30 shadow-lg hover:border-[{}]/60 transition-all space-y-4 text-white", surface, accent, accent),
                BlockCategory::Pricing => format!("grid grid-cols-1 md:grid-cols-3 gap-8 p-8 max-w-6xl mx-auto bg-[{}] text-white", bg),
                BlockCategory::Modal => format!("fixed inset-0 z-50 flex items-center justify-center bg-[{}]/80 backdrop-blur-md p-4 text-white", bg),
                BlockCategory::Footer => format!("border-t border-[{}]/30 px-6 py-12 flex flex-col md:flex-row items-center justify-between gap-4 text-white/70 text-sm bg-[{}]", accent, bg),
                _ => format!("p-6 rounded-xl border border-[{}]/30 bg-[{}]/80 shadow-md space-y-4 text-white", accent, surface),
            }
        } else {
            match category {
                BlockCategory::Navbar => "flex items-center justify-between px-6 py-4 bg-background/80 backdrop-blur border-b border-border".to_string(),
                BlockCategory::Hero => "flex flex-col items-center justify-center text-center py-20 px-4 max-w-4xl mx-auto space-y-6".to_string(),
                BlockCategory::Card => "p-6 rounded-2xl bg-card border border-border shadow-sm hover:shadow-md transition-all space-y-4".to_string(),
                BlockCategory::Pricing => "grid grid-cols-1 md:grid-cols-3 gap-8 p-8 max-w-6xl mx-auto".to_string(),
                BlockCategory::Modal => "fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4".to_string(),
                BlockCategory::Footer => "border-t border-border px-6 py-12 flex flex-col md:flex-row items-center justify-between gap-4 text-muted-foreground text-sm".to_string(),
                _ => "p-6 rounded-xl border border-border bg-card shadow-sm space-y-4".to_string(),
            }
        }
    } else {
        "custom-component-container".to_string()
    };

    let title_class = if has_tw {
        if let Some(pal) = palette {
            format!(
                "text-xl font-semibold tracking-tight text-[{}]",
                pal.colors[3]
            )
        } else {
            "text-xl font-semibold tracking-tight text-foreground".to_string()
        }
    } else {
        "custom-component-title".to_string()
    };

    let desc_class = if has_tw {
        if palette.is_some() {
            "text-sm text-white/70".to_string()
        } else {
            "text-sm text-muted-foreground".to_string()
        }
    } else {
        "custom-component-desc".to_string()
    };

    let has_title_in_props = props.iter().any(|p| p.trim().eq_ignore_ascii_case("title"));
    let has_desc_in_props = props.iter().any(|p| {
        p.trim().eq_ignore_ascii_case("description") || p.trim().eq_ignore_ascii_case("desc")
    });

    let h2_element = if has_title_in_props {
        format!("<h2 className=\"{}\">{{title}}</h2>", title_class)
    } else {
        format!("<h2 className=\"{}\">{}</h2>", title_class, clean_name)
    };

    let p_element = if has_desc_in_props {
        format!("<p className=\"{}\">{{description}}</p>", desc_class)
    } else {
        format!("<p className=\"{}\">{}</p>", desc_class, description)
    };

    let cta_snippet = if let Some(pal) = palette {
        format!(
            "      <div className=\"pt-2\">\n        <button className=\"inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-[{}] text-white font-medium hover:opacity-90 transition-opacity\">\n          <span>Explore</span>\n        </button>\n      </div>\n",
            pal.colors[2]
        )
    } else {
        String::new()
    };

    let code = if conventions.is_default_export {
        format!(
            "import React from \"react\";\n{}{}\nexport default function {}({}) {{\n  return (\n    <section className={{`{} ${{className}}`}}>\n      {}\n      {}\n{}    </section>\n  );\n}}\n",
            icon_import,
            props_interface,
            clean_name,
            props_destructure,
            container_class,
            h2_element,
            p_element,
            cta_snippet
        )
    } else {
        format!(
            "import React from \"react\";\n{}{}\nexport function {}({}) {{\n  return (\n    <section className={{`{} ${{className}}`}}>\n      {}\n      {}\n{}    </section>\n  );\n}}\n",
            icon_import,
            props_interface,
            clean_name,
            props_destructure,
            container_class,
            h2_element,
            p_element,
            cta_snippet
        )
    };

    (filename, code)
}

/// Helper to convert a string to PascalCase.
fn to_pascal_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if capitalize_next {
                result.extend(c.to_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        } else {
            capitalize_next = true;
        }
    }
    if result.is_empty() {
        "CustomComponent".to_string()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("user_card"), "UserCard");
        assert_eq!(to_pascal_case("responsive-navbar"), "ResponsiveNavbar");
        assert_eq!(to_pascal_case("pricing table"), "PricingTable");
        assert_eq!(to_pascal_case(""), "CustomComponent");
    }

    #[test]
    fn test_resolve_import_path_with_alias() {
        let temp = tempdir().unwrap();
        let conventions = ProjectConventions {
            framework: BlockFramework::Tailwind,
            is_typescript: true,
            file_extension: "tsx".to_string(),
            component_dir: PathBuf::from("src/components"),
            path_alias_prefix: Some("@/".to_string()),
            path_alias_base: Some("src/".to_string()),
            is_default_export: false,
            icon_library: Some("lucide-react".to_string()),
            has_tailwind: true,
            has_scss: false,
        };

        let comp_file = temp.path().join("src/components/Navbar.tsx");
        let consumer_file = temp.path().join("src/App.tsx");

        let import_path =
            resolve_import_path(temp.path(), &comp_file, &consumer_file, &conventions);
        assert_eq!(import_path, "@/components/Navbar");

        let stmt = generate_import_statement("Navbar", &import_path, false);
        assert_eq!(stmt, "import { Navbar } from \"@/components/Navbar\";");

        let stmt_default = generate_import_statement("Navbar", &import_path, true);
        assert_eq!(stmt_default, "import Navbar from \"@/components/Navbar\";");
    }

    #[test]
    fn test_resolve_import_path_relative() {
        let temp = tempdir().unwrap();
        let conventions = ProjectConventions {
            framework: BlockFramework::React,
            is_typescript: true,
            file_extension: "tsx".to_string(),
            component_dir: PathBuf::from("src/components"),
            path_alias_prefix: None,
            path_alias_base: None,
            is_default_export: false,
            icon_library: None,
            has_tailwind: false,
            has_scss: false,
        };

        let comp_file = temp.path().join("src/components/Navbar.tsx");
        let consumer_file = temp.path().join("src/App.tsx");

        let import_path =
            resolve_import_path(temp.path(), &comp_file, &consumer_file, &conventions);
        assert_eq!(import_path, "./components/Navbar");
    }

    #[test]
    fn test_wire_import_into_file() {
        let temp = tempdir().unwrap();
        let target = temp.path().join("App.tsx");
        std::fs::write(
            &target,
            "import React from 'react';\n\nexport default function App() {\n  return <div>App</div>;\n}\n",
        )
        .unwrap();

        let stmt = "import { Navbar } from \"./components/Navbar\";";
        let wired = wire_import_into_file(&target, stmt, "Navbar").unwrap();
        assert!(wired);

        let content = std::fs::read_to_string(&target).unwrap();
        assert!(content.contains("import { Navbar } from \"./components/Navbar\";"));

        // Second attempt to wire should be ignored as duplicate
        let wired_again = wire_import_into_file(&target, stmt, "Navbar").unwrap();
        assert!(!wired_again);
    }

    #[test]
    fn test_scaffold_custom_component() {
        let conventions = ProjectConventions {
            framework: BlockFramework::Tailwind,
            is_typescript: true,
            file_extension: "tsx".to_string(),
            component_dir: PathBuf::from("src/components"),
            path_alias_prefix: Some("@/".to_string()),
            path_alias_base: Some("src/".to_string()),
            is_default_export: false,
            icon_library: Some("lucide-react".to_string()),
            has_tailwind: true,
            has_scss: false,
        };

        let (filename, code) = scaffold_custom_component(
            &conventions,
            "user_profile_card",
            BlockCategory::Card,
            "User profile card with avatar and badges",
            &["username".to_string(), "role".to_string()],
            None,
        );

        assert_eq!(filename, "UserProfileCard.tsx");
        assert!(code.contains("export interface UserProfileCardProps"));
        assert!(code.contains("export function UserProfileCard"));
        assert!(code.contains("import { ArrowRight, Sparkles } from \"lucide-react\";"));
        assert!(code.contains("username?: string;"));
        assert!(code.contains("role?: string;"));
    }

    #[test]
    fn test_scaffold_custom_component_with_palette() {
        let conventions = ProjectConventions {
            framework: BlockFramework::Tailwind,
            is_typescript: true,
            file_extension: "tsx".to_string(),
            component_dir: PathBuf::from("src/components"),
            path_alias_prefix: Some("@/".to_string()),
            path_alias_base: Some("src/".to_string()),
            is_default_export: false,
            icon_library: Some("lucide-react".to_string()),
            has_tailwind: true,
            has_scss: false,
        };

        let pal = BlockPalette::new(
            "Cyberpunk Neon",
            [
                "#0F0C1B".to_string(),
                "#1F1A3A".to_string(),
                "#FF007F".to_string(),
                "#00F0FF".to_string(),
            ],
            vec!["cyberpunk".to_string()],
        )
        .unwrap();

        let (filename, code) = scaffold_custom_component(
            &conventions,
            "cyber_card",
            BlockCategory::Card,
            "Cyberpunk styled card",
            &["metric".to_string()],
            Some(&pal),
        );

        assert_eq!(filename, "CyberCard.tsx");
        // Verify surface (#1F1A3A), accent border (#FF007F), text color (#00F0FF), and themed button
        assert!(code.contains("bg-[#1F1A3A]/80"));
        assert!(code.contains("border-[#FF007F]/30"));
        assert!(code.contains("text-[#00F0FF]"));
        assert!(code.contains("bg-[#FF007F] text-white"));
    }

    #[test]
    fn test_detect_typescript_from_package_json() {
        let temp = tempfile::tempdir().unwrap();
        // Create a package.json with typescript in devDependencies without any tsconfig
        let pkg = r#"{
            "name": "fresh-project",
            "devDependencies": {
                "typescript": "^5.3.3"
            }
        }"#;
        std::fs::write(temp.path().join("package.json"), pkg).unwrap();

        let conventions = ProjectConventions::detect(temp.path());
        assert!(conventions.is_typescript);
        assert_eq!(conventions.file_extension, "tsx");
    }

    #[test]
    fn test_ensure_vite_env_scaffolded() {
        let temp = tempfile::tempdir().unwrap();
        // Create a mock vite.config.ts in workspace root
        std::fs::write(temp.path().join("vite.config.ts"), "export default {}").unwrap();

        let conventions = ProjectConventions::detect(temp.path());
        assert!(conventions.is_typescript);
        let vite_env = temp.path().join("src/vite-env.d.ts");
        assert!(vite_env.is_file());
        let content = std::fs::read_to_string(&vite_env).unwrap();
        assert!(content.contains("/// <reference types=\"vite/client\" />"));
    }
}
