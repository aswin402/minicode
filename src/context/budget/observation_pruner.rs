use crate::context::budget::json_crusher::JsonCrusher;
use crate::context::budget::log_pruner::LogPruner;

/// Unified observation pruner that transparently handles structured JSON payloads,
/// compiler outputs, stacktraces, and execution logs before they enter the LLM context.
pub struct ObservationPruner;

impl ObservationPruner {
    /// Prunes tool output for LLM consumption, selecting the optimal compressor
    /// (JSON crusher or Log pruner) while preserving lossless recovery via CCR cache.
    pub fn prune_for_llm(tool_name: &str, raw_output: &str) -> String {
        // Fast path for trivial outputs
        if raw_output.len() < 128 {
            return raw_output.to_string();
        }

        // 1. Structured JSON compression
        if let Some((crushed, _)) = JsonCrusher::crush(raw_output) {
            return crushed;
        }

        // 2. Command / Build / Test / Log compression
        let is_log_or_command = matches!(
            tool_name,
            "run_command" | "execute_command" | "bash" | "sh" | "terminal"
        ) || raw_output.contains("error[")
            || raw_output.contains("warning:")
            || raw_output.contains("panicked at")
            || raw_output.contains("test result:")
            || raw_output.contains("... ok")
            || raw_output.contains("PASS")
            || raw_output.contains("Traceback (most recent call last)")
            || raw_output.lines().count() > 15;

        if is_log_or_command {
            if let Some((pruned, _)) = LogPruner::prune(raw_output) {
                return pruned;
            }
        }

        // Fallback: raw uncompressed output
        raw_output.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prune_for_llm_json() {
        let items = vec![
            serde_json::json!({"id": 1, "status": "ok", "env": "prod"}),
            serde_json::json!({"id": 2, "status": "ok", "env": "prod"}),
            serde_json::json!({"id": 3, "status": "ok", "env": "prod"}),
            serde_json::json!({"id": 4, "status": "ok", "env": "prod"}),
            serde_json::json!({"id": 5, "status": "ok", "env": "prod"}),
            serde_json::json!({"id": 6, "status": "ok", "env": "prod"}),
        ];
        let raw_json = serde_json::to_string(&items).unwrap();
        let result = ObservationPruner::prune_for_llm("query_api", &raw_json);
        assert!(result.contains("_crushed"));
        assert!(result.contains("_common_fields"));
    }

    #[test]
    fn test_prune_for_llm_log() {
        let mut log = String::new();
        log.push_str("Compiling foo v0.1.0\n");
        for i in 0..25 {
            log.push_str(&format!("Compiling dep_{} v0.1.0\n", i));
        }
        for i in 0..10 {
            log.push_str(&format!("  at tokio::thread_{}:{}\n", i, i * 5));
        }
        log.push_str("Finished dev profile\n");

        let result = ObservationPruner::prune_for_llm("run_command", &log);
        assert!(result.contains("lines collapsed"));
        assert!(result.contains("Use retrieve_observation"));
    }

    #[test]
    fn test_prune_for_llm_warning_cascade() {
        let mut log = String::new();
        for i in 0..10 {
            log.push_str(&format!(
                "warning: unused import: `crate::mod_{}::Helper`\n",
                i
            ));
            log.push_str(&format!(" --> src/file_{}.rs:2:5\n", i));
            log.push_str("  |\n");
            log.push_str(&format!("2 | use crate::mod_{}::Helper;\n", i));
            log.push_str("  |     ^^^^^^^^^^^^^^^^^^^^^^^\n");
        }
        log.push_str("error[E0425]: cannot find value `foo` in this scope\n");
        log.push_str(" --> src/main.rs:12:5\n");

        let result = ObservationPruner::prune_for_llm("run_command", &log);
        assert!(result.contains("compiler warnings collapsed"));
        assert!(result.contains("error[E0425]"));
        assert!(result.contains("Use retrieve_observation"));
    }

    #[test]
    fn test_prune_for_llm_test_runner_flood() {
        let mut log = String::new();
        log.push_str("running 50 tests\n");
        for i in 0..49 {
            log.push_str(&format!("test module::test_{} ... ok\n", i));
        }
        log.push_str("test module::test_failed ... FAILED\n");
        log.push_str("test result: FAILED. 49 passed; 1 failed;\n");

        let result = ObservationPruner::prune_for_llm("run_command", &log);
        assert!(result.contains("passing tests collapsed"));
        assert!(result.contains("test module::test_failed ... FAILED"));
        assert!(result.contains("test result: FAILED"));
        assert!(result.contains("Use retrieve_observation"));
    }

    #[test]
    fn test_prune_for_llm_short_text() {
        let short = "file created successfully";
        let result = ObservationPruner::prune_for_llm("write_file", short);
        assert_eq!(result, short);
    }
}
