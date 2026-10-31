use crate::context::budget::ccr_cache::CcrCache;
use crate::context::budget::compressor::ContextCompressor;
use crate::context::budget::headroom::{
    CodeCompressor, CsvCompressor, DiffCompressor, SearchCompressor,
};
use crate::context::budget::json_crusher::JsonCrusher;
use crate::context::budget::log_pruner::LogPruner;

/// Unified Headroom `ContentRouter` & `LiveZoneDispatcher` that transparently classifies
/// and compresses structured JSON payloads (arrays + nested object arrays), unified `git diff`
/// outputs, `grep`/`ripgrep` search results, compiler/test/runtime logs, and large source code
/// blobs before they enter the LLM context, while enforcing Token-Aware Validation (Invariant I5)
/// and preserving lossless recovery via `CcrCache`.
pub struct ObservationPruner;

impl ObservationPruner {
    /// Routes and compresses tool output for LLM consumption, validating that the compressed
    /// candidate strictly reduces token count (Headroom Invariant I5).
    pub fn prune_for_llm(tool_name: &str, raw_output: &str) -> String {
        // Fast path for trivial outputs
        if raw_output.len() < 128 {
            return raw_output.to_string();
        }

        let orig_tokens = ContextCompressor::count_tokens_fast(raw_output);
        let orig_bytes = raw_output.len();

        // 1. Structured JSON compression (`SmartCrusher` + `AdaptiveSizer` + `AnchorSelector`)
        if let Some((crushed, _)) = JsonCrusher::crush(raw_output) {
            let comp_tokens = ContextCompressor::count_tokens_fast(&crushed);
            if comp_tokens < orig_tokens || crushed.len() < orig_bytes {
                CcrCache::record_compression(
                    "json",
                    orig_bytes,
                    crushed.len(),
                    orig_tokens,
                    comp_tokens,
                );
                return crushed;
            }
        }

        // 1b. Tabular CSV / TSV dataset compression (`CsvCompressor`)
        if let Some((compressed_csv, _)) = CsvCompressor::compress(raw_output) {
            let comp_tokens = ContextCompressor::count_tokens_fast(&compressed_csv);
            if comp_tokens < orig_tokens {
                CcrCache::record_compression(
                    "csv",
                    orig_bytes,
                    compressed_csv.len(),
                    orig_tokens,
                    comp_tokens,
                );
                return compressed_csv;
            }
        }

        // 2. Unified Git Diff compression (`DiffCompressor`)
        if matches!(tool_name, "git_diff" | "exec_cmd" | "run_command" | "bash")
            || DiffCompressor::is_unified_diff(raw_output)
        {
            if let Some((compressed_diff, _)) = DiffCompressor::compress(raw_output) {
                let comp_tokens = ContextCompressor::count_tokens_fast(&compressed_diff);
                if comp_tokens < orig_tokens {
                    CcrCache::record_compression(
                        "diff",
                        orig_bytes,
                        compressed_diff.len(),
                        orig_tokens,
                        comp_tokens,
                    );
                    return compressed_diff;
                }
            }
        }

        // 3. Grep / Ripgrep Search results compression (`SearchCompressor`)
        if matches!(
            tool_name,
            "grep_search" | "exec_cmd" | "run_command" | "bash"
        ) || raw_output.lines().take(5).any(|l| l.contains(':'))
        {
            if let Some((compressed_search, _)) = SearchCompressor::compress(raw_output) {
                let comp_tokens = ContextCompressor::count_tokens_fast(&compressed_search);
                if comp_tokens < orig_tokens {
                    CcrCache::record_compression(
                        "search",
                        orig_bytes,
                        compressed_search.len(),
                        orig_tokens,
                        comp_tokens,
                    );
                    return compressed_search;
                }
            }
        }

        // 4. AST-Preserving Source Code Body Folding (`CodeCompressor`) for raw code outputs
        if !matches!(
            tool_name,
            "exec_cmd" | "run_command" | "execute_command" | "bash" | "sh" | "terminal"
        ) {
            if let Some(ext) = Self::infer_code_extension(raw_output) {
                if let Some((compressed_code, _)) = CodeCompressor::compress(ext, raw_output) {
                    let comp_tokens = ContextCompressor::count_tokens_fast(&compressed_code);
                    if comp_tokens < orig_tokens {
                        CcrCache::record_compression(
                            "code",
                            orig_bytes,
                            compressed_code.len(),
                            orig_tokens,
                            comp_tokens,
                        );
                        return compressed_code;
                    }
                }
            }
        }

        // 5. Command / Build / Test / Log compression (`LogPruner` + Log-Template Clustering)
        let is_log_or_command = matches!(
            tool_name,
            "exec_cmd" | "run_command" | "execute_command" | "bash" | "sh" | "terminal"
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
                let comp_tokens = ContextCompressor::count_tokens_fast(&pruned);
                if comp_tokens < orig_tokens {
                    CcrCache::record_compression(
                        "log",
                        orig_bytes,
                        pruned.len(),
                        orig_tokens,
                        comp_tokens,
                    );
                    return pruned;
                }
            }
        }

        // Fallback: raw uncompressed output
        raw_output.to_string()
    }

    /// Probes Tree-sitter AST grammars across supported languages (`rs`, `ts`, `py`, `js`)
    /// and selects the language grammar that parses the highest number of valid AST definition symbols.
    fn infer_code_extension(text: &str) -> Option<&'static str> {
        if text.len() < 1_500 {
            return None;
        }
        let extractor = crate::context::ast::repomap::RepoMapExtractor::new();
        let candidates: &[&'static str] = &["rs", "ts", "py", "js"];
        let mut best_ext: Option<&'static str> = None;
        let mut best_score = 0usize;

        for &ext in candidates {
            if let Ok(symbols) = extractor.extract_symbols_from_str(ext, text) {
                let fn_count = symbols
                    .iter()
                    .filter(|s| {
                        matches!(s.kind.as_str(), "function" | "method")
                            && s.end_line > s.line_number + 3
                    })
                    .count();
                let def_count = symbols
                    .iter()
                    .filter(|s| s.kind != "import" && !s.name.is_empty())
                    .count();
                // Weight multi-line functions higher since CodeCompressor folds function bodies
                let score = fn_count * 3 + def_count;
                if fn_count >= 2 && score > best_score {
                    best_score = score;
                    best_ext = Some(ext);
                }
            }
        }

        best_ext
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::budget::headroom::{
        AdaptiveSizer, AnchorSelector, CacheAligner, DiffCompressor, SearchCompressor, TagProtector,
    };

    #[test]
    fn test_prune_for_llm_json_and_nested_wrapper_with_numeric_stats() {
        let mut items = Vec::new();
        for i in 1..=25 {
            if i == 14 {
                items.push(serde_json::json!({
                    "id": i,
                    "status": "error",
                    "env": "production-us-east-1",
                    "service": "payment-gateway-worker",
                    "latency_ms": 950.0,
                    "error": "timeout connecting to upstream"
                }));
            } else {
                items.push(serde_json::json!({
                    "id": i,
                    "status": "ok",
                    "env": "production-us-east-1",
                    "service": "payment-gateway-worker",
                    "latency_ms": 10.0 + (i as f64 * 0.5)
                }));
            }
        }
        let wrapped = serde_json::json!({
            "total": 25,
            "results": items
        });
        let raw_json = serde_json::to_string_pretty(&wrapped).unwrap();
        let result = ObservationPruner::prune_for_llm("query_api", &raw_json);
        assert!(result.contains("_crushed"));
        assert!(result.contains("_common_fields"));
        assert!(result.contains("_numeric_stats"));
        // Error item #14 must be preserved
        assert!(result.contains("timeout connecting to upstream"));
    }

    #[test]
    fn test_prune_for_llm_log_and_template_clustering() {
        let mut log = String::new();
        log.push_str("Compiling foo v0.1.0\n");
        for i in 0..25 {
            log.push_str(&format!("Compiling dep_{} v0.1.0\n", i));
        }
        for i in 0..10 {
            log.push_str(&format!(
                "2026-10-31T10:00:{:02}Z INFO [req={}] GET /api/v1/health status=200 latency={}ms\n",
                i,
                1000 + i,
                i + 2
            ));
        }
        log.push_str("2026-10-31T10:00:59Z FATAL Database connection lost at item 67\n");

        let result = ObservationPruner::prune_for_llm("run_command", &log);
        assert!(result.contains("lines collapsed"));
        assert!(result.contains("repetitive log lines collapsed"));
        // FATAL line must survive byte-for-byte
        assert!(result.contains("FATAL Database connection lost at item 67"));
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
    fn test_diff_compressor_and_ccr_query_search() {
        let mut diff = String::from("diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,40 +1,41 @@\n");
        for i in 1..=20 {
            diff.push_str(&format!(" unchanged_line_before_{}\n", i));
        }
        diff.push_str("+pub fn critical_new_entrypoint() {}\n");
        for i in 1..=20 {
            diff.push_str(&format!(" unchanged_line_after_{}\n", i));
        }

        let (compressed, ccr_id) = DiffCompressor::compress(&diff).expect("diff should compress");
        assert!(compressed.contains("unchanged context lines trimmed"));
        assert!(compressed.contains("+pub fn critical_new_entrypoint() {}"));

        // Verify CCR in-cache query search
        let search_out = CcrCache::search(&ccr_id, "critical_new_entrypoint", 1, 10).unwrap();
        assert!(search_out.contains("critical_new_entrypoint"));
        assert!(search_out.contains("unchanged_line_before_20"));
    }

    #[test]
    fn test_search_compressor_and_tag_protector_and_cache_aligner() {
        let mut grep_out = String::new();
        for i in 1..=35 {
            let line_body = if i == 15 {
                "pub fn execute_critical_pipeline() {"
            } else {
                "    let x = compute_helper_value();"
            };
            grep_out.push_str(&format!("src/engine.rs:{}:{}\n", i * 2, line_body));
        }
        let (comp_search, _) =
            SearchCompressor::compress(&grep_out).expect("search output should compress");
        assert!(comp_search.contains("pub fn execute_critical_pipeline()"));
        assert!(comp_search.contains("more match(es) in `src/engine.rs`"));

        // TagProtector test
        let raw_tagged = "Hello <scoped_developer_rules>\n- Keep this intact\n</scoped_developer_rules> and <div>html</div>";
        let protected = TagProtector::protect_tags(raw_tagged);
        assert!(protected.text_with_placeholders.contains("{{HR_TAG_0000}}"));
        assert!(protected.text_with_placeholders.contains("<div>html</div>"));
        let restored = TagProtector::restore_tags(&protected.text_with_placeholders, &protected);
        assert_eq!(restored, raw_tagged);

        // AdaptiveSizer & AnchorSelector determinism
        let items: Vec<String> = (0..30).map(|i| format!("item number {}", i % 4)).collect();
        let k = AdaptiveSizer::compute_optimal_k(&items, 4, 15);
        assert!((4..=15).contains(&k));

        // CacheAligner tool schema normalization
        let mut schemas = vec![
            crate::agent::provider::ToolSchema {
                name: "z_tool".to_string(),
                description: "Multiple   spaces\n  here".to_string(),
                parameters: serde_json::json!({"b": 1, "a": 2}),
            },
            crate::agent::provider::ToolSchema {
                name: "a_tool".to_string(),
                description: "First".to_string(),
                parameters: serde_json::json!({"z": 1, "a": 2}),
            },
        ];
        CacheAligner::normalize_tool_schemas(&mut schemas);
        assert_eq!(schemas[0].name, "a_tool");
        assert_eq!(schemas[1].description, "Multiple spaces here");
        let _ = AnchorSelector::select_indices(&[], 4);
    }

    #[test]
    fn test_code_compressor_and_deterministic_ccr_sha256() {
        let id1 = CcrCache::store("deterministic payload test");
        let id2 = CcrCache::store("deterministic payload test");
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16); // "ccr_" + 12 hex chars

        let mut rust_src = String::from(
            "use std::collections::HashMap;\n\npub struct Config {\n    pub name: String,\n}\n\n",
        );
        for fn_idx in 1..=3 {
            rust_src.push_str(&format!(
                "pub fn compute_stage_{}(x: usize) -> usize {{\n",
                fn_idx
            ));
            for step in 1..=26 {
                rust_src.push_str(&format!(
                    "    let v_{} = x.wrapping_add({});\n",
                    step,
                    step * 7
                ));
            }
            rust_src.push_str("    x + 1\n}\n\n");
        }
        let pruned = ObservationPruner::prune_for_llm("read_file", &rust_src);
        assert!(pruned.contains("pub struct Config"));
        assert!(pruned.contains("pub fn compute_stage_1"));
        assert!(pruned.contains("folded — retrieve_observation"));
    }

    #[test]
    fn test_csv_compressor_global_ccr_search_usd_and_rtk_progress_noise() {
        let mut csv = String::from("id,service,status,latency_ms\n");
        for i in 1..=35 {
            if i == 19 {
                csv.push_str(&format!("{},auth-gateway,error_timeout,1250.5\n", i));
            } else {
                csv.push_str(&format!("{},auth-gateway,ok,{:.1}\n", i, 12.0 + (i as f64)));
            }
        }
        let pruned_csv = ObservationPruner::prune_for_llm("read_file", &csv);
        assert!(pruned_csv.contains("Headers: id,service,status,latency_ms"));
        assert!(pruned_csv.contains("error_timeout"));
        assert!(pruned_csv.contains("Numeric Summary: latency_ms("));
        assert!(pruned_csv.contains("[CCR Summary: CSV contains 35 data rows total"));

        // Global cross-cache search (`id = "*"`)
        let global_search = CcrCache::search("*", "error_timeout", 1, 10).unwrap();
        assert!(global_search.contains("Global CCR Cache Search"));
        assert!(global_search.contains("error_timeout"));

        // Model-aware USD cost estimation
        let stats = CcrCache::stats();
        assert!(stats.tokens_saved() > 0);
        assert!(stats.estimated_usd_saved("claude-sonnet-4") > 0.0);

        // RTK progress noise stripping (git, pip, npm spinners)
        let noisy_cmd = "Enumerating objects: 56, done.\nCounting objects: 100% (56/56), done.\nRequirement already satisfied: pip in /usr/lib\nnpm notice Cleaned 12 packages\n⠋ Building bundle...\nTo github.com:aswin402/headroom-mcp.git\n * [new branch] main -> main";
        let clean_cmd =
            crate::tools::rtk_filter::RtkFilter::strip_command_progress_noise(noisy_cmd);
        assert!(!clean_cmd.contains("Enumerating objects:"));
        assert!(!clean_cmd.contains("Requirement already satisfied:"));
        assert!(!clean_cmd.contains("npm notice"));
        assert!(!clean_cmd.contains("⠋ Building"));
        assert!(clean_cmd.contains("To github.com:aswin402/headroom-mcp.git"));
    }

    #[test]
    fn test_prune_for_llm_short_text() {
        let short = "file created successfully";
        let result = ObservationPruner::prune_for_llm("write_file", short);
        assert_eq!(result, short);
    }
}
