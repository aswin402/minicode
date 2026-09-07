//! Integration tests for Dynamic Execution DAG & Inter-Tool JSONPath Pipelining (Phase 97, v0.2.8).

use minicode::agent::dag::{
    DagCompiler, DagExecutor, DagNodeSpec, DagSpec, JsonPathResolver, NodeExecutionStatus,
};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_dag_linear_execution_and_pipelining() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    // Setup input file
    let input_path = ws.join("hello.txt");
    std::fs::write(&input_path, "Hello minicode DAG pipelining!").expect("Write input file");

    let spec = DagSpec {
        name: "linear_read_and_copy".to_string(),
        nodes: vec![
            DagNodeSpec {
                id: "reader".to_string(),
                tool: "read_file".to_string(),
                args: json!({ "path": "hello.txt" }),
                depends_on: vec![],
            },
            DagNodeSpec {
                id: "writer".to_string(),
                tool: "write_file".to_string(),
                args: json!({
                    "path": "copy.txt",
                    "content": "$reader.output"
                }),
                depends_on: vec!["reader".to_string()],
            },
        ],
    };

    let report = DagExecutor::execute(ws, &spec)
        .await
        .expect("DAG execution should succeed");

    assert_eq!(report.total_nodes, 2);
    assert_eq!(report.completed_nodes, 2);
    assert_eq!(report.failed_nodes, 0);
    assert_eq!(report.skipped_nodes, 0);
    assert_eq!(report.waves_executed, 2);

    let copy_path = ws.join("copy.txt");
    assert!(copy_path.exists(), "Pipelined copy.txt must exist");
    let copy_content = std::fs::read_to_string(&copy_path).expect("Read copy.txt");
    assert!(copy_content.contains("Hello minicode DAG pipelining!"));
}

#[tokio::test]
async fn test_dag_fork_join_wave_execution() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let path_a = ws.join("file_a.txt");
    let path_b = ws.join("file_b.txt");
    std::fs::write(&path_a, "alpha").expect("Write file_a");
    std::fs::write(&path_b, "beta").expect("Write file_b");

    let spec = DagSpec {
        name: "fork_join_pipeline".to_string(),
        nodes: vec![
            DagNodeSpec {
                id: "read_a".to_string(),
                tool: "read_file".to_string(),
                args: json!({ "path": "file_a.txt" }),
                depends_on: vec![],
            },
            DagNodeSpec {
                id: "read_b".to_string(),
                tool: "read_file".to_string(),
                args: json!({ "path": "file_b.txt" }),
                depends_on: vec![],
            },
            DagNodeSpec {
                id: "merge".to_string(),
                tool: "write_file".to_string(),
                args: json!({
                    "path": "merged.txt",
                    "content": "Merged: $read_a.output and $read_b.output"
                }),
                depends_on: vec!["read_a".to_string(), "read_b".to_string()],
            },
        ],
    };

    let report = DagExecutor::execute(ws, &spec)
        .await
        .expect("Fork-join execution should succeed");

    assert_eq!(report.total_nodes, 3);
    assert_eq!(report.completed_nodes, 3);
    assert_eq!(report.failed_nodes, 0);
    assert_eq!(report.skipped_nodes, 0);
    assert_eq!(report.waves_executed, 2);

    let merged_path = ws.join("merged.txt");
    assert!(merged_path.exists());
    let merged_content = std::fs::read_to_string(&merged_path).expect("Read merged");
    assert!(merged_content.contains("alpha"));
    assert!(merged_content.contains("beta"));
}

#[tokio::test]
async fn test_dag_upstream_failure_skips_dependents() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let spec = DagSpec {
        name: "failure_cascade_test".to_string(),
        nodes: vec![
            DagNodeSpec {
                id: "bad_node".to_string(),
                tool: "read_file".to_string(),
                args: json!({ "path": "nonexistent_secret_file.txt" }),
                depends_on: vec![],
            },
            DagNodeSpec {
                id: "dependent_node".to_string(),
                tool: "write_file".to_string(),
                args: json!({
                    "path": "unwanted.txt",
                    "content": "Should never be written"
                }),
                depends_on: vec!["bad_node".to_string()],
            },
        ],
    };

    let report = DagExecutor::execute(ws, &spec)
        .await
        .expect("Execution itself returns report without panicking");

    assert_eq!(report.total_nodes, 2);
    assert_eq!(report.completed_nodes, 0);
    assert_eq!(report.failed_nodes, 1);
    assert_eq!(report.skipped_nodes, 1);

    // Dependent file must NOT have been created
    let unwanted = ws.join("unwanted.txt");
    assert!(!unwanted.exists(), "Dependent node must have been skipped");

    // Check status
    let dep_result = report
        .node_results
        .iter()
        .find(|r| r.node_id == "dependent_node")
        .expect("dependent_node result must exist");
    assert_eq!(
        dep_result.status,
        NodeExecutionStatus::SkippedDependencyFailed
    );
    assert!(dep_result.output.contains("bad_node"));
}

#[tokio::test]
async fn test_dag_cycle_rejection() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let spec = DagSpec {
        name: "cyclic_test".to_string(),
        nodes: vec![
            DagNodeSpec {
                id: "node_1".to_string(),
                tool: "read_file".to_string(),
                args: json!({}),
                depends_on: vec!["node_2".to_string()],
            },
            DagNodeSpec {
                id: "node_2".to_string(),
                tool: "read_file".to_string(),
                args: json!({}),
                depends_on: vec!["node_1".to_string()],
            },
        ],
    };

    let err = DagExecutor::execute(ws, &spec).await.unwrap_err();
    assert!(err.to_string().contains("Cycle detected in DAG"));
}

#[tokio::test]
async fn test_dag_nested_execute_dag_disallowed() {
    let spec = DagSpec {
        name: "nested_guard".to_string(),
        nodes: vec![DagNodeSpec {
            id: "sub_dag".to_string(),
            tool: "execute_dag".to_string(),
            args: json!({}),
            depends_on: vec![],
        }],
    };

    let err = DagCompiler::topological_waves(&spec).unwrap_err();
    assert!(err.to_string().contains("nested DAGs are not permitted"));
}

#[tokio::test]
async fn test_dag_tool_registry_dispatch() {
    let temp = TempDir::new().expect("Failed to create tempdir");
    let ws = temp.path();

    let sample_file = ws.join("message.txt");
    std::fs::write(&sample_file, "Registry dispatch test message").expect("Write sample");

    let dag_args = json!({
        "name": "registry_dispatch_flow",
        "nodes": [
            {
                "id": "read_step",
                "tool": "read_file",
                "args": { "path": "message.txt" },
                "depends_on": []
            },
            {
                "id": "write_step",
                "tool": "write_file",
                "args": {
                    "path": "dispatched.txt",
                    "content": "Received: $read_step.output"
                },
                "depends_on": ["read_step"]
            }
        ]
    });

    let res = ToolRegistry::dispatch(ws, "dag_call_1", "execute_dag", &dag_args, None, 1).await;

    assert!(
        res.success,
        "execute_dag tool call should succeed: {}",
        res.output
    );
    assert!(res
        .output
        .contains("DAG Execution Report: `registry_dispatch_flow`"));
    assert!(res.output.contains("read_step"));
    assert!(res.output.contains("write_step"));
    assert!(res.output.contains("✅ Success"));

    let out_file = ws.join("dispatched.txt");
    assert!(out_file.exists());
    let content = std::fs::read_to_string(out_file).expect("Read dispatched file");
    assert!(content.contains("Registry dispatch test message"));
}

#[tokio::test]
async fn test_dag_jsonpath_template_interpolation() {
    let mut outputs = HashMap::new();
    outputs.insert(
        "analysis".to_string(),
        json!({
            "target": "src/main.rs",
            "lines_scanned": 120,
            "status": "clean"
        }),
    );

    let template = json!(
        "Scanned $analysis.lines_scanned lines in ${analysis.target} (Status: $analysis.status)"
    );
    let resolved = JsonPathResolver::resolve(&template, &outputs).expect("Resolve template");
    assert_eq!(
        resolved,
        json!("Scanned 120 lines in src/main.rs (Status: clean)")
    );
}
