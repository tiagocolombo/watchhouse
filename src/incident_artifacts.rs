//! Durable investigation and draft review records. Plans and text never execute actions.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    incident_ops::{self, IncidentOperations},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finding {
    Proposed,
    Testing,
    Supported,
    Refuted,
    Inconclusive,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResult {
    pub outcome: Finding,
    pub evidence_refs: Vec<String>,
    pub interpretation: String,
    pub uncertainty: String,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArtifactData {
    Hypothesis {
        claim: String,
        scope: String,
        state: Finding,
        supporting: Vec<String>,
        counterevidence: Vec<String>,
        uncertainty: String,
        next_experiment: String,
        experiment_id: Option<String>,
    },
    Experiment {
        hypothesis_id: String,
        hypothesis_revision: u32,
        question: String,
        method: String,
        environment: String,
        expected_outcomes: String,
        authorization_scope: String,
        risk: String,
        due_at: String,
        result: Option<ExperimentResult>,
    },
    Communication {
        audience: String,
        body: String,
        evidence_refs: Vec<String>,
        review_due_at: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub reviewer: String,
    pub reviewed_case_version: i64,
    pub incident_revision: u64,
    pub source_digest: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub schema_version: V1,
    pub id: String,
    pub revision: u32,
    pub case_id: String,
    pub author: String,
    pub owner: String,
    pub recorded_at: String,
    pub incident_revision: u64,
    pub source_digest: String,
    pub data: ArtifactData,
    #[serde(default)]
    pub approval: Option<Approval>,
    #[serde(default)]
    pub approval_current: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCommand {
    pub command: OperatorCommand,
    pub action: ArtifactAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactAction {
    Hypothesis {
        id: Option<String>,
        expected_revision: Option<u32>,
        claim: String,
        scope: String,
        state: Finding,
        supporting: Vec<String>,
        counterevidence: Vec<String>,
        uncertainty: String,
        next_experiment: String,
        experiment_id: Option<String>,
    },
    PlanExperiment {
        hypothesis_id: String,
        hypothesis_revision: u32,
        question: String,
        method: String,
        environment: String,
        expected_outcomes: String,
        authorization_scope: String,
        risk: String,
        due_seconds: u32,
    },
    FinishExperiment {
        id: String,
        expected_revision: u32,
        outcome: Finding,
        evidence_refs: Vec<String>,
        interpretation: String,
        uncertainty: String,
    },
    DraftCommunication {
        id: Option<String>,
        expected_revision: Option<u32>,
        audience: String,
        body: String,
        evidence_refs: Vec<String>,
        review_seconds: u32,
    },
    ApproveCommunication {
        id: String,
        revision: u32,
        evidence_refs: Vec<String>,
    },
}
fn source_digest(db: &Connection, id: &str) -> Result<String> {
    Ok(db.query_row(
        "SELECT digest FROM source_cases WHERE case_id=?1",
        [id],
        |r| r.get(0),
    )?)
}
fn record(db: &Connection, case: &str, id: &str) -> Result<Artifact> {
    let raw:Option<String>=db.query_row("SELECT record FROM incident_artifacts WHERE id=?1 AND case_id=?2 ORDER BY revision DESC LIMIT 1",params![id,case],|r|r.get(0)).optional()?;
    let mut a: Artifact = serde_json::from_str(&raw.ok_or(Error::NotFound)?)?;
    let approval: Option<String> = db
        .query_row(
            "SELECT record FROM communication_approvals WHERE id=?1 AND revision=?2",
            params![a.id, a.revision],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = approval {
        let p: Approval = serde_json::from_str(&raw)?;
        let ops = incident_ops::snapshot(db, case)?.ok_or(Error::NotFound)?;
        a.approval_current = ops.revision == p.incident_revision
            && source_digest(db, case)? == p.source_digest
            && store::case(db, case)?.version == p.reviewed_case_version + 1;
        a.approval = Some(p)
    }
    Ok(a)
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<Artifact>> {
    let mut stmt=db.prepare("SELECT id FROM incident_artifacts WHERE case_id=?1 GROUP BY id ORDER BY MAX(rowid) DESC LIMIT 200")?;
    let ids = stmt
        .query_map([case], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ids.iter().map(|id| record(db, case, id)).collect()
}
fn previous(
    db: &Connection,
    case: &str,
    id: &Option<String>,
    revision: Option<u32>,
) -> Result<Option<Artifact>> {
    match id {
        None if revision.is_none() => Ok(None),
        Some(id) => {
            let a = record(db, case, id)?;
            if Some(a.revision) != revision {
                return Err(Error::Conflict("stale artifact revision".into()));
            }
            Ok(Some(a))
        }
        _ => Err(Error::Invalid(
            "new artifacts cannot have an expected revision".into(),
        )),
    }
}
fn researcher(ops: &IncidentOperations, actor: &str) -> Result<()> {
    if actor != ops.roles.commander && actor != ops.roles.operations {
        return Err(Error::Invalid(
            "current commander or operations owner required".into(),
        ));
    }
    Ok(())
}
fn optional_refs(tx: &Transaction<'_>, case: &str, refs: &[String]) -> Result<()> {
    if !refs.is_empty() {
        crate::qualification::evidence_refs(tx, case, refs, false)?
    }
    Ok(())
}
fn interval(a: &Application, tx: &Transaction<'_>, case: &str, seconds: u32) -> Result<String> {
    if seconds == 0 || seconds > crate::policy::for_case(tx, case)?.max_review_seconds {
        return Err(Error::Invalid(
            "artifact deadline exceeds governing policy".into(),
        ));
    }
    Ok((a.clock.now() + Duration::seconds(seconds.into()))
        .to_rfc3339_opts(SecondsFormat::Millis, true))
}
fn obligation(tx: &Transaction<'_>, a: &Artifact, due: &str, owner: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO artifact_obligations VALUES (?1,?2,?3,?4,'pending')",
        params![format!("{}:{}", a.id, a.revision), a.case_id, owner, due],
    )?;
    Ok(())
}
fn finish_duty(tx: &Transaction<'_>, a: &Artifact, state: &str, now: &str) -> Result<()> {
    let id = format!("{}:{}", a.id, a.revision);
    let due:Option<(String,String)>=tx.query_row("SELECT due_at,owner FROM artifact_obligations WHERE id=?1 AND state IN ('pending','due')",[&id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((due, owner)) = due {
        if due.as_str() <= now {
            crate::team::escalate(
                tx,
                &a.case_id,
                "incident_artifact",
                &id,
                &format!("Artifact response overdue; {owner} owns explicit follow-up"),
                now,
            )?;
        }
        tx.execute(
            "UPDATE artifact_obligations SET state=?1 WHERE id=?2",
            params![state, id],
        )?;
    }
    Ok(())
}
pub(crate) fn due(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut stmt=tx.prepare("SELECT id,case_id,owner FROM artifact_obligations WHERE state='pending' AND due_at<=?1 ORDER BY due_at LIMIT 100")?;
    let rows = stmt
        .query_map([now], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (id, case, owner) in &rows {
        crate::team::escalate(
            tx,
            case,
            "incident_artifact",
            id,
            &format!("Artifact evidence/review overdue; {owner} remains accountable"),
            now,
        )?;
        tx.execute(
            "UPDATE artifact_obligations SET state='due' WHERE id=?1",
            [id],
        )?;
    }
    Ok(rows.len())
}
pub(crate) fn ensure_no_open(db: &Connection, case: &str) -> Result<()> {
    let open:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM artifact_obligations WHERE case_id=?1 AND state IN ('pending','due'))",[case],|r|r.get(0))?;
    if open {
        return Err(Error::Conflict(
            "pending or overdue investigation/review duty prevents assessment closure".into(),
        ));
    }
    Ok(())
}
impl Application {
    pub fn record_incident_artifact(&self, case: &str, cmd: ArtifactCommand) -> Result<Artifact> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        incident_ops::human(&m.actor)?;
        let fp = store::digest(&format!(
            "incident-artifact:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let current = store::case(&tx, case)?;
        if current.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "stale case version; reload before retrying".into(),
            ));
        }
        let ops = incident_ops::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
        let now = self.now();
        let digest = source_digest(&tx, case)?;
        let mut a = Artifact {
            schema_version: V1,
            id: store::id("art"),
            revision: 1,
            case_id: case.into(),
            author: m.actor.clone(),
            owner: m.actor.clone(),
            recorded_at: now.clone(),
            incident_revision: ops.revision,
            source_digest: digest.clone(),
            data: ArtifactData::Communication {
                audience: String::new(),
                body: String::new(),
                evidence_refs: vec![],
                review_due_at: now.clone(),
            },
            approval: None,
            approval_current: false,
        };
        match &cmd.action {
            ArtifactAction::Hypothesis {
                id,
                expected_revision,
                claim,
                scope,
                state,
                supporting,
                counterevidence,
                uncertainty,
                next_experiment,
                experiment_id,
            } => {
                researcher(&ops, &m.actor)?;
                for (name, value) in [
                    ("claim", claim),
                    ("scope", scope),
                    ("uncertainty", uncertainty),
                    ("next experiment", next_experiment),
                ] {
                    text(value, name, 2000)?
                }
                optional_refs(&tx, case, supporting)?;
                optional_refs(&tx, case, counterevidence)?;
                let old = previous(&tx, case, id, *expected_revision)?;
                if let Some(old) = &old {
                    if !matches!(old.data, ArtifactData::Hypothesis { .. }) {
                        return Err(Error::Invalid("artifact is not a hypothesis".into()));
                    }
                    a.id = old.id.clone();
                    a.revision = old.revision + 1;
                    a.owner = old.owner.clone();
                }
                if matches!(state, Finding::Supported | Finding::Refuted) {
                    if !old.as_ref().is_some_and(|old|matches!(&old.data,ArtifactData::Hypothesis{claim:old_claim,scope:old_scope,..} if old_claim==claim && old_scope==scope)){return Err(Error::Conflict("changed claims need a new discriminating experiment".into()))}
                    let exp = record(
                        &tx,
                        case,
                        experiment_id.as_deref().ok_or_else(|| {
                            Error::Invalid("discriminating experiment required".into())
                        })?,
                    )?;
                    match exp.data {
                        ArtifactData::Experiment {
                            hypothesis_id,
                            hypothesis_revision,
                            result: Some(result),
                            ..
                        } if hypothesis_id == a.id
                            && Some(hypothesis_revision) == old.as_ref().map(|v| v.revision)
                            && result.outcome == *state =>
                        {
                            let refs = if *state == Finding::Refuted {
                                counterevidence
                            } else {
                                supporting
                            };
                            if !result.evidence_refs.iter().all(|r| refs.contains(r)) {
                                return Err(Error::Invalid("experiment evidence must support the exact hypothesis interpretation".into()));
                            }
                        }
                        _ => {
                            return Err(Error::Conflict(
                                "experiment is incomplete or refers to another hypothesis revision"
                                    .into(),
                            ));
                        }
                    }
                }
                a.data = ArtifactData::Hypothesis {
                    claim: claim.clone(),
                    scope: scope.clone(),
                    state: *state,
                    supporting: supporting.clone(),
                    counterevidence: counterevidence.clone(),
                    uncertainty: uncertainty.clone(),
                    next_experiment: next_experiment.clone(),
                    experiment_id: experiment_id.clone(),
                };
            }
            ArtifactAction::PlanExperiment {
                hypothesis_id,
                hypothesis_revision,
                question,
                method,
                environment,
                expected_outcomes,
                authorization_scope,
                risk,
                due_seconds,
            } => {
                researcher(&ops, &m.actor)?;
                let h = record(&tx, case, hypothesis_id)?;
                if h.revision != *hypothesis_revision
                    || !matches!(h.data, ArtifactData::Hypothesis { .. })
                {
                    return Err(Error::Conflict(
                        "experiment must freeze the current hypothesis revision".into(),
                    ));
                }
                for (name, value) in [
                    ("question", question),
                    ("method", method),
                    ("environment", environment),
                    ("expected outcomes", expected_outcomes),
                    ("authorization scope", authorization_scope),
                    ("risk", risk),
                ] {
                    text(value, name, 2000)?
                }
                let due = interval(self, &tx, case, *due_seconds)?;
                a.data = ArtifactData::Experiment {
                    hypothesis_id: h.id,
                    hypothesis_revision: h.revision,
                    question: question.clone(),
                    method: method.clone(),
                    environment: environment.clone(),
                    expected_outcomes: expected_outcomes.clone(),
                    authorization_scope: authorization_scope.clone(),
                    risk: risk.clone(),
                    due_at: due.clone(),
                    result: None,
                };
                obligation(&tx, &a, &due, &a.owner)?;
            }
            ArtifactAction::FinishExperiment {
                id,
                expected_revision,
                outcome,
                evidence_refs,
                interpretation,
                uncertainty,
            } => {
                let old = record(&tx, case, id)?;
                if old.revision != *expected_revision {
                    return Err(Error::Conflict("stale experiment revision".into()));
                }
                if m.actor != old.owner {
                    researcher(&ops, &m.actor)?
                }
                if matches!(outcome, Finding::Proposed | Finding::Testing) {
                    return Err(Error::Invalid(
                        "experiment needs an observed outcome".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                text(interpretation, "interpretation", 4000)?;
                text(uncertainty, "uncertainty", 2000)?;
                let mut data = old.data.clone();
                if let ArtifactData::Experiment { result, .. } = &mut data {
                    if result.is_some() {
                        return Err(Error::Conflict("experiment result already recorded; retain history and plan a new experiment".into()));
                    }
                    *result = Some(ExperimentResult {
                        outcome: *outcome,
                        evidence_refs: evidence_refs.clone(),
                        interpretation: interpretation.clone(),
                        uncertainty: uncertainty.clone(),
                        recorded_at: now.clone(),
                    });
                } else {
                    return Err(Error::Invalid("artifact is not an experiment".into()));
                }
                finish_duty(&tx, &old, "complete", &now)?;
                a.id = old.id;
                a.revision = old.revision + 1;
                a.owner = old.owner;
                a.data = data;
            }
            ArtifactAction::DraftCommunication {
                id,
                expected_revision,
                audience,
                body,
                evidence_refs,
                review_seconds,
            } => {
                if m.actor != ops.roles.communications {
                    return Err(Error::Invalid(
                        "current communications owner must prepare the draft".into(),
                    ));
                }
                text(audience, "audience", 128)?;
                text(body, "draft body", 8000)?;
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                if let Some(old) = previous(&tx, case, id, *expected_revision)? {
                    if !matches!(old.data, ArtifactData::Communication { .. }) {
                        return Err(Error::Invalid(
                            "artifact is not a communication draft".into(),
                        ));
                    }
                    finish_duty(&tx, &old, "superseded", &now)?;
                    a.id = old.id;
                    a.revision = old.revision + 1;
                }
                let due = interval(self, &tx, case, *review_seconds)?;
                a.data = ArtifactData::Communication {
                    audience: audience.clone(),
                    body: body.clone(),
                    evidence_refs: evidence_refs.clone(),
                    review_due_at: due.clone(),
                };
                obligation(&tx, &a, &due, &ops.roles.commander)?;
            }
            ArtifactAction::ApproveCommunication {
                id,
                revision,
                evidence_refs,
            } => {
                researcher(&ops, &m.actor)?;
                let draft = record(&tx, case, id)?;
                if draft.revision != *revision
                    || !matches!(draft.data, ArtifactData::Communication { .. })
                {
                    return Err(Error::Conflict(
                        "draft revision changed or wrong artifact type".into(),
                    ));
                }
                if draft.author == m.actor {
                    return Err(Error::Invalid(
                        "draft author cannot approve their own message".into(),
                    ));
                }
                if draft.incident_revision != ops.revision || draft.source_digest != digest {
                    return Err(Error::Conflict(
                        "incident/source context changed; revise the draft before approval".into(),
                    ));
                }
                crate::challenges::ensure_clear(&tx, case)?;
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                if draft.approval.is_some() {
                    return Err(Error::Conflict(
                        "draft already reviewed; revise it for another review".into(),
                    ));
                }
                finish_duty(&tx, &draft, "reviewed", &now)?;
                let approval = Approval {
                    reviewer: m.actor.clone(),
                    reviewed_case_version: store::case(&tx, case)?.version,
                    incident_revision: ops.revision,
                    source_digest: digest,
                    evidence_refs: evidence_refs.clone(),
                    recorded_at: now.clone(),
                    reason: m.reason.clone(),
                };
                tx.execute(
                    "INSERT INTO communication_approvals VALUES (?1,?2,?3)",
                    params![draft.id, draft.revision, serde_json::to_string(&approval)?],
                )?;
                store::emit(
                    &tx,
                    case,
                    operator(&m.actor),
                    &m.idempotency_key,
                    &now,
                    EventData::CommunicationApproved {
                        artifact_id: draft.id.clone(),
                        revision: draft.revision,
                        evidence_refs: evidence_refs.clone(),
                        reason: m.reason.clone(),
                    },
                )?;
                let out = record(&tx, case, &draft.id)?;
                save_receipt(&tx, &m.idempotency_key, &fp, &out)?;
                tx.commit()?;
                return Ok(out);
            }
        }
        if a.revision > 1000 {
            return Err(Error::Invalid("artifact revision budget exhausted".into()));
        }
        tx.execute(
            "INSERT INTO incident_artifacts VALUES (?1,?2,?3,?4)",
            params![a.id, a.revision, case, serde_json::to_string(&a)?],
        )?;
        store::emit(
            &tx,
            case,
            operator(&m.actor),
            &m.idempotency_key,
            &now,
            EventData::IncidentArtifactRecorded {
                artifact: Box::new(a.clone()),
                reason: m.reason.clone(),
            },
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &a)?;
        tx.commit()?;
        Ok(a)
    }
    pub fn investigation_markdown(&self, case: &str) -> Result<String> {
        let db = self.store.connect()?;
        crate::incidents::snapshot(&db, case)?.ok_or(Error::NotFound)?;
        let records = snapshot(&db, case)?;
        let mut out = String::from(
            "\n## Hypothesis register\n\n| ID / revision | Claim | State | Owner | Uncertainty |\n| --- | --- | --- | --- | --- |\n",
        );
        for a in &records {
            if let ArtifactData::Hypothesis {
                claim,
                state,
                uncertainty,
                ..
            } = &a.data
            {
                out.push_str(&format!(
                    "| {} / {} | {} | {:?} | {} | {} |\n",
                    a.id,
                    a.revision,
                    crate::incidents::safe(claim),
                    state,
                    crate::incidents::safe(&a.owner),
                    crate::incidents::safe(uncertainty)
                ));
            }
        }
        out.push_str("\n## Experiments and communication drafts\n\nThese are records and unsent drafts, not execution or delivery receipts. Supported hypotheses are scoped interpretations, not universal proof.\n\n```json\n");
        out.push_str(
            &serde_json::to_string_pretty(&records)?
                .replace('`', "\\u0060")
                .replace('<', "\\u003c"),
        );
        out.push_str("\n```\n");
        Ok(out)
    }
}
