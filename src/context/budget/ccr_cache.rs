use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{OnceLock, RwLock};

const DEFAULT_MAX_CACHE_BYTES: usize = 50 * 1024 * 1024; // 50 MB
const DEFAULT_MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone)]
pub struct CachedObservation {
    #[allow(dead_code)]
    pub id: String,
    pub full_text: String,
    pub byte_size: usize,
}

#[derive(Debug)]
pub struct CcrCacheInner {
    entries: HashMap<String, CachedObservation>,
    order: VecDeque<String>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl CcrCacheInner {
    pub fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            total_bytes: 0,
            max_bytes,
            max_entries,
        }
    }

    pub fn insert(&mut self, content: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        let hash_val = hasher.finish();
        let id = format!("ccr_{:010x}", hash_val);

        if self.entries.contains_key(&id) {
            // Already cached; refresh LRU order
            self.order.retain(|k| k != &id);
            self.order.push_back(id.clone());
            return id;
        }

        let byte_size = content.len();

        // Evict if capacity exceeded
        while (self.total_bytes + byte_size > self.max_bytes
            || self.entries.len() >= self.max_entries)
            && !self.order.is_empty()
        {
            if let Some(oldest_id) = self.order.pop_front() {
                if let Some(removed) = self.entries.remove(&oldest_id) {
                    self.total_bytes = self.total_bytes.saturating_sub(removed.byte_size);
                }
            }
        }

        self.entries.insert(
            id.clone(),
            CachedObservation {
                id: id.clone(),
                full_text: content.to_string(),
                byte_size,
            },
        );
        self.order.push_back(id.clone());
        self.total_bytes += byte_size;

        id
    }

    pub fn get(&mut self, id: &str) -> Option<&CachedObservation> {
        if self.entries.contains_key(id) {
            self.order.retain(|k| k != id);
            self.order.push_back(id.to_string());
            self.entries.get(id)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

static GLOBAL_CCR_CACHE: OnceLock<RwLock<CcrCacheInner>> = OnceLock::new();

fn get_cache() -> &'static RwLock<CcrCacheInner> {
    GLOBAL_CCR_CACHE.get_or_init(|| {
        RwLock::new(CcrCacheInner::new(
            DEFAULT_MAX_CACHE_BYTES,
            DEFAULT_MAX_ENTRIES,
        ))
    })
}

pub struct CcrCache;

impl CcrCache {
    /// Stores an observation in the cache and returns its CCR reference id (`ccr_xxxx`).
    pub fn store(content: &str) -> String {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.insert(content)
    }

    /// Losslessly retrieves an observation by its CCR reference id with optional offset and limit (line-based).
    pub fn retrieve(id: &str, offset: Option<usize>, limit: Option<usize>) -> Option<String> {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        let obs = cache.get(id)?;
        let full = &obs.full_text;

        if offset.is_none() && limit.is_none() {
            return Some(full.clone());
        }

        let lines: Vec<&str> = full.lines().collect();
        let start = offset.unwrap_or(0).min(lines.len());
        let count = limit.unwrap_or(lines.len().saturating_sub(start));
        let end = (start + count).min(lines.len());

        let sliced = lines[start..end].join("\n");
        Some(sliced)
    }

    /// Returns the number of cached items in the CCR cache.
    #[allow(dead_code)]
    pub fn len() -> usize {
        let cache = get_cache().read().unwrap_or_else(|e| e.into_inner());
        cache.len()
    }

    /// Clears the global CCR cache.
    #[allow(dead_code)]
    pub fn clear() {
        let mut cache = get_cache().write().unwrap_or_else(|e| e.into_inner());
        cache.clear();
    }
}
