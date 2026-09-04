use crate::constants::{
    STUCK_CONSECUTIVE_FAILURE_THRESHOLD, STUCK_CONSECUTIVE_TOOL_CALL_THRESHOLD,
    STUCK_MAX_HISTORY_ENTRIES, STUCK_OSCILLATION_MIN_CYCLES,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

/// Deterministic fingerprint representing a tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallFingerprint {
    pub tool_name: String,
    pub args_hash: u64,
    pub success: bool,
}

/// Category of algorithmic loop detected during ReAct execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopType {
    /// The exact same tool and arguments were executed consecutively
    ConsecutiveRepetition {
        tool_name: String,
        count: usize,
        is_failure: bool,
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

/// Algorithmic Stuck Detector & Loop Breaker.
///
/// Tracks tool execution fingerprints and halts repetitive doom loops
/// by injecting authoritative, prescriptive circuit-breaker guidance.
#[derive(Debug, Clone)]
pub struct StuckDetector {
    history: VecDeque<ToolCallFingerprint>,
    max_history: usize,
    consecutive_threshold: usize,
    consecutive_failure_threshold: usize,
}

impl Default for StuckDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StuckDetector {
    /// Creates a new StuckDetector with default thresholds.
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(STUCK_MAX_HISTORY_ENTRIES),
            max_history: STUCK_MAX_HISTORY_ENTRIES,
            consecutive_threshold: STUCK_CONSECUTIVE_TOOL_CALL_THRESHOLD,
            consecutive_failure_threshold: STUCK_CONSECUTIVE_FAILURE_THRESHOLD,
        }
    }

    /// Resets the loop detector history (typically called at the start of each user turn).
    pub fn reset(&mut self) {
        self.history.clear();
    }

    /// Records a tool call execution and returns an intervention message if a loop is detected.
    pub fn record_and_check(
        &mut self,
        tool_name: &str,
        args: &Value,
        success: bool,
    ) -> Option<String> {
        let args_hash = Self::compute_args_hash(args);
        let record = ToolCallFingerprint {
            tool_name: tool_name.to_string(),
            args_hash,
            success,
        };

        self.history.push_back(record);
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        let loop_type = self.detect_loop()?;
        Some(self.format_intervention(&loop_type))
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

        // 2. Ping-pong alternating loop detection (Period 2: A, B, A, B)
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

        // 3. Triangular cyclic loop detection (Period 3: A, B, C, A, B, C)
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
    fn test_consecutive_failure_triggers_early() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs", "search": "foo"});

        // 1st failure: no trigger
        let res1 = detector.record_and_check("patch_file", &args, false);
        assert!(res1.is_none());

        // 2nd failure with identical args: triggers circuit breaker!
        let res2 = detector.record_and_check("patch_file", &args, false);
        assert!(res2.is_some());
        let msg = res2.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CONSECUTIVE REPETITION]"));
        assert!(msg.contains("patch_file"));
        assert!(msg.contains("failing consecutively"));
    }

    #[test]
    fn test_consecutive_success_triggers_at_threshold() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs"});

        // 1st call
        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
        // 2nd call
        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
        // 3rd call with same args: triggers!
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

        // 4th step: completes 2 cycles of A -> B -> A -> B
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

        // 6th step: completes 2 cycles of A -> B -> C -> A -> B -> C
        let res = detector.record_and_check("read_file", &args_c, true);
        assert!(res.is_some());
        let msg = res.unwrap();
        assert!(msg.contains("[CIRCUIT BREAKER TRIGGERED: CYCLIC OSCILLATION]"));
        assert!(msg.contains("grep_search -> locate_symbol -> read_file"));
    }

    #[test]
    fn test_reset_clears_history() {
        let mut detector = StuckDetector::new();
        let args = json!({"path": "src/main.rs"});

        detector.record_and_check("read_file", &args, true);
        detector.record_and_check("read_file", &args, true);

        detector.reset();

        // After reset, 1st call does not trigger
        assert!(detector
            .record_and_check("read_file", &args, true)
            .is_none());
    }
}
