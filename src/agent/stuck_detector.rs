//! Algorithmic Anti-Thrashing Circuit Breaker & Execution Guard (Phase 112).
//!
//! Protects against runaway agent loops, file editing thrash cycles, and execution collapse
//! by injecting authoritative prescriptive guidance and enforcing hard breaker trips when ignored.

use crate::constants::{
    ANTI_THRASH_COLLAPSE_THRESHOLD, ANTI_THRASH_FILE_FAILURE_THRESHOLD,
    ANTI_THRASH_HARD_TRIP_LIMIT, STUCK_CONSECUTIVE_FAILURE_THRESHOLD,
    STUCK_CONSECUTIVE_TOOL_CALL_THRESHOLD, STUCK_MAX_HISTORY_ENTRIES, STUCK_OSCILLATION_MIN_CYCLES,
};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

/// Deterministic fingerprint representing a tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallFingerprint {
    pub tool_name: String,
    pub args_hash: u64,
    pub target_file: Option<String>,
    pub success: bool,
}

/// Category of algorithmic loop or thrashing detected during ReAct execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopType {
    /// The exact same tool and arguments were executed consecutively
    ConsecutiveRepetition {
        tool_name: String,
        count: usize,
        is_failure: bool,
    },
    /// Thrashing repeatedly on the same target file (even with differing arguments)
    FileTargetThrashing {
        file: String,
        consecutive_failures: usize,
        last_tool: String,
    },
    /// Multiple consecutive failed tool calls in a row across any tools (execution collapse)
    ExecutionCollapse {
        consecutive_failures: usize,
        tools: Vec<String>,
    },
    /// Alternating oscillation between two actions (A -> B -> A -> B)
    PingPongOscillation {
        tool_a: String,
        tool_b: String,
        cycles: usize,
    },
    /// Triangular cyclic oscillation across three actions (A -> B -> C -> A -> B -> C)
    TriangularOscillation { pattern: Vec<String>, cycles: usize },
}

impl LoopType {
    /// Machine-readable pattern name for event telemetry
    pub fn pattern_name(&self) -> &'static str {
        match self {
            Self::ConsecutiveRepetition { .. } => "consecutive_repetition",
            Self::FileTargetThrashing { .. } => "file_target_thrashing",
            Self::ExecutionCollapse { .. } => "execution_collapse",
            Self::PingPongOscillation { .. } => "ping_pong_oscillation",
            Self::TriangularOscillation { .. } => "triangular_oscillation",
        }
    }

    /// Internal key identifying this specific pattern instance
    pub fn pattern_key(&self) -> String {
        match self {
            Self::ConsecutiveRepetition {
                tool_name,
                is_failure,
                ..
            } => {
                format!("repeat:{}:{}", tool_name, is_failure)
            }
            Self::FileTargetThrashing { file, .. } => format!("file:{}", file),
            Self::ExecutionCollapse { .. } => "collapse".to_string(),
            Self::PingPongOscillation { tool_a, tool_b, .. } => {
                format!("pingpong:{}:{}", tool_a, tool_b)
            }
            Self::TriangularOscillation { pattern, .. } => {
                format!("triangular:{}", pattern.join("-"))
            }
        }
    }

    /// Target file if this loop is tied to a specific file
    pub fn target_file(&self) -> Option<String> {
        match self {
            Self::FileTargetThrashing { file, .. } => Some(file.clone()),
            _ => None,
        }
    }

    /// Number of failure or oscillation iterations observed
    pub fn failures(&self) -> usize {
        match self {
            Self::ConsecutiveRepetition {
                count, is_failure, ..
            } => {
                if *is_failure {
                    *count
                } else {
                    0
                }
            }
            Self::FileTargetThrashing {
                consecutive_failures,
                ..
            } => *consecutive_failures,
            Self::ExecutionCollapse {
                consecutive_failures,
                ..
            } => *consecutive_failures,
            Self::PingPongOscillation { cycles, .. } => *cycles,
            Self::TriangularOscillation { cycles, .. } => *cycles,
        }
    }

    /// Short human-readable summary
    pub fn summary(&self) -> String {
        match self {
            Self::ConsecutiveRepetition {
                tool_name,
                count,
                is_failure,
            } => {
                let status = if *is_failure { "failing" } else { "repeated" };
                format!(
                    "`{}` called with identical arguments {} times ({})",
                    tool_name, count, status
                )
            }
            Self::FileTargetThrashing {
                file,
                consecutive_failures,
                last_tool,
            } => {
                format!(
                    "Failed {} consecutive file modification attempts on `{}` using `{}`",
                    consecutive_failures, file, last_tool
                )
            }
            Self::ExecutionCollapse {
                consecutive_failures,
                tools,
            } => {
                format!(
                    "{} consecutive tool failures across: {}",
                    consecutive_failures,
                    tools.join(", ")
                )
            }
            Self::PingPongOscillation {
                tool_a,
                tool_b,
                cycles,
            } => {
                format!(
                    "Alternating ping-pong loop between `{}` and `{}` ({} cycles)",
                    tool_a, tool_b, cycles
                )
            }
            Self::TriangularOscillation { pattern, cycles } => {
                format!(
                    "Cyclic oscillation {} ({} cycles)",
                    pattern.join(" -> "),
                    cycles
                )
            }
        }
    }
}

/// Action decided by the Anti-Thrashing Circuit Breaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerAction {
    /// Normal operation: continue
    Pass,
    /// Warning: soft intervention appended to tool output for LLM self-correction
    Warning(String),
    /// Hard Trip: consecutive violations after warning. Halt the runaway loop!
    Trip { reason: String, loop_type: LoopType },
}

#[allow(dead_code)]
impl BreakerAction {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    pub fn is_trip(&self) -> bool {
        matches!(self, Self::Trip { .. })
    }

    pub fn is_warning(&self) -> bool {
        matches!(self, Self::Warning(_))
    }

    pub fn intervention(&self) -> Option<&str> {
        match self {
            Self::Warning(msg) => Some(msg),
            Self::Trip { reason, .. } => Some(reason),
            Self::Pass => None,
        }
    }
}

/// Algorithmic Anti-Thrashing Circuit Breaker & Runaway Loop Guard.
///
/// Tracks tool execution fingerprints and halts repetitive failure loops
/// by injecting authoritative, prescriptive circuit-breaker guidance and tripping
/// when warnings are ignored.
#[derive(Debug, Clone)]
pub struct StuckDetector {
    history: VecDeque<ToolCallFingerprint>,
    max_history: usize,
    consecutive_threshold: usize,
    consecutive_failure_threshold: usize,
    file_failure_threshold: usize,
    collapse_threshold: usize,
    hard_trip_limit: usize,
    warning_counts: HashMap<String, usize>,
    tripped: bool,
}

impl Default for StuckDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StuckDetector {
    /// Creates a new StuckDetector with standard anti-thrashing thresholds.
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(STUCK_MAX_HISTORY_ENTRIES),
            max_history: STUCK_MAX_HISTORY_ENTRIES,
            consecutive_threshold: STUCK_CONSECUTIVE_TOOL_CALL_THRESHOLD,
            consecutive_failure_threshold: STUCK_CONSECUTIVE_FAILURE_THRESHOLD,
            file_failure_threshold: ANTI_THRASH_FILE_FAILURE_THRESHOLD,
            collapse_threshold: ANTI_THRASH_COLLAPSE_THRESHOLD,
            hard_trip_limit: ANTI_THRASH_HARD_TRIP_LIMIT,
            warning_counts: HashMap::new(),
            tripped: false,
        }
    }

    /// Resets the loop detector history and circuit breaker state.
    pub fn reset(&mut self) {
        self.history.clear();
        self.warning_counts.clear();
        self.tripped = false;
    }

    /// Returns whether the circuit breaker has tripped.
    #[allow(dead_code)]
    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    /// Extracts a target file path from tool arguments if present.
    pub fn extract_target_file(args: &Value) -> Option<String> {
        for key in &["path", "file_path", "target_file", "file"] {
            if let Some(s) = args.get(*key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    /// Records a tool execution and evaluates circuit breaker policy.
    pub fn check(&mut self, tool_name: &str, args: &Value, success: bool) -> BreakerAction {
        if self.tripped {
            let last_type = self.detect_loop().unwrap_or(LoopType::ExecutionCollapse {
                consecutive_failures: self.collapse_threshold,
                tools: vec![tool_name.to_string()],
            });
            return BreakerAction::Trip {
                reason: "🛑 [CIRCUIT BREAKER: TRIPPED] Execution was halted because previous loop warnings were ignored. Please review the blocker above and request guidance from the user."
                    .to_string(),
                loop_type: last_type,
            };
        }

        let args_hash = Self::compute_args_hash(args);
        let target_file = Self::extract_target_file(args);

        let record = ToolCallFingerprint {
            tool_name: tool_name.to_string(),
            args_hash,
            target_file,
            success,
        };

        self.history.push_back(record);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        // Successful execution clears failure streaks for target files and execution collapse
        if success {
            self.warning_counts
                .retain(|k, _| !k.starts_with("file:") && !k.starts_with("collapse"));
        }

        let loop_type = match self.detect_loop() {
            Some(lt) => lt,
            None => return BreakerAction::Pass,
        };

        let key = loop_type.pattern_key();
        let count = self.warning_counts.entry(key).or_insert(0);
        *count += 1;

        if *count >= self.hard_trip_limit {
            self.tripped = true;
            let reason = self.format_trip_reason(&loop_type);
            BreakerAction::Trip { reason, loop_type }
        } else {
            let warning = self.format_intervention(&loop_type);
            BreakerAction::Warning(warning)
        }
    }

    /// Backward-compatible helper returning an intervention message if warning or tripped.
    #[allow(dead_code)]
    pub fn record_and_check(
        &mut self,
        tool_name: &str,
        args: &Value,
        success: bool,
    ) -> Option<String> {
        match self.check(tool_name, args, success) {
            BreakerAction::Pass => None,
            BreakerAction::Warning(msg) => Some(msg),
            BreakerAction::Trip { reason, .. } => Some(reason),
        }
    }

    /// Inspects recent history for repetition patterns.
    pub fn detect_loop(&self) -> Option<LoopType> {
        if self.history.is_empty() {
            return None;
        }

        let len = self.history.len();
        let last = &self.history[len - 1];

        // 1. Consecutive identical call detection
        let mut consecutive_count = 1;
        let mut all_failed = !last.success;

        for i in (0..len.saturating_sub(1)).rev() {
            let item = &self.history[i];
            if item.tool_name == last.tool_name && item.args_hash == last.args_hash {
                consecutive_count += 1;
                if item.success {
                    all_failed = false;
                }
            } else {
                break;
            }
        }

        if all_failed && consecutive_count >= self.consecutive_failure_threshold {
            return Some(LoopType::ConsecutiveRepetition {
                tool_name: last.tool_name.clone(),
                count: consecutive_count,
                is_failure: true,
            });
        }

        if consecutive_count >= self.consecutive_threshold {
            return Some(LoopType::ConsecutiveRepetition {
                tool_name: last.tool_name.clone(),
                count: consecutive_count,
                is_failure: false,
            });
        }

        // 2. File target thrashing detection (consecutive failures on the same file)
        if !last.success {
            if let Some(ref target) = last.target_file {
                let mut file_failures = 1;
                for i in (0..len.saturating_sub(1)).rev() {
                    let item = &self.history[i];
                    if !item.success && item.target_file.as_ref() == Some(target) {
                        file_failures += 1;
                    } else {
                        break;
                    }
                }
                if file_failures >= self.file_failure_threshold {
                    return Some(LoopType::FileTargetThrashing {
                        file: target.clone(),
                        consecutive_failures: file_failures,
                        last_tool: last.tool_name.clone(),
                    });
                }
            }
        }

        // 3. Execution collapse detection (global consecutive failures across all tools)
        if !last.success {
            let mut global_failures = 1;
            let mut failed_tools = vec![last.tool_name.clone()];
            for i in (0..len.saturating_sub(1)).rev() {
                let item = &self.history[i];
                if !item.success {
                    global_failures += 1;
                    if !failed_tools.contains(&item.tool_name) {
                        failed_tools.push(item.tool_name.clone());
                    }
                } else {
                    break;
                }
            }
            if global_failures >= self.collapse_threshold {
                return Some(LoopType::ExecutionCollapse {
                    consecutive_failures: global_failures,
                    tools: failed_tools,
                });
            }
        }

        // 4. Ping-pong alternating loop detection (Period 2: A, B, A, B)
        if len >= 4 {
            let a = &self.history[len - 2];
            let b = &self.history[len - 1];

            // Ensure A and B are distinct and matching period 2
            if (a.tool_name != b.tool_name || a.args_hash != b.args_hash)
                && &self.history[len - 4] == a
                && &self.history[len - 3] == b
            {
                let mut cycles = 2;
                if len >= 6 && &self.history[len - 6] == a && &self.history[len - 5] == b {
                    cycles = 3;
                }
                if cycles >= STUCK_OSCILLATION_MIN_CYCLES {
                    return Some(LoopType::PingPongOscillation {
                        tool_a: a.tool_name.clone(),
                        tool_b: b.tool_name.clone(),
                        cycles,
                    });
                }
            }
        }

        // 5. Triangular cyclic loop detection (Period 3: A, B, C, A, B, C)
        if len >= 6 {
            let a = &self.history[len - 3];
            let b = &self.history[len - 2];
            let c = &self.history[len - 1];

            // Ensure distinct items and matching period 3
            if a != b
                && b != c
                && a != c
                && &self.history[len - 6] == a
                && &self.history[len - 5] == b
                && &self.history[len - 4] == c
            {
                return Some(LoopType::TriangularOscillation {
                    pattern: vec![
                        a.tool_name.clone(),
                        b.tool_name.clone(),
                        c.tool_name.clone(),
                    ],
                    cycles: 2,
                });
            }
        }

        None
    }

    /// Formats an actionable circuit-breaker warning for the model.
    pub fn format_intervention(&self, loop_type: &LoopType) -> String {
        match loop_type {
            LoopType::ConsecutiveRepetition {
                tool_name,
                count,
                is_failure,
            } => {
                let status_desc = if *is_failure {
                    "failing consecutively"
                } else {
                    "executed repeatedly"
                };
                format!(
                    "============================================================\n\
                     ⚠️ [CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]\n\
                     You have called `{tool_name}` with identical arguments {count} times ({status_desc}).\n\
                     STOP repeating this action immediately.\n\n\
                     Prescriptive Guidance:\n\
                     1. Do NOT re-execute `{tool_name}` with these identical arguments.\n\
                     2. If the operation is failing, carefully inspect the diagnostic above and adjust your approach.\n\
                     3. Consider using alternative search or inspection tools (e.g. locate_symbol, grep_search, read_file with broader bounds).\n\
                     4. If you are uncertain of how to proceed, present your findings and ask the user for direction.\n\
                     ============================================================"
                )
            }
            LoopType::FileTargetThrashing {
                file,
                consecutive_failures,
                last_tool,
            } => {
                format!(
                    "============================================================\n\
                     ⚠️ [CIRCUIT BREAKER TRIGGERED: FILE TARGET THRASHING]\n\
                     You have failed {consecutive_failures} consecutive file modification attempts on `{file}` (last tool: `{last_tool}`).\n\
                     STOP blindly re-attempting edits on this file.\n\n\
                     Prescriptive Guidance:\n\
                     1. Re-read the file with `read_file` to confirm current line numbers and surrounding context.\n\
                     2. If you are attempting a patch that fails to match, inspect exact lines with `grep_search`.\n\
                     3. Consider using `locate_fault` or `ast_diff` to analyze AST structure.\n\
                     4. If the file structure does not match expectations, ask the user for clarification rather than repeatedly trying edits.\n\
                     ============================================================"
                )
            }
            LoopType::ExecutionCollapse {
                consecutive_failures,
                tools,
            } => {
                let tools_str = tools.join(", ");
                format!(
                    "============================================================\n\
                     ⚠️ [CIRCUIT BREAKER TRIGGERED: EXECUTION COLLAPSE]\n\
                     The last {consecutive_failures} tool operations have ALL failed consecutively ({tools_str}).\n\
                     Your current recovery strategy is not working.\n\n\
                     Prescriptive Guidance:\n\
                     1. Step back and analyze why recent commands have all failed.\n\
                     2. Check workspace state with `get_git_status` or inspect error logs.\n\
                     3. Formulate a fundamentally different hypothesis before invoking further tools.\n\
                     4. If blocked, state the failure causes clearly and ask the user for guidance.\n\
                     ============================================================"
                )
            }
            LoopType::PingPongOscillation {
                tool_a,
                tool_b,
                cycles,
            } => {
                format!(
                    "============================================================\n\
                     ⚠️ [CIRCUIT BREAKER TRIGGERED: PING-PONG OSCILLATION]\n\
                     You are stuck in an alternating loop between `{tool_a}` and `{tool_b}` ({cycles} complete cycles).\n\
                     STOP alternating between these two operations.\n\n\
                     Prescriptive Guidance:\n\
                     1. Step back and break this loop.\n\
                     2. Synthesize the findings you already gathered from `{tool_a}` and `{tool_b}`.\n\
                     3. Make a decisive move: apply a concrete code edit, run verification tests, or update the user.\n\
                     ============================================================"
                )
            }
            LoopType::TriangularOscillation { pattern, cycles } => {
                let pat_str = pattern.join(" -> ");
                format!(
                    "============================================================\n\
                     ⚠️ [CIRCUIT BREAKER TRIGGERED: CYCLIC OSCILLATION]\n\
                     You are stuck in a cyclic loop: {pat_str} ({cycles} complete cycles).\n\
                     STOP repeating this cycle.\n\n\
                     Prescriptive Guidance:\n\
                     1. Halt this repetitive sequence immediately.\n\
                     2. Formulate a new hypothesis or report your blocker directly to the user.\n\
                     ============================================================"
                )
            }
        }
    }

    /// Formats a hard circuit-breaker trip message when warnings are ignored.
    pub fn format_trip_reason(&self, loop_type: &LoopType) -> String {
        format!(
            "============================================================\n\
             🛑 [CIRCUIT BREAKER HARD TRIP: RUNAWAY LOOP HALTED]\n\
             The anti-thrashing circuit breaker has tripped and halted turn execution.\n\
             Reason: {}\n\
             Previous warnings were ignored and repetitive failure persisted.\n\
             Turn execution stopped early to preserve tokens and prevent workspace corruption.\n\
             ============================================================",
            loop_type.summary()
        )
    }

    /// Recursively canonicalizes a JSON Value into a deterministic string representation.
    pub fn canonicalize_json(val: &Value) -> String {
        match val {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(Self::canonicalize_json).collect();
                format!("[{}]", items.join(","))
            }
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let items: Vec<String> = keys
                    .into_iter()
                    .map(|k| format!("{}:{}", k, Self::canonicalize_json(&map[k])))
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }

    /// Computes a 64-bit deterministic hash of canonicalized JSON arguments.
    pub fn compute_args_hash(val: &Value) -> u64 {
        let canonical = Self::canonicalize_json(val);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        canonical.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_canonical_json_sorting() {
        let json1 = json!({"b": 2, "a": 1, "c": [3, 2, 1]});
        let json2 = json!({"a": 1, "c": [3, 2, 1], "b": 2});

        assert_eq!(
            StuckDetector::canonicalize_json(&json1),
            StuckDetector::canonicalize_json(&json2)
        );
        assert_eq!(
            StuckDetector::compute_args_hash(&json1),
            StuckDetector::compute_args_hash(&json2)
        );
    }

    #[test]
    fn test_extract_target_file() {
        assert_eq!(
            StuckDetector::extract_target_file(&json!({"path": "src/main.rs"})),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            StuckDetector::extract_target_file(&json!({"file_path": "Cargo.toml"})),
            Some("Cargo.toml".to_string())
        );
        assert_eq!(
            StuckDetector::extract_target_file(&json!({"target_file": "tests/test.rs"})),
            Some("tests/test.rs".to_string())
        );
        assert_eq!(
            StuckDetector::extract_target_file(&json!({"command": "cargo test"})),
            None
        );
    }

    #[test]
    fn test_consecutive_failure_triggers_early() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs", "search": "foo"});

        // 1st failure: no trigger
        let res1 = detector.record_and_check("patch_file", &args, false);
        assert!(res1.is_none());

        // 2nd failure with identical args: triggers circuit breaker warning!
        let res2 = detector.record_and_check("patch_file", &args, false);
        assert!(res2.is_some());
        let msg = res2.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]"));
        assert!(msg.contains("patch_file"));
        assert!(msg.contains("failing consecutively"));
    }

    #[test]
    fn test_file_target_thrashing_detected() {
        let mut detector = StuckDetector::new();

        // 3 consecutive failures on the same file with DIFFERENT arguments
        let args1 = json!({"path": "src/calc.rs", "search": "let a = 1;"});
        let args2 = json!({"path": "src/calc.rs", "search": "let a = 2;"});
        let args3 = json!({"path": "src/calc.rs", "search": "let a = 3;"});

        assert!(detector
            .record_and_check("patch_file", &args1, false)
            .is_none());
        assert!(detector
            .record_and_check("patch_file", &args2, false)
            .is_none());

        // 3rd failure on src/calc.rs -> triggers file thrashing!
        let action = detector.check("patch_file", &args3, false);
        assert!(action.is_warning());
        let msg = action.intervention().unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: FILE TARGET THRASHING]"));
        assert!(msg.contains("src/calc.rs"));
    }

    #[test]
    fn test_execution_collapse_detected() {
        let mut detector = StuckDetector::new();

        // 4 consecutive failures across different tools
        assert!(detector
            .record_and_check("exec_cmd", &json!({"command": "cargo check"}), false)
            .is_none());
        assert!(detector
            .record_and_check("locate_symbol", &json!({"name": "MyStruct"}), false)
            .is_none());
        assert!(detector
            .record_and_check("read_file", &json!({"path": "missing.rs"}), false)
            .is_none());

        // 4th failure
        let action = detector.check(
            "fetch_or_browse",
            &json!({"url": "http://bad.local"}),
            false,
        );
        assert!(action.is_warning());
        let msg = action.intervention().unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: EXECUTION COLLAPSE]"));
        assert!(msg.contains("4 tool operations have ALL failed"));
    }

    #[test]
    fn test_hard_trip_after_ignored_warnings() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs", "search": "foo"});

        // 1st failure: Pass
        assert_eq!(
            detector.check("patch_file", &args, false),
            BreakerAction::Pass
        );

        // 2nd failure: Warning (1st warning on this pattern)
        let act2 = detector.check("patch_file", &args, false);
        assert!(act2.is_warning());
        assert!(!detector.is_tripped());

        // 3rd failure: Hard trip! (2nd warning on this pattern -> hard trip)
        let act3 = detector.check("patch_file", &args, false);
        assert!(act3.is_trip());
        assert!(detector.is_tripped());
        assert!(act3
            .intervention()
            .unwrap()
            .contains("HARD TRIP: RUNAWAY LOOP HALTED"));
    }

    #[test]
    fn test_consecutive_success_triggers_at_threshold() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs"});

        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
        let res3 = detector.record_and_check("read_file", &args, true);
        assert!(res3.is_some());
        let msg = res3.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]"));
        assert!(msg.contains("3 times"));
    }

    #[test]
    fn test_ping_pong_oscillation_detected() {
        let mut detector = StuckDetector::new();
        let args_a = json!({"path": "src/a.rs"});
        let args_b = json!({"path": "src/b.rs"});

        detector.record_and_check("read_file", &args_a, true);
        detector.record_and_check("read_file", &args_b, true);
        detector.record_and_check("read_file", &args_a, true);

        let res = detector.record_and_check("read_file", &args_b, true);
        assert!(res.is_some());
        let msg = res.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: PING-PONG OSCILLATION]"));
        assert!(msg.contains("2 complete cycles"));
    }

    #[test]
    fn test_triangular_oscillation_detected() {
        let mut detector = StuckDetector::new();
        let args_a = json!({"q": "query1"});
        let args_b = json!({"q": "query2"});
        let args_c = json!({"q": "query3"});

        detector.record_and_check("grep_search", &args_a, true);
        detector.record_and_check("locate_symbol", &args_b, true);
        detector.record_and_check("read_file", &args_c, true);

        detector.record_and_check("grep_search", &args_a, true);
        detector.record_and_check("locate_symbol", &args_b, true);

        let res = detector.record_and_check("read_file", &args_c, true);
        assert!(res.is_some());
        let msg = res.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CYCLIC OSCILLATION]"));
        assert!(msg.contains("grep_search -> locate_symbol -> read_file"));
    }

    #[test]
    fn test_reset_clears_history_and_trip_state() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs"});

        detector.record_and_check("read_file", &args, true);
        detector.record_and_check("read_file", &args, true);

        detector.reset();

        assert!(!detector.is_tripped());
        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
    }
}
