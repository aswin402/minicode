use minicode::context::smell_detector::{AstSmellDetector, SmellCategory};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_god_function_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let mut long_fn = String::from("pub fn massive_function() {\n");
    for i in 0..95 {
        long_fn.push_str(&format!("    let _var_{} = {};\n", i, i));
    }
    long_fn.push_str("}\n");

    let file_path = src_dir.join("long_code.rs");
    fs::write(&file_path, long_fn).expect("write long_code.rs");

    let report = AstSmellDetector::scan_workspace(dir.path(), None, Some("src/long_code.rs"))
        .expect("scan workspace");

    assert!(report.total_smells >= 1);
    let god_smell = report
        .smells
        .iter()
        .find(|s| s.category == SmellCategory::GodFunction);
    assert!(god_smell.is_some());
    assert!(god_smell.unwrap().message.contains("massive_function"));
}

#[test]
fn test_excessive_parameters_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let fn_code = r#"
pub fn too_many_args(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32) -> i32 {
    a + b + c + d + e + f + g
}
"#;
    let file_path = src_dir.join("params.rs");
    fs::write(&file_path, fn_code).expect("write params.rs");

    let report = AstSmellDetector::scan_workspace(dir.path(), None, Some("src/params.rs"))
        .expect("scan workspace");

    let param_smell = report
        .smells
        .iter()
        .find(|s| s.category == SmellCategory::ExcessiveParameters);
    assert!(param_smell.is_some());
    assert!(param_smell.unwrap().message.contains("too_many_args"));
}

#[test]
fn test_deep_nesting_detection() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    let nested_code = r#"
pub fn nested_logic() {
    if true {
        if true {
            if true {
                if true {
                    if true {
                        let _deep = 42;
                    }
                }
            }
        }
    }
}
"#;
    let file_path = src_dir.join("nested.rs");
    fs::write(&file_path, nested_code).expect("write nested.rs");

    let report = AstSmellDetector::scan_workspace(dir.path(), None, Some("src/nested.rs"))
        .expect("scan workspace");

    let nest_smell = report
        .smells
        .iter()
        .find(|s| s.category == SmellCategory::DeepNesting);
    assert!(nest_smell.is_some());
}

#[tokio::test]
async fn test_code_smells_tool_dispatch() {
    let dir = tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).expect("create src");

    fs::write(
        src_dir.join("clean.rs"),
        r#"
pub fn clean_fn() -> i32 {
    10
}
"#,
    )
    .expect("write clean.rs");

    let args = json!({
        "target_file": "src/clean.rs"
    });

    let res =
        minicode::tools::registry::context_tools::dispatch("code_smells", &args, dir.path()).await;

    assert!(res.is_some());
    let output = res.unwrap().expect("tool execution success");
    assert!(output.contains("AST Code Smells & Architectural Health Scorecard"));
}
