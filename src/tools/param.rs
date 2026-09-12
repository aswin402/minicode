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

/// Standard path key aliases across file and search tools
pub const PATH_ALIASES: &[&str] = &[
    "path",
    "file_path",
    "filepath",
    "file",
    "target_file",
    "target",
    "filename",
];

/// Standard command key aliases across execution tools
pub const COMMAND_ALIASES: &[&str] = &["command", "cmd", "script", "exec", "run"];

/// Standard search block key aliases across editing tools
pub const SEARCH_BLOCK_ALIASES: &[&str] =
    &["search_block", "search", "old_string", "old_code", "find"];

/// Standard replace block key aliases across editing tools
pub const REPLACE_BLOCK_ALIASES: &[&str] = &[
    "replace_block",
    "replace",
    "new_string",
    "new_code",
    "replacement",
];

/// Standard query key aliases across search and retrieval tools
pub const QUERY_ALIASES: &[&str] = &["query", "pattern", "search", "term", "regex"];

/// Standard limit/count key aliases
pub const LIMIT_ALIASES: &[&str] = &["limit", "count", "max_results", "max_items", "max_files"];

/// Extracts an argument checking a list of candidate aliases in priority order
pub fn get_str_with_aliases<'a>(args: &'a Value, aliases: &[&str]) -> Option<&'a str> {
    for alias in aliases {
        if let Some(s) = args.get(*alias).and_then(|v| v.as_str()) {
            return Some(s);
        }
    }
    None
}

/// Extracts a mandatory path string checking standard aliases (`path`, `file_path`, `file`, etc.)
pub fn require_path<'a>(args: &'a Value, tool_name: &str) -> Result<&'a str, ToolError> {
    get_str_with_aliases(args, PATH_ALIASES).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason:
            "Missing required path argument (accepted: 'path', 'file_path', 'file', 'target_file')"
                .to_string(),
    })
}

/// Extracts an optional path string checking standard aliases (`path`, `file_path`, `file`, etc.)
pub fn opt_path(args: &Value) -> Option<&str> {
    get_str_with_aliases(args, PATH_ALIASES)
}

/// Extracts a mandatory command string checking standard aliases (`command`, `cmd`, `script`, etc.)
pub fn require_command<'a>(args: &'a Value, tool_name: &str) -> Result<&'a str, ToolError> {
    get_str_with_aliases(args, COMMAND_ALIASES).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: "Missing required command argument (accepted: 'command', 'cmd')".to_string(),
    })
}

/// Extracts an optional command string checking standard aliases (`command`, `cmd`, `script`, etc.)
#[allow(dead_code)]
pub fn opt_command(args: &Value) -> Option<&str> {
    get_str_with_aliases(args, COMMAND_ALIASES)
}

/// Extracts a mandatory query string checking standard aliases (`query`, `pattern`, `search`, etc.)
pub fn require_query<'a>(args: &'a Value, tool_name: &str) -> Result<&'a str, ToolError> {
    get_str_with_aliases(args, QUERY_ALIASES).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: "Missing required query argument (accepted: 'query', 'pattern', 'search')"
            .to_string(),
    })
}

/// Extracts an optional query string checking standard aliases
#[allow(dead_code)]
pub fn opt_query(args: &Value) -> Option<&str> {
    get_str_with_aliases(args, QUERY_ALIASES)
}

/// Extracts a mandatory search block checking standard aliases (`search_block`, `search`, `old_string`, etc.)
pub fn require_search_block<'a>(args: &'a Value, tool_name: &str) -> Result<&'a str, ToolError> {
    get_str_with_aliases(args, SEARCH_BLOCK_ALIASES).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: "Missing required search_block argument (accepted: 'search_block', 'search', 'old_string')".to_string(),
    })
}

/// Extracts a mandatory replace block checking standard aliases (`replace_block`, `replace`, `new_string`, etc.)
pub fn require_replace_block<'a>(args: &'a Value, tool_name: &str) -> Result<&'a str, ToolError> {
    get_str_with_aliases(args, REPLACE_BLOCK_ALIASES).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: "Missing required replace_block argument (accepted: 'replace_block', 'replace', 'new_string')".to_string(),
    })
}

/// Extracts an optional limit parameter checking standard aliases (`limit`, `count`, `max_results`, etc.)
pub fn opt_limit(args: &Value, default: usize) -> usize {
    for alias in LIMIT_ALIASES {
        if let Some(n) = get_usize(args, alias) {
            return n;
        }
    }
    default
}

/// Extracts an optional boolean argument with permissive coercion (native bool, "true"/"false", "1"/"0", "yes"/"no", 1/0)
#[allow(dead_code)]
pub fn get_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| {
        if let Some(b) = v.as_bool() {
            Some(b)
        } else if let Some(s) = v.as_str() {
            match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" | "on" => Some(true),
                "false" | "0" | "no" | "off" => Some(false),
                _ => None,
            }
        } else if let Some(n) = v.as_i64() {
            Some(n != 0)
        } else {
            v.as_u64().map(|n| n != 0)
        }
    })
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

/// Extracts a mandatory `usize` parameter or returns `ToolError::InvalidArguments`
#[allow(dead_code)]
pub fn require_usize(args: &Value, key: &str, tool_name: &str) -> Result<usize, ToolError> {
    get_usize(args, key).ok_or_else(|| ToolError::InvalidArguments {
        name: tool_name.to_string(),
        reason: format!("Missing required argument '{}'", key),
    })
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

/// Extracts an optional array of strings with permissive coercion (JSON array, single string, or comma-separated string)
#[allow(dead_code)]
pub fn opt_string_array(args: &Value, key: &str) -> Option<Vec<String>> {
    args.get(key).and_then(|v| {
        if let Some(arr) = v.as_array() {
            Some(
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect(),
            )
        } else if let Some(s) = v.as_str() {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Some(Vec::new())
            } else if trimmed.contains(',') {
                Some(trimmed.split(',').map(|p| p.trim().to_string()).collect())
            } else {
                Some(vec![trimmed.to_string()])
            }
        } else {
            None
        }
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
pub fn get_f64(args: &Value, key: &str) -> Option<f64> {
    args.get(key).and_then(|v| {
        if let Some(n) = v.as_f64() {
            Some(n)
        } else if let Some(s) = v.as_str() {
            s.parse::<f64>().ok()
        } else {
            None
        }
    })
}

/// Extracts an optional `f64` parameter, falling back to default
#[allow(dead_code)]
pub fn opt_f64(args: &Value, key: &str, default: f64) -> f64 {
    get_f64(args, key).unwrap_or(default)
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

    #[test]
    fn test_alias_extraction() {
        // Test path aliases
        let args_file = json!({"file": "src/main.rs"});
        let args_file_path = json!({"file_path": "src/lib.rs"});
        let args_target = json!({"target_file": "src/app.rs"});
        assert_eq!(require_path(&args_file, "test").unwrap(), "src/main.rs");
        assert_eq!(require_path(&args_file_path, "test").unwrap(), "src/lib.rs");
        assert_eq!(require_path(&args_target, "test").unwrap(), "src/app.rs");
        assert!(require_path(&json!({"other": 123}), "test").is_err());

        // Test command aliases
        let args_cmd = json!({"cmd": "cargo test"});
        let args_script = json!({"script": "npm run build"});
        assert_eq!(require_command(&args_cmd, "test").unwrap(), "cargo test");
        assert_eq!(
            require_command(&args_script, "test").unwrap(),
            "npm run build"
        );
        assert!(require_command(&json!({"exec_target": "ls"}), "test").is_err());

        // Test search block & replace block aliases
        let args_edit = json!({"old_string": "let a = 1;", "new_string": "let a = 2;"});
        assert_eq!(
            require_search_block(&args_edit, "test").unwrap(),
            "let a = 1;"
        );
        assert_eq!(
            require_replace_block(&args_edit, "test").unwrap(),
            "let a = 2;"
        );

        // Test query & limit aliases
        let args_search = json!({"pattern": "fn main", "max_results": 25});
        assert_eq!(require_query(&args_search, "test").unwrap(), "fn main");
        assert_eq!(opt_limit(&args_search, 10), 25);
        assert_eq!(opt_limit(&json!({"count": "50"}), 10), 50);
        assert_eq!(opt_limit(&json!({}), 10), 10);
    }

    #[test]
    fn test_permissive_coercions() {
        // Boolean string & int coercions
        assert_eq!(get_bool(&json!({"val": "true"}), "val"), Some(true));
        assert_eq!(get_bool(&json!({"val": "TRUE"}), "val"), Some(true));
        assert_eq!(get_bool(&json!({"val": "1"}), "val"), Some(true));
        assert_eq!(get_bool(&json!({"val": "yes"}), "val"), Some(true));
        assert_eq!(get_bool(&json!({"val": "on"}), "val"), Some(true));
        assert_eq!(get_bool(&json!({"val": 1}), "val"), Some(true));

        assert_eq!(get_bool(&json!({"val": "false"}), "val"), Some(false));
        assert_eq!(get_bool(&json!({"val": "0"}), "val"), Some(false));
        assert_eq!(get_bool(&json!({"val": "no"}), "val"), Some(false));
        assert_eq!(get_bool(&json!({"val": "off"}), "val"), Some(false));
        assert_eq!(get_bool(&json!({"val": 0}), "val"), Some(false));
        assert_eq!(get_bool(&json!({"val": "invalid"}), "val"), None);

        // String array coercions
        let arr = opt_string_array(&json!({"files": ["a.rs", "b.rs"]}), "files").unwrap();
        assert_eq!(arr, vec!["a.rs", "b.rs"]);

        // Single string coerced to single-element array
        let single = opt_string_array(&json!({"files": "single.rs"}), "files").unwrap();
        assert_eq!(single, vec!["single.rs"]);

        // Comma-separated string coerced to array
        let split = opt_string_array(&json!({"files": "x.rs, y.rs, z.rs"}), "files").unwrap();
        assert_eq!(split, vec!["x.rs", "y.rs", "z.rs"]);
    }
}
