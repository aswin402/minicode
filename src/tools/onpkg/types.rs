use serde::{Deserialize, Serialize};

/// Metadata information about an available onpkg stack template.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnpkgStackInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    pub version: String,
    pub files_count: usize,
    pub technologies: Vec<String>,
}

/// Metadata information about an onpkg agent skill.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnpkgSkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}
