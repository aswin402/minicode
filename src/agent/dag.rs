use crate::error::{Result, ToolError};
use crate::tools::concurrency::classify_tool;
use crate::tools::ToolRegistry;
use futures::future::join_all;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

/// Specification for a single node within an execution DAG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagNodeSpec {
    /// Unique identifier for this node within the DAG.
    pub id: String,
    /// Registered tool name to invoke (e.g. "read_file", "locate_fault", "patch_file").
    pub tool: String,
    /// Arguments to pass to the tool. Values can reference upstream outputs via `$node_id.path`.
    #[serde(default = "default_node_args")]
    pub args: Value,
    /// IDs of upstream parent nodes that must complete successfully before this node executes.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_node_args() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Specification for a complete Directed Acyclic Graph of tool invocations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagSpec {
    /// Optional human-readable name for the workflow.
    #[serde(default = "default_dag_name")]
    pub name: String,
    /// List of node specifications forming the DAG.
    pub nodes: Vec<DagNodeSpec>,
}

fn default_dag_name() -> String {
    "workflow".to_string()
}

/// Lifecycle status of an executed DAG node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeExecutionStatus {
    /// Tool completed successfully.
    Success,
    /// Tool returned an error or argument resolution failed.
    Failed,
    /// Node was skipped because one or more upstream dependencies failed or were skipped.
    SkippedDependencyFailed,
}

/// Result of executing an individual node in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNodeResult {
    pub node_id: String,
    pub tool: String,
    pub status: NodeExecutionStatus,
    pub output: String,
    #[serde(default)]
    pub parsed_json: Option<Value>,
    pub duration_ms: u64,
}

/// Comprehensive execution report for the entire DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagExecutionReport {
    pub dag_name: String,
    pub total_nodes: usize,
    pub completed_nodes: usize,
    pub failed_nodes: usize,
    pub skipped_nodes: usize,
    pub wall_clock_ms: u64,
    pub waves_executed: usize,
    pub node_results: Vec<DagNodeResult>,
}

impl DagExecutionReport {
    /// Format a concise, human-readable and LLM-friendly markdown summary.
    pub fn format_summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "### ⚡ DAG Execution Report: `{}`\n",
            self.dag_name
        ));
        out.push_str(&format!(
            "- **Nodes:** {} total ({} succeeded, {} failed, {} skipped)\n",
            self.total_nodes, self.completed_nodes, self.failed_nodes, self.skipped_nodes
        ));
        out.push_str(&format!(
            "- **Waves Executed:** {} | **Total Wall-Clock:** {}ms\n\n",
            self.waves_executed, self.wall_clock_ms
        ));

        out.push_str("| Node ID | Tool | Status | Duration | Output Preview |\n");
        out.push_str("|---------|------|--------|----------|----------------|\n");

        for r in &self.node_results {
            let status_icon = match r.status {
                NodeExecutionStatus::Success => "✅ Success",
                NodeExecutionStatus::Failed => "❌ Failed",
                NodeExecutionStatus::SkippedDependencyFailed => "⚠️ Skipped",
            };

            let preview: String = r
                .output
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            let preview_clean = preview.replace('|', "\\|");

            out.push_str(&format!(
                "| `{}` | `{}` | {} | {}ms | {} |\n",
                r.node_id, r.tool, status_icon, r.duration_ms, preview_clean
            ));
        }

        out
    }
}

/// Compiles a DAG specification into topologically sorted execution waves using Kahn's algorithm.
pub struct DagCompiler;

impl DagCompiler {
    /// Validates node identifiers, dependency references, checks for cycles,
    /// and partitions nodes into parallel execution waves `Vec<Vec<DagNodeSpec>>`.
    pub fn topological_waves(spec: &DagSpec) -> Result<Vec<Vec<DagNodeSpec>>> {
        if spec.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let mut node_map: HashMap<String, DagNodeSpec> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        // 1. Validate uniqueness & index all nodes
        for node in &spec.nodes {
            if node.tool == "execute_dag" {
                return Err(ToolError::InvalidArguments {
                    name: "execute_dag".to_string(),
                    reason: format!(
                        "Node '{}' cannot invoke 'execute_dag': nested DAGs are not permitted",
                        node.id
                    ),
                }
                .into());
            }
            if node_map.contains_key(&node.id) {
                return Err(ToolError::InvalidArguments {
                    name: "execute_dag".to_string(),
                    reason: format!("Duplicate node ID detected in DAG: '{}'", node.id),
                }
                .into());
            }
            node_map.insert(node.id.clone(), node.clone());
            in_degree.insert(node.id.clone(), 0);
            dependents.insert(node.id.clone(), Vec::new());
        }

        // 2. Validate dependencies & construct adjacency graph
        for node in &spec.nodes {
            let mut seen_deps = HashSet::new();
            for dep in &node.depends_on {
                if dep == &node.id {
                    return Err(ToolError::InvalidArguments {
                        name: "execute_dag".to_string(),
                        reason: format!("Node '{}' cannot depend on itself", node.id),
                    }
                    .into());
                }
                if !node_map.contains_key(dep) {
                    return Err(ToolError::InvalidArguments {
                        name: "execute_dag".to_string(),
                        reason: format!("Node '{}' depends on unknown node '{}'", node.id, dep),
                    }
                    .into());
                }
                if seen_deps.insert(dep.clone()) {
                    *in_degree.entry(node.id.clone()).or_insert(0) += 1;
                    dependents
                        .entry(dep.clone())
                        .or_default()
                        .push(node.id.clone());
                }
            }
        }

        // 3. Kahn's wave computation
        let mut waves: Vec<Vec<DagNodeSpec>> = Vec::new();
        let mut current_wave: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Sort wave deterministically by original declaration order
        let original_order: HashMap<String, usize> = spec
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, n)| (n.id.clone(), idx))
            .collect();
        current_wave.sort_by_key(|id| original_order.get(id).copied().unwrap_or(0));

        let mut processed_count = 0;

        while !current_wave.is_empty() {
            let mut wave_nodes = Vec::new();
            let mut next_wave = Vec::new();

            for id in &current_wave {
                if let Some(node) = node_map.get(id) {
                    wave_nodes.push(node.clone());
                }
                processed_count += 1;

                if let Some(children) = dependents.get(id) {
                    for child_id in children {
                        if let Some(deg) = in_degree.get_mut(child_id) {
                            *deg = deg.saturating_sub(1);
                            if *deg == 0 {
                                next_wave.push(child_id.clone());
                            }
                        }
                    }
                }
            }

            waves.push(wave_nodes);
            next_wave.sort_by_key(|id| original_order.get(id).copied().unwrap_or(0));
            current_wave = next_wave;
        }

        if processed_count != spec.nodes.len() {
            return Err(ToolError::InvalidArguments {
                name: "execute_dag".to_string(),
                reason: format!(
                    "Cycle detected in DAG: {} of {} nodes could not be scheduled",
                    spec.nodes.len() - processed_count,
                    spec.nodes.len()
                ),
            }
            .into());
        }

        Ok(waves)
    }
}

/// Inter-Tool JSONPath & Value Resolver.
/// Dynamically resolves `$node_id.path.to.field` and `${node_id.path}` references
/// within downstream tool arguments using outputs produced by upstream nodes.
pub struct JsonPathResolver;

impl JsonPathResolver {
    /// Resolves all template references within an arbitrary JSON value.
    pub fn resolve(args: &Value, outputs: &HashMap<String, Value>) -> Result<Value> {
        Self::resolve_value(args, outputs)
    }

    fn resolve_value(val: &Value, outputs: &HashMap<String, Value>) -> Result<Value> {
        match val {
            Value::String(s) => Self::resolve_string(s, outputs),
            Value::Object(map) => {
                let mut resolved_map = serde_json::Map::new();
                for (k, v) in map {
                    resolved_map.insert(k.clone(), Self::resolve_value(v, outputs)?);
                }
                Ok(Value::Object(resolved_map))
            }
            Value::Array(arr) => {
                let mut resolved_arr = Vec::with_capacity(arr.len());
                for item in arr {
                    resolved_arr.push(Self::resolve_value(item, outputs)?);
                }
                Ok(Value::Array(resolved_arr))
            }
            primitive => Ok(primitive.clone()),
        }
    }

    /// Resolves string expressions. If the string is solely a reference (e.g. `"$node.field"`),
    /// the resolved value retains its original JSON type (number, boolean, object, array).
    /// If embedded in text, values are interpolated as strings.
    fn resolve_string(s: &str, outputs: &HashMap<String, Value>) -> Result<Value> {
        let trimmed = s.trim();

        // Exact match case 1: "${...}"
        if trimmed.starts_with("${") && trimmed.ends_with('}') {
            let inner = &trimmed[2..trimmed.len() - 1];
            if !inner.contains('{') && !inner.contains('}') && !inner.contains(' ') {
                return Self::resolve_path_expr(inner, outputs);
            }
        }

        // Exact match case 2: "$node.path..." with no whitespace
        if trimmed.starts_with('$')
            && !trimmed.contains(' ')
            && !trimmed.contains('\t')
            && !trimmed.contains('\n')
        {
            let inner = &trimmed[1..];
            return Self::resolve_path_expr(inner, outputs);
        }

        // Embedded template interpolation case: substitute all occurrences of ${...} or $ident.path
        if s.contains('$') {
            return Self::interpolate_template(s, outputs);
        }

        Ok(Value::String(s.to_string()))
    }

    /// Interpolates embedded references within a larger string template.
    fn interpolate_template(s: &str, outputs: &HashMap<String, Value>) -> Result<Value> {
        // Match either ${var.path} or $var.path
        let re = Regex::new(
            r"\$\{([a-zA-Z0-9_\.\[\]]+)\}|\$([a-zA-Z0-9_]+(?:\.[a-zA-Z0-9_]+|\[\d+\])*)",
        )
        .map_err(|e| ToolError::InvalidArguments {
            name: "execute_dag".to_string(),
            reason: format!("Failed to compile JSONPath interpolation regex: {}", e),
        })?;

        let mut result = String::new();
        let mut last_idx = 0;

        for cap in re.captures_iter(s) {
            let full_match = match cap.get(0) {
                Some(m) => m,
                None => continue,
            };

            result.push_str(&s[last_idx..full_match.start()]);

            let expr = if let Some(m) = cap.get(1) {
                m.as_str()
            } else if let Some(m) = cap.get(2) {
                m.as_str()
            } else {
                full_match.as_str()
            };

            let resolved_val = Self::resolve_path_expr(expr, outputs)?;
            let str_val = match resolved_val {
                Value::String(st) => st,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                other => serde_json::to_string(&other).unwrap_or_default(),
            };

            result.push_str(&str_val);
            last_idx = full_match.end();
        }

        result.push_str(&s[last_idx..]);
        Ok(Value::String(result))
    }

    /// Resolves an expression path like `"node_1.field.sub[0].path"` against `outputs`.
    pub fn resolve_path_expr(expr: &str, outputs: &HashMap<String, Value>) -> Result<Value> {
        let clean_expr = expr.trim();
        let segments = Self::parse_path_segments(clean_expr);

        if segments.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "execute_dag".to_string(),
                reason: "Empty JSONPath reference expression".to_string(),
            }
            .into());
        }

        let node_id = &segments[0];
        let root_val: &Value = outputs
            .get(node_id)
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "execute_dag".to_string(),
                reason: format!(
                    "Referenced node '{}' not found or has not produced output yet",
                    node_id
                ),
            })?;

        // If only node_id is referenced without path segments
        if segments.len() == 1 {
            // If output was non-JSON text wrapped in __raw_text, return the raw string
            if let Some(raw) = root_val.get("__raw_text").and_then(|v: &Value| v.as_str()) {
                return Ok(Value::String(raw.to_string()));
            }
            return Ok(root_val.clone());
        }

        // Traverse remaining path segments
        let mut curr: &Value = root_val;
        for (i, seg) in segments.iter().enumerate().skip(1) {
            match curr {
                Value::Object(obj) => {
                    if let Some(child) = obj.get(seg) {
                        curr = child;
                    } else {
                        return Err(ToolError::InvalidArguments {
                            name: "execute_dag".to_string(),
                            reason: format!(
                                "Key '{}' not found in node '{}' output at path '{}'",
                                seg,
                                node_id,
                                segments[..=i].join(".")
                            ),
                        }
                        .into());
                    }
                }
                Value::Array(arr) => {
                    let idx: usize = seg.parse().map_err(|_| ToolError::InvalidArguments {
                        name: "execute_dag".to_string(),
                        reason: format!(
                            "Invalid array index '{}' at path '{}'",
                            seg,
                            segments[..=i].join(".")
                        ),
                    })?;
                    if let Some(child) = arr.get(idx) {
                        curr = child;
                    } else {
                        return Err(ToolError::InvalidArguments {
                            name: "execute_dag".to_string(),
                            reason: format!(
                                "Array index {} out of bounds (len: {}) at path '{}'",
                                idx,
                                arr.len(),
                                segments[..=i].join(".")
                            ),
                        }
                        .into());
                    }
                }
                _ => {
                    return Err(ToolError::InvalidArguments {
                        name: "execute_dag".to_string(),
                        reason: format!(
                            "Cannot index into non-composite value at path '{}'",
                            segments[..=i].join(".")
                        ),
                    }
                    .into());
                }
            }
        }

        Ok(curr.clone())
    }

    /// Tokenizes a path string like `"node.matches[0].path"` into `["node", "matches", "0", "path"]`.
    pub fn parse_path_segments(path: &str) -> Vec<String> {
        let mut segments = Vec::new();
        let mut current = String::new();
        let mut in_bracket = false;

        for ch in path.chars() {
            if ch == '.' && !in_bracket {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
            } else if ch == '[' {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
                in_bracket = true;
            } else if ch == ']' {
                if !current.is_empty() {
                    segments.push(current.clone());
                    current.clear();
                }
                in_bracket = false;
            } else {
                current.push(ch);
            }
        }

        if !current.is_empty() {
            segments.push(current);
        }

        segments
    }
}

/// Orchestrator for wave-based concurrent async DAG execution.
pub struct DagExecutor;

impl DagExecutor {
    /// Executes the DAG specification against the workspace.
    /// Concurrently executes read-only nodes in each wave while serializing mutating nodes.
    /// Halts/skips downstream nodes if an upstream parent dependency fails.
    pub fn execute<'a>(
        workspace_root: &'a Path,
        spec: &'a DagSpec,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<DagExecutionReport>> + Send + 'a>>
    {
        Box::pin(Self::execute_inner(workspace_root, spec))
    }

    async fn execute_inner(workspace_root: &Path, spec: &DagSpec) -> Result<DagExecutionReport> {
        let waves = DagCompiler::topological_waves(spec)?;
        let start_time = Instant::now();

        let mut outputs: HashMap<String, Value> = HashMap::new();
        let mut node_statuses: HashMap<String, NodeExecutionStatus> = HashMap::new();
        let mut node_results: Vec<DagNodeResult> = Vec::new();

        for wave in &waves {
            let mut executable_nodes = Vec::new();

            for node in wave {
                // Check if any upstream parent dependency failed or was skipped
                let failed_dep = node
                    .depends_on
                    .iter()
                    .find(|dep| node_statuses.get(*dep) != Some(&NodeExecutionStatus::Success));

                if let Some(bad_dep) = failed_dep {
                    let reason = format!(
                        "Skipped: upstream dependency '{}' failed or was skipped",
                        bad_dep
                    );
                    node_statuses.insert(
                        node.id.clone(),
                        NodeExecutionStatus::SkippedDependencyFailed,
                    );
                    node_results.push(DagNodeResult {
                        node_id: node.id.clone(),
                        tool: node.tool.clone(),
                        status: NodeExecutionStatus::SkippedDependencyFailed,
                        output: reason,
                        parsed_json: None,
                        duration_ms: 0,
                    });
                    continue;
                }

                // Resolve parameters via JSONPath
                let resolved_args = match JsonPathResolver::resolve(&node.args, &outputs) {
                    Ok(args) => args,
                    Err(err) => {
                        let err_msg = format!("Argument resolution failed: {}", err);
                        node_statuses.insert(node.id.clone(), NodeExecutionStatus::Failed);
                        node_results.push(DagNodeResult {
                            node_id: node.id.clone(),
                            tool: node.tool.clone(),
                            status: NodeExecutionStatus::Failed,
                            output: err_msg,
                            parsed_json: None,
                            duration_ms: 0,
                        });
                        continue;
                    }
                };

                executable_nodes.push((node, resolved_args));
            }

            // Partition into read-only (parallel) and mutating (sequential)
            let mut ro_nodes = Vec::new();
            let mut mut_nodes = Vec::new();

            for (node, args) in executable_nodes {
                let safety = classify_tool(&node.tool);
                if safety.is_read_only() {
                    ro_nodes.push((node, args));
                } else {
                    mut_nodes.push((node, args));
                }
            }

            // Execute read-only nodes concurrently in parallel
            if !ro_nodes.is_empty() {
                let ro_futures: Vec<_> = ro_nodes
                    .iter()
                    .map(|(node, args)| {
                        let root = workspace_root.to_path_buf();
                        let node_id = node.id.clone();
                        let tool_name = node.tool.clone();
                        let args = args.clone();
                        async move {
                            let t_start = Instant::now();
                            let result =
                                ToolRegistry::dispatch(&root, &node_id, &tool_name, &args, None, 0)
                                    .await;
                            let duration = t_start.elapsed().as_millis() as u64;
                            (node_id, tool_name, result, duration)
                        }
                    })
                    .collect();

                let executed_ro = join_all(ro_futures).await;
                for (node_id, tool_name, res, dur) in executed_ro {
                    let status = if res.success {
                        NodeExecutionStatus::Success
                    } else {
                        NodeExecutionStatus::Failed
                    };

                    let parsed = match serde_json::from_str::<Value>(&res.output) {
                        Ok(v) => v,
                        Err(_) => json!({
                            "output": res.output.clone(),
                            "text": res.output.clone(),
                            "__raw_text": res.output.clone(),
                        }),
                    };

                    outputs.insert(node_id.clone(), parsed.clone());
                    node_statuses.insert(node_id.clone(), status);
                    node_results.push(DagNodeResult {
                        node_id,
                        tool: tool_name,
                        status,
                        output: res.output,
                        parsed_json: Some(parsed),
                        duration_ms: dur,
                    });
                }
            }

            // Execute mutating nodes sequentially to preserve workspace safety
            for (node, args) in mut_nodes {
                let t_start = Instant::now();
                let res =
                    ToolRegistry::dispatch(workspace_root, &node.id, &node.tool, &args, None, 0)
                        .await;
                let duration = t_start.elapsed().as_millis() as u64;

                let status = if res.success {
                    NodeExecutionStatus::Success
                } else {
                    NodeExecutionStatus::Failed
                };

                let parsed = match serde_json::from_str::<Value>(&res.output) {
                    Ok(v) => v,
                    Err(_) => json!({
                        "output": res.output.clone(),
                        "text": res.output.clone(),
                        "__raw_text": res.output.clone(),
                    }),
                };

                outputs.insert(node.id.clone(), parsed.clone());
                node_statuses.insert(node.id.clone(), status);
                node_results.push(DagNodeResult {
                    node_id: node.id.clone(),
                    tool: node.tool.clone(),
                    status,
                    output: res.output,
                    parsed_json: Some(parsed),
                    duration_ms: duration,
                });
            }
        }

        let wall_clock_ms = start_time.elapsed().as_millis() as u64;
        let completed_nodes = node_results
            .iter()
            .filter(|r| r.status == NodeExecutionStatus::Success)
            .count();
        let failed_nodes = node_results
            .iter()
            .filter(|r| r.status == NodeExecutionStatus::Failed)
            .count();
        let skipped_nodes = node_results
            .iter()
            .filter(|r| r.status == NodeExecutionStatus::SkippedDependencyFailed)
            .count();

        Ok(DagExecutionReport {
            dag_name: spec.name.clone(),
            total_nodes: spec.nodes.len(),
            completed_nodes,
            failed_nodes,
            skipped_nodes,
            wall_clock_ms,
            waves_executed: waves.len(),
            node_results,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_waves_linear() {
        let spec = DagSpec {
            name: "test_linear".to_string(),
            nodes: vec![
                DagNodeSpec {
                    id: "step1".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({ "path": "foo.txt" }),
                    depends_on: vec![],
                },
                DagNodeSpec {
                    id: "step2".to_string(),
                    tool: "grep_search".to_string(),
                    args: json!({ "query": "bar" }),
                    depends_on: vec!["step1".to_string()],
                },
            ],
        };

        let waves = DagCompiler::topological_waves(&spec).expect("Should compile waves");
        assert_eq!(waves.len(), 2);
        assert_eq!(waves[0].len(), 1);
        assert_eq!(waves[0][0].id, "step1");
        assert_eq!(waves[1].len(), 1);
        assert_eq!(waves[1][0].id, "step2");
    }

    #[test]
    fn test_topological_waves_diamond() {
        let spec = DagSpec {
            name: "test_diamond".to_string(),
            nodes: vec![
                DagNodeSpec {
                    id: "root".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec![],
                },
                DagNodeSpec {
                    id: "left".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec!["root".to_string()],
                },
                DagNodeSpec {
                    id: "right".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec!["root".to_string()],
                },
                DagNodeSpec {
                    id: "join".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec!["left".to_string(), "right".to_string()],
                },
            ],
        };

        let waves = DagCompiler::topological_waves(&spec).expect("Should compile diamond waves");
        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0].len(), 1);
        assert_eq!(waves[0][0].id, "root");
        assert_eq!(waves[1].len(), 2);
        let wave1_ids: Vec<_> = waves[1].iter().map(|n| n.id.as_str()).collect();
        assert!(wave1_ids.contains(&"left"));
        assert!(wave1_ids.contains(&"right"));
        assert_eq!(waves[2].len(), 1);
        assert_eq!(waves[2][0].id, "join");
    }

    #[test]
    fn test_topological_waves_cycle_detection() {
        let spec = DagSpec {
            name: "test_cycle".to_string(),
            nodes: vec![
                DagNodeSpec {
                    id: "node_a".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec!["node_b".to_string()],
                },
                DagNodeSpec {
                    id: "node_b".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec!["node_a".to_string()],
                },
            ],
        };

        let err = DagCompiler::topological_waves(&spec).unwrap_err();
        assert!(err.to_string().contains("Cycle detected in DAG"));
    }

    #[test]
    fn test_jsonpath_resolution_exact_and_nested() {
        let mut outputs = HashMap::new();
        outputs.insert(
            "locate".to_string(),
            json!({
                "matches": [
                    { "path": "src/lib.rs", "line": 42 },
                    { "path": "src/main.rs", "line": 10 }
                ],
                "count": 2
            }),
        );

        // Exact typed lookup: integer
        let expr1 = json!("$locate.matches[0].line");
        let res1 = JsonPathResolver::resolve(&expr1, &outputs).expect("Resolve line");
        assert_eq!(res1, json!(42));

        // Exact typed lookup: string
        let expr2 = json!("${locate.matches[1].path}");
        let res2 = JsonPathResolver::resolve(&expr2, &outputs).expect("Resolve path");
        assert_eq!(res2, json!("src/main.rs"));

        // Template interpolation
        let expr3 = json!("Found $locate.count matches in ${locate.matches[0].path}!");
        let res3 = JsonPathResolver::resolve(&expr3, &outputs).expect("Resolve template");
        assert_eq!(res3, json!("Found 2 matches in src/lib.rs!"));
    }

    #[test]
    fn test_unknown_dependency_rejected() {
        let spec = DagSpec {
            name: "test_unknown".to_string(),
            nodes: vec![DagNodeSpec {
                id: "step1".to_string(),
                tool: "read_file".to_string(),
                args: json!({}),
                depends_on: vec!["nonexistent".to_string()],
            }],
        };

        let err = DagCompiler::topological_waves(&spec).unwrap_err();
        assert!(err.to_string().contains("unknown node 'nonexistent'"));
    }

    #[test]
    fn test_duplicate_node_id_rejected() {
        let spec = DagSpec {
            name: "test_dup".to_string(),
            nodes: vec![
                DagNodeSpec {
                    id: "step1".to_string(),
                    tool: "read_file".to_string(),
                    args: json!({}),
                    depends_on: vec![],
                },
                DagNodeSpec {
                    id: "step1".to_string(),
                    tool: "grep_search".to_string(),
                    args: json!({}),
                    depends_on: vec![],
                },
            ],
        };

        let err = DagCompiler::topological_waves(&spec).unwrap_err();
        assert!(err.to_string().contains("Duplicate node ID detected"));
    }

    #[test]
    fn test_raw_text_output_wrapping() {
        let mut outputs = HashMap::new();
        outputs.insert(
            "cmd".to_string(),
            json!({
                "output": "line 1\nline 2",
                "text": "line 1\nline 2",
                "__raw_text": "line 1\nline 2",
            }),
        );

        let expr = json!("$cmd");
        let res = JsonPathResolver::resolve(&expr, &outputs).expect("Resolve raw text");
        assert_eq!(res, json!("line 1\nline 2"));

        let expr2 = json!("$cmd.output");
        let res2 = JsonPathResolver::resolve(&expr2, &outputs).expect("Resolve .output");
        assert_eq!(res2, json!("line 1\nline 2"));
    }

    #[test]
    fn test_execution_report_formatting() {
        let report = DagExecutionReport {
            dag_name: "sample_pipeline".to_string(),
            total_nodes: 3,
            completed_nodes: 2,
            failed_nodes: 0,
            skipped_nodes: 1,
            wall_clock_ms: 120,
            waves_executed: 2,
            node_results: vec![
                DagNodeResult {
                    node_id: "step1".to_string(),
                    tool: "locate_fault".to_string(),
                    status: NodeExecutionStatus::Success,
                    output: "Found match".to_string(),
                    parsed_json: None,
                    duration_ms: 45,
                },
                DagNodeResult {
                    node_id: "step2".to_string(),
                    tool: "read_file".to_string(),
                    status: NodeExecutionStatus::Success,
                    output: "File contents".to_string(),
                    parsed_json: None,
                    duration_ms: 15,
                },
                DagNodeResult {
                    node_id: "step3".to_string(),
                    tool: "patch_file".to_string(),
                    status: NodeExecutionStatus::SkippedDependencyFailed,
                    output: "Skipped".to_string(),
                    parsed_json: None,
                    duration_ms: 0,
                },
            ],
        };

        let summary = report.format_summary();
        assert!(summary.contains("sample_pipeline"));
        assert!(summary.contains("step1"));
        assert!(summary.contains("✅ Success"));
        assert!(summary.contains("⚠️ Skipped"));
    }
}
