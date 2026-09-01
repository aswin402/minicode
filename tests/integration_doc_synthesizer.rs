use minicode::context::doc_synthesizer::{ArchitectureDocOptions, ArchitectureDocSynthesizer};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_architecture_doc_synthesis_markdown() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    let ui_dir = src_dir.join("ui");
    let agent_dir = src_dir.join("agent");
    let store_dir = src_dir.join("session");
    fs::create_dir_all(&ui_dir).expect("create ui");
    fs::create_dir_all(&agent_dir).expect("create agent");
    fs::create_dir_all(&store_dir).expect("create session");

    fs::write(ui_dir.join("view.rs"), "pub fn render_view() {}").expect("write view.rs");
    fs::write(agent_dir.join("loop.rs"), "pub fn run_agent_loop() {}").expect("write loop.rs");
    fs::write(store_dir.join("store.rs"), "pub struct SessionStore;").expect("write store.rs");

    let options = ArchitectureDocOptions {
        include_mermaid: true,
        include_symbol_catalog: true,
        write_to_file: false,
    };

    let report = ArchitectureDocSynthesizer::synthesize(dir.path(), options)
        .expect("synthesize architecture docs");

    assert!(report.total_files >= 3);
    assert!(report
        .markdown_content
        .contains("Architecture Documentation"));
    assert!(report.markdown_content.contains("flowchart TD"));
    assert!(report.markdown_content.contains("Presentation"));
}

#[test]
fn test_architecture_doc_file_write() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(src_dir.join("main.rs"), "fn main() {}").expect("write main.rs");

    let options = ArchitectureDocOptions {
        include_mermaid: true,
        include_symbol_catalog: true,
        write_to_file: true,
    };

    let report = ArchitectureDocSynthesizer::synthesize(dir.path(), options)
        .expect("synthesize architecture docs");

    assert_eq!(report.file_written, Some("ARCHITECTURE.md".to_string()));
    let file_on_disk = dir.path().join("ARCHITECTURE.md");
    assert!(file_on_disk.exists());
    let content = fs::read_to_string(file_on_disk).expect("read written file");
    assert!(content.contains("System Overview"));
}

#[tokio::test]
async fn test_generate_architecture_docs_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(src_dir.join("main.rs"), "fn main() {}").expect("write main.rs");

    let args = json!({
        "write_to_file": false,
        "include_mermaid": true
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "generate_architecture_docs",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Architecture Documentation"));
}
