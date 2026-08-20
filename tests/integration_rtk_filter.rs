use minicode::tools::rtk_filter::RtkFilter;

#[test]
fn test_rtk_filter_compresses_verbose_cargo_test_logs() {
    let mut verbose_log = String::from("running 50 tests\n");
    for i in 1..=50 {
        verbose_log.push_str(&format!("test module::unit_test_{} ... ok\n", i));
    }
    verbose_log.push_str("\ntest result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s\n");

    let result = RtkFilter::filter("cargo test -- --test-threads=1", &verbose_log, Some(0));

    assert!(
        result.saved_pct > 70.0,
        "Expected >70% token savings, got {}%",
        result.saved_pct
    );
    assert!(result.content.contains("All tests passed successfully"));
    assert!(result.content.contains("test result: ok. 50 passed"));
}

#[test]
fn test_rtk_filter_isolates_critical_failures_on_error() {
    let failure_log = r#"
running 5 tests
test tests::test_1 ... ok
test tests::test_2 ... ok
test tests::test_3 ... FAILED
test tests::test_4 ... ok
test tests::test_5 ... ok

failures:

---- tests::test_3 stdout ----
thread 'tests::test_3' panicked at src/service.rs:108:5:
assertion `left == right` failed
  left: "expected_token"
 right: "received_token"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

failures:
    tests::test_3

test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
"#;

    let result = RtkFilter::filter("cargo test", failure_log, Some(101));

    assert!(result.content.contains("failures:"));
    assert!(result.content.contains("assertion `left == right` failed"));
    assert!(result
        .content
        .contains("test result: FAILED. 4 passed; 1 failed"));
}
