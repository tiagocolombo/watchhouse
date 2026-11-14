//! Versioned, strict protocol DTOs. V1 rejects unsupported versions.
use crate::domain::{Assignment, Case};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V1;
impl Serialize for V1 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(1)
    }
}
impl<'de> Deserialize<'de> for V1 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if u8::deserialize(d)? == 1 {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("unsupported schema_version"))
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCase {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
    pub title: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenameCase {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
    pub expected_version: i64,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub kind: String,
    pub id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", deny_unknown_fields)]
pub enum EventData {
    #[serde(rename = "case.created")]
    CaseCreated {
        title: String,
        reason: String,
        owner: String,
    },
    #[serde(rename = "assignment.queued")]
    AssignmentQueued {
        assignment_id: String,
        role: String,
        reason: String,
    },
    #[serde(rename = "run.started")]
    RunStarted {
        assignment_id: String,
        run_id: String,
        attempt: i64,
        reason: String,
    },
    #[serde(rename = "run.lease_expired")]
    RunLeaseExpired { run_id: String, reason: String },
    #[serde(rename = "assignment.completed")]
    AssignmentCompleted {
        assignment_id: String,
        run_id: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "case.renamed")]
    CaseRenamed { title: String, reason: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub cursor: i64,
    pub event_id: String,
    pub schema_version: V1,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub recorded_at: String,
    pub occurred_at: Option<String>,
    pub actor: Actor,
    pub correlation_id: String,
    pub causation_id: String,
    pub policy_version: String,
    #[serde(flatten)]
    pub data: EventData,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResult {
    pub schema_version: V1,
    pub assignment_id: String,
    pub summary: String,
    pub conclusion: String,
    pub actual_model: Option<String>,
    pub usage: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: V1,
    pub case: Case,
    pub assignment: Assignment,
    pub events: Vec<Event>,
    pub cursor: i64,
    pub results: Vec<ExecutionResult>,
}
