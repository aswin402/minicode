use minicode::context::semantic::SemanticIndex;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_semantic_index_chunking_and_cosine_retrieval() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let src = ws.join("src");
    fs::create_dir_all(&src).unwrap();

    let db_file = src.join("db.rs");
    fs::write(
        &db_file,
        r#"
pub struct ConnectionPool {
    max_connections: u32,
}

impl ConnectionPool {
    pub fn connect(db_url: &str) -> Result<Self, String> {
        println!("Opening postgres SQL database connection to {}", db_url);
        Ok(Self { max_connections: 10 })
    }
}
"#,
    )
    .unwrap();

    let ui_file = src.join("ui.rs");
    fs::write(
        &ui_file,
        r#"
pub fn render_gradient_banner(title: &str) {
    println!("Drawing colorful banner with title {}", title);
}
"#,
    )
    .unwrap();

    let mut index = SemanticIndex::new();
    let indexed = index.build_index(ws).unwrap();
    assert_eq!(indexed, 2);

    let results = index.search("postgres sql database connection pool", 5);
    assert!(!results.is_empty());
    assert_eq!(results[0].file_path, "src/db.rs");
    assert!(results[0].snippet.contains("ConnectionPool"));
}

#[tokio::test]
async fn test_semantic_search_tool_dispatch() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let src = ws.join("src");
    fs::create_dir_all(&src).unwrap();

    let auth_file = src.join("crypto.rs");
    fs::write(
        &auth_file,
        r#"
pub fn hash_password(password: &str) -> String {
    format!("argon2_hash_{}", password)
}
"#,
    )
    .unwrap();

    let args = json!({
        "query": "hash user passwords with argon2",
        "limit": 3
    });

    let res = ToolRegistry::dispatch(ws, "call_sem", "semantic_search", &args, None, 1).await;
    assert!(res.success);
    assert!(res.output.contains("src/crypto.rs"));
    assert!(res.output.contains("hash_password"));
}
