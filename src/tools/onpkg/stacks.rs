use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StackFile {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_content: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StackHook {
    pub command: String,
    pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Stack {
    pub name: String,
    pub runtime: String,
    pub description: String,
    pub packages: Vec<String>,
    pub dev_packages: Vec<String>,
    #[serde(default)]
    pub transitive_packages: Vec<String>,
    pub files: Vec<StackFile>,
    #[serde(default)]
    pub hooks: Vec<StackHook>,
}

#[path = "templates/builtin/stacks_mod.rs"]
pub mod builtin;
