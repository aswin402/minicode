use crate::context::budget::ccr_cache::CcrCache;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub struct JsonCrusher;

impl JsonCrusher {
    /// Inspects text to see if it is a large JSON array of objects.
    /// If so, factors common fields, preserves all errors, samples repetitive middle elements,
    /// caches the uncompressed raw output into CcrCache, and returns `(crushed_json, ccr_id)`.
    pub fn crush(raw_json: &str) -> Option<(String, String)> {
        let trimmed = raw_json.trim();
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return None;
        }

        let parsed: Value = serde_json::from_str(trimmed).ok()?;
        let array = match parsed {
            Value::Array(arr) if arr.len() >= 4 => arr,
            _ => return None,
        };

        // Ensure all elements are JSON objects
        if !array.iter().all(|item| item.is_object()) {
            return None;
        }

        let total_count = array.len();

        // 1. Identify common fields identical across ALL elements
        let mut common_fields: Map<String, Value> = Map::new();
        if let Some(first_obj) = array[0].as_object() {
            for (key, val) in first_obj {
                let is_common = array
                    .iter()
                    .all(|item| item.as_object().and_then(|obj| obj.get(key)) == Some(val));
                if is_common {
                    common_fields.insert(key.clone(), val.clone());
                }
            }
        }

        // 2. Scan for error/failure objects that must NEVER be omitted
        let mut must_keep_indices: HashSet<usize> = HashSet::new();
        for (idx, item) in array.iter().enumerate() {
            if let Some(obj) = item.as_object() {
                let has_error = obj.iter().any(|(k, v)| {
                    let k_lower = k.to_lowercase();
                    if k_lower.contains("error")
                        || k_lower.contains("fail")
                        || k_lower.contains("panic")
                    {
                        return true;
                    }
                    if let Some(s) = v.as_str() {
                        let s_lower = s.to_lowercase();
                        if s_lower.contains("error")
                            || s_lower.contains("failed")
                            || s_lower.contains("timeout")
                            || s_lower.contains("panic")
                        {
                            return true;
                        }
                    }
                    false
                });
                if has_error {
                    must_keep_indices.insert(idx);
                }
            }
        }

        // Always keep head (first 2) and tail (last 2)
        must_keep_indices.insert(0);
        must_keep_indices.insert(1);
        if total_count > 2 {
            must_keep_indices.insert(total_count - 1);
            must_keep_indices.insert(total_count - 2);
        }

        // 3. Remove common fields from retained elements and construct compressed array
        let mut retained_items: Vec<Value> = Vec::new();
        let mut omitted_count = 0;

        for (idx, mut item) in array.into_iter().enumerate() {
            if must_keep_indices.contains(&idx) {
                if let Some(obj) = item.as_object_mut() {
                    for key in common_fields.keys() {
                        obj.remove(key);
                    }
                }
                retained_items.push(item);
            } else {
                omitted_count += 1;
            }
        }

        // Store original into CcrCache for lossless recovery
        let ccr_id = CcrCache::store(raw_json);

        // Build crushed envelope
        let mut envelope: Map<String, Value> = Map::new();
        envelope.insert("_crushed".to_string(), Value::Bool(true));
        envelope.insert(
            "_total_count".to_string(),
            Value::Number(serde_json::Number::from(total_count)),
        );

        if omitted_count > 0 {
            envelope.insert(
                "_omitted_count".to_string(),
                Value::Number(serde_json::Number::from(omitted_count)),
            );
            envelope.insert("_ccr_id".to_string(), Value::String(ccr_id.clone()));
            envelope.insert(
                "_ccr_hint".to_string(),
                Value::String(format!(
                    "Use tool retrieve_observation(id=\"{}\") to retrieve all {} items losslessly.",
                    ccr_id, total_count
                )),
            );
        }

        if !common_fields.is_empty() {
            envelope.insert("_common_fields".to_string(), Value::Object(common_fields));
        }

        envelope.insert("items".to_string(), Value::Array(retained_items));

        let crushed_json = serde_json::to_string_pretty(&Value::Object(envelope)).ok()?;

        // If compression did not actually save space (or was negligible), don't crush
        if crushed_json.len() >= raw_json.len() && omitted_count == 0 {
            return None;
        }

        Some((crushed_json, ccr_id))
    }
}
