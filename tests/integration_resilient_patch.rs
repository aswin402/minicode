use minicode::tools::fs::{patch_file, read_file, write_file};
use tempfile::tempdir;

#[test]
fn test_tier1_exact_match_and_ambiguity_rejection() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/service.rs";

    // 1. Single exact match succeeds
    let content = "fn process() {\n    let status = 200;\n    println!(\"done\");\n}\n";
    write_file(workspace, rel_path, content).unwrap();

    let res = patch_file(
        workspace,
        rel_path,
        "    let status = 200;",
        "    let status = 201;",
    );
    assert!(res.is_ok());
    let read_back = read_file(workspace, rel_path, None, None).unwrap();
    assert!(read_back.contains("let status = 201;"));

    // 2. Ambiguous exact matches are rejected with line numbers
    let ambig_content = "fn a() {\n    let x = 1;\n}\nfn b() {\n    let x = 1;\n}\n";
    write_file(workspace, rel_path, ambig_content).unwrap();

    let err_res = patch_file(workspace, rel_path, "let x = 1;", "let x = 99;");
    assert!(err_res.is_err());
    let err_msg = err_res.unwrap_err().to_string();
    assert!(err_msg.contains("matches multiple (2 times) locations at lines: [2, 5]"));
}

#[test]
fn test_tier2_crlf_and_trailing_whitespace_relaxation() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "crlf_file.rs";

    // Windows CRLF file with trailing whitespace
    let content = "fn calculate() {\r\n    let base = 10;   \r\n    base * 2\r\n}\r\n";
    write_file(workspace, rel_path, content).unwrap();

    // Unix LF search block without trailing spaces
    let search = "fn calculate() {\n    let base = 10;\n    base * 2\n}";
    let replace = "fn calculate() {\n    let base = 20;\n    base * 3\n}";

    let res = patch_file(workspace, rel_path, search, replace);
    assert!(res.is_ok());

    let read_back = read_file(workspace, rel_path, None, None).unwrap();
    assert!(read_back.contains("let base = 20;"));
    assert!(read_back.contains("base * 3"));
}

#[test]
fn test_tier3_deep_indentation_auto_realignment() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/nested.rs";

    // File has 12-space deep indentation
    let content = "\
impl Engine {
    pub fn execute(&self) {
        if self.is_ready() {
            let config = self.load_config();
            let state = self.init_state(config);
            state.run();
        }
    }
}
";
    write_file(workspace, rel_path, content).unwrap();

    // LLM supplies 0-space unindented search block and replacement with inner relative indents
    let search = "\
let config = self.load_config();
let state = self.init_state(config);
state.run();";

    let replace = "\
let config = self.load_config();
if config.is_valid() {
    let state = self.init_state(config);
    state.run_verified();
}";

    let res = patch_file(workspace, rel_path, search, replace);
    assert!(res.is_ok());

    let read_back = std::fs::read_to_string(workspace.join(rel_path)).unwrap();
    // Verify that the replacement was re-aligned to 12 spaces, and inner block to 16 spaces
    assert!(read_back.contains("            let config = self.load_config();\n            if config.is_valid() {\n                let state = self.init_state(config);"));
    assert!(read_back.contains("                state.run_verified();"));
}

#[test]
fn test_tier4_blank_line_relaxation() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/sparse.rs";

    // File contains blank lines between statements
    let content = "\
fn setup() {
    init_logger();

    load_env();

    start_server();
}
";
    write_file(workspace, rel_path, content).unwrap();

    // Search block has no blank lines
    let search = "\
    init_logger();
    load_env();
    start_server();";

    let replace = "\
    init_logger();
    load_env();
    init_metrics();
    start_server();";

    let res = patch_file(workspace, rel_path, search, replace);
    assert!(res.is_ok());

    let read_back = read_file(workspace, rel_path, None, None).unwrap();
    assert!(read_back.contains("init_metrics();"));
}

#[test]
fn test_tier5_fuzzy_diff_and_uniqueness_gap() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/fuzzy_target.rs";

    let content = "\
fn process_payload(data: &[u8]) -> Result<Vec<u8>, Error> {
    let validated = validate_payload_checksum(data)?;
    let decompressed = decompress_buffer(&validated)?;
    Ok(decompressed)
}
";
    write_file(workspace, rel_path, content).unwrap();

    // LLM has minor typo in variable name in search block (`validate_payload_chksum` instead of `validate_payload_checksum`)
    let search = "\
    let validated = validate_payload_chksum(data)?;
    let decompressed = decompress_buffer(&validated)?;
    Ok(decompressed)";

    let replace = "\
    let validated = validate_payload_checksum(data)?;
    let decompressed = decompress_buffer_fast(&validated)?;
    Ok(decompressed)";

    let res = patch_file(workspace, rel_path, search, replace);
    assert!(res.is_ok());

    let read_back = read_file(workspace, rel_path, None, None).unwrap();
    assert!(read_back.contains("decompress_buffer_fast(&validated)"));
}

#[test]
fn test_actionable_four_part_diagnostic_on_mismatch() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let rel_path = "src/algo.rs";

    let content = "\
pub fn quick_sort(arr: &mut [i32]) {
    if arr.len() <= 1 {
        return;
    }
    let pivot_idx = partition(arr);
    quick_sort(&mut arr[0..pivot_idx]);
    quick_sort(&mut arr[pivot_idx + 1..]);
}
";
    write_file(workspace, rel_path, content).unwrap();

    // Completely mismatched search block
    let search = "\
pub fn merge_sort(items: &mut [i32]) {
    let mid = items.len() / 2;
    merge_halves(items, mid);
}";
    let replace = "pub fn sort_linear(items: &mut [i32]) {}";

    let res = patch_file(workspace, rel_path, search, replace);
    assert!(res.is_err());

    let err_str = res.unwrap_err().to_string();

    // 1. What failed
    assert!(err_str.contains("Search block could not be found in 'src/algo.rs'"));

    // 2. Where (nearest match snippet and similarity)
    assert!(err_str.contains("[Where] Nearest match found at lines"));
    assert!(err_str.contains("% similarity)"));

    // 3. Expected search block
    assert!(err_str.contains("[Expected Search Block]"));
    assert!(err_str.contains("pub fn merge_sort(items: &mut [i32])"));

    // 4. Suggested prescriptive next action with read_file command
    assert!(err_str.contains("[Suggested Next Action]"));
    assert!(err_str.contains("read_file(path: \"src/algo.rs\""));
}
