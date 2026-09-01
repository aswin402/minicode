use minicode::context::reranker::CrossEncoderReranker;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_semantic_code_search_intent_matching() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    // Target module
    fs::write(
        src_dir.join("stream_writer.rs"),
        r#"
pub struct StreamWriter {
    pub buffer: Vec<u8>,
}

impl StreamWriter {
    pub fn flush_stream_to_disk(&mut self) {
        // sync and flush pending bytes
    }
}
"#,
    )
    .expect("write stream_writer.rs");

    // Unrelated module
    fs::write(
        src_dir.join("network_client.rs"),
        r#"
pub fn send_http_request(url: &str) -> String {
    format!("GET {}", url)
}
"#,
    )
    .expect("write network_client.rs");

    let result =
        CrossEncoderReranker::search_and_rerank(dir.path(), "flush stream buffer to disk", 5, None)
            .expect("semantic search and rerank");

    assert!(!result.hits.is_empty());
    let top_hit = &result.hits[0];
    assert!(top_hit.file_path.contains("stream_writer.rs"));
    assert!(top_hit.rerank_score > 0.4);
}

#[tokio::test]
async fn test_semantic_code_search_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("auth.rs"),
        r#"
pub fn verify_jwt_token(token: &str) -> bool {
    !token.is_empty()
}
"#,
    )
    .expect("write auth.rs");

    let args = json!({
        "query": "jwt token verification",
        "limit": 3
    });

    let res = minicode::tools::registry::context_tools::dispatch(
        "semantic_code_search",
        &args,
        dir.path(),
    )
    .await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("Semantic Code Search Results"));
    assert!(output.contains("jwt token verification"));
}
