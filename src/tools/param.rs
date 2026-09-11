//! Reusable parameter extraction helpers for tool arguments
//!
//! Standardizes JSON validation and reduces boilerplate across tool registries.

use crate::error::ToolError;
use serde_json::Value;

/// Extracts a mandatory string argument or returns `ToolError::InvalidArguments`
pub fn require_str<'a>(args: &'a Value, key: &str, tool_name: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidArguments {
            name: tool_name.to_string(),
            reason: format!("Missing required argument '{}'", key),
        })
}

/// Extracts an optional string argument
#[allow(dead_code)]
pub fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

/// Extracts an optional boolean argument
#[allow(dead_code)]
pub fn get_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

/// Extracts a boolean argument with a default fallback
#[allow(dead_code)]
pub fn opt_bool(args: &Value, key: &str, default: bool) -> bool {
    get_bool(args, key).unwrap_or(default)
}

/// Extracts an optional numeric `usize` parameter (accepts JSON integer or string)
#[allow(dead_code)]
pub fn get_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| {
        if let Some(n) = v.as_u64() {
            usize::try_from(n).ok()
        } else if let Some(s) = v.as_str() {
            s.parse::<usize>().ok()
        } else {
            None
        }
    })
}

/// Extracts a numeric `usize` parameter (accepts JSON integer or string), falling back to default
#[allow(dead_code)]
pub fn opt_usize(args: &Value, key: &str, default: usize) -> usize {
    get_usize(args, key).unwrap_or(default)
}

/// Extracts an optional `u64` parameter using permissive parsing
#[allow(dead_code)]
pub fn opt_u64(args: &Value, key: &str) -> Option<u64> {
    crate::tools::parse_u64_param(args.get(key))
}

/// Extracts a mandatory `u64` parameter using permissive parsing
#[allow(dead_code)]
pub fn require_u64(args: &Value, key: &str, tool_name: &str) -> Result<u64, ToolError> {
    opt_u64(args, key).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: format!("Missing required argument '{}'", key),
    })
}

/// Extracts an optional array of strings
#[allow(dead_code)]
pub fn opt_string_array(args: &Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
    })
}

/// Extracts an optional key-value string map (e.g. environment variables)
#[allow(dead_code)]
pub fn opt_string_map(
    args: &Value,
    key: &str,
) -> Option<std::collections::HashMap<String, String>> {
    args.get(key).and_then(|v| {
        v.as_object().map(|obj| {
            obj.iter()
                .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
    })
}

/// Extracts a mandatory array argument or returns `ToolError::InvalidArguments`
#[allow(dead_code)]
pub fn require_array<'a>(
    args: &'a Value,
    key: &str,
    tool_name: &str,
) -> Result<&'a Vec<Value>, ToolError> {
    args.get(key).and_then(|v| v.as_array()).ok_or_else(|| {
        ToolError::invalid_args(
            tool_name,
            format!("Missing or invalid array argument '{}'", key),
        )
    })
}

/// Extracts an optional `f64` parameter (accepts JSON number or float string)
#[allow(dead_code)]
pub fn opt_f64(args: &Value, key: &str, default: f64) -> f64 {
    args.get(key)
        .and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        })
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_param_extraction() {
        let args = json!({
            "name": "minicode",
            "active": true,
            "count": 42,
            "count_str": "100",
            "tags": ["rust", "tui", "agent"]
        });

        assert_eq!(require_str(&args, "name", "test").unwrap(), "minicode");
        assert!(require_str(&args, "missing", "test").is_err());

        assert_eq!(opt_str(&args, "name"), Some("minicode"));
        assert_eq!(opt_str(&args, "missing"), None);

        assert!(opt_bool(&args, "active", false));
        assert!(!opt_bool(&args, "missing", false));

        assert_eq!(opt_usize(&args, "count", 10), 42);
        assert_eq!(opt_usize(&args, "count_str", 10), 100);
        assert_eq!(opt_usize(&args, "missing", 10), 10);

        assert_eq!(opt_u64(&args, "count"), Some(42));
        assert_eq!(opt_u64(&args, "missing"), None);

        let tags = opt_string_array(&args, "tags").unwrap();
        assert_eq!(tags, vec!["rust", "tui", "agent"]);

        let arr = require_array(&args, "tags", "test").unwrap();
        assert_eq!(arr.len(), 3);
        assert!(require_array(&args, "missing", "test").is_err());

        assert_eq!(opt_f64(&args, "missing", 1.5), 1.5);
    }
}
