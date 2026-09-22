use serde::{Deserialize, Serialize};

/// Metadata information about an available MiniKit stack template.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MiniKitStackInfo {
    pub name: String,
    pub category: String,
    pub description: String,
    pub version: String,
    pub files_count: usize,
    pub technologies: Vec<String>,
}

/// Metadata information about a MiniKit agent skill.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MiniKitSkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[allow(dead_code)]
pub type OnpkgStackInfo = MiniKitStackInfo;
#[allow(dead_code)]
pub type OnpkgSkillInfo = MiniKitSkillInfo;
