use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub use super::BlockError;

/// Validates whether a given string is a valid hex color code (#RGB or #RRGGBB).
pub fn is_valid_hex_color(s: &str) -> bool {
    let bytes = s.trim().as_bytes();
    (bytes.len() == 4 || bytes.len() == 7)
        && bytes.first() == Some(&b'#')
        && bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockCategory {
    Navbar,
    Hero,
    Footer,
    Sidebar,
    Card,
    Form,
    Modal,
    Table,
    Pricing,
    Testimonial,
    Cta,
    Feature,
    Faq,
    Contact,
    Auth,
    Dashboard,
    Settings,
    Profile,
    Landing,
    Blog,
    Ecommerce,
    Error,
    Loading,
    Notification,
    Button,
    Input,
    Section,
    Other,
}

impl BlockCategory {
    /// Flexible loose string parser supporting case-insensitivity, hyphens, underscores, and common aliases.
    pub fn from_str_loose(s: &str) -> Self {
        let clean = s.trim().to_lowercase().replace(['-', '_', ' '], "");
        match clean.as_str() {
            "navbar" | "nav" | "navigation" | "header" => Self::Navbar,
            "hero" | "herosection" => Self::Hero,
            "footer" | "footersection" | "footers" => Self::Footer,
            "sidebar" | "sidebars" | "aside" => Self::Sidebar,
            "card" | "cards" => Self::Card,
            "form" | "forms" => Self::Form,
            "modal" | "modals" | "dialog" | "dialogs" | "popup" => Self::Modal,
            "table" | "tables" | "datatable" => Self::Table,
            "pricing" | "pricingtable" | "pricetable" | "prices" | "price" => Self::Pricing,
            "testimonial" | "testimonials" | "review" | "reviews" => Self::Testimonial,
            "cta" | "calltoaction" => Self::Cta,
            "feature" | "features" => Self::Feature,
            "faq" | "faqs" => Self::Faq,
            "contact" | "contactus" => Self::Contact,
            "auth" | "authentication" | "login" | "signin" | "signup" | "register" => Self::Auth,
            "dashboard" | "dashboards" => Self::Dashboard,
            "settings" | "setting" => Self::Settings,
            "profile" | "profiles" | "userprofile" => Self::Profile,
            "landing" | "landingpage" => Self::Landing,
            "blog" | "blogs" | "post" | "posts" | "article" => Self::Blog,
            "ecommerce" | "ecom" | "shop" | "store" | "cart" | "checkout" => Self::Ecommerce,
            "error" | "404" | "500" => Self::Error,
            "loading" | "spinner" | "skeleton" => Self::Loading,
            "notification" | "notifications" | "alert" | "alerts" | "toast" | "toasts" => {
                Self::Notification
            }
            "button" | "buttons" | "btn" => Self::Button,
            "input" | "inputs" | "field" | "formcontrol" | "textfield" => Self::Input,
            "section" | "sections" => Self::Section,
            "other" | "misc" | "general" => Self::Other,
            other => {
                if other.contains("pricing") {
                    Self::Pricing
                } else if other.contains("hero") {
                    Self::Hero
                } else if other.contains("nav") {
                    Self::Navbar
                } else if other.contains("footer") {
                    Self::Footer
                } else if other.contains("sidebar") {
                    Self::Sidebar
                } else if other.contains("modal") || other.contains("dialog") {
                    Self::Modal
                } else if other.contains("testimonial") || other.contains("review") {
                    Self::Testimonial
                } else if other.contains("landing") {
                    Self::Landing
                } else if other.contains("table") {
                    Self::Table
                } else if other.contains("form") {
                    Self::Form
                } else if other.contains("card") {
                    Self::Card
                } else if other.contains("cta") {
                    Self::Cta
                } else if other.contains("feature") {
                    Self::Feature
                } else if other.contains("faq") {
                    Self::Faq
                } else if other.contains("contact") {
                    Self::Contact
                } else if other.contains("auth")
                    || other.contains("login")
                    || other.contains("signup")
                {
                    Self::Auth
                } else if other.contains("dash") {
                    Self::Dashboard
                } else if other.contains("setting") {
                    Self::Settings
                } else if other.contains("profile") {
                    Self::Profile
                } else if other.contains("blog") {
                    Self::Blog
                } else if other.contains("ecom")
                    || other.contains("shop")
                    || other.contains("store")
                {
                    Self::Ecommerce
                } else if other.contains("error") {
                    Self::Error
                } else if other.contains("loading")
                    || other.contains("spinner")
                    || other.contains("skeleton")
                {
                    Self::Loading
                } else if other.contains("notif")
                    || other.contains("toast")
                    || other.contains("alert")
                {
                    Self::Notification
                } else if other.contains("button") || other == "btn" {
                    Self::Button
                } else if other.contains("input") {
                    Self::Input
                } else if other.contains("section") {
                    Self::Section
                } else {
                    Self::Other
                }
            }
        }
    }
}

impl std::fmt::Display for BlockCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Navbar => "navbar",
            Self::Hero => "hero",
            Self::Footer => "footer",
            Self::Sidebar => "sidebar",
            Self::Card => "card",
            Self::Form => "form",
            Self::Modal => "modal",
            Self::Table => "table",
            Self::Pricing => "pricing",
            Self::Testimonial => "testimonial",
            Self::Cta => "cta",
            Self::Feature => "feature",
            Self::Faq => "faq",
            Self::Contact => "contact",
            Self::Auth => "auth",
            Self::Dashboard => "dashboard",
            Self::Settings => "settings",
            Self::Profile => "profile",
            Self::Landing => "landing",
            Self::Blog => "blog",
            Self::Ecommerce => "ecommerce",
            Self::Error => "error",
            Self::Loading => "loading",
            Self::Notification => "notification",
            Self::Button => "button",
            Self::Input => "input",
            Self::Section => "section",
            Self::Other => "other",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for BlockCategory {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str_loose(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockFramework {
    Tailwind,
    Css,
    Scss,
    Shadcn,
    React,
    Svelte,
}

impl BlockFramework {
    /// Flexible loose string parser supporting common names, extensions, and aliases.
    pub fn from_str_loose(s: &str) -> Self {
        let clean = s.trim().to_lowercase().replace(['-', '_', ' '], "");
        match clean.as_str() {
            "react" | "jsx" | "tsx" | "next" | "nextjs" => Self::React,
            "tailwind" | "tailwindcss" | "tw" => Self::Tailwind,
            "css" | "vanillacss" | "vanilla" | "html" => Self::Css,
            "scss" | "sass" => Self::Scss,
            "shadcn" | "shadcnui" | "shadcn-ui" => Self::Shadcn,
            "svelte" | "sveltekit" => Self::Svelte,
            _ => {
                if clean.contains("react") || clean.contains("jsx") || clean.contains("tsx") {
                    Self::React
                } else if clean.contains("tailwind") {
                    Self::Tailwind
                } else if clean.contains("shadcn") {
                    Self::Shadcn
                } else if clean.contains("svelte") {
                    Self::Svelte
                } else if clean.contains("scss") || clean.contains("sass") {
                    Self::Scss
                } else if clean.contains("css") {
                    Self::Css
                } else {
                    Self::Tailwind
                }
            }
        }
    }
}

impl std::fmt::Display for BlockFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Tailwind => "tailwind",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Shadcn => "shadcn",
            Self::React => "react",
            Self::Svelte => "svelte",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for BlockFramework {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_str_loose(s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockComponent {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: BlockCategory,
    pub framework: BlockFramework,
    pub code: String,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BlockComponent {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        category: BlockCategory,
        framework: BlockFramework,
        code: impl Into<String>,
        dependencies: Vec<String>,
        tags: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            category,
            framework,
            code: code.into(),
            dependencies,
            tags,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockPalette {
    pub id: Uuid,
    pub name: String,
    pub colors: [String; 4], // Exactly 4 hex tokens: [Background, Surface, Accent, Text]
    pub tags: Vec<String>,
}

impl BlockPalette {
    pub fn new(
        name: impl Into<String>,
        colors: [String; 4],
        tags: Vec<String>,
    ) -> Result<Self, BlockError> {
        let normalized_colors = [
            colors[0].trim().to_uppercase(),
            colors[1].trim().to_uppercase(),
            colors[2].trim().to_uppercase(),
            colors[3].trim().to_uppercase(),
        ];
        for color in &normalized_colors {
            if !is_valid_hex_color(color) {
                return Err(BlockError::InvalidHexColor(color.clone()));
            }
        }
        Ok(Self {
            id: Uuid::new_v4(),
            name: name.into(),
            colors: normalized_colors,
            tags,
        })
    }

    /// Formats the palette tokens as CSS custom properties (`:root { ... }`).
    pub fn to_css_variables(&self) -> String {
        format!(
            "/* CSS Variables Export */\n:root {{\n  --bg: {};\n  --surface: {};\n  --accent: {};\n  --text: {};\n}}",
            self.colors[0], self.colors[1], self.colors[2], self.colors[3]
        )
    }

    /// Formats the palette tokens as a Tailwind CSS theme color configuration object.
    pub fn to_tailwind_config(&self) -> String {
        format!(
            "// Tailwind CSS Theme Colors\n// Classes: bg-palette-bg, bg-palette-surface, text-palette-accent, border-palette-accent, text-palette-text\n// Direct aliases: bg-theme-bg, bg-theme-surface, text-theme-accent, border-theme-accent, text-theme-text\ncolors: {{\n  palette: {{\n    bg: '{}',\n    surface: '{}',\n    accent: '{}',\n    text: '{}',\n  }},\n  bg: '{}',\n  surface: '{}',\n  accent: '{}',\n  text: '{}',\n  'theme-bg': '{}',\n  'theme-surface': '{}',\n  'theme-accent': '{}',\n  'theme-text': '{}',\n}}",
            self.colors[0], self.colors[1], self.colors[2], self.colors[3],
            self.colors[0], self.colors[1], self.colors[2], self.colors[3],
            self.colors[0], self.colors[1], self.colors[2], self.colors[3]
        )
    }

    /// Formats the palette tokens as SCSS variables.
    pub fn to_scss_variables(&self) -> String {
        format!(
            "// SCSS Variables Export\n$color-bg: {};\n$color-surface: {};\n$color-accent: {};\n$color-text: {};",
            self.colors[0], self.colors[1], self.colors[2], self.colors[3]
        )
    }

    /// Formats the palette tokens as a structured JSON object.
    pub fn to_json_tokens(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "name": self.name,
            "tokens": {
                "bg": self.colors[0],
                "surface": self.colors[1],
                "accent": self.colors[2],
                "text": self.colors[3]
            }
        }))
        .unwrap_or_else(|_| "{}".to_string())
    }

    /// Exports palette tokens in the requested format (`css`, `tailwind`, `scss`, `json`).
    /// Returns a tuple of `(code_content, language_fence_tag)`.
    pub fn format_tokens(&self, format: &str) -> (String, String) {
        let fmt_clean = format.trim().to_lowercase();
        match fmt_clean.as_str() {
            "tailwind" | "tw" => (self.to_tailwind_config(), "javascript".to_string()),
            "scss" | "sass" => (self.to_scss_variables(), "scss".to_string()),
            "json" | "tokens" => (self.to_json_tokens(), "json".to_string()),
            _ => (self.to_css_variables(), "css".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockGradient {
    pub id: Uuid,
    pub name: String,
    pub css: String,
    pub colors: Vec<String>,
    pub tags: Vec<String>,
}

impl BlockGradient {
    pub fn new(
        name: impl Into<String>,
        css: impl Into<String>,
        colors: Vec<String>,
        tags: Vec<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            css: css.into(),
            colors,
            tags,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub component_ids: Vec<Uuid>,
    pub base_layout: String,
    pub default_variables: serde_json::Value,
}

impl BlockTemplate {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        component_ids: Vec<Uuid>,
        base_layout: impl Into<String>,
        default_variables: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            component_ids,
            base_layout: base_layout.into(),
            default_variables,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockStats {
    pub total_components: usize,
    pub total_palettes: usize,
    pub total_gradients: usize,
    pub total_templates: usize,
    pub category_counts: HashMap<BlockCategory, usize>,
    pub framework_counts: HashMap<BlockFramework, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_serialization_and_parsing() {
        assert_eq!(
            BlockCategory::from_str_loose("navbar"),
            BlockCategory::Navbar
        );
        assert_eq!(
            BlockCategory::from_str_loose("hero-section"),
            BlockCategory::Hero
        );
        assert_eq!(
            BlockCategory::from_str_loose("pricing_table"),
            BlockCategory::Pricing
        );
        assert_eq!(
            BlockCategory::from_str_loose("unknown_foo"),
            BlockCategory::Other
        );
        assert_eq!(BlockCategory::from_str_loose("CTA"), BlockCategory::Cta);
        assert_eq!(
            BlockCategory::from_str_loose("e-commerce"),
            BlockCategory::Ecommerce
        );

        // Test serde serialization
        let cat = BlockCategory::Navbar;
        let json = serde_json::to_string(&cat).unwrap();
        assert_eq!(json, "\"navbar\"");
        let deserialized: BlockCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, BlockCategory::Navbar);
    }

    #[test]
    fn test_framework_serialization_and_parsing() {
        assert_eq!(
            BlockFramework::from_str_loose("react"),
            BlockFramework::React
        );
        assert_eq!(
            BlockFramework::from_str_loose("tailwind"),
            BlockFramework::Tailwind
        );
        assert_eq!(
            BlockFramework::from_str_loose("tailwindcss"),
            BlockFramework::Tailwind
        );
        assert_eq!(BlockFramework::from_str_loose("css"), BlockFramework::Css);
        assert_eq!(
            BlockFramework::from_str_loose("vanillacss"),
            BlockFramework::Css
        );
        assert_eq!(
            BlockFramework::from_str_loose("shadcn"),
            BlockFramework::Shadcn
        );
        assert_eq!(
            BlockFramework::from_str_loose("shadcnui"),
            BlockFramework::Shadcn
        );
        assert_eq!(BlockFramework::from_str_loose("sass"), BlockFramework::Scss);
        assert_eq!(BlockFramework::from_str_loose("scss"), BlockFramework::Scss);
        assert_eq!(BlockFramework::from_str_loose("tsx"), BlockFramework::React);
        assert_eq!(
            BlockFramework::from_str_loose("svelte"),
            BlockFramework::Svelte
        );

        // Test serde serialization
        let fw = BlockFramework::Tailwind;
        let json = serde_json::to_string(&fw).unwrap();
        assert_eq!(json, "\"tailwind\"");
        let deserialized: BlockFramework = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, BlockFramework::Tailwind);
    }

    #[test]
    fn test_palette_hex_validation() {
        let valid = BlockPalette::new(
            "Nordic Frost",
            [
                "#2E3440".into(),
                "#3B4252".into(),
                "#88C0D0".into(),
                "#ECEFF4".into(),
            ],
            vec!["cold".into(), "nordic".into()],
        );
        assert!(valid.is_ok());
        let pal = valid.unwrap();
        assert_eq!(pal.name, "Nordic Frost");
        assert_eq!(pal.colors[0], "#2E3440");

        // Valid 3-char hex with '#'
        let valid_short = BlockPalette::new(
            "Short Hex",
            ["#fff".into(), "#000".into(), "#abc".into(), "#123".into()],
            vec![],
        );
        assert!(valid_short.is_ok());

        // Invalid: missing '#'
        let invalid_no_hash = BlockPalette::new(
            "Missing Hash",
            [
                "2E3440".into(),
                "#3B4252".into(),
                "#88C0D0".into(),
                "#ECEFF4".into(),
            ],
            vec![],
        );
        assert!(invalid_no_hash.is_err());

        // Invalid: malformed string
        let invalid_str = BlockPalette::new(
            "Bad Hex",
            [
                "#123".into(),
                "not-a-hex".into(),
                "#FFFFFF".into(),
                "#000000".into(),
            ],
            vec![],
        );
        assert!(invalid_str.is_err());

        // Invalid: wrong length
        let invalid_len = BlockPalette::new(
            "Bad Length",
            [
                "#12".into(),
                "#3B4252".into(),
                "#88C0D0".into(),
                "#ECEFF4".into(),
            ],
            vec![],
        );
        assert!(invalid_len.is_err());

        // Invalid: non-hex characters
        let invalid_chars = BlockPalette::new(
            "Bad Chars",
            [
                "#GGGGGG".into(),
                "#3B4252".into(),
                "#88C0D0".into(),
                "#ECEFF4".into(),
            ],
            vec![],
        );
        assert!(invalid_chars.is_err());
    }

    #[test]
    fn test_component_instantiation() {
        let comp = BlockComponent::new(
            "Responsive Navbar",
            "A clean responsive navbar with mobile drawer",
            BlockCategory::Navbar,
            BlockFramework::Tailwind,
            "<nav className=\"flex items-center justify-between p-4\">...</nav>",
            vec!["lucide-react".into()],
            vec!["nav".into(), "responsive".into()],
        );

        assert_eq!(comp.name, "Responsive Navbar");
        assert_eq!(
            comp.description,
            "A clean responsive navbar with mobile drawer"
        );
        assert_eq!(comp.category, BlockCategory::Navbar);
        assert_eq!(comp.framework, BlockFramework::Tailwind);
        assert_eq!(comp.version, 1);
        assert_eq!(comp.dependencies, vec!["lucide-react"]);
        assert_eq!(comp.tags, vec!["nav", "responsive"]);
        assert!(comp.created_at <= Utc::now());
        assert_eq!(comp.created_at, comp.updated_at);

        // Test serde serialization of component
        let json = serde_json::to_string(&comp).unwrap();
        let deserialized: BlockComponent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, comp.id);
        assert_eq!(deserialized.name, comp.name);
    }

    #[test]
    fn test_palette_token_formatting() {
        let pal = BlockPalette::new(
            "Cyber Neon",
            [
                "#0D0E15".into(),
                "#1A1C29".into(),
                "#00FFCC".into(),
                "#FFFFFF".into(),
            ],
            vec!["cyberpunk".into(), "neon".into()],
        )
        .unwrap();

        // 1. CSS Variables
        let css = pal.to_css_variables();
        assert!(css.contains("/* CSS Variables Export */"));
        assert!(css.contains("--bg: #0D0E15;"));
        assert!(css.contains("--surface: #1A1C29;"));
        assert!(css.contains("--accent: #00FFCC;"));
        assert!(css.contains("--text: #FFFFFF;"));

        // 2. Tailwind Config
        let tw = pal.to_tailwind_config();
        assert!(tw.contains("// Tailwind CSS Theme Colors"));
        assert!(tw.contains("bg: '#0D0E15'"));
        assert!(tw.contains("surface: '#1A1C29'"));
        assert!(tw.contains("accent: '#00FFCC'"));
        assert!(tw.contains("text: '#FFFFFF'"));
        assert!(tw.contains("'theme-bg': '#0D0E15'"));
        assert!(tw.contains("'theme-accent': '#00FFCC'"));

        // 3. SCSS Variables
        let scss = pal.to_scss_variables();
        assert!(scss.contains("// SCSS Variables Export"));
        assert!(scss.contains("$color-bg: #0D0E15;"));
        assert!(scss.contains("$color-surface: #1A1C29;"));
        assert!(scss.contains("$color-accent: #00FFCC;"));
        assert!(scss.contains("$color-text: #FFFFFF;"));

        // 4. JSON Tokens
        let json_tokens = pal.to_json_tokens();
        assert!(json_tokens.contains("\"name\": \"Cyber Neon\""));
        assert!(json_tokens.contains("\"bg\": \"#0D0E15\""));
        assert!(json_tokens.contains("\"surface\": \"#1A1C29\""));
        assert!(json_tokens.contains("\"accent\": \"#00FFCC\""));
        assert!(json_tokens.contains("\"text\": \"#FFFFFF\""));

        // 5. format_tokens dispatcher
        let (code, lang) = pal.format_tokens("tailwind");
        assert_eq!(lang, "javascript");
        assert!(code.contains("colors:"));

        let (code, lang) = pal.format_tokens("tw");
        assert_eq!(lang, "javascript");
        assert!(code.contains("colors:"));

        let (code, lang) = pal.format_tokens("scss");
        assert_eq!(lang, "scss");
        assert!(code.contains("$color-bg:"));

        let (code, lang) = pal.format_tokens("sass");
        assert_eq!(lang, "scss");
        assert!(code.contains("$color-bg:"));

        let (code, lang) = pal.format_tokens("json");
        assert_eq!(lang, "json");
        assert!(code.contains("\"Cyber Neon\""));

        let (code, lang) = pal.format_tokens("tokens");
        assert_eq!(lang, "json");
        assert!(code.contains("\"Cyber Neon\""));

        let (code, lang) = pal.format_tokens("css");
        assert_eq!(lang, "css");
        assert!(code.contains(":root"));

        // Unknown defaults to CSS
        let (code, lang) = pal.format_tokens("unknown_fmt");
        assert_eq!(lang, "css");
        assert!(code.contains(":root"));
    }
}
