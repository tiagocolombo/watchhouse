//! Versioned, strict protocol DTOs. V1 rejects unsupported versions.
pub use crate::domain::qualification::*;
pub use crate::domain::team::*;
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
    #[serde(rename = "repair.candidate_changed")]
    CandidateChanged {
        candidate_id: String,
        kind: String,
        reason: String,
    },
    #[serde(rename = "execution.container_changed")]
    ContainerJobChanged {
        job_id: String,
        kind: String,
        state: String,
        reason: String,
    },
    #[serde(rename = "learning.changed")]
    LearningChanged {
        id: String,
        revision: u32,
        kind: String,
        reason: String,
    },
    #[serde(rename = "incident.recovery_recorded")]
    RecoveryRecorded {
        record_id: String,
        kind: String,
        reason: String,
    },
    #[serde(rename = "adapter.trial_changed")]
    AdapterTrialChanged {
        trial_id: String,
        sequence: i64,
        state: String,
        reason: String,
    },
    #[serde(rename = "worker.qualification_reconciled")]
    WorkerQualificationReconciled {
        qualification_id: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "worker.qualification_changed")]
    WorkerQualificationChanged {
        qualification: Box<crate::worker_qualification::Qualification>,
        reason: String,
    },
    #[serde(rename = "worker.qualification_late")]
    WorkerQualificationLate {
        qualification_id: String,
        digest: String,
        reason: String,
    },
    #[serde(rename = "repair.requested")]
    RepairRequested {
        request: Box<crate::repair::RepairRequest>,
        reason: String,
    },
    #[serde(rename = "case.correlated")]
    CaseCorrelated {
        relation: Box<crate::correlation::Relation>,
        reason: String,
    },
    #[serde(rename = "incident.artifact_recorded")]
    IncidentArtifactRecorded {
        artifact: Box<crate::incident_artifacts::Artifact>,
        reason: String,
    },
    #[serde(rename = "incident.communication_approved")]
    CommunicationApproved {
        artifact_id: String,
        revision: u32,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "incident.updated")]
    IncidentUpdated {
        operations: Box<crate::incident_ops::IncidentOperations>,
        reason: String,
    },
    #[serde(rename = "incident.role_changed")]
    IncidentRoleChanged {
        offer_id: String,
        state: String,
        reason: String,
    },
    #[serde(rename = "source.observed")]
    SourceObserved {
        evidence_refs: Vec<String>,
        source_key: String,
        authoritative: bool,
        reason: String,
    },
    #[serde(rename = "incident.declared")]
    IncidentDeclared {
        incident: Box<crate::incidents::Incident>,
        reason: String,
    },
    #[serde(rename = "escalation.delivery_changed")]
    DeliveryChanged {
        delivery_id: String,
        state: String,
        recipient: String,
        reason: String,
    },
    #[serde(rename = "escalation.resolved")]
    EscalationResolved {
        escalation_id: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "challenge.changed")]
    ChallengeChanged {
        challenge: Box<crate::challenges::Challenge>,
        reason: String,
    },
    #[serde(rename = "case.gate_recorded")]
    GateRecorded {
        gate: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
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
    #[serde(rename = "evidence.recorded")]
    EvidenceRecorded {
        evidence_refs: Vec<String>,
        provenance: String,
        scope: String,
        reason: String,
    },
    #[serde(rename = "disposition.recorded")]
    DispositionRecorded {
        disposition: Box<Disposition>,
        supersedes: Option<String>,
        reason: String,
    },
    #[serde(rename = "obligation.scheduled")]
    ObligationScheduled {
        obligation: Obligation,
        reason: String,
    },
    #[serde(rename = "disposition.reconsidered")]
    DispositionReconsidered {
        disposition_id: String,
        trigger: ReconsiderTrigger,
        occurrence_count: u32,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "case.reopened")]
    CaseReopened {
        disposition_id: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "obligation.due")]
    ObligationDue {
        obligation_id: String,
        owner: String,
        reason: String,
    },
    #[serde(rename = "team.message_posted")]
    MessagePosted {
        message: Box<Message>,
        reason: String,
    },
    #[serde(rename = "team.message_acknowledged")]
    MessageAcknowledged {
        message_id: String,
        recipient: String,
        reason: String,
    },
    #[serde(rename = "run.heartbeat")]
    RunHeartbeat { run_id: String, reason: String },
    #[serde(rename = "run.progressed")]
    RunProgressed {
        run_id: String,
        evidence_refs: Vec<String>,
        reason: String,
    },
    #[serde(rename = "assignment.blocked")]
    AssignmentBlocked { block: Box<Block>, reason: String },
    #[serde(rename = "handoff.proposed")]
    HandoffProposed {
        handoff: Box<Handoff>,
        reason: String,
    },
    #[serde(rename = "handoff.accepted")]
    HandoffAccepted {
        handoff_id: String,
        run_id: String,
        owner: String,
        reason: String,
    },
    #[serde(rename = "handoff.expired")]
    HandoffExpired { handoff_id: String, reason: String },
    #[serde(rename = "escalation.opened")]
    EscalationOpened {
        escalation: Box<Escalation>,
        reason: String,
    },
    #[serde(rename = "escalation.acknowledged")]
    EscalationAcknowledged {
        escalation_id: String,
        reason: String,
    },
    #[serde(rename = "run.late_result_recorded")]
    LateResultRecorded {
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
    #[serde(default)]
    pub qualification: QualificationSnapshot,
    #[serde(default)]
    pub team: TeamSnapshot,
    #[serde(default)]
    pub challenges: Vec<crate::challenges::Challenge>,
    #[serde(default)]
    pub gate: Option<crate::challenges::Gate>,
    #[serde(default)]
    pub policy: Option<crate::policy::Policy>,
    #[serde(default)]
    pub notifications: Vec<crate::notifications::Delivery>,
    #[serde(default)]
    pub source: Option<crate::sentry::SourceObservation>,
    #[serde(default)]
    pub source_freshness: Option<crate::sentry_refresh::SourceFreshness>,
    #[serde(default)]
    pub incident: Option<crate::incidents::Incident>,
    #[serde(default)]
    pub incident_operations: Option<crate::incident_ops::IncidentOperations>,
    #[serde(default)]
    pub incident_role_offers: Vec<crate::incident_ops::RoleOffer>,
    #[serde(default)]
    pub incident_artifacts: Vec<crate::incident_artifacts::Artifact>,
    #[serde(default)]
    pub correlations: crate::correlation::Correlations,
    #[serde(default)]
    pub repairs: Vec<crate::repair::RepairRequest>,
    #[serde(default)]
    pub worker_qualifications: Vec<crate::worker_qualification::Qualification>,
    #[serde(default)]
    pub adapter_trials: Vec<crate::adapter_trials::Trial>,
    #[serde(default)]
    pub recovery: crate::recovery::RecoverySnapshot,
    #[serde(default)]
    pub learning: crate::learning::LearningSnapshot,
    #[serde(default)]
    pub container_jobs: Vec<crate::real_execution::worker::ContainerJob>,
    #[serde(default)]
    pub repair_progress: crate::real_execution::pipeline::RepairProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvidence {
    pub case: Case,
    pub evidence: EvidenceReference,
}
