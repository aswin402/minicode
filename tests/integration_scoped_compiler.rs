use minicode::context::syntax_guard::SyntaxGuard;
use minicode::tools::compiler::ScopedCompiler;
use minicode::tools::fs::{patch_file, write_file};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_ast_syntax_barrier_blocks_syntax_corruption() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/calculator.rs";

    // 1. Initial valid Rust file written to disk
    let initial_content = "\
pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
";
    let write_res = write_file(workspace, rel_path, initial_content);
    assert!(write_res.is_ok());

    // 2. Attempt a patch that introduces a broken unclosed syntax error
    let search = "    a * b\n}";
    let broken_replace = "    let x = (a * b;\n    x\n"; // Unclosed parenthesis and missing closing brace

    let patch_res = patch_file(workspace, rel_path, search, broken_replace);
    assert!(patch_res.is_err());

    let err_msg = patch_res.unwrap_err().to_string();
    assert!(err_msg.contains("[AST Syntax Barrier Rejected]"));
    assert!(err_msg.contains("calculator.rs"));
    assert!(err_msg.contains("The original file on disk was preserved untouched."));

    // 3. Verify disk content is 100% pristine and unmodified
    let on_disk = std::fs::read_to_string(workspace.join(rel_path)).unwrap();
    assert_eq!(on_disk, initial_content);
}

#[test]
fn test_ast_syntax_barrier_allows_syntax_repair() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/broken_initial.rs";

    // Write a file that starts out with syntax errors (e.g. half-written by user)
    // Directly write to disk bypassing tool or using non-code extension, or write raw
    let broken_initial = "fn incomplete_func(";
    std::fs::create_dir_all(workspace.join("src")).unwrap();
    std::fs::write(workspace.join(rel_path), broken_initial).unwrap();

    // Now call patch_file or write_file to fix it
    let fixed_content = "fn incomplete_func() {}\n";
    let fix_res = write_file(workspace, rel_path, fixed_content);
    assert!(fix_res.is_ok());

    let on_disk = std::fs::read_to_string(workspace.join(rel_path)).unwrap();
    assert_eq!(on_disk, fixed_content);
}

#[test]
fn test_ast_syntax_barrier_python_validation() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "app/service.py";

    let valid_py = "\
def calculate_tax(subtotal):
    return subtotal * 0.08
";
    assert!(write_file(workspace, rel_path, valid_py).is_ok());

    // Attempt to introduce Python syntax error
    let search = "    return subtotal * 0.08";
    let broken_replace = "    return subtotal * ";

    let patch_res = patch_file(workspace, rel_path, search, broken_replace);
    assert!(patch_res.is_err());
    let err_msg = patch_res.unwrap_err().to_string();
    assert!(err_msg.contains("[AST Syntax Barrier Rejected]"));
    assert!(err_msg.contains("service.py"));

    // File preserved
    let on_disk = std::fs::read_to_string(workspace.join(rel_path)).unwrap();
    assert_eq!(on_disk, valid_py);
}

#[test]
fn test_ast_syntax_barrier_typescript_validation() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/models/user.ts";

    let valid_ts = "\
export interface UserProfile {
    id: string;
    email: string;
}
";
    assert!(write_file(workspace, rel_path, valid_ts).is_ok());

    // Corrupted syntax
    let search = "export interface UserProfile {";
    let broken_replace = "export interface UserProfile"; // Missing opening brace

    let patch_res = patch_file(workspace, rel_path, search, broken_replace);
    assert!(patch_res.is_err());
    let err_msg = patch_res.unwrap_err().to_string();
    assert!(err_msg.contains("[AST Syntax Barrier Rejected]"));
    assert!(err_msg.contains("user.ts"));
}

#[test]
fn test_scoped_compiler_python_linter() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "valid_script.py";

    let content = "x = 42\nprint(f'value: {x}')\n";
    std::fs::write(workspace.join(rel_path), content).unwrap();

    let feedback = ScopedCompiler::check_python(workspace, rel_path);
    if let Some(msg) = feedback {
        assert!(msg.contains("py_compile passed cleanly"));
    }
}

#[test]
fn test_syntax_guard_direct_api() {
    let path = PathBuf::from("nested/mod.rs");
    let orig = "pub fn is_ready() -> bool { true }\n";
    let valid_edit = "pub fn is_ready() -> bool { false }\n";
    let invalid_edit = "pub fn is_ready() -> bool { \n";

    assert!(SyntaxGuard::check_syntax_barrier(&path, orig, valid_edit).is_ok());
    assert!(SyntaxGuard::check_syntax_barrier(&path, orig, invalid_edit).is_err());
}
