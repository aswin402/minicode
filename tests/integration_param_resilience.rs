use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_read_file_and_write_file_path_aliases() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    // 1. write_file using 'file_path' alias instead of 'path'
    let write_res = ToolRegistry::dispatch(
        ws,
        "call_w1",
        "write_file",
        &json!({
            "file_path": "hello.txt",
            "content": "Hello from minicode universal core!\n"
        }),
        None,
        1,
    )
    .await;
    assert!(
        write_res.success,
        "write_file with file_path alias failed: {}",
        write_res.output
    );
    assert!(ws.join("hello.txt").exists());

    // 2. read_file using 'file' alias instead of 'path'
    let read_res1 = ToolRegistry::dispatch(
        ws,
        "call_r1",
        "read_file",
        &json!({
            "file": "hello.txt"
        }),
        None,
        1,
    )
    .await;
    assert!(
        read_res1.success,
        "read_file with 'file' alias failed: {}",
        read_res1.output
    );
    assert!(read_res1.output.contains("Hello from minicode"));

    // 3. read_file using 'filename' alias
    let read_res2 = ToolRegistry::dispatch(
        ws,
        "call_r2",
        "read_file",
        &json!({
            "filename": "hello.txt"
        }),
        None,
        1,
    )
    .await;
    assert!(
        read_res2.success,
        "read_file with 'filename' alias failed: {}",
        read_res2.output
    );
    assert!(read_res2.output.contains("Hello from minicode"));
}

#[tokio::test]
async fn test_patch_file_string_aliases() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let target = ws.join("code.rs");
    fs::write(&target, "fn calculate() -> i32 {\n    41\n}\n").expect("write code.rs");

    // patch_file using 'old_string' and 'new_string' (aider / cline aliases)
    let patch_res = ToolRegistry::dispatch(
        ws,
        "call_p1",
        "patch_file",
        &json!({
            "path": "code.rs",
            "old_string": "    41",
            "new_string": "    42"
        }),
        None,
        1,
    )
    .await;

    assert!(
        patch_res.success,
        "patch_file with old_string/new_string failed: {}",
        patch_res.output
    );
    let updated = fs::read_to_string(&target).expect("read code.rs");
    assert!(updated.contains("42"));
}

#[tokio::test]
async fn test_exec_cmd_aliases() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    // exec_cmd with 'cmd' alias instead of 'command'
    let exec_res = ToolRegistry::dispatch(
        ws,
        "call_e1",
        "exec_cmd",
        &json!({
            "cmd": "echo 'tolerant execution works'"
        }),
        None,
        1,
    )
    .await;

    assert!(
        exec_res.success,
        "exec_cmd with 'cmd' alias failed: {}",
        exec_res.output
    );
    assert!(exec_res.output.contains("tolerant execution works"));
}

#[tokio::test]
async fn test_search_tools_aliases_and_bool_coercion() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let file_path = ws.join("app.rs");
    fs::write(
        &file_path,
        "pub fn run_daemon() {\n    let active = true;\n}\n",
    )
    .expect("write app.rs");

    // grep_search with 'pattern' alias and string boolean coercion "is_regex": "false"
    let grep_res = ToolRegistry::dispatch(
        ws,
        "call_g1",
        "grep_search",
        &json!({
            "pattern": "run_daemon",
            "is_regex": "false"
        }),
        None,
        1,
    )
    .await;

    assert!(
        grep_res.success,
        "grep_search with pattern alias and string bool failed: {}",
        grep_res.output
    );
    assert!(grep_res.output.contains("app.rs"));

    // file_search with 'search' alias
    let search_res = ToolRegistry::dispatch(
        ws,
        "call_fs1",
        "file_search",
        &json!({
            "search": "app.rs"
        }),
        None,
        1,
    )
    .await;

    assert!(
        search_res.success,
        "file_search with 'search' alias failed: {}",
        search_res.output
    );
    assert!(search_res.output.contains("app.rs"));
}

#[tokio::test]
async fn test_update_progress_requires_status_with_aliases() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    // 1. Missing status must fail cleanly with InvalidArguments
    let fail_res = ToolRegistry::dispatch(
        ws,
        "call_up1",
        "update_progress",
        &json!({
            "step": "Step 1: Setup workspace"
        }),
        None,
        1,
    )
    .await;

    assert!(
        !fail_res.success,
        "update_progress without status must not succeed silently"
    );
    assert!(fail_res.output.contains("status"));

    // 2. Status with alias 'state' must succeed
    let ok_res = ToolRegistry::dispatch(
        ws,
        "call_up2",
        "update_progress",
        &json!({
            "step": "Step 1: Setup workspace",
            "state": "In Progress"
        }),
        None,
        1,
    )
    .await;

    assert!(
        ok_res.success,
        "update_progress with 'state' alias failed: {}",
        ok_res.output
    );
    assert!(ok_res.output.contains("In Progress"));
}

#[tokio::test]
async fn test_lsp_goto_definition_defaults_line_and_col() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let dummy_file = ws.join("lib.rs");
    fs::write(&dummy_file, "pub fn hello() {}\n").expect("write lib.rs");

    // lsp_goto_definition with only 'path' (missing line and character defaults to 1)
    let res = ToolRegistry::dispatch(
        ws,
        "call_lsp1",
        "lsp_goto_definition",
        &json!({
            "path": "lib.rs"
        }),
        None,
        1,
    )
    .await;

    // Even if no LSP server is running on the test machine, it should not fail with "Missing required argument"
    assert!(
        !res.output.contains("Missing required argument"),
        "lsp_goto_definition should not complain about missing line/character: {}",
        res.output
    );
}

#[tokio::test]
async fn test_param_single_string_array_coercion() {
    let temp = tempdir().expect("Failed to create tempdir");
    let ws = temp.path();

    let file_path = ws.join("search_target.rs");
    fs::write(&file_path, "pub fn find_me() {}\n").expect("write file");

    // locate_fault with candidate_files as a single string instead of array
    let res = ToolRegistry::dispatch(
        ws,
        "call_lf1",
        "locate_fault",
        &json!({
            "query": "find_me",
            "candidate_files": "search_target.rs"
        }),
        None,
        1,
    )
    .await;

    assert!(
        res.success,
        "locate_fault with single-string candidate_files should succeed via coercion: {}",
        res.output
    );
}
