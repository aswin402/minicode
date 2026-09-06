use minicode::agent::speculative::{ExecutionPlanner, ExecutionStage, SpeculativeExecutor};
use minicode::agent::types::ToolCall;
use minicode::tools::concurrency::{classify_tool, is_barrier, is_read_only, ToolSafetyLevel};
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_exhaustive_tool_classification() {
    let mut all_schemas = ToolRegistry::get_tool_schemas();
    all_schemas.push(minicode::tools::category::activate_tools_schema());

    assert!(
        all_schemas.len() >= 100,
        "Expected at least 100 tools, found {}",
        all_schemas.len()
    );

    let mut read_only_count = 0;
    let mut mutating_count = 0;
    let mut control_barrier_count = 0;

    for schema in &all_schemas {
        let level = classify_tool(&schema.name);
        match level {
            ToolSafetyLevel::ReadOnly => {
                read_only_count += 1;
                assert!(is_read_only(&schema.name));
                assert!(!is_barrier(&schema.name));
            }
            ToolSafetyLevel::Mutating => {
                mutating_count += 1;
                assert!(!is_read_only(&schema.name));
                assert!(is_barrier(&schema.name));
            }
            ToolSafetyLevel::ControlBarrier => {
                control_barrier_count += 1;
                assert!(!is_read_only(&schema.name));
                assert!(is_barrier(&schema.name));
                assert!(level.is_control_barrier());
            }
        }
    }

    // Verify key invariants
    assert!(
        read_only_count > 30,
        "Expected >30 read only tools, found {}",
        read_only_count
    );
    assert!(
        mutating_count > 30,
        "Expected >30 mutating tools, found {}",
        mutating_count
    );
    assert_eq!(
        control_barrier_count, 1,
        "activate_tools should be the control barrier"
    );

    // Specific spot checks
    assert_eq!(classify_tool("read_file"), ToolSafetyLevel::ReadOnly);
    assert_eq!(classify_tool("grep_search"), ToolSafetyLevel::ReadOnly);
    assert_eq!(classify_tool("list_dir"), ToolSafetyLevel::ReadOnly);
    assert_eq!(classify_tool("get_ast_outline"), ToolSafetyLevel::ReadOnly);
    assert_eq!(classify_tool("write_file"), ToolSafetyLevel::Mutating);
    assert_eq!(classify_tool("patch_file"), ToolSafetyLevel::Mutating);
    assert_eq!(classify_tool("exec_cmd"), ToolSafetyLevel::Mutating);
    assert_eq!(classify_tool("git_commit"), ToolSafetyLevel::Mutating);
    assert_eq!(
        classify_tool("activate_tools"),
        ToolSafetyLevel::ControlBarrier
    );
}

#[test]
fn test_stage_planner_mixed_boundaries() {
    let c1 = ToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "src/main.rs"}),
    };
    let c2 = ToolCall {
        id: "call_2".to_string(),
        name: "grep_search".to_string(),
        arguments: json!({"pattern": "fn main"}),
    };
    let c3 = ToolCall {
        id: "call_3".to_string(),
        name: "write_file".to_string(),
        arguments: json!({"path": "src/test.rs", "content": "fn test() {}"}),
    };
    let c4 = ToolCall {
        id: "call_4".to_string(),
        name: "list_dir".to_string(),
        arguments: json!({"path": "src"}),
    };
    let c5 = ToolCall {
        id: "call_5".to_string(),
        name: "activate_tools".to_string(),
        arguments: json!({"category": "git"}),
    };

    let stages = ExecutionPlanner::plan(vec![
        c1.clone(),
        c2.clone(),
        c3.clone(),
        c4.clone(),
        c5.clone(),
    ]);

    // Expected stages:
    // Stage 0: Parallel([c1, c2])
    // Stage 1: Sequential(c3)
    // Stage 2: Parallel([c4])
    // Stage 3: Sequential(c5)
    assert_eq!(stages.len(), 4);

    assert_eq!(stages[0], ExecutionStage::Parallel(vec![c1, c2]));
    assert_eq!(stages[1], ExecutionStage::Sequential(c3));
    assert_eq!(stages[2], ExecutionStage::Parallel(vec![c4]));
    assert_eq!(stages[3], ExecutionStage::Sequential(c5));
}

#[tokio::test]
async fn test_speculative_executor_parallel_execution_and_deterministic_order() {
    let dir = tempdir().expect("create temp dir");

    // Create 4 test files
    for i in 1..=4 {
        fs::write(
            dir.path().join(format!("file_{}.txt", i)),
            format!("Content of file {}", i),
        )
        .expect("write test file");
    }

    let executor = SpeculativeExecutor::new(dir.path().to_path_buf(), 4, true, true);

    let calls = vec![
        ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "file_1.txt"}),
        },
        ToolCall {
            id: "call_2".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "file_2.txt"}),
        },
        ToolCall {
            id: "call_3".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "file_3.txt"}),
        },
        ToolCall {
            id: "call_4".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "file_4.txt"}),
        },
    ];

    let results = executor.execute_parallel_stage(&calls, 1).await;
    assert_eq!(results.len(), 4);

    // Assert strictly deterministic ordering of output matches input
    assert_eq!(results[0].tool_id, "call_1");
    assert!(results[0].output.contains("Content of file 1"));

    assert_eq!(results[1].tool_id, "call_2");
    assert!(results[1].output.contains("Content of file 2"));

    assert_eq!(results[2].tool_id, "call_3");
    assert!(results[2].output.contains("Content of file 3"));

    assert_eq!(results[3].tool_id, "call_4");
    assert!(results[3].output.contains("Content of file 4"));

    let telem = executor.telemetry().await;
    assert_eq!(telem.parallel_tools_executed, 4);
    assert_eq!(telem.parallel_stages_executed, 1);
}

#[tokio::test]
async fn test_speculative_streaming_pre_execution_hit() {
    let dir = tempdir().expect("create temp dir");

    fs::write(
        dir.path().join("streaming_test.txt"),
        "Speculative streaming hit!",
    )
    .expect("write test file");

    let executor = SpeculativeExecutor::new(dir.path().to_path_buf(), 4, true, true);

    let call = ToolCall {
        id: "call_stream_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "streaming_test.txt"}),
    };

    // Pre-execute during simulated LLM token stream
    executor.on_tool_call_streamed(1, &call).await;

    // Allow background tokio task to finish execution
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Now execute parallel stage with the same call
    let results = executor.execute_parallel_stage(&[call], 1).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_id, "call_stream_1");
    assert!(results[0].output.contains("Speculative streaming hit!"));

    let telem = executor.telemetry().await;
    assert_eq!(telem.speculative_hits, 1);
    assert_eq!(telem.speculative_launched, 1);
}

#[tokio::test]
async fn test_speculative_executor_cancellation_and_reset() {
    let dir = tempdir().expect("create temp dir");

    let executor = SpeculativeExecutor::new(dir.path().to_path_buf(), 4, true, true);

    let call = ToolCall {
        id: "call_cancel_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "nonexistent.txt"}),
    };

    executor.on_tool_call_streamed(1, &call).await;
    executor.cancel_all().await;
    executor.reset_turn().await;

    let telem = executor.telemetry().await;
    assert_eq!(telem.speculative_hits, 0);
}
