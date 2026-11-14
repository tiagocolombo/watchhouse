//! Qualification is an operator decision, distinct from execution or recovery.
use crate::contracts::V1;
use serde::{Deserialize, Serialize};

pub const MAX_REVIEW_SECONDS: u32 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum QualificationPlan {
    ExpectedBehavior {
        after_seconds: u32,
    },
    ApparentlyTransient {
        after_seconds: u32,
        recovery_indicators: String,
    },
    InsufficientEvidence {
        after_seconds: u32,
        collection_task: String,
    },
}
impl QualificationPlan {
    pub fn after_seconds(&self) -> u32 {
        match self {
            Self::ExpectedBehavior { after_seconds }
            | Self::ApparentlyTransient { after_seconds, .. }
            | Self::InsufficientEvidence { after_seconds, .. } => *after_seconds,
        }
    }
    pub fn review_task(&self) -> String {
        match self {
            Self::ExpectedBehavior { .. } => "Review whether the expected-behavior decision still applies to its scope and evidence".into(),
            Self::ApparentlyTransient { recovery_indicators, .. } => format!("Observe {recovery_indicators}; reassess the transient hypothesis. Missing traffic or observations cannot prove recovery"),
            Self::InsufficientEvidence { collection_task, .. } => format!("Collect evidence: {collection_task}; reassess the case"),
        }
    }
    pub fn needs_reported_evidence(&self) -> bool {
        !matches!(self, Self::InsufficientEvidence { .. })
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispositionState {
    Active,
    Reopened,
    Superseded,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObligationState {
    Pending,
    Due,
    Superseded,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconsiderTrigger {
    OperatorReview,
    Recurrence,
    ContradictoryEvidence,
    ScopeChanged,
}
impl ReconsiderTrigger {
    pub fn reopens(self, count: u32, threshold: u32) -> bool {
        self != Self::Recurrence || count >= threshold
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDisposition {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
    pub expected_version: i64,
    pub owner: String,
    pub scope: String,
    pub uncertainty: String,
    pub evidence_refs: Vec<String>,
    pub reconsideration_conditions: String,
    pub reopen_after_occurrences: u32,
    pub plan: QualificationPlan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconsiderCase {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
    pub expected_version: i64,
    pub disposition_id: String,
    pub trigger: ReconsiderTrigger,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordEvidence {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
    pub expected_version: i64,
    pub scope: String,
    pub statement: String,
    pub collection_method: String,
    pub observed_at: Option<String>,
    pub uncertainty: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceReference {
    pub digest: String,
    pub provenance: String,
    pub recorded_at: String,
    #[serde(default)]
    pub summary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Disposition {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub recorded_at: String,
    pub due_at: String,
    pub state: DispositionState,
    pub occurrence_count: u32,
    pub record: RecordDisposition,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub schema_version: V1,
    pub id: String,
    pub disposition_id: String,
    pub owner: String,
    pub task: String,
    pub due_at: String,
    pub state: ObligationState,
    pub triggered_at: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualificationSnapshot {
    pub dispositions: Vec<Disposition>,
    pub obligations: Vec<Obligation>,
    pub evidence: Vec<EvidenceReference>,
}
