//! Divergence detection and conflict resolution for agent council

use crate::agent::council::types::{AgentPerspective, MemberId};
use std::collections::HashMap;

/// Types of conflicts that can arise between perspectives
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::enum_variant_names)]
pub enum ConflictKind {
    /// Direct stance disagreement
    StanceConflict,
    /// Different priority ordering of concerns
    PriorityConflict,
    /// Opposing evidence for the same claim
    EvidenceConflict,
    /// Risk assessment disagreement
    RiskConflict,
    /// Implementation approach disagreement
    ApproachConflict,
}

impl ConflictKind {
    pub fn label(&self) -> &'static str {
        match self {
            ConflictKind::StanceConflict => "Stance Conflict",
            ConflictKind::PriorityConflict => "Priority Conflict",
            ConflictKind::EvidenceConflict => "Evidence Conflict",
            ConflictKind::RiskConflict => "Risk Conflict",
            ConflictKind::ApproachConflict => "Approach Conflict",
        }
    }

    pub fn severity_weight(&self) -> f64 {
        match self {
            ConflictKind::StanceConflict => 1.0,
            ConflictKind::PriorityConflict => 0.7,
            ConflictKind::EvidenceConflict => 0.8,
            ConflictKind::RiskConflict => 0.9,
            ConflictKind::ApproachConflict => 0.6,
        }
    }
}

/// A detected conflict between perspectives
#[derive(Debug, Clone)]
pub struct DetectedConflict {
    pub kind: ConflictKind,
    pub involved_members: Vec<MemberId>,
    pub description: String,
    pub severity: f64,
    pub resolution_hints: Vec<String>,
}

impl DetectedConflict {
    pub fn new(kind: ConflictKind, members: Vec<MemberId>, description: &str) -> Self {
        let severity = kind.severity_weight();
        Self {
            kind,
            involved_members: members,
            description: description.to_string(),
            severity,
            resolution_hints: Vec::new(),
        }
    }

    pub fn with_hints(mut self, hints: Vec<&str>) -> Self {
        self.resolution_hints = hints.into_iter().map(|s| s.to_string()).collect();
        self
    }
}

/// Report of all detected divergences in the council
#[derive(Debug, Clone)]
pub struct DivergenceReport {
    pub topic_id: String,
    pub conflicts: Vec<DetectedConflict>,
    pub overall_divergence_score: f64,
    pub can_proceed: bool,
    pub resolution_strategy: Option<String>,
}

impl Default for DivergenceReport {
    fn default() -> Self {
        Self {
            topic_id: String::new(),
            conflicts: Vec::new(),
            overall_divergence_score: 0.0,
            can_proceed: true,
            resolution_strategy: None,
        }
    }
}

impl DivergenceReport {
    pub fn new(topic_id: &str) -> Self {
        Self {
            topic_id: topic_id.to_string(),
            ..Default::default()
        }
    }

    pub fn high_conflict_threshold(&self) -> f64 {
        0.6
    }

    pub fn is_high_conflict(&self) -> bool {
        self.overall_divergence_score >= self.high_conflict_threshold()
    }
}

/// Analyzer for detecting divergence between agent perspectives
pub struct DivergenceAnalyzer {
    conflict_sensitivity: f64,
}

impl Default for DivergenceAnalyzer {
    fn default() -> Self {
        Self {
            conflict_sensitivity: 0.5,
        }
    }
}

impl DivergenceAnalyzer {
    pub fn new(sensitivity: f64) -> Self {
        Self {
            conflict_sensitivity: sensitivity.clamp(0.0, 1.0),
        }
    }

    /// Detect all conflicts between perspectives
    pub fn detect_conflicts(&self, perspectives: &[AgentPerspective]) -> Vec<DetectedConflict> {
        let mut conflicts = Vec::new();

        // Stance conflicts
        conflicts.extend(self::detect_stance_conflicts(perspectives));

        // Priority conflicts (based on concern ordering)
        conflicts.extend(self::detect_priority_conflicts(perspectives));

        // Risk conflicts
        conflicts.extend(self::detect_risk_conflicts(perspectives));

        // Approach conflicts
        conflicts.extend(self::detect_approach_conflicts(perspectives));

        // Deduplicate overlapping conflicts
        self::deduplicate_conflicts(conflicts)
    }

    /// Analyze divergence and generate a report
    pub fn analyze(&self, perspectives: &[AgentPerspective], topic_id: &str) -> DivergenceReport {
        let mut report = DivergenceReport::new(topic_id);
        report.conflicts = self.detect_conflicts(perspectives);

        // Calculate overall divergence score
        let total_severity: f64 = report.conflicts.iter().map(|c| c.severity).sum();
        let max_possible = perspectives.len() as f64 * self.conflict_sensitivity;
        report.overall_divergence_score = if max_possible > 0.0 {
            (total_severity / max_possible).min(1.0)
        } else {
            0.0
        };

        // Determine if council can proceed
        report.can_proceed = report.overall_divergence_score < 0.8;

        // Suggest resolution strategy
        report.resolution_strategy = self::suggest_resolution(&report.conflicts);

        report
    }
}

fn detect_stance_conflicts(perspectives: &[AgentPerspective]) -> Vec<DetectedConflict> {
    let mut conflicts = Vec::new();
    let mut stance_groups: HashMap<String, Vec<MemberId>> = HashMap::new();

    for p in perspectives {
        if let Some(ref stance) = p.stance {
            stance_groups
                .entry(stance.clone())
                .or_default()
                .push(p.member_id.clone());
        }
    }

    let stances: Vec<_> = stance_groups.keys().collect();
    if stances.len() > 1 {
        let all_members: Vec<_> = perspectives.iter().map(|p| p.member_id.clone()).collect();
        conflicts.push(
            DetectedConflict::new(
                ConflictKind::StanceConflict,
                all_members,
                &format!(
                    "Members disagree on stance: {}",
                    stances
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" vs ")
                ),
            )
            .with_hints(vec![
                "Seek common ground between stances",
                "Identify underlying shared values",
                "Consider compromise positions",
            ]),
        );
    }

    conflicts
}

fn detect_priority_conflicts(perspectives: &[AgentPerspective]) -> Vec<DetectedConflict> {
    let mut conflicts = Vec::new();

    // Check if concern ordering differs significantly
    let concern_counts: Vec<_> = perspectives.iter().map(|p| p.concerns.len()).collect();

    if concern_counts.windows(2).any(|w| w[0] != w[1]) {
        let members: Vec<_> = perspectives.iter().map(|p| p.member_id.clone()).collect();
        conflicts.push(
            DetectedConflict::new(
                ConflictKind::PriorityConflict,
                members,
                "Members have different numbers of prioritized concerns",
            )
            .with_hints(vec![
                "List all concerns and rank jointly",
                "Identify shared top priorities",
            ]),
        );
    }

    conflicts
}

fn detect_risk_conflicts(perspectives: &[AgentPerspective]) -> Vec<DetectedConflict> {
    let mut conflicts = Vec::new();

    // Check confidence variance as proxy for risk assessment
    let confidences: Vec<_> = perspectives.iter().map(|p| p.confidence).collect();

    if confidences.len() >= 2 {
        let max_conf = confidences
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let min_conf = confidences.iter().cloned().fold(f64::INFINITY, f64::min);
        let variance = max_conf - min_conf;

        if variance > 0.4 {
            let members: Vec<_> = perspectives.iter().map(|p| p.member_id.clone()).collect();
            conflicts.push(
                DetectedConflict::new(
                    ConflictKind::RiskConflict,
                    members,
                    &format!(
                        "High confidence variance ({:.1}) suggests risk disagreement",
                        variance
                    ),
                )
                .with_hints(vec![
                    "Explicitly discuss risk tolerance",
                    "Identify what each member sees as risks",
                ]),
            );
        }
    }

    conflicts
}

fn detect_approach_conflicts(perspectives: &[AgentPerspective]) -> Vec<DetectedConflict> {
    let mut conflicts = Vec::new();

    // Check for conflicting counter-arguments
    let all_counters: Vec<_> = perspectives
        .iter()
        .flat_map(|p| p.counter_arguments.clone())
        .collect();

    if all_counters.len() > perspectives.len() * 2 {
        let members: Vec<_> = perspectives.iter().map(|p| p.member_id.clone()).collect();
        conflicts.push(
            DetectedConflict::new(
                ConflictKind::ApproachConflict,
                members,
                &format!(
                    "Multiple counter-arguments suggest approach disagreement ({} total)",
                    all_counters.len()
                ),
            )
            .with_hints(vec![
                "List each counter-argument explicitly",
                "Find which concerns are addressable",
                "Consider phased implementation",
            ]),
        );
    }

    conflicts
}

fn deduplicate_conflicts(mut conflicts: Vec<DetectedConflict>) -> Vec<DetectedConflict> {
    conflicts.sort_by_key(|c| c.kind.clone());
    conflicts.dedup_by(|a, b| a.kind == b.kind && a.description == b.description);
    conflicts
}

fn suggest_resolution(conflicts: &[DetectedConflict]) -> Option<String> {
    if conflicts.is_empty() {
        return Some("Consensus achieved - no resolution needed".to_string());
    }

    let has_stance = conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::StanceConflict);
    let has_risk = conflicts
        .iter()
        .any(|c| c.kind == ConflictKind::RiskConflict);

    if has_stance && has_risk {
        Some(
            "Complex multi-dimensional conflict - recommend breaking into sub-decisions"
                .to_string(),
        )
    } else if has_stance {
        Some("Stance conflict - recommend deliberation round focusing on shared values".to_string())
    } else if has_risk {
        Some("Risk conflict - recommend explicit risk assessment discussion".to_string())
    } else {
        Some("Approach-level conflict - recommend compromise or phased implementation".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stance_conflict_detection() {
        let analyzer = DivergenceAnalyzer::default();
        let perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1").with_stance("Approach A", 0.9),
            AgentPerspective::new(MemberId::new("b"), "t1").with_stance("Approach B", 0.8),
        ];

        let conflicts = analyzer.detect_conflicts(&perspectives);
        assert!(conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::StanceConflict));
    }

    #[test]
    fn test_divergence_report() {
        let analyzer = DivergenceAnalyzer::default();
        let perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1")
                .with_stance("A", 0.9)
                .with_confidence(0.9),
            AgentPerspective::new(MemberId::new("b"), "t1")
                .with_stance("B", 0.3)
                .with_confidence(0.3),
        ];

        let report = analyzer.analyze(&perspectives, "topic1");
        assert!(report.is_high_conflict());
        assert!(!report.can_proceed);
    }

    #[test]
    fn test_no_conflict_homogeneous() {
        let analyzer = DivergenceAnalyzer::default();
        let perspectives = vec![
            AgentPerspective::new(MemberId::new("a"), "t1")
                .with_stance("A", 0.9)
                .with_confidence(0.85),
            AgentPerspective::new(MemberId::new("b"), "t1")
                .with_stance("A", 0.85)
                .with_confidence(0.9),
        ];

        let report = analyzer.analyze(&perspectives, "topic1");
        assert!(!report.is_high_conflict());
        assert!(report.can_proceed);
    }
}
