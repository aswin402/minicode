/// Integration tests for Phase 37: RTK-Style Token Output Compactor & Fast Diff Folding Engine
///
/// Tests output compaction across Cargo, Pytest, Go, Git Diff, Git Log, Npm,
/// and token savings metrics.
use minicode::tools::compactor::{
    calculate_compaction_stats, compact_tool_output, detect_strategy, CompactStrategy,
};

#[test]
fn test_detect_compaction_strategies() {
    assert_eq!(detect_strategy("cargo check"), CompactStrategy::CargoCheck);
    assert_eq!(
        detect_strategy("cargo test --lib"),
        CompactStrategy::CargoTest
    );
    assert_eq!(
        detect_strategy("pytest tests/test_api.py"),
        CompactStrategy::Pytest
    );
    assert_eq!(
        detect_strategy("python -m unittest discover"),
        CompactStrategy::Pytest
    );
    assert_eq!(detect_strategy("go test ./..."), CompactStrategy::GoTest);
    assert_eq!(detect_strategy("git diff HEAD~1"), CompactStrategy::GitDiff);
    assert_eq!(detect_strategy("git log -n 10"), CompactStrategy::GitLog);
    assert_eq!(detect_strategy("npm test"), CompactStrategy::Npm);
    assert_eq!(detect_strategy("cat Cargo.toml"), CompactStrategy::Generic);
}

#[test]
fn test_cargo_compaction_pass_and_fail() {
    let raw_cargo_pass = "   Compiling minicode v0.0.46\n    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.14s\n";
    let comp_pass = compact_tool_output("cargo check", raw_cargo_pass, Some(0));
    assert!(comp_pass.starts_with("✔ Finished"));

    let raw_cargo_fail = "   Compiling minicode v0.0.46\nerror[E0425]: cannot find value `foo` in this scope\n  --> src/main.rs:12:5\nerror: could not compile `minicode`\n";
    let comp_fail = compact_tool_output("cargo check", raw_cargo_fail, Some(101));
    assert!(comp_fail.contains("cannot find value `foo`"));
    assert!(!comp_fail.contains("Compiling minicode"));
}

#[test]
fn test_pytest_and_go_compaction() {
    let raw_pytest_pass = "============================= test session starts ==============================\ncollected 24 items\n........................\n============================== 24 passed in 1.12s ==============================\n";
    let comp_pytest = compact_tool_output("pytest", raw_pytest_pass, Some(0));
    assert!(comp_pytest.starts_with("✔"));
    assert!(comp_pytest.contains("24 passed in 1.12s"));

    let raw_go_pass = "ok  	github.com/my/project/pkg/agent	0.420s\nok  	github.com/my/project/pkg/tools	0.180s\n";
    let comp_go = compact_tool_output("go test ./...", raw_go_pass, Some(0));
    assert!(comp_go.contains("✔ ok  	github.com/my/project/pkg/agent"));
}

#[test]
fn test_token_savings_calculation() {
    let raw = "a".repeat(10000);
    let compacted = "a".repeat(500);
    let stats = calculate_compaction_stats(&raw, &compacted);
    assert_eq!(stats.savings_percent, 95);
}
