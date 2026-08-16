use crate::error::{Result, ToolError};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// A single node in a Sequential Thinking / Graph of Thoughts (GoT) reasoning session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThoughtNode {
    pub thought_number: usize,
    pub total_thoughts: usize,
    pub thought: String,
    #[serde(default)]
    pub is_revision: bool,
    #[serde(default)]
    pub revises_thought: Option<usize>,
    #[serde(default)]
    pub branch_from_thought: Option<usize>,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default = "default_needs_more")]
    pub needs_more_thoughts: bool,
    #[serde(default)]
    pub score: Option<f32>, // 0.0 to 1.0 confidence score
}

fn default_needs_more() -> bool {
    true
}

/// Structured response output after processing a thought.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThoughtResponse {
    pub thought_number: usize,
    pub total_thoughts: usize,
    pub current_branch: String,
    pub total_branches: usize,
    pub is_complete: bool,
    pub summary_so_far: String,
}

/// An active reasoning session tracking thoughts, branches, revisions, and scores.
#[derive(Debug, Default)]
pub struct ThinkingSession {
    pub thoughts: Vec<ThoughtNode>,
    pub graph: DiGraph<usize, ()>, // Graph of thought indices
    pub number_to_node: HashMap<usize, NodeIndex>,
    pub branches: HashMap<String, Vec<usize>>,
}

static ACTIVE_THINKING_SESSION: Mutex<Option<ThinkingSession>> = Mutex::new(None);

impl ThinkingSession {
    pub fn new() -> Self {
        Self {
            thoughts: Vec::new(),
            graph: DiGraph::new(),
            number_to_node: HashMap::new(),
            branches: HashMap::new(),
        }
    }

    /// Processes a new incoming thought step in the reasoning session.
    pub fn add_thought(&mut self, node: ThoughtNode) -> Result<ThoughtResponse> {
        let thought_num = node.thought_number;
        let total_thoughts = node.total_thoughts.max(thought_num);
        let branch_name = node.branch_id.clone().unwrap_or_else(|| "main".to_string());

        let graph_node = self.graph.add_node(thought_num);
        self.number_to_node.insert(thought_num, graph_node);

        // Connect edges for sequential, branching, or revision flows
        if let Some(parent_num) = node.branch_from_thought {
            if let Some(&parent_node) = self.number_to_node.get(&parent_num) {
                self.graph.add_edge(parent_node, graph_node, ());
            }
        } else if let Some(revised_num) = node.revises_thought {
            if let Some(&revised_node) = self.number_to_node.get(&revised_num) {
                self.graph.add_edge(revised_node, graph_node, ());
            }
        } else if thought_num > 1 {
            if let Some(&prev_node) = self.number_to_node.get(&(thought_num - 1)) {
                self.graph.add_edge(prev_node, graph_node, ());
            }
        }

        self.branches
            .entry(branch_name.clone())
            .or_default()
            .push(thought_num);

        let is_complete = !node.needs_more_thoughts || thought_num >= total_thoughts;
        self.thoughts.push(node);

        let total_branches = self.branches.len();
        let summary = self.format_summary();

        Ok(ThoughtResponse {
            thought_number: thought_num,
            total_thoughts,
            current_branch: branch_name,
            total_branches,
            is_complete,
            summary_so_far: summary,
        })
    }

    /// Resets the global active thinking session.
    #[allow(dead_code)]
    pub fn reset_session() {
        if let Ok(mut guard) = ACTIVE_THINKING_SESSION.lock() {
            *guard = Some(ThinkingSession::new());
        }
    }

    /// Dispatches a thought into the active singleton session.
    pub fn step(node: ThoughtNode) -> Result<String> {
        let mut guard = ACTIVE_THINKING_SESSION
            .lock()
            .map_err(|e| ToolError::CommandExec(format!("Lock poisoned: {}", e)))?;

        let session = guard.get_or_insert_with(ThinkingSession::new);
        let resp = session.add_thought(node)?;

        let mut out = format!(
            "🧠 Thought Step {}/{} [Branch: '{}', Total Branches: {}]\n",
            resp.thought_number, resp.total_thoughts, resp.current_branch, resp.total_branches
        );

        if resp.is_complete {
            out.push_str("🎯 Reasoning Completed. Final synthesized plan:\n\n");
        } else {
            out.push_str("⏳ Reasoning in progress...\n\n");
        }

        out.push_str(&resp.summary_so_far);
        Ok(out)
    }

    /// Formats the current reasoning trajectory into a clean Markdown outline.
    pub fn format_summary(&self) -> String {
        let mut out = String::new();
        for t in &self.thoughts {
            let prefix = if t.is_revision {
                format!("🔄 [Revision of #{:?}]", t.revises_thought.unwrap_or(0))
            } else if let Some(branch) = &t.branch_id {
                format!("🔀 [Branch '{}']", branch)
            } else {
                "•".to_string()
            };

            let score_badge = match t.score {
                Some(s) => format!(" (confidence: {:.0}%)", s * 100.0),
                None => String::new(),
            };

            out.push_str(&format!(
                "{} Thought #{}{}:\n   {}\n\n",
                prefix, t.thought_number, score_badge, t.thought
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thinking_session_linear_and_branching() {
        let mut session = ThinkingSession::new();

        // 1. Initial thought
        let t1 = session
            .add_thought(ThoughtNode {
                thought_number: 1,
                total_thoughts: 3,
                thought: "Analyze memory leak in tokio broadcast channel".to_string(),
                is_revision: false,
                revises_thought: None,
                branch_from_thought: None,
                branch_id: None,
                needs_more_thoughts: true,
                score: Some(0.8),
            })
            .unwrap();

        assert_eq!(t1.thought_number, 1);
        assert!(!t1.is_complete);

        // 2. Branch alternative hypothesis
        let t2 = session
            .add_thought(ThoughtNode {
                thought_number: 2,
                total_thoughts: 3,
                thought: "Hypothesis A: Slow consumers without lagging handling".to_string(),
                is_revision: false,
                revises_thought: None,
                branch_from_thought: Some(1),
                branch_id: Some("hypothesis_lagging".to_string()),
                needs_more_thoughts: true,
                score: Some(0.9),
            })
            .unwrap();

        assert_eq!(t2.total_branches, 2);
        assert_eq!(t2.current_branch, "hypothesis_lagging");

        // 3. Finalizing thought
        let t3 = session
            .add_thought(ThoughtNode {
                thought_number: 3,
                total_thoughts: 3,
                thought: "Solution: Add RecvError::Lagged handling with ring buffer clamp"
                    .to_string(),
                is_revision: false,
                revises_thought: None,
                branch_from_thought: Some(2),
                branch_id: Some("hypothesis_lagging".to_string()),
                needs_more_thoughts: false,
                score: Some(0.98),
            })
            .unwrap();

        assert!(t3.is_complete);
    }
}
