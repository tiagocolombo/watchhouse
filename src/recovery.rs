//! Evidence-backed operator recovery records. No production action or telemetry is executed.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criteria {
    pub workflow: String,
    pub min_requests: u64,
    pub min_window_seconds: u32,
    pub max_error_ppm: u32,
    pub max_p95_ms: u64,
    pub max_source_lag_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub started_at: String,
    pub ended_at: String,
    pub requests: u64,
    pub errors: u64,
    pub p95_ms: Option<u64>,
    pub coverage_complete: bool,
    pub source_lag_seconds: u32,
    pub collection_method: String,
    pub uncertainty: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Met,
    Partial,
    Ineffective,
    Inconclusive,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub author: String,
    pub recorded_at: String,
    pub review_by: String,
    pub service: String,
    pub environment: String,
    pub source_digest: String,
    pub incident_revision: u64,
    pub policy_provenance: String,
    pub criteria: Criteria,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub plan_id: String,
    pub author: String,
    pub recorded_at: String,
    pub window: Window,
    pub outcome: Outcome,
    pub basis: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryReview {
    pub id: String,
    pub observation_id: String,
    pub reviewer: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryState {
    pub revision: u64,
    pub response_resolved: bool,
    pub plan: Option<Plan>,
    pub observation: Option<Observation>,
    pub review: Option<RecoveryReview>,
    pub resolved_at: Option<String>,
    pub resolution_reason: Option<String>,
    pub context_current: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectiveAction {
    pub id: String,
    pub case_id: String,
    pub revision: u64,
    pub title: String,
    pub category: String,
    pub owner: String,
    pub verifier: String,
    pub acceptance: String,
    pub due_at: String,
    pub state: String,
    pub submitted_at: Option<String>,
    pub submission_refs: Vec<String>,
    pub verification_refs: Vec<String>,
    pub residual_risk: Option<String>,
    pub escalated: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoverySnapshot {
    pub recovery: RecoveryState,
    pub actions: Vec<CorrectiveAction>,
    pub records: Vec<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCommand {
    pub command: OperatorCommand,
    pub action: RecoveryAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryAction {
    Plan {
        criteria: Criteria,
        review_seconds: u32,
    },
    Observe {
        plan_id: String,
        window: Window,
    },
    Review {
        observation_id: String,
        evidence_refs: Vec<String>,
    },
    Resolve {
        review_id: String,
        evidence_refs: Vec<String>,
    },
    Reopen {
        evidence_refs: Vec<String>,
    },
    ProposeMitigation {
        runbook: String,
        target: String,
        steps: String,
        preconditions: String,
        stop_conditions: String,
        rollback: String,
        risk: String,
        evidence_refs: Vec<String>,
    },
    RecordDeployment {
        repository: String,
        revision: String,
        environment: String,
        observed_at: String,
        evidence_refs: Vec<String>,
    },
    OpenAction {
        title: String,
        category: String,
        owner: String,
        verifier: String,
        acceptance: String,
        due_seconds: u32,
    },
    SubmitAction {
        id: String,
        revision: u64,
        evidence_refs: Vec<String>,
    },
    VerifyAction {
        id: String,
        revision: u64,
        evidence_refs: Vec<String>,
    },
    WaiveAction {
        id: String,
        revision: u64,
        residual_risk: String,
        evidence_refs: Vec<String>,
    },
}
pub(crate) fn state(db: &Connection, id: &str) -> Result<RecoveryState> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM recovery_state WHERE case_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let mut s = raw
        .map(|v| serde_json::from_str::<RecoveryState>(&v))
        .transpose()?
        .unwrap_or_default();
    s.context_current = if let Some(p) = &s.plan {
        source_digest(db, id)? == p.source_digest
            && crate::incident_ops::snapshot(db, id)?
                .is_some_and(|o| o.revision == p.incident_revision)
    } else {
        false
    };
    Ok(s)
}
pub(crate) fn source_digest(db: &Connection, id: &str) -> Result<String> {
    Ok(db.query_row(
        "SELECT digest FROM source_cases WHERE case_id=?1",
        [id],
        |r| r.get(0),
    )?)
}
pub(crate) fn resolved(db: &Connection, id: &str) -> Result<bool> {
    let s = state(db, id)?;
    Ok(s.response_resolved && s.context_current && crate::challenges::ensure_clear(db, id).is_ok())
}
fn action(db: &Connection, case: &str, id: &str) -> Result<CorrectiveAction> {
    let raw: Option<(String, bool)> = db
        .query_row(
            "SELECT record,escalated FROM corrective_actions WHERE id=?1 AND case_id=?2",
            params![id, case],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, escalated) = raw.ok_or(Error::NotFound)?;
    let mut a: CorrectiveAction = serde_json::from_str(&raw)?;
    a.escalated = escalated;
    Ok(a)
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<RecoverySnapshot> {
    let actions = {
        let mut q = db.prepare(
            "SELECT id FROM corrective_actions WHERE case_id=?1 ORDER BY rowid DESC LIMIT 200",
        )?;
        q.query_map([id], |r| r.get::<_, String>(0))?
            .map(|r| action(db, id, &r?))
            .collect::<Result<Vec<_>>>()?
    };
    let records = {
        let mut q = db.prepare(
            "SELECT record FROM recovery_records WHERE case_id=?1 ORDER BY rowid DESC LIMIT 200",
        )?;
        q.query_map([id], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<Result<Vec<_>>>()?
    };
    Ok(RecoverySnapshot {
        recovery: state(db, id)?,
        actions,
        records,
    })
}
fn save_state(tx: &Transaction<'_>, id: &str, s: &mut RecoveryState) -> Result<()> {
    s.revision += 1;
    tx.execute("INSERT INTO recovery_state VALUES (?1,?2) ON CONFLICT(case_id) DO UPDATE SET record=excluded.record",params![id,serde_json::to_string(s)?])?;
    Ok(())
}
fn save_action(tx: &Transaction<'_>, a: &CorrectiveAction) -> Result<()> {
    tx.execute("INSERT INTO corrective_actions VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![a.id,a.case_id,a.state,a.due_at,a.escalated,serde_json::to_string(a)?])?;
    Ok(())
}
fn retain<T: Serialize>(
    tx: &Transaction<'_>,
    case: &str,
    kind: &str,
    value: &T,
    meta: &CommandMeta,
    now: &str,
) -> Result<String> {
    let id = store::id("rec");
    let raw = serde_json::to_string(
        &serde_json::json!({"schema_version":1,"id":id,"case_id":case,"kind":kind,"actor":meta.actor,"reason":meta.reason,"recorded_at":now,"origin":"operator-reported-v1","data":value}),
    )?;
    tx.execute(
        "INSERT INTO recovery_records VALUES (?1,?2,?3,?4)",
        params![id, case, kind, raw],
    )?;
    store::emit(
        tx,
        case,
        operator(&meta.actor),
        &meta.idempotency_key,
        now,
        EventData::RecoveryRecorded {
            record_id: id.clone(),
            kind: kind.into(),
            reason: meta.reason.clone(),
        },
    )?;
    Ok(id)
}
fn timestamp(raw: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| Error::Invalid("timestamp must be RFC3339".into()))
}
pub(crate) fn independent_evidence(
    tx: &Transaction<'_>,
    case: &str,
    refs: &[String],
    who: &str,
    after: &str,
    exclude: &[String],
) -> Result<()> {
    crate::qualification::evidence_refs(tx, case, refs, true)?;
    let mut own = false;
    for digest in refs {
        if exclude.contains(digest) {
            return Err(Error::Invalid(
                "independent evidence must differ from submitted evidence".into(),
            ));
        }
        let (raw, provenance, at): (String, String, String) = tx.query_row(
            "SELECT content,provenance,recorded_at FROM evidence WHERE digest=?1",
            [digest],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if provenance == "operator-reported-v1" {
            let report: RecordEvidence = serde_json::from_str(&raw)?;
            own |= report.actor == who && timestamp(&at)? >= timestamp(after)?;
        }
    }
    if !own {
        return Err(Error::Invalid(
            "review requires the reviewer's own evidence collected after submission".into(),
        ));
    }
    Ok(())
}
fn evaluate(c: &Criteria, w: &Window) -> Outcome {
    let seconds =
        (timestamp(&w.ended_at).unwrap() - timestamp(&w.started_at).unwrap()).num_seconds();
    if !w.coverage_complete
        || w.source_lag_seconds > c.max_source_lag_seconds
        || w.requests < c.min_requests
        || seconds < i64::from(c.min_window_seconds)
        || w.p95_ms.is_none()
    {
        return Outcome::Inconclusive;
    }
    let errors_ok =
        u128::from(w.errors) * 1_000_000 <= u128::from(w.requests) * u128::from(c.max_error_ppm);
    let latency_ok = w.p95_ms.unwrap() <= c.max_p95_ms;
    match (errors_ok, latency_ok) {
        (true, true) => Outcome::Met,
        (false, false) => Outcome::Ineffective,
        _ => Outcome::Partial,
    }
}
fn commander(actor: &str, ops: &crate::incident_ops::IncidentOperations) -> Result<()> {
    if actor != ops.roles.commander {
        return Err(Error::Invalid("current incident commander required".into()));
    }
    Ok(())
}
fn invalidate(s: &mut RecoveryState) {
    s.response_resolved = false;
    s.resolved_at = None;
    s.resolution_reason = None;
    s.review = None;
}
impl Application {
    pub fn recovery_command(&self, case: &str, cmd: RecoveryCommand) -> Result<RecoverySnapshot> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        let fp = store::digest(&format!("recovery:{case}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, case)?.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale recovery command".into()));
        }
        let ops = crate::incident_ops::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
        let source = crate::sentry::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
        let policy = crate::policy::for_case(&tx, case)?;
        let now = self.now();
        supervise(&tx, &now)?;
        let mut s = state(&tx, case)?;
        match &cmd.action {
            RecoveryAction::Plan {
                criteria,
                review_seconds,
            } => {
                commander(&m.actor, &ops)?;
                text(&criteria.workflow, "workflow", 500)?;
                if criteria.min_requests == 0
                    || criteria.min_requests > 1_000_000_000_000
                    || criteria.min_window_seconds == 0
                    || criteria.min_window_seconds > 2_592_000
                    || criteria.max_error_ppm > 1_000_000
                    || criteria.max_p95_ms == 0
                    || criteria.max_p95_ms > 86_400_000
                    || *review_seconds < criteria.min_window_seconds
                    || *review_seconds > policy.max_review_seconds
                    || criteria.max_source_lag_seconds > 86400
                {
                    return Err(Error::Invalid("invalid recovery criteria/deadline".into()));
                }
                if s.response_resolved {
                    return Err(Error::Conflict(
                        "reopen response before planning another recovery episode".into(),
                    ));
                }
                let p = Plan {
                    id: store::id("plan"),
                    author: m.actor.clone(),
                    recorded_at: now.clone(),
                    review_by: (self.clock.now() + Duration::seconds((*review_seconds).into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    service: source.service,
                    environment: source.environment,
                    source_digest: source_digest(&tx, case)?,
                    incident_revision: ops.revision,
                    policy_provenance: policy.provenance(),
                    criteria: criteria.clone(),
                };
                retain(&tx, case, "recovery_plan", &p, m, &now)?;
                invalidate(&mut s);
                s.plan = Some(p);
                s.observation = None;
            }
            RecoveryAction::Observe { plan_id, window } => {
                if m.actor != ops.roles.operations && m.actor != ops.roles.commander {
                    return Err(Error::Invalid(
                        "incident operations or commander must report observation".into(),
                    ));
                }
                if s.response_resolved {
                    return Err(Error::Conflict(
                        "reopen response before recording a new recovery observation".into(),
                    ));
                }
                let p = s.plan.as_ref().ok_or(Error::NotFound)?;
                if p.id != *plan_id || !s.context_current || p.review_by <= now {
                    return Err(Error::Conflict("recovery plan is stale or expired".into()));
                }
                let start = timestamp(&window.started_at)?;
                let end = timestamp(&window.ended_at)?;
                if start < timestamp(&p.recorded_at)?
                    || end <= start
                    || end > self.clock.now()
                    || window.errors > window.requests
                    || window.requests > 1_000_000_000_000
                    || window.p95_ms.is_some_and(|v| v > 86_400_000)
                {
                    return Err(Error::Invalid(
                        "invalid recovery observation window/counts".into(),
                    ));
                }
                text(&window.collection_method, "collection method", 2000)?;
                text(&window.uncertainty, "uncertainty", 2000)?;
                crate::qualification::evidence_refs(&tx, case, &window.evidence_refs, true)?;
                let o=Observation{id:store::id("obs"),plan_id:plan_id.clone(),author:m.actor.clone(),recorded_at:now.clone(),window:window.clone(),outcome:evaluate(&p.criteria,window),basis:"Operator-reported numeric window; application evaluates criteria, not telemetry authenticity".into()};
                retain(&tx, case, "recovery_observation", &o, m, &now)?;
                invalidate(&mut s);
                s.observation = Some(o);
            }
            RecoveryAction::Review {
                observation_id,
                evidence_refs,
            } => {
                crate::challenges::ensure_clear(&tx, case)?;
                let o = s.observation.as_ref().ok_or(Error::NotFound)?;
                let p = s.plan.as_ref().ok_or(Error::NotFound)?;
                if o.id != *observation_id
                    || o.outcome != Outcome::Met
                    || !s.context_current
                    || p.review_by <= now
                {
                    return Err(Error::Conflict(
                        "latest recovery criteria must be met in current context".into(),
                    ));
                }
                if m.actor == o.author || m.actor == p.author {
                    return Err(Error::Invalid(
                        "recovery review must be independent of plan and observation authors"
                            .into(),
                    ));
                }
                independent_evidence(
                    &tx,
                    case,
                    evidence_refs,
                    &m.actor,
                    &o.recorded_at,
                    &o.window.evidence_refs,
                )?;
                let r = RecoveryReview {
                    id: store::id("rreview"),
                    observation_id: o.id.clone(),
                    reviewer: m.actor.clone(),
                    evidence_refs: evidence_refs.clone(),
                    recorded_at: now.clone(),
                };
                retain(&tx, case, "recovery_review", &r, m, &now)?;
                s.review = Some(r);
            }
            RecoveryAction::Resolve {
                review_id,
                evidence_refs,
            } => {
                commander(&m.actor, &ops)?;
                crate::challenges::ensure_clear(&tx, case)?;
                let p = s.plan.as_ref().ok_or(Error::NotFound)?;
                let r = s.review.as_ref().ok_or(Error::NotFound)?;
                let o = s.observation.as_ref().ok_or(Error::NotFound)?;
                if r.id != *review_id
                    || r.observation_id != o.id
                    || o.outcome != Outcome::Met
                    || !s.context_current
                    || p.review_by <= now
                {
                    return Err(Error::Conflict(
                        "current independently reviewed recovery required".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, evidence_refs, true)?;
                crate::qualification::evidence_refs(&tx, case, &r.evidence_refs, true)?;
                crate::qualification::evidence_refs(&tx, case, &o.window.evidence_refs, true)?;
                retain(
                    &tx,
                    case,
                    "response_resolved",
                    &serde_json::json!({"review_id":review_id,"evidence_refs":evidence_refs,"basis":"operator_reported_review","remaining_work":"Repair, deployment, postmortem and corrective actions remain separate"}),
                    m,
                    &now,
                )?;
                s.response_resolved = true;
                s.resolved_at = Some(now.clone());
                s.resolution_reason = Some(m.reason.clone());
            }
            RecoveryAction::Reopen { evidence_refs } => {
                commander(&m.actor, &ops)?;
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                retain(&tx, case, "response_reopened", evidence_refs, m, &now)?;
                invalidate(&mut s);
                s.plan = None;
                s.observation = None;
            }
            RecoveryAction::ProposeMitigation {
                runbook,
                target,
                steps,
                preconditions,
                stop_conditions,
                rollback,
                risk,
                evidence_refs,
            } => {
                commander(&m.actor, &ops)?;
                for (name, value) in [
                    ("runbook", runbook),
                    ("target", target),
                    ("steps", steps),
                    ("preconditions", preconditions),
                    ("stop conditions", stop_conditions),
                    ("rollback", rollback),
                    ("risk", risk),
                ] {
                    text(value, name, 4000)?;
                }
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                let id = retain(
                    &tx,
                    case,
                    "mitigation_blocked",
                    &serde_json::json!({"proposal":cmd.action,"state":"blocked_authority","executed":false}),
                    m,
                    &now,
                )?;
                crate::team::escalate(
                    &tx,
                    case,
                    "mitigation_authority",
                    &id,
                    "Mitigation remains a proposal; no production executor/authority is configured",
                    &now,
                )?;
            }
            RecoveryAction::RecordDeployment {
                repository,
                revision,
                environment,
                observed_at,
                evidence_refs,
            } => {
                if m.actor != ops.roles.operations && m.actor != ops.roles.commander {
                    return Err(Error::Invalid("incident operations required".into()));
                }
                if *repository != source.repository
                    || *environment != source.environment
                    || !crate::repair::oid(revision)
                    || timestamp(observed_at)? > self.clock.now()
                {
                    return Err(Error::Invalid(
                        "deployment report must match explicit incident scope and revision/time"
                            .into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, evidence_refs, true)?;
                retain(&tx, case, "deployment_reported", &cmd.action, m, &now)?;
            }
            RecoveryAction::OpenAction {
                title,
                category,
                owner,
                verifier,
                acceptance,
                due_seconds,
            } => {
                commander(&m.actor, &ops)?;
                text(title, "title", 500)?;
                text(acceptance, "acceptance", 2000)?;
                crate::incident_ops::human(owner)?;
                crate::incident_ops::human(verifier)?;
                if owner == verifier
                    || *due_seconds == 0
                    || *due_seconds > policy.max_review_seconds
                    || ![
                        "prevent",
                        "detect",
                        "mitigate",
                        "resilience",
                        "test",
                        "runbook",
                        "process",
                    ]
                    .contains(&category.as_str())
                {
                    return Err(Error::Invalid(
                        "action needs independent verifier, valid category and bounded deadline"
                            .into(),
                    ));
                }
                let a = CorrectiveAction {
                    id: store::id("action"),
                    case_id: case.into(),
                    revision: 1,
                    title: title.clone(),
                    category: category.clone(),
                    owner: owner.clone(),
                    verifier: verifier.clone(),
                    acceptance: acceptance.clone(),
                    due_at: (self.clock.now() + Duration::seconds((*due_seconds).into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    state: "open".into(),
                    submitted_at: None,
                    submission_refs: vec![],
                    verification_refs: vec![],
                    residual_risk: None,
                    escalated: false,
                };
                save_action(&tx, &a)?;
                retain(&tx, case, "action_opened", &a, m, &now)?;
            }
            RecoveryAction::SubmitAction {
                id,
                revision,
                evidence_refs,
            }
            | RecoveryAction::VerifyAction {
                id,
                revision,
                evidence_refs,
            }
            | RecoveryAction::WaiveAction {
                id,
                revision,
                evidence_refs,
                ..
            } => {
                let mut a = action(&tx, case, id)?;
                if a.revision != *revision || ["verified", "waived"].contains(&a.state.as_str()) {
                    return Err(Error::Conflict(
                        "action revision is stale or terminal".into(),
                    ));
                }
                match &cmd.action {
                    RecoveryAction::SubmitAction { .. } => {
                        if a.owner != m.actor {
                            return Err(Error::Invalid("action owner must submit proof".into()));
                        }
                        crate::qualification::evidence_refs(&tx, case, evidence_refs, true)?;
                        a.state = "submitted".into();
                        a.submitted_at = Some(now.clone());
                        a.submission_refs = evidence_refs.clone();
                        a.verification_refs.clear();
                    }
                    RecoveryAction::VerifyAction { .. } => {
                        crate::challenges::ensure_clear(&tx, case)?;
                        if a.verifier != m.actor || a.owner == m.actor || a.state != "submitted" {
                            return Err(Error::Invalid(
                                "assigned independent verifier and submitted action required"
                                    .into(),
                            ));
                        }
                        independent_evidence(
                            &tx,
                            case,
                            evidence_refs,
                            &m.actor,
                            a.submitted_at.as_deref().unwrap(),
                            &a.submission_refs,
                        )?;
                        a.state = "verified".into();
                        a.verification_refs = evidence_refs.clone();
                    }
                    RecoveryAction::WaiveAction { residual_risk, .. } => {
                        crate::challenges::ensure_clear(&tx, case)?;
                        commander(&m.actor, &ops)?;
                        text(residual_risk, "residual risk", 2000)?;
                        crate::qualification::evidence_refs(&tx, case, evidence_refs, true)?;
                        a.state = "waived".into();
                        a.residual_risk = Some(residual_risk.clone());
                        a.verification_refs = evidence_refs.clone();
                    }
                    _ => unreachable!(),
                }
                a.revision += 1;
                save_action(&tx, &a)?;
                retain(&tx, case, "action_updated", &a, m, &now)?;
            }
        }
        save_state(&tx, case, &mut s)?;
        let result = snapshot(&tx, case)?;
        save_receipt(&tx, &m.idempotency_key, &fp, &result)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn recovery_markdown(&self, case: &str) -> Result<String> {
        let s = snapshot(&self.store.connect()?, case)?;
        let raw = serde_json::to_string_pretty(&s)?
            .replace('`', "\\u0060")
            .replace('<', "\\u003c");
        Ok(format!(
            "# Recovery and follow-through\n\nOperator-reported observations and review. No production action is executed by these records. Merge, deployment, response resolution and permanent repair remain separate.\n\n```json\n{raw}\n```\n"
        ))
    }
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let ids: Vec<(String, String)> = {
        let mut q=tx.prepare("SELECT id,case_id FROM corrective_actions WHERE state IN ('open','submitted') AND escalated=0 AND due_at<=?1 ORDER BY due_at,rowid LIMIT 100")?;
        q.query_map([now], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?
    };
    for (id, case) in &ids {
        let a = action(tx, case, id)?;
        crate::team::escalate(
            tx,
            case,
            "corrective_action",
            id,
            &format!(
                "Corrective action overdue: {} owns {}; verifier {}. Incident resolution does not close follow-ups",
                a.owner, a.title, a.verifier
            ),
            now,
        )?;
        tx.execute(
            "UPDATE corrective_actions SET escalated=1 WHERE id=?1",
            [id],
        )?;
    }
    let cases: Vec<String> = {
        let mut q=tx.prepare("SELECT r.case_id FROM recovery_state r JOIN source_cases c ON c.case_id=r.case_id LEFT JOIN incident_operations o ON o.case_id=r.case_id WHERE json_extract(r.record,'$.response_resolved')=1 AND (json_extract(r.record,'$.plan.source_digest')!=c.digest OR json_extract(r.record,'$.plan.incident_revision')!=COALESCE(json_extract(o.record,'$.revision'),1) OR EXISTS(SELECT 1 FROM challenges h WHERE h.case_id=r.case_id AND json_extract(h.record,'$.blocking')=1 AND h.state!='resolved')) ORDER BY r.rowid LIMIT 100")?;
        q.query_map([], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?
    };
    let plans: Vec<(String, String)> = {
        let mut q=tx.prepare("SELECT r.case_id,json_extract(r.record,'$.plan.id') FROM recovery_state r WHERE json_extract(r.record,'$.response_resolved')=0 AND json_extract(r.record,'$.plan.review_by')<=?1 AND NOT EXISTS(SELECT 1 FROM escalations e WHERE e.source_type='recovery_review' AND e.source_id=json_extract(r.record,'$.plan.id')) ORDER BY r.rowid LIMIT 100")?;
        q.query_map([now], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?
    };
    for (case, id) in &plans {
        crate::team::escalate(
            tx,
            case,
            "recovery_review",
            id,
            "Recovery review deadline reached; commander must collect sufficient evidence or record a revised plan",
            now,
        )?;
    }
    let mut changed = ids.len() + plans.len();
    for case in cases {
        let mut s = state(tx, &case)?;
        if !s.context_current || crate::challenges::ensure_clear(tx, &case).is_err() {
            invalidate(&mut s);
            save_state(tx, &case, &mut s)?;
            let id = store::id("reopen");
            store::emit(tx,&case,Actor{kind:"supervisor".into(),id:"recovery-supervisor".into()},&id,now,EventData::RecoveryRecorded{record_id:id.clone(),kind:"response_reassessment".into(),reason:"Source or incident context changed after reported recovery; response reopened".into()})?;
            crate::team::escalate(
                tx,
                &case,
                "recovery_reassessment",
                &id,
                "Recovery context changed; commander must assess recurrence and create a new plan",
                now,
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}
