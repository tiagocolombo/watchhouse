//! Independent, bounded review of immutable public claims, never private transcripts.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub id: String,
    pub case_id: String,
    pub state: String,
    pub target_event_id: String,
    pub target_version: i64,
    pub author: String,
    pub challenger: String,
    pub recipient: String,
    pub arbiter: String,
    pub requested_evidence: String,
    pub uncertainty: String,
    pub blocking: bool,
    pub acknowledge_by: String,
    pub respond_by: String,
    pub answer: Option<String>,
    pub answer_evidence: Vec<String>,
    pub resolution: Option<String>,
    pub resolution_evidence: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeCommand {
    pub command: OperatorCommand,
    pub action: ChallengeAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChallengeAction {
    Open {
        target_event_id: String,
        target_version: i64,
        recipient: String,
        arbiter: String,
        requested_evidence: String,
        uncertainty: String,
        blocking: bool,
        acknowledge_seconds: u32,
        response_seconds: u32,
    },
    Acknowledge {
        id: String,
    },
    Investigate {
        id: String,
    },
    Answer {
        id: String,
        answer: String,
        evidence_refs: Vec<String>,
    },
    Resolve {
        id: String,
        disposition: String,
        uncertainty: String,
        evidence_refs: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateCommand {
    pub command: OperatorCommand,
    pub close: bool,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub validated_version: i64,
    pub verifier: String,
    pub evidence_refs: Vec<String>,
    pub closed_version: Option<i64>,
}
pub(crate) fn snapshot(db: &Connection, case_id: &str) -> Result<Vec<Challenge>> {
    let mut s = db.prepare("SELECT record FROM challenges WHERE case_id=?1 ORDER BY rowid")?;
    s.query_map([case_id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
pub(crate) fn gate(db: &Connection, id: &str) -> Result<Option<Gate>> {
    let raw:Option<(i64,String,String,Option<i64>)>=db.query_row("SELECT validated_version,verifier,evidence_refs,closed_version FROM case_gates WHERE case_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    raw.map(|(validated_version, verifier, refs, closed_version)| {
        Ok(Gate {
            validated_version,
            verifier,
            evidence_refs: serde_json::from_str(&refs)?,
            closed_version,
        })
    })
    .transpose()
}
pub(crate) fn ensure_clear(db: &Connection, id: &str) -> Result<()> {
    if snapshot(db, id)?
        .iter()
        .any(|c| c.blocking && c.state != "resolved")
    {
        return Err(Error::Conflict(
            "unresolved blocking challenge prevents acceptance/validation/closure".into(),
        ));
    }
    Ok(())
}
fn save(
    tx: &Transaction<'_>,
    c: &Challenge,
    actor: Actor,
    key: &str,
    now: &str,
    reason: &str,
) -> Result<()> {
    tx.execute("INSERT INTO challenges VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![c.id,c.case_id,c.state,c.acknowledge_by,c.respond_by,serde_json::to_string(c)?])?;
    store::emit(
        tx,
        &c.case_id,
        actor,
        key,
        now,
        EventData::ChallengeChanged {
            challenge: Box::new(c.clone()),
            reason: reason.into(),
        },
    )
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut s=tx.prepare("SELECT record FROM challenges WHERE (state='open' AND acknowledge_by<=?1) OR (state IN ('open','acknowledged','investigating','answered') AND respond_by<=?1) LIMIT 100")?;
    let rows = s
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for raw in &rows {
        let mut c: Challenge = serde_json::from_str(raw)?;
        c.state = "escalated".into();
        save(
            tx,
            &c,
            operator("challenge-supervisor"),
            &c.id,
            now,
            "Required challenge response overdue; independent human arbitration required",
        )?;
        crate::team::escalate(
            tx,
            &c.case_id,
            "challenge",
            &c.id,
            "Challenge deadline missed",
            now,
        )?;
    }
    Ok(rows.len())
}
impl Application {
    pub fn challenge(&self, case_id: &str, cmd: ChallengeCommand) -> Result<Challenge> {
        let m = &cmd.command.meta;
        text(&m.actor, "actor", 128)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!(
            "challenge:{case_id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, case_id)?.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale case version".into()));
        }
        let now = self.now();
        let c = match cmd.action {
            ChallengeAction::Open {
                target_event_id,
                target_version,
                recipient,
                arbiter,
                requested_evidence,
                uncertainty,
                blocking,
                acknowledge_seconds,
                response_seconds,
            } => {
                text(&recipient, "recipient", 128)?;
                text(&arbiter, "arbiter", 128)?;
                text(&requested_evidence, "requested evidence", 2000)?;
                text(&uncertainty, "uncertainty", 2000)?;
                if m.actor != "fake-challenger"
                    || arbiter != "fake-verifier"
                    || recipient == arbiter
                    || recipient == m.actor
                {
                    return Err(Error::Invalid("separate admitted challenger, respondent and verifier responsibilities required".into()));
                }
                if acknowledge_seconds == 0
                    || response_seconds < acknowledge_seconds
                    || response_seconds > 86400
                {
                    return Err(Error::Invalid(
                        "challenge deadlines must be ordered and bounded to one day".into(),
                    ));
                }
                let raw:String=tx.query_row("SELECT envelope FROM events WHERE event_id=?1 AND case_id=?2 AND aggregate_version=?3",params![target_event_id,case_id,target_version],|r|r.get(0)).optional()?.ok_or(Error::NotFound)?;
                let e: Event = serde_json::from_str(&raw)?;
                if e.actor.id == arbiter || e.actor.id == m.actor {
                    return Err(Error::Invalid(
                        "claim author cannot challenge or arbitrate its own claim".into(),
                    ));
                }
                Challenge {
                    id: store::id("chl"),
                    case_id: case_id.into(),
                    state: "open".into(),
                    target_event_id,
                    target_version,
                    author: e.actor.id,
                    challenger: m.actor.clone(),
                    recipient,
                    arbiter,
                    requested_evidence,
                    uncertainty,
                    blocking,
                    acknowledge_by: (self.clock.now()
                        + Duration::seconds(acknowledge_seconds.into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                    respond_by: (self.clock.now() + Duration::seconds(response_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    answer: None,
                    answer_evidence: vec![],
                    resolution: None,
                    resolution_evidence: vec![],
                }
            }
            action => {
                let id = match &action {
                    ChallengeAction::Acknowledge { id }
                    | ChallengeAction::Investigate { id }
                    | ChallengeAction::Answer { id, .. }
                    | ChallengeAction::Resolve { id, .. } => id,
                    _ => unreachable!(),
                };
                let mut c = snapshot(&tx, case_id)?
                    .into_iter()
                    .find(|c| &c.id == id)
                    .ok_or(Error::NotFound)?;
                if c.state == "resolved" {
                    return Err(Error::Conflict("challenge already resolved".into()));
                }
                let resolving = matches!(action, ChallengeAction::Resolve { .. });
                if (resolving && m.actor != c.arbiter) || (!resolving && m.actor != c.recipient) {
                    return Err(Error::Invalid(
                        "actor does not hold the required challenge responsibility".into(),
                    ));
                }
                if !resolving && (c.respond_by <= now || c.state == "escalated") {
                    return Err(Error::Conflict(
                        "challenge is overdue; arbiter required".into(),
                    ));
                }
                match action {
                    ChallengeAction::Acknowledge { .. }
                        if c.state == "open" && c.acknowledge_by > now =>
                    {
                        c.state = "acknowledged".into()
                    }
                    ChallengeAction::Investigate { .. } if c.state == "acknowledged" => {
                        c.state = "investigating".into()
                    }
                    ChallengeAction::Answer {
                        answer,
                        evidence_refs,
                        ..
                    } if matches!(c.state.as_str(), "acknowledged" | "investigating") => {
                        text(&answer, "answer", 8000)?;
                        crate::qualification::evidence_refs(&tx, case_id, &evidence_refs, false)?;
                        c.answer = Some(answer);
                        c.answer_evidence = evidence_refs;
                        c.state = "answered".into();
                    }
                    ChallengeAction::Resolve {
                        disposition,
                        uncertainty,
                        evidence_refs,
                        ..
                    } if matches!(c.state.as_str(), "answered" | "escalated") => {
                        text(&disposition, "disposition", 2000)?;
                        text(&uncertainty, "uncertainty", 2000)?;
                        crate::qualification::evidence_refs(&tx, case_id, &evidence_refs, true)?;
                        // Verifier must gather a separately attributed observation, not repeat the answer.
                        let independent:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE case_id=?1 AND json_extract(envelope,'$.type')='evidence.recorded' AND json_extract(envelope,'$.actor.id')=?2 AND json_extract(envelope,'$.payload.evidence_refs[0]') IN (SELECT value FROM json_each(?3)))",params![case_id,m.actor,serde_json::to_string(&evidence_refs)?],|r|r.get(0))?;
                        if !independent
                            || evidence_refs.iter().all(|r| c.answer_evidence.contains(r))
                        {
                            return Err(Error::Invalid("resolution requires fresh, separately attributed verifier evidence".into()));
                        }
                        c.resolution = Some(disposition);
                        c.uncertainty = uncertainty;
                        c.resolution_evidence = evidence_refs;
                        c.state = "resolved".into();
                    }
                    _ => return Err(Error::Conflict("invalid challenge transition".into())),
                }
                c
            }
        };
        save(
            &tx,
            &c,
            operator(&m.actor),
            &m.idempotency_key,
            &now,
            &m.reason,
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &c)?;
        tx.commit()?;
        Ok(c)
    }
    pub fn record_gate(&self, case_id: &str, cmd: GateCommand) -> Result<Gate> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        let fp = store::digest(&format!("gate:{case_id}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let case = store::case(&tx, case_id)?;
        if case.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale gate version".into()));
        }
        ensure_clear(&tx, case_id)?;
        let complete: bool = tx.query_row(
            "SELECT state='completed' FROM assignments WHERE case_id=?1",
            [case_id],
            |r| r.get(0),
        )?;
        if !complete {
            return Err(Error::Conflict(
                "investigation must complete before validation".into(),
            ));
        }
        crate::qualification::evidence_refs(&tx, case_id, &cmd.evidence_refs, true)?;
        let g = if cmd.close {
            let mut g = gate(&tx, case_id)?
                .ok_or_else(|| Error::Conflict("independent validation required".into()))?;
            if m.actor != case.owner
                || g.validated_version + 1 != case.version
                || g.closed_version.is_some()
            {
                return Err(Error::Conflict(
                    "closure requires current independent validation and accountable owner".into(),
                ));
            }
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM obligations o JOIN dispositions d ON d.id=o.disposition_id WHERE d.case_id=?1 AND o.state IN ('pending','due'))",[case_id],|r|r.get(0))?;
            if pending {
                return Err(Error::Conflict(
                    "open review obligations prevent closure".into(),
                ));
            }
            g.closed_version = Some(case.version + 1);
            g
        } else {
            if m.actor != "fake-verifier" || m.actor == case.owner {
                return Err(Error::Invalid(
                    "separate fake verifier responsibility required".into(),
                ));
            }
            let independent:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE case_id=?1 AND json_extract(envelope,'$.type')='evidence.recorded' AND json_extract(envelope,'$.actor.id')=?2 AND json_extract(envelope,'$.payload.evidence_refs[0]') IN (SELECT value FROM json_each(?3)))",params![case_id,m.actor,serde_json::to_string(&cmd.evidence_refs)?],|r|r.get(0))?;
            if !independent {
                return Err(Error::Invalid(
                    "validation requires verifier observation".into(),
                ));
            }
            Gate {
                validated_version: case.version,
                verifier: m.actor.clone(),
                evidence_refs: cmd.evidence_refs.clone(),
                closed_version: None,
            }
        };
        tx.execute("INSERT INTO case_gates VALUES (?1,?2,?3,?4,?5) ON CONFLICT(case_id) DO UPDATE SET validated_version=excluded.validated_version,verifier=excluded.verifier,evidence_refs=excluded.evidence_refs,closed_version=excluded.closed_version",params![case_id,g.validated_version,g.verifier,serde_json::to_string(&g.evidence_refs)?,g.closed_version])?;
        store::emit(
            &tx,
            case_id,
            operator(&m.actor),
            &m.idempotency_key,
            &self.now(),
            EventData::GateRecorded {
                gate: if cmd.close { "closed" } else { "validated" }.into(),
                evidence_refs: cmd.evidence_refs,
                reason: m.reason.clone(),
            },
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &g)?;
        tx.commit()?;
        Ok(g)
    }
}
