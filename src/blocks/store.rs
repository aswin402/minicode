use crate::blocks::models::{
    BlockCategory, BlockComponent, BlockGradient, BlockPalette, BlockStats, BlockTemplate,
};
use crate::blocks::BlockError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use uuid::Uuid;

/// Search filter parameters for querying components.
#[derive(Debug, Clone, Default)]
pub struct BlockSearchFilter {
    pub query: Option<String>,
    pub category: Option<BlockCategory>,
    pub framework: Option<crate::blocks::models::BlockFramework>,
    pub tags: Option<Vec<String>>,
    pub limit: usize,
}

/// Search result item containing component metadata and match score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSearchResult {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub category: BlockCategory,
    pub framework: crate::blocks::models::BlockFramework,
    pub tags: Vec<String>,
    pub version: u32,
    pub score: f64,
}

/// Internal serialization schema for disk persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreData {
    components: Vec<BlockComponent>,
    component_versions: HashMap<Uuid, Vec<(u32, String, DateTime<Utc>)>>,
    palettes: Vec<BlockPalette>,
    gradients: Vec<BlockGradient>,
    templates: Vec<BlockTemplate>,
}

/// Fast in-memory component and design token warehouse with inverted indices.
pub struct BlockStore {
    components: HashMap<Uuid, BlockComponent>,
    component_versions: HashMap<Uuid, Vec<(u32, String, DateTime<Utc>)>>,
    palettes: HashMap<Uuid, BlockPalette>,
    gradients: HashMap<Uuid, BlockGradient>,
    templates: HashMap<Uuid, BlockTemplate>,
    // Inverted indices for O(1) filtering
    category_index: HashMap<BlockCategory, HashSet<Uuid>>,
    framework_index: HashMap<crate::blocks::models::BlockFramework, HashSet<Uuid>>,
    tag_index: HashMap<String, HashSet<Uuid>>,
    // Persistence path
    persistence_path: Option<PathBuf>,
}

impl BlockStore {
    /// Creates a new empty `BlockStore` without default persistence.
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            component_versions: HashMap::new(),
            palettes: HashMap::new(),
            gradients: HashMap::new(),
            templates: HashMap::new(),
            category_index: HashMap::new(),
            framework_index: HashMap::new(),
            tag_index: HashMap::new(),
            persistence_path: None,
        }
    }

    /// Creates an isolated `BlockStore` with a dedicated persistence file path.
    pub fn new_isolated(persistence_path: PathBuf) -> Self {
        let mut store = Self::new();
        store.persistence_path = Some(persistence_path);
        store
    }

    // --- Component Operations ---

    /// Inserts a component and updates all inverted indices.
    pub fn insert_component(&mut self, component: BlockComponent) -> Result<(), BlockError> {
        let id = component.id;

        // Update category index
        self.category_index
            .entry(component.category)
            .or_default()
            .insert(id);

        // Update framework index
        self.framework_index
            .entry(component.framework)
            .or_default()
            .insert(id);

        // Update tag index (normalized lowercase)
        for tag in &component.tags {
            let norm = tag.trim().to_lowercase();
            if !norm.is_empty() {
                self.tag_index.entry(norm).or_default().insert(id);
            }
        }

        self.components.insert(id, component);
        let _ = self.persist_if_configured();
        Ok(())
    }

    /// Retrieves a component reference by its UUID.
    pub fn get_component(&self, id: &Uuid) -> Option<&BlockComponent> {
        self.components.get(id)
    }

    /// Retrieves a component by case-insensitive name.
    pub fn get_component_by_name(&self, name: &str) -> Option<&BlockComponent> {
        let target = name.trim().to_lowercase();
        self.components
            .values()
            .find(|c| c.name.trim().to_lowercase() == target)
    }

    /// Searches components using inverted indices and multi-token fuzzy/substring scoring.
    pub fn search_components(&self, filter: &BlockSearchFilter) -> Vec<BlockSearchResult> {
        let mut candidate_ids: Option<HashSet<Uuid>> = None;

        // 1. Filter by category index
        if let Some(cat) = filter.category {
            if let Some(ids) = self.category_index.get(&cat) {
                candidate_ids = Some(ids.clone());
            } else {
                return Vec::new();
            }
        }

        // 2. Filter by framework index
        if let Some(fw) = filter.framework {
            if let Some(ids) = self.framework_index.get(&fw) {
                candidate_ids = match candidate_ids {
                    Some(existing) => Some(existing.intersection(ids).copied().collect()),
                    None => Some(ids.clone()),
                };
            } else {
                return Vec::new();
            }
        }

        // 3. Filter by required tags
        if let Some(ref req_tags) = filter.tags {
            for tag in req_tags {
                let norm = tag.trim().to_lowercase();
                if norm.is_empty() {
                    continue;
                }
                if let Some(ids) = self.tag_index.get(&norm) {
                    candidate_ids = match candidate_ids {
                        Some(existing) => Some(existing.intersection(ids).copied().collect()),
                        None => Some(ids.clone()),
                    };
                } else {
                    return Vec::new();
                }
            }
        }

        // 4. Score matching components against query tokens
        let query_tokens: Vec<String> = filter
            .query
            .as_deref()
            .map(|q| {
                q.split_whitespace()
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let mut scored_results: Vec<BlockSearchResult> = Vec::new();

        let candidates_iter: Box<dyn Iterator<Item = &BlockComponent>> = match candidate_ids {
            Some(ref ids) => Box::new(ids.iter().filter_map(|id| self.components.get(id))),
            None => Box::new(self.components.values()),
        };

        for comp in candidates_iter {
            let mut score = 1.0;

            if !query_tokens.is_empty() {
                let name_lower = comp.name.to_lowercase();
                let desc_lower = comp.description.to_lowercase();
                let tags_lower: Vec<String> = comp.tags.iter().map(|t| t.to_lowercase()).collect();

                let mut matched_all = true;
                let mut token_score = 0.0;

                for token in &query_tokens {
                    let in_name = name_lower.contains(token);
                    let in_tags = tags_lower.iter().any(|t| t.contains(token));
                    let in_desc = desc_lower.contains(token);

                    if in_name || in_tags || in_desc {
                        if in_name {
                            token_score += 10.0;
                            if name_lower == *token {
                                token_score += 15.0;
                            }
                        }
                        if in_tags {
                            token_score += 5.0;
                        }
                        if in_desc {
                            token_score += 2.0;
                        }
                    } else {
                        matched_all = false;
                        break;
                    }
                }

                if !matched_all {
                    continue;
                }
                score += token_score;
            }

            scored_results.push(BlockSearchResult {
                id: comp.id,
                name: comp.name.clone(),
                description: comp.description.clone(),
                category: comp.category,
                framework: comp.framework,
                tags: comp.tags.clone(),
                version: comp.version,
                score,
            });
        }

        // Sort descending by score
        scored_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let limit = if filter.limit > 0 { filter.limit } else { 20 };
        scored_results.truncate(limit);
        scored_results
    }

    /// Updates component code, description, and/or tags.
    /// Appends old code to version history and increments version counter.
    pub fn update_component(
        &mut self,
        id: &Uuid,
        new_code: Option<&str>,
        new_description: Option<&str>,
        new_tags: Option<Vec<String>>,
    ) -> Result<BlockComponent, BlockError> {
        let component = self
            .components
            .get_mut(id)
            .ok_or_else(|| BlockError::ComponentNotFound(id.to_string()))?;

        // Archive previous code version
        let current_version = component.version;
        let current_code = component.code.clone();
        let now = Utc::now();

        self.component_versions
            .entry(*id)
            .or_default()
            .push((current_version, current_code, now));

        if let Some(code) = new_code {
            component.code = code.to_string();
        }
        if let Some(desc) = new_description {
            component.description = desc.to_string();
        }
        if let Some(tags) = new_tags {
            // Remove old tags from index
            for old_tag in &component.tags {
                let norm = old_tag.trim().to_lowercase();
                if let Some(ids) = self.tag_index.get_mut(&norm) {
                    ids.remove(id);
                }
            }
            // Add new tags to index
            for new_tag in &tags {
                let norm = new_tag.trim().to_lowercase();
                if !norm.is_empty() {
                    self.tag_index.entry(norm).or_default().insert(*id);
                }
            }
            component.tags = tags;
        }

        component.version += 1;
        component.updated_at = now;

        let updated_copy = component.clone();
        let _ = self.persist_if_configured();
        Ok(updated_copy)
    }

    /// Deletes a component from storage and cleans up all inverted indices.
    pub fn delete_component(&mut self, id: &Uuid) -> Result<(), BlockError> {
        let component = self
            .components
            .remove(id)
            .ok_or_else(|| BlockError::ComponentNotFound(id.to_string()))?;

        // Remove from category index
        if let Some(ids) = self.category_index.get_mut(&component.category) {
            ids.remove(id);
        }

        // Remove from framework index
        if let Some(ids) = self.framework_index.get_mut(&component.framework) {
            ids.remove(id);
        }

        // Remove from tag index
        for tag in &component.tags {
            let norm = tag.trim().to_lowercase();
            if let Some(ids) = self.tag_index.get_mut(&norm) {
                ids.remove(id);
            }
        }

        self.component_versions.remove(id);
        let _ = self.persist_if_configured();
        Ok(())
    }

    /// Retrieves the version history for a given component.
    pub fn get_component_history(&self, id: &Uuid) -> Vec<(u32, String, DateTime<Utc>)> {
        self.component_versions.get(id).cloned().unwrap_or_default()
    }

    // --- Palette Operations ---

    /// Inserts a 4-hex color palette.
    pub fn insert_palette(&mut self, palette: BlockPalette) -> Result<(), BlockError> {
        self.palettes.insert(palette.id, palette);
        let _ = self.persist_if_configured();
        Ok(())
    }

    /// Retrieves a palette by UUID.
    pub fn get_palette(&self, id: &Uuid) -> Option<&BlockPalette> {
        self.palettes.get(id)
    }

    /// Lists all palettes.
    pub fn list_palettes(&self) -> Vec<&BlockPalette> {
        self.palettes.values().collect()
    }

    /// Searches palettes by name or tags.
    pub fn search_palettes(&self, query: &str) -> Vec<&BlockPalette> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.list_palettes();
        }
        self.palettes
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Deletes a palette.
    pub fn delete_palette(&mut self, id: &Uuid) -> Result<(), BlockError> {
        self.palettes
            .remove(id)
            .ok_or_else(|| BlockError::PaletteNotFound(id.to_string()))?;
        let _ = self.persist_if_configured();
        Ok(())
    }

    // --- Gradient Operations ---

    /// Inserts a CSS gradient.
    pub fn insert_gradient(&mut self, gradient: BlockGradient) -> Result<(), BlockError> {
        self.gradients.insert(gradient.id, gradient);
        let _ = self.persist_if_configured();
        Ok(())
    }

    /// Retrieves a gradient by UUID.
    pub fn get_gradient(&self, id: &Uuid) -> Option<&BlockGradient> {
        self.gradients.get(id)
    }

    /// Lists all gradients.
    pub fn list_gradients(&self) -> Vec<&BlockGradient> {
        self.gradients.values().collect()
    }

    /// Searches gradients by name or tags.
    pub fn search_gradients(&self, query: &str) -> Vec<&BlockGradient> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.list_gradients();
        }
        self.gradients
            .values()
            .filter(|g| {
                g.name.to_lowercase().contains(&q)
                    || g.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Deletes a gradient.
    pub fn delete_gradient(&mut self, id: &Uuid) -> Result<(), BlockError> {
        self.gradients
            .remove(id)
            .ok_or_else(|| BlockError::GradientNotFound(id.to_string()))?;
        let _ = self.persist_if_configured();
        Ok(())
    }

    // --- Template Operations ---

    /// Inserts a layout template.
    pub fn insert_template(&mut self, template: BlockTemplate) -> Result<(), BlockError> {
        self.templates.insert(template.id, template);
        let _ = self.persist_if_configured();
        Ok(())
    }

    /// Retrieves a template by UUID.
    pub fn get_template(&self, id: &Uuid) -> Option<&BlockTemplate> {
        self.templates.get(id)
    }

    /// Lists all templates.
    pub fn list_templates(&self) -> Vec<&BlockTemplate> {
        self.templates.values().collect()
    }

    /// Deletes a template.
    pub fn delete_template(&mut self, id: &Uuid) -> Result<(), BlockError> {
        self.templates
            .remove(id)
            .ok_or_else(|| BlockError::TemplateNotFound(id.to_string()))?;
        let _ = self.persist_if_configured();
        Ok(())
    }

    // --- Library Stats ---

    /// Returns aggregate counts and breakdown by category and framework.
    pub fn stats(&self) -> BlockStats {
        let mut category_counts = HashMap::new();
        for (cat, ids) in &self.category_index {
            if !ids.is_empty() {
                category_counts.insert(*cat, ids.len());
            }
        }

        let mut framework_counts = HashMap::new();
        for (fw, ids) in &self.framework_index {
            if !ids.is_empty() {
                framework_counts.insert(*fw, ids.len());
            }
        }

        BlockStats {
            total_components: self.components.len(),
            total_palettes: self.palettes.len(),
            total_gradients: self.gradients.len(),
            total_templates: self.templates.len(),
            category_counts,
            framework_counts,
        }
    }

    // --- Disk Persistence ---

    /// Saves all components, version history, palettes, gradients, and templates to a JSON file.
    pub fn save_to_disk(&self, path: &Path) -> Result<(), BlockError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = StoreData {
            components: self.components.values().cloned().collect(),
            component_versions: self.component_versions.clone(),
            palettes: self.palettes.values().cloned().collect(),
            gradients: self.gradients.values().cloned().collect(),
            templates: self.templates.values().cloned().collect(),
        };

        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Loads store data from a JSON file and populates all in-memory inverted indices.
    pub fn load_from_disk(path: &Path) -> Result<Self, BlockError> {
        if !path.exists() {
            let mut store = Self::new();
            store.persistence_path = Some(path.to_path_buf());
            return Ok(store);
        }

        let content = std::fs::read_to_string(path)?;
        let data: StoreData = serde_json::from_str(&content)?;

        let mut store = Self::new();
        store.persistence_path = Some(path.to_path_buf());

        for comp in data.components {
            let _ = store.insert_component(comp);
        }
        store.component_versions = data.component_versions;

        for pal in data.palettes {
            let _ = store.insert_palette(pal);
        }
        for grad in data.gradients {
            let _ = store.insert_gradient(grad);
        }
        for tmpl in data.templates {
            let _ = store.insert_template(tmpl);
        }

        Ok(store)
    }

    /// Helper to auto-persist if a persistence path is configured.
    pub fn persist_if_configured(&self) -> Result<(), BlockError> {
        if let Some(ref path) = self.persistence_path {
            self.save_to_disk(path)?;
        }
        Ok(())
    }
}

impl Default for BlockStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Global singleton store instance for the active minicode session.
static GLOBAL_BLOCK_STORE: OnceLock<RwLock<BlockStore>> = OnceLock::new();

/// Returns a reference to the global thread-safe `BlockStore`.
pub fn get_global_block_store() -> &'static RwLock<BlockStore> {
    GLOBAL_BLOCK_STORE.get_or_init(|| {
        let default_path = dirs::data_local_dir()
            .map(|d| d.join("minicode").join("miniblocks").join("store.json"))
            .unwrap_or_else(|| PathBuf::from(".minicode/blocks/store.json"));

        let store = BlockStore::load_from_disk(&default_path).unwrap_or_else(|_| {
            let mut s = BlockStore::new();
            s.persistence_path = Some(default_path);
            s
        });

        RwLock::new(store)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::models::{BlockCategory, BlockFramework};
    use tempfile::tempdir;

    #[test]
    fn test_store_crud_and_indexing() {
        let temp = tempdir().unwrap();
        let mut store = BlockStore::new_isolated(temp.path().join("blocks.json"));

        let comp = BlockComponent::new(
            "modern-hero",
            "Modern SaaS hero with animated beam",
            BlockCategory::Hero,
            BlockFramework::React,
            "export function Hero() { return <div>Hero</div>; }",
            vec!["framer-motion".into()],
            vec!["hero".into(), "dark".into(), "saas".into()],
        );
        let id = comp.id;
        store.insert_component(comp).unwrap();

        // 1. Query by id
        let fetched = store.get_component(&id).unwrap();
        assert_eq!(fetched.name, "modern-hero");

        // 2. Query by name
        let by_name = store.get_component_by_name("modern-hero").unwrap();
        assert_eq!(by_name.id, id);

        // 3. Search with query and filters
        let results = store.search_components(&BlockSearchFilter {
            query: Some("animated beam".into()),
            category: Some(BlockCategory::Hero),
            framework: Some(BlockFramework::React),
            tags: Some(vec!["dark".into()]),
            limit: 10,
        });
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
        assert!(results[0].score > 1.0);

        // 4. Update component with version increment
        let updated = store
            .update_component(&id, Some("New Hero Code"), None, None)
            .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(store.get_component(&id).unwrap().code, "New Hero Code");

        let history = store.get_component_history(&id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].0, 1);
        assert!(history[0].1.contains("<div>Hero</div>"));

        // 5. Delete component
        assert!(store.delete_component(&id).is_ok());
        assert!(store.get_component(&id).is_none());
        assert!(store.get_component_by_name("modern-hero").is_none());
    }

    #[test]
    fn test_palette_and_gradient_search() {
        let mut store = BlockStore::new();

        let palette = BlockPalette::new(
            "Nordic Frost",
            [
                "#2E3440".into(),
                "#3B4252".into(),
                "#88C0D0".into(),
                "#ECEFF4".into(),
            ],
            vec!["nordic".into(), "frost".into()],
        )
        .unwrap();
        let pal_id = palette.id;
        store.insert_palette(palette).unwrap();

        let gradient = BlockGradient::new(
            "Neon Glow",
            "linear-gradient(90deg, #FF007F 0%, #7F00FF 100%)",
            vec!["#FF007F".into(), "#7F00FF".into()],
            vec!["neon".into(), "vibrant".into()],
        );
        let grad_id = gradient.id;
        store.insert_gradient(gradient).unwrap();

        assert_eq!(store.list_palettes().len(), 1);
        assert_eq!(store.list_gradients().len(), 1);

        let pal_results = store.search_palettes("frost");
        assert_eq!(pal_results.len(), 1);
        assert_eq!(pal_results[0].id, pal_id);

        let grad_results = store.search_gradients("neon");
        assert_eq!(grad_results.len(), 1);
        assert_eq!(grad_results[0].id, grad_id);

        assert!(store.delete_palette(&pal_id).is_ok());
        assert!(store.delete_gradient(&grad_id).is_ok());
        assert_eq!(store.list_palettes().len(), 0);
        assert_eq!(store.list_gradients().len(), 0);
    }

    #[test]
    fn test_disk_persistence_roundtrip() {
        let temp = tempdir().unwrap();
        let store_file = temp.path().join("miniblocks").join("store.json");

        let mut store = BlockStore::new_isolated(store_file.clone());
        let comp = BlockComponent::new(
            "Test Card",
            "A test card component",
            BlockCategory::Card,
            BlockFramework::Tailwind,
            "<div className=\"card\">Test</div>",
            vec![],
            vec!["test".into(), "card".into()],
        );
        let id = comp.id;
        store.insert_component(comp).unwrap();

        // Ensure file exists
        assert!(store_file.exists());

        // Reload from disk
        let loaded = BlockStore::load_from_disk(&store_file).unwrap();
        assert_eq!(loaded.stats().total_components, 1);
        let loaded_comp = loaded.get_component(&id).unwrap();
        assert_eq!(loaded_comp.name, "Test Card");
    }
}
