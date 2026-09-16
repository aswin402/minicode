use crate::agent::provider::ToolSchema;
use serde_json::Value;

/// Default maximum character length for a tool's top-level description.
pub const DEFAULT_MAX_TOOL_DESC_CHARS: usize = 180;

/// Default maximum character length for an individual parameter's description.
pub const DEFAULT_MAX_PROP_DESC_CHARS: usize = 120;

/// High-efficiency tool schema minification engine.
///
/// Trims verbose paragraph explanations, repetitive markdown examples, and
/// conversational preamble from tool definitions while preserving 100% of
/// the structural JSON Schema contracts (types, enums, required fields, and nested objects).
pub struct ToolSchemaCompactor;

impl ToolSchemaCompactor {
    /// Condenses a single tool schema for high-density prompt insertion.
    #[must_use]
    pub fn compact_schema(
        schema: &ToolSchema,
        max_tool_desc: usize,
        max_prop_desc: usize,
    ) -> ToolSchema {
        let compacted_desc = Self::condense_description(&schema.description, max_tool_desc);
        let compacted_params = Self::condense_parameters(&schema.parameters, max_prop_desc);

        ToolSchema {
            name: schema.name.clone(),
            description: compacted_desc,
            parameters: compacted_params,
        }
    }

    /// Condenses an entire slice of tool schemas.
    #[must_use]
    pub fn compact_schemas(schemas: &[ToolSchema]) -> Vec<ToolSchema> {
        schemas
            .iter()
            .map(|s| {
                Self::compact_schema(s, DEFAULT_MAX_TOOL_DESC_CHARS, DEFAULT_MAX_PROP_DESC_CHARS)
            })
            .collect()
    }

    /// Condenses a description string to its primary sentence or first `max_chars`.
    #[must_use]
    pub fn condense_description(desc: &str, max_chars: usize) -> String {
        let trimmed = desc.trim();
        if trimmed.len() <= max_chars {
            return trimmed.to_string();
        }

        // 1. Try to find first sentence boundary (. followed by whitespace or end)
        if let Some(dot_idx) = trimmed.find(". ") {
            let candidate = &trimmed[..=dot_idx];
            if candidate.len() <= max_chars {
                return candidate.to_string();
            }
        } else if let Some(dot_idx) = trimmed.find(".\n") {
            let candidate = &trimmed[..=dot_idx];
            if candidate.len() <= max_chars {
                return candidate.trim().to_string();
            }
        }

        // 2. Otherwise truncate at word boundary before max_chars
        let slice = &trimmed[..max_chars];
        if let Some(last_space) = slice.rfind(' ') {
            if last_space > max_chars / 2 {
                return format!("{}...", &slice[..last_space]);
            }
        }

        format!("{}...", slice)
    }

    /// Recursively strips verbose markdown fences and long comments from parameter JSON schema.
    #[must_use]
    pub fn condense_parameters(params: &Value, max_prop_desc: usize) -> Value {
        match params {
            Value::Object(map) => {
                let mut new_map = serde_json::Map::with_capacity(map.len());
                for (k, v) in map {
                    if k == "description" {
                        if let Value::String(s) = v {
                            // Strip code blocks and condense
                            let cleaned = Self::strip_markdown_code_blocks(s);
                            let condensed = Self::condense_description(&cleaned, max_prop_desc);
                            new_map.insert(k.clone(), Value::String(condensed));
                            continue;
                        }
                    } else if k == "properties" {
                        if let Value::Object(props) = v {
                            let mut new_props = serde_json::Map::with_capacity(props.len());
                            for (pk, pv) in props {
                                new_props.insert(
                                    pk.clone(),
                                    Self::condense_parameters(pv, max_prop_desc),
                                );
                            }
                            new_map.insert(k.clone(), Value::Object(new_props));
                            continue;
                        }
                    } else if k == "items" {
                        new_map.insert(k.clone(), Self::condense_parameters(v, max_prop_desc));
                        continue;
                    }

                    // Keep other fields intact (type, enum, required, default, anyOf, etc.)
                    new_map.insert(k.clone(), v.clone());
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => Value::Array(
                arr.iter()
                    .map(|item| Self::condense_parameters(item, max_prop_desc))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Strips markdown code blocks (```...```) often embedded in MCP parameter descriptions.
    fn strip_markdown_code_blocks(s: &str) -> String {
        let mut result = s.to_string();
        while let Some(start) = result.find("```") {
            if let Some(end) = result[start + 3..].find("```") {
                let end_idx = start + 3 + end + 3;
                result.replace_range(start..end_idx, "");
            } else {
                result.replace_range(start.., "");
                break;
            }
        }
        result.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_condense_description_sentence_boundary() {
        let long_desc = "Creates a new rectangle node on canvas. Use this tool when you want to draw a shape or container box. Requires coordinates and dimensions.";
        let condensed = ToolSchemaCompactor::condense_description(long_desc, 60);
        assert_eq!(condensed, "Creates a new rectangle node on canvas.");
    }

    #[test]
    fn test_condense_description_word_boundary_fallback() {
        let no_dot =
            "Creates a large complex widget in the current canvas without any punctuation at all";
        let condensed = ToolSchemaCompactor::condense_description(no_dot, 30);
        assert!(condensed.ends_with("..."));
        assert!(condensed.len() <= 33);
    }

    #[test]
    fn test_condense_parameters_strips_code_blocks_and_truncates() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "type": "string",
                    "description": "Configuration string. Example: ```json\n{\"foo\": \"bar\"}\n```. Must be valid JSON."
                },
                "count": {
                    "type": "integer",
                    "description": "The number of items to create."
                }
            },
            "required": ["config"]
        });

        let condensed = ToolSchemaCompactor::condense_parameters(&schema, 50);

        // Check required preserved
        assert_eq!(condensed["required"], json!(["config"]));
        // Check types preserved
        assert_eq!(condensed["properties"]["config"]["type"], "string");
        assert_eq!(condensed["properties"]["count"]["type"], "integer");

        // Check code block stripped from description
        let desc = condensed["properties"]["config"]["description"]
            .as_str()
            .unwrap();
        assert!(!desc.contains("```"));
        assert!(desc.len() <= 50);
    }

    #[test]
    fn test_compact_schema_full() {
        let raw = ToolSchema {
            name: "mcp__figma__create_frame".to_string(),
            description: "Creates an auto-layout frame in the active document. Very useful for grouping UI elements and setting flexbox direction. Do not call this for basic shapes.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The human-readable label of the frame shown in Figma's layer hierarchy tree."
                    }
                },
                "required": ["name"]
            }),
        };

        let compacted = ToolSchemaCompactor::compact_schema(&raw, 70, 50);
        assert_eq!(compacted.name, "mcp__figma__create_frame");
        assert_eq!(
            compacted.description,
            "Creates an auto-layout frame in the active document."
        );
        assert_eq!(compacted.parameters["required"], json!(["name"]));
    }
}
