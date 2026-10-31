use crate::context::budget::ccr_cache::CcrCache;
use crate::context::budget::headroom::{AdaptiveSizer, AnchorSelector};
use serde_json::{Map, Value};

pub struct JsonCrusher;

impl JsonCrusher {
    /// Inspects text to see if it is a large JSON array of objects OR a JSON object wrapping a large array.
    /// Uses `AdaptiveSizer` (Kneedle unique-bigram saturation + 4-gram SimHash) and `AnchorSelector`
    /// (error preservation, numeric z-score outliers, and middle-slot information density) to compress
    /// the array, computes `_numeric_stats` across all items, caches the uncompressed raw output into
    /// `CcrCache`, and returns `(crushed_json, ccr_id)`.
    pub fn crush(raw_json: &str) -> Option<(String, String)> {
        let trimmed = raw_json.trim();
        if !(trimmed.starts_with('[') && trimmed.ends_with(']'))
            && !(trimmed.starts_with('{') && trimmed.ends_with('}'))
        {
            return None;
        }

        let parsed: Value = serde_json::from_str(trimmed).ok()?;

        match parsed {
            Value::Array(arr) if arr.len() >= 5 => Self::crush_object_array(raw_json, arr, None),
            Value::Object(mut root_obj) => {
                // Find the largest array-of-objects field inside the root JSON object
                let mut target_key: Option<String> = None;
                let mut best_len = 0usize;
                for (k, v) in &root_obj {
                    if let Value::Array(arr) = v {
                        if arr.len() >= 6
                            && arr.len() > best_len
                            && arr.iter().all(|item| item.is_object())
                        {
                            best_len = arr.len();
                            target_key = Some(k.clone());
                        }
                    }
                }
                let key = target_key?;
                let Some(Value::Array(arr)) = root_obj.remove(&key) else {
                    return None;
                };
                Self::crush_object_array(raw_json, arr, Some((root_obj, key)))
            }
            _ => None,
        }
    }

    fn crush_object_array(
        raw_json: &str,
        array: Vec<Value>,
        wrapper: Option<(Map<String, Value>, String)>,
    ) -> Option<(String, String)> {
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

        // 2. Compute numeric summary statistics across the full array before omitting items
        let numeric_stats = AnchorSelector::compute_numeric_summaries(&array);

        // 3. Determine optimal K via AdaptiveSizer (Kneedle + SimHash) and select indices via AnchorSelector
        let serialized_items: Vec<String> = array.iter().map(|v| v.to_string()).collect();
        let optimal_k = AdaptiveSizer::compute_optimal_k(&serialized_items, 4, 16);
        let must_keep_indices = AnchorSelector::select_indices(&array, optimal_k);

        // 4. Remove common fields from retained elements and construct compressed array
        let mut retained_items: Vec<Value> = Vec::new();
        let mut omitted_count = 0usize;

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
        let mut envelope: Map<String, Value> = match wrapper {
            Some((existing_root, _)) => existing_root,
            None => Map::new(),
        };
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
                    "Use retrieve_observation(id=\"{}\") or retrieve_observation(id=\"{}\", query=\"...\") to retrieve all {} items losslessly.",
                    ccr_id, ccr_id, total_count
                )),
            );
        }

        if !common_fields.is_empty() {
            envelope.insert("_common_fields".to_string(), Value::Object(common_fields));
        }

        if !numeric_stats.is_empty() && omitted_count > 0 {
            envelope.insert("_numeric_stats".to_string(), Value::Object(numeric_stats));
        }

        envelope.insert("items".to_string(), Value::Array(retained_items));

        let crushed_json = if raw_json.lines().count() <= 3 {
            serde_json::to_string(&Value::Object(envelope)).ok()?
        } else {
            serde_json::to_string_pretty(&Value::Object(envelope)).ok()?
        };

        if crushed_json.len() >= raw_json.len() {
            return None;
        }

        Some((crushed_json, ccr_id))
    }
}
