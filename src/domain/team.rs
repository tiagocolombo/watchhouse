//! Durable coordination records. Messages are data, never grants of authority.
use crate::contracts::{Actor, V1};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandMeta {
    pub schema_version: V1,
    pub idempotency_key: String,
    pub actor: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorCommand {
    pub meta: CommandMeta,
    pub expected_version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimWork {
    pub meta: CommandMeta,
    pub executor: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunProof {
    pub run_id: String,
    pub fence: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunCommand {
    pub meta: CommandMeta,
    pub proof: RunProof,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Observation,
    Question,
    Status,
    Block,
    Handoff,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostMessage {
    pub command: OperatorCommand,
    pub kind: MessageKind,
    pub body: String,
    pub recipients: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub reply_to: Option<String>,
    pub ack_within_seconds: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeMessage {
    pub command: OperatorCommand,
    pub message_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportProgress {
    pub run: RunCommand,
    pub checkpoint: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    MissingEvidence,
    MissingCapability,
    HeartbeatLost,
    ProgressDeadline,
    AssignmentDeadline,
    AttemptsExhausted,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockDetails {
    pub reason_code: BlockReason,
    pub work_attempted: String,
    pub last_useful_progress: String,
    pub checkpoint: String,
    pub assistance_needed: String,
    pub uncertainty: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportBlock {
    pub run: RunCommand,
    pub details: BlockDetails,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferHandoff {
    pub command: OperatorCommand,
    pub block_id: String,
    pub successor: String,
    pub next_owner: String,
    pub acknowledge_within_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptHandoff {
    pub command: OperatorCommand,
    pub handoff_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcknowledgeEscalation {
    pub command: OperatorCommand,
    pub escalation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Briefing {
    #[serde(default)]
    pub accepted_knowledge: Vec<crate::learning::KnowledgeReference>,
    pub schema_version: V1,
    pub objective: String,
    pub role: String,
    pub checkpoint_digest: Option<String>,
    pub evidence_refs: Vec<String>,
    pub message_refs: Vec<String>,
    pub previous_run_id: Option<String>,
    pub overall_deadline: String,
    pub remaining_attempts: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentControl {
    pub executor: String,
    pub state: String,
    pub overall_deadline: Option<String>,
    pub max_attempts: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub schema_version: V1,
    pub id: String,
    pub thread_id: String,
    pub case_id: String,
    pub sender: Actor,
    pub run_id: Option<String>,
    pub kind: MessageKind,
    pub body: String,
    pub recipients: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub reply_to: Option<String>,
    pub referenced_version: i64,
    pub recorded_at: String,
    pub acknowledge_by: Option<String>,
    #[serde(default)]
    pub acknowledged_by: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub assignment_id: String,
    pub run_id: String,
    pub owner: String,
    pub reported_at: String,
    pub response_due_at: String,
    pub state: String,
    pub escalation_path: String,
    pub checkpoint_digest: String,
    pub details: BlockDetails,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub block_id: String,
    pub from_run_id: String,
    pub previous_owner: String,
    pub successor: String,
    pub next_owner: String,
    pub state: String,
    pub acknowledge_by: String,
    pub briefing: Briefing,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Escalation {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub source_type: String,
    pub source_id: String,
    pub owner: String,
    pub reason: String,
    pub recorded_at: String,
    pub state: String,
    pub acknowledged_at: Option<String>,
    pub next_action: String,
    #[serde(default)]
    pub resolution: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatus {
    pub run_id: String,
    pub executor: String,
    pub heartbeat_at: String,
    pub progress_at: String,
    pub progress_deadline: String,
    pub checkpoint_digest: Option<String>,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LateResult {
    pub run_id: String,
    pub digest: String,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamSnapshot {
    pub thread_id: String,
    pub control: Option<AssignmentControl>,
    pub messages: Vec<Message>,
    pub blocks: Vec<Block>,
    pub handoffs: Vec<Handoff>,
    pub escalations: Vec<Escalation>,
    pub runs: Vec<RunStatus>,
    pub late_results: Vec<LateResult>,
}
