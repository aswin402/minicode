use minicode::context::ast_transform::AstTransformer;
use minicode::tools::ToolRegistry;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_ast_transformer_queries_and_extraction() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let server_file = ws.join("server.py");
    fs::write(
        &server_file,
        r#"
class MicroService:
    def __init__(self, port: int):
        self.port = port

    def start(self):
        print(f"Starting on {self.port}")

def health_check():
    return {"status": "ok"}
"#,
    )
    .unwrap();

    let nodes = AstTransformer::query_nodes(ws, "server.py", None, None).unwrap();
    assert!(nodes.iter().any(|n| n.name == "MicroService"));
    assert!(nodes.iter().any(|n| n.name == "health_check"));

    let extracted = AstTransformer::extract_symbol(ws, "server.py", "health_check").unwrap();
    assert_eq!(extracted.name, "health_check");
    assert!(extracted.snippet.contains("return {\"status\": \"ok\"}"));
}

#[tokio::test]
async fn test_ast_tools_dispatch() {
    let dir = tempdir().unwrap();
    let ws = dir.path();

    let ts_file = ws.join("handler.ts");
    fs::write(
        &ts_file,
        r#"
export interface UserPayload {
    id: string;
    role: string;
}

export function processUser(payload: UserPayload): boolean {
    return payload.role === "admin";
}
"#,
    )
    .unwrap();

    // 1. ast_query tool dispatch
    let query_args = json!({
        "file_path": "handler.ts",
        "node_kind": "function_declaration"
    });
    let query_res =
        ToolRegistry::dispatch(ws, "call_ast1", "ast_query", &query_args, None, 1).await;
    assert!(query_res.success);
    assert!(query_res.output.contains("processUser"));

    // 2. ast_extract_symbol tool dispatch
    let extract_args = json!({
        "file_path": "handler.ts",
        "symbol_name": "processUser"
    });
    let extract_res = ToolRegistry::dispatch(
        ws,
        "call_ast2",
        "ast_extract_symbol",
        &extract_args,
        None,
        1,
    )
    .await;
    assert!(extract_res.success);
    assert!(extract_res.output.contains("payload.role === \"admin\""));
}
