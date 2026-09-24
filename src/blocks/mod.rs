pub mod models;
pub mod store;

pub use models::*;
pub use store::*;

#[derive(Debug, thiserror::Error)]
pub enum BlockError {
    #[error("Component not found: {0}")]
    ComponentNotFound(String),
    #[error("Palette not found: {0}")]
    PaletteNotFound(String),
    #[error("Gradient not found: {0}")]
    GradientNotFound(String),
    #[error("Template not found: {0}")]
    TemplateNotFound(String),
    #[error("Invalid hex color code: '{0}' (expected #RGB or #RRGGBB)")]
    InvalidHexColor(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
