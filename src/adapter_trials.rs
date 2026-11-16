//! Durable fake protocol conformance. No subprocess, model, candidate, or external effect.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    repair::RepairRequest,
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    Complete,
    InvalidOutput,
    Quota,
    Stall,
    StartUnknown,
    CancelUnknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialCommand {
    pub command: OperatorCommand,
    pub action: Action,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Start {
        repair_id: String,
        scenario: Scenario,
        wall_seconds: u32,
    },
    Advance {
        trial_id: String,
    },
    Cancel {
        trial_id: String,
    },
    Reconcile {
        trial_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub schema_version: V1,
    pub trial_id: String,
    pub case_id: String,
    pub repair_id: String,
    pub attempt: i64,
    pub fence: String,
    pub owner: String,
    pub policy_provenance: String,
    pub source_revision: i64,
    pub manifest_digest: String,
    pub base_commit: String,
    pub objective: String,
    pub expected_behavior: String,
    pub regression_plan: String,
    pub evidence_refs: Vec<String>,
    pub deadline: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Running,
    Unknown,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrialResult {
    pub schema_version: V1,
    pub trial_id: String,
    pub fence: String,
    pub conclusion: String,
    pub synthetic: bool,
    pub actual_model: Option<String>,
    pub usage: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCode {
    Quota,
    Timeout,
    InvalidOutput,
    Cancelled,
    UnknownOutcome,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    pub failure_code: Option<FailureCode>,
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub repair_id: String,
    pub adapter: String,
    pub scope: String,
    pub real_execution_admitted: bool,
    pub envelope: Envelope,
    pub envelope_digest: String,
    pub scenario: Scenario,
    pub state: State,
    pub pending_operation: Option<String>,
    pub sequence: i64,
    pub request_reason: String,
    pub requested_at: String,
    pub updated_at: String,
    pub reason: String,
    pub result: Option<TrialResult>,
    pub evidence_digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrialEvent {
    pub schema_version: V1,
    pub trial_id: String,
    pub sequence: i64,
    pub state: State,
    pub kind: String,
    pub reason: String,
    pub recorded_at: String,
}
fn get(db: &Connection, id: &str) -> Result<Trial> {
    let raw: Option<String> = db
        .query_row("SELECT record FROM adapter_trials WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
pub(crate) fn snapshot(db: &Connection, case_id: &str) -> Result<Vec<Trial>> {
    let mut s = db.prepare(
        "SELECT record FROM adapter_trials WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    s.query_map([case_id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
fn save(
    tx: &Transaction<'_>,
    t: &mut Trial,
    kind: &str,
    reason: &str,
    actor: Actor,
    cause: &str,
    now: &str,
) -> Result<()> {
    t.sequence += 1;
    t.updated_at = now.into();
    t.reason = reason.into();
    tx.execute("INSERT INTO adapter_trials VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![t.id,t.case_id,t.repair_id,serde_json::to_value(&t.state)?.as_str().unwrap(),t.envelope.deadline,serde_json::to_string(t)?])?;
    let event = TrialEvent {
        schema_version: V1,
        trial_id: t.id.clone(),
        sequence: t.sequence,
        state: t.state.clone(),
        kind: kind.into(),
        reason: reason.into(),
        recorded_at: now.into(),
    };
    tx.execute(
        "INSERT INTO adapter_trial_events VALUES (?1,?2,?3)",
        params![t.id, t.sequence, serde_json::to_string(&event)?],
    )?;
    store::emit(
        tx,
        &t.case_id,
        actor,
        cause,
        now,
        EventData::AdapterTrialChanged {
            trial_id: t.id.clone(),
            sequence: t.sequence,
            state: serde_json::to_value(&t.state)?.as_str().unwrap().into(),
            reason: reason.into(),
        },
    )
}
fn evidence(tx: &Transaction<'_>, t: &mut Trial, raw: &str, now: &str) -> Result<()> {
    let bytes = serde_json::to_string(
        &serde_json::json!({"schema_version":1,"scope":"fake-adapter-protocol-v1","envelope_digest":t.envelope_digest,"trial_id":t.id,"output":raw}),
    )?;
    let digest = store::digest(&bytes);
    tx.execute("INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,'adapter-trial-synthetic-v1',?3)",params![digest,bytes,now])?;
    tx.execute(
        "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
        params![t.case_id, digest],
    )?;
    t.evidence_digest = Some(digest);
    Ok(())
}
fn escalate(tx: &Transaction<'_>, t: &Trial, now: &str) -> Result<()> {
    crate::team::escalate(
        tx,
        &t.case_id,
        "adapter_trial",
        &t.id,
        &format!(
            "Synthetic adapter trial requires reconciliation/review: {}. No repair or real execution was accepted",
            t.reason
        ),
        now,
    )?;
    Ok(())
}
fn supervisor() -> Actor {
    Actor {
        kind: "supervisor".into(),
        id: "adapter-trial-supervisor".into(),
    }
}
fn expire(tx: &Transaction<'_>, t: &mut Trial, now: &str) -> Result<bool> {
    if !matches!(t.state, State::Running | State::Unknown) || t.envelope.deadline.as_str() > now {
        return Ok(false);
    }
    // A lost start/cancel acknowledgment must be reconciled, never guessed terminal.
    if t.state == State::Unknown {
        return Ok(false);
    }
    let observed: Option<String> = tx
        .query_row(
            "SELECT state FROM fake_adapter_sessions WHERE id=?1",
            [&t.id],
            |r| r.get(0),
        )
        .optional()?;
    if !matches!(observed.as_deref(), Some("running" | "cancelled")) {
        t.state = State::Unknown;
        t.failure_code = Some(FailureCode::UnknownOutcome);
        t.pending_operation = Some("cancel".into());
        save(
            tx,
            t,
            "backend_unknown",
            "Deadline reached but fake backend handle is missing/conflicting; cancellation unconfirmed",
            supervisor(),
            "adapter-deadline",
            now,
        )?;
        escalate(tx, t, now)?;
        return Ok(true);
    }
    if t.scenario == Scenario::CancelUnknown {
        tx.execute(
            "UPDATE fake_adapter_sessions SET state='cancelled',result=NULL WHERE id=?1",
            [&t.id],
        )?;
        t.failure_code = Some(FailureCode::UnknownOutcome);
        t.state = State::Unknown;
        t.pending_operation = Some("cancel".into());
        save(
            tx,
            t,
            "cancel_unknown",
            "Deadline reached; fake cancellation acknowledgment lost; reconcile before any retry",
            supervisor(),
            "adapter-deadline",
            now,
        )?;
    } else {
        tx.execute(
            "UPDATE fake_adapter_sessions SET state='cancelled',result=NULL WHERE id=?1",
            [&t.id],
        )?;
        t.failure_code = Some(FailureCode::Timeout);
        t.state = State::TimedOut;
        t.pending_operation = None;
        save(
            tx,
            t,
            "timed_out",
            "Fake deadline exceeded; simulated backend cancellation confirmed",
            supervisor(),
            "adapter-deadline",
            now,
        )?;
    }
    escalate(tx, t, now)?;
    Ok(true)
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let ids: Vec<String> = {
        let mut s=tx.prepare("SELECT id FROM adapter_trials WHERE state='running' AND deadline<=?1 ORDER BY rowid LIMIT 100")?;
        s.query_map([now], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?
    };
    let mut n = 0;
    for id in ids {
        n += usize::from(expire(tx, &mut get(tx, &id)?, now)?);
    }
    Ok(n)
}
impl Application {
    /// Only the deterministic fake backend is available. This grants no repair authority.
    pub fn adapter_trial(&self, case_id: &str, cmd: TrialCommand) -> Result<Trial> {
        let meta = &cmd.command.meta;
        text(&meta.actor, "operator", 128)?;
        text(&meta.reason, "reason", 2000)?;
        text(&meta.idempotency_key, "key", 128)?;
        if meta.actor.starts_with("fake-") {
            return Err(Error::Invalid(
                "adapter trials require the case operator".into(),
            ));
        }
        let fp = store::digest(&format!(
            "adapter-trial:{case_id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(id) = receipt::<String>(&tx, &meta.idempotency_key, &fp)? {
            return get(&tx, &id);
        }
        let case = store::case(&tx, case_id)?;
        if case.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "case changed; reload before adapter command".into(),
            ));
        }
        if case.owner != meta.actor {
            return Err(Error::Invalid(
                "current case owner must operate adapter trials".into(),
            ));
        }
        let now = self.now();
        let mut t = match &cmd.action {
            Action::Start {
                repair_id,
                scenario,
                wall_seconds,
            } => {
                let raw: Option<String> = tx
                    .query_row(
                        "SELECT record FROM repair_requests WHERE id=?1 AND case_id=?2",
                        params![repair_id, case_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                let repair: RepairRequest = serde_json::from_str(&raw.ok_or(Error::NotFound)?)?;
                let policy = crate::policy::for_case(&tx, case_id)?;
                if *wall_seconds == 0 || i64::from(*wall_seconds) > policy.assignment_seconds {
                    return Err(Error::Invalid(
                        "trial deadline exceeds governing policy".into(),
                    ));
                }
                let (count,active):(i64,i64)=tx.query_row("SELECT COUNT(*),COALESCE(SUM(state IN ('running','unknown')),0) FROM adapter_trials WHERE repair_id=?1",[repair_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
                if active > 0 || count >= policy.max_attempts {
                    return Err(Error::Conflict("reconcile active trial or inspect exhausted attempt budget; retries cannot reset it".into()));
                }
                let id = store::id("trial");
                let envelope = Envelope {
                    schema_version: V1,
                    trial_id: id.clone(),
                    case_id: case_id.into(),
                    repair_id: repair_id.clone(),
                    attempt: count + 1,
                    fence: store::id("fence"),
                    owner: case.owner,
                    policy_provenance: policy.provenance(),
                    source_revision: repair.source_revision,
                    manifest_digest: repair.manifest_digest,
                    base_commit: repair.base_commit,
                    objective: repair.objective,
                    expected_behavior: repair.expected_behavior,
                    regression_plan: repair.regression_plan,
                    evidence_refs: repair.evidence_refs,
                    deadline: (self.clock.now() + Duration::seconds((*wall_seconds).into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                };
                let mut t = Trial {
                    failure_code: None,
                    schema_version: V1,
                    id: id.clone(),
                    case_id: case_id.into(),
                    repair_id: repair_id.clone(),
                    adapter: "deterministic-fake-v1".into(),
                    scope: "fake-adapter-protocol-v1".into(),
                    real_execution_admitted: false,
                    envelope_digest: store::digest(&serde_json::to_string(&envelope)?),
                    envelope,
                    scenario: *scenario,
                    state: State::Running,
                    pending_operation: None,
                    sequence: 0,
                    request_reason: meta.reason.clone(),
                    requested_at: now.clone(),
                    updated_at: now.clone(),
                    reason: String::new(),
                    result: None,
                    evidence_digest: None,
                };
                if *scenario == Scenario::StartUnknown {
                    t.failure_code = Some(FailureCode::UnknownOutcome);
                    t.state = State::Unknown;
                    t.pending_operation = Some("start".into());
                }
                save(
                    &tx,
                    &mut t,
                    "start",
                    &format!(
                        "{}: reserved synthetic trial; no candidate, model or process is executed",
                        meta.reason
                    ),
                    operator(&meta.actor),
                    &meta.idempotency_key,
                    &now,
                )?;
                tx.execute(
                    "INSERT INTO fake_adapter_sessions VALUES (?1,'running',NULL)",
                    [&id],
                )?;
                if t.state == State::Unknown {
                    t.reason="Fake start response lost; reconcile by the existing handle without another start".into();
                    save(
                        &tx,
                        &mut t,
                        "start_unknown",
                        "Fake start response lost; reconcile by the existing handle without another start",
                        supervisor(),
                        &id,
                        &now,
                    )?;
                    escalate(&tx, &t, &now)?;
                }
                t
            }
            Action::Advance { trial_id }
            | Action::Cancel { trial_id }
            | Action::Reconcile { trial_id } => {
                let mut t = get(&tx, trial_id)?;
                if t.case_id != case_id {
                    return Err(Error::NotFound);
                }
                // Commit expiry even when the triggering request was a late poll/cancel.
                let expired = expire(&tx, &mut t, &now)?;
                if !expired {
                    match &cmd.action {
                        Action::Advance { .. } => advance(&tx, &mut t, meta, &now)?,
                        Action::Cancel { .. } => cancel(&tx, &mut t, meta, &now)?,
                        Action::Reconcile { .. } => reconcile(&tx, &mut t, meta, &now)?,
                        _ => unreachable!(),
                    }
                }
                t
            }
        };
        // Unknown starts past their deadline are reconciled and then deterministically expired.
        expire(&tx, &mut t, &now)?;
        save_receipt(&tx, &meta.idempotency_key, &fp, &t.id)?;
        tx.commit()?;
        Ok(t)
    }
    pub fn adapter_trial_status(&self, case_id: &str, trial_id: &str) -> Result<Trial> {
        let db = self.store.connect()?;
        let trial = get(&db, trial_id)?;
        if trial.case_id != case_id {
            return Err(Error::NotFound);
        }
        Ok(trial)
    }
    pub fn adapter_trial_events(
        &self,
        case_id: &str,
        trial_id: &str,
        after: i64,
    ) -> Result<Vec<TrialEvent>> {
        if after < 0 {
            return Err(Error::Invalid("cursor must be nonnegative".into()));
        }
        let db = self.store.connect()?;
        if get(&db, trial_id)?.case_id != case_id {
            return Err(Error::NotFound);
        }
        let mut s=db.prepare("SELECT record FROM adapter_trial_events WHERE trial_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 200")?;
        s.query_map(params![trial_id, after], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect()
    }
}
fn advance(tx: &Transaction<'_>, t: &mut Trial, meta: &CommandMeta, now: &str) -> Result<()> {
    if t.state != State::Running {
        return Err(Error::Conflict(
            "only an observed running trial can advance; reconcile unknown outcomes".into(),
        ));
    }
    if !backend_running(tx, t, meta, now)? {
        return Ok(());
    }
    if matches!(t.scenario, Scenario::Stall | Scenario::CancelUnknown) {
        // Heartbeats intentionally do not extend the absolute deadline or grow the event journal.
        return Ok(());
    }
    let raw = if t.scenario == Scenario::InvalidOutput {
        "{\"exit_code\":0,\"claim\":\"fixed\"}".into()
    } else {
        serde_json::to_string(&TrialResult {
            schema_version: V1,
            trial_id: t.id.clone(),
            fence: t.envelope.fence.clone(),
            conclusion: "synthetic_only".into(),
            synthetic: true,
            actual_model: None,
            usage: None,
        })?
    };
    if t.scenario == Scenario::Quota {
        t.failure_code = Some(FailureCode::Quota);
        t.state = State::Failed;
        evidence(
            tx,
            t,
            "{\"failure_code\":\"quota\",\"synthetic\":true}",
            now,
        )?;
        tx.execute(
            "UPDATE fake_adapter_sessions SET state='failed' WHERE id=?1",
            [&t.id],
        )?;
        save(
            tx,
            t,
            "quota",
            &format!(
                "{}: structured synthetic quota failure; no automatic retry",
                meta.reason
            ),
            operator(&meta.actor),
            &meta.idempotency_key,
            now,
        )?;
        escalate(tx, t, now)?;
        return Ok(());
    }
    // A process-style exit success is insufficient: validate the role schema and exact fence.
    let result = validated_result(&raw, &t.id, &t.envelope.fence);
    evidence(tx, t, &raw, now)?;
    t.state = if result.is_some() {
        State::Completed
    } else {
        State::Failed
    };
    t.failure_code = if result.is_some() {
        None
    } else {
        Some(FailureCode::InvalidOutput)
    };
    t.result = result;
    tx.execute(
        "UPDATE fake_adapter_sessions SET state=?2,result=?3 WHERE id=?1",
        params![
            t.id,
            if t.state == State::Completed {
                "completed"
            } else {
                "failed"
            },
            t.result.as_ref().map(serde_json::to_string).transpose()?
        ],
    )?;
    let reason = if t.state == State::Completed {
        "Synthetic schema/fence validated; no repair or regression accepted"
    } else {
        "Invalid synthetic output despite exit success; no result accepted"
    };
    save(
        tx,
        t,
        "result",
        &format!("{}: {reason}", meta.reason),
        operator(&meta.actor),
        &meta.idempotency_key,
        now,
    )?;
    if t.state == State::Failed {
        escalate(tx, t, now)?;
    }
    Ok(())
}
fn cancel(tx: &Transaction<'_>, t: &mut Trial, meta: &CommandMeta, now: &str) -> Result<()> {
    if t.state != State::Running {
        return Err(Error::Conflict(
            "cancel requires running state; reconcile uncertainty first".into(),
        ));
    }
    if !backend_running(tx, t, meta, now)? {
        return Ok(());
    }
    tx.execute(
        "UPDATE fake_adapter_sessions SET state='cancelled',result=NULL WHERE id=?1",
        [&t.id],
    )?;
    t.state = if t.scenario == Scenario::CancelUnknown {
        State::Unknown
    } else {
        State::Cancelled
    };
    t.failure_code = Some(if t.state == State::Unknown {
        FailureCode::UnknownOutcome
    } else {
        FailureCode::Cancelled
    });
    t.pending_operation = if t.state == State::Unknown {
        Some("cancel".into())
    } else {
        None
    };
    save(
        tx,
        t,
        "cancel",
        &format!(
            "{}: simulated cancellation; missing acknowledgment stays unknown until reconciliation",
            meta.reason
        ),
        operator(&meta.actor),
        &meta.idempotency_key,
        now,
    )?;
    if t.state == State::Unknown {
        escalate(tx, t, now)?;
    }
    Ok(())
}
fn reconcile(tx: &Transaction<'_>, t: &mut Trial, meta: &CommandMeta, now: &str) -> Result<()> {
    if t.state != State::Unknown {
        return Ok(());
    }
    let observed: Option<String> = tx
        .query_row(
            "SELECT state FROM fake_adapter_sessions WHERE id=?1",
            [&t.id],
            |r| r.get(0),
        )
        .optional()?;
    match observed.as_deref() {
        Some("running") => {
            t.state = State::Running;
            t.failure_code = None;
            t.pending_operation = None;
        }
        Some("cancelled") => {
            t.state = State::Cancelled;
            t.failure_code = Some(FailureCode::Cancelled);
            t.pending_operation = None;
        }
        _ => return Ok(()), // missing provider record never proves that no start/effect occurred
    }
    save(
        tx,
        t,
        "reconciled",
        &format!(
            "{}: read existing fake backend handle; no repeated start/cancel or automatic escalation resolution",
            meta.reason
        ),
        operator(&meta.actor),
        &meta.idempotency_key,
        now,
    )
}

fn validated_result(raw: &str, trial_id: &str, fence: &str) -> Option<TrialResult> {
    serde_json::from_str::<TrialResult>(raw).ok().filter(|r| {
        r.trial_id == trial_id
            && r.fence == fence
            && r.synthetic
            && r.conclusion == "synthetic_only"
            && r.actual_model.is_none()
            && r.usage.is_none()
    })
}

fn backend_running(
    tx: &Transaction<'_>,
    t: &mut Trial,
    meta: &CommandMeta,
    now: &str,
) -> Result<bool> {
    let observed: Option<String> = tx
        .query_row(
            "SELECT state FROM fake_adapter_sessions WHERE id=?1",
            [&t.id],
            |r| r.get(0),
        )
        .optional()?;
    if observed.as_deref() == Some("running") {
        return Ok(true);
    }
    t.state = State::Unknown;
    t.failure_code = Some(FailureCode::UnknownOutcome);
    t.pending_operation = Some("observe".into());
    save(
        tx,
        t,
        "backend_unknown",
        "Fake backend handle missing or conflicting; no terminal outcome inferred",
        operator(&meta.actor),
        &meta.idempotency_key,
        now,
    )?;
    escalate(tx, t, now)?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_foreign_or_non_synthetic_outputs_never_become_results() {
        let raw = serde_json::json!({"schema_version":1,"trial_id":"trial","fence":"fresh","synthetic":true,"conclusion":"synthetic_only","actual_model":null,"usage":null});
        assert!(validated_result(&raw.to_string(), "trial", "fresh").is_some());
        for (field, value) in [
            ("fence", serde_json::json!("old")),
            ("trial_id", serde_json::json!("foreign")),
            ("synthetic", serde_json::json!(false)),
            ("conclusion", serde_json::json!("fixed")),
            ("actual_model", serde_json::json!("claimed")),
            ("schema_version", serde_json::json!(2)),
            ("extra", serde_json::json!(true)),
        ] {
            let mut invalid = raw.clone();
            invalid[field] = value;
            assert!(validated_result(&invalid.to_string(), "trial", "fresh").is_none());
        }
    }
}
