//! Application commands for operator evidence, dispositions, and durable review timers.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    domain::{Case, CaseStatus},
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::HashSet;

fn metadata(key: &str, actor: &str, reason: &str) -> Result<()> {
    text(key, "idempotency_key", 128)?;
    text(actor, "actor", 128)?;
    text(reason, "reason", 2000)
}
fn expected(tx: &Transaction<'_>, case_id: &str, version: i64) -> Result<Case> {
    let case = store::case(tx, case_id)?;
    if case.version != version {
        return Err(Error::Conflict(format!(
            "expected version {version}, current version {}; reload before retrying",
            case.version
        )));
    }
    Ok(case)
}
pub(crate) fn evidence_refs(
    tx: &Connection,
    case_id: &str,
    refs: &[String],
    reported: bool,
) -> Result<()> {
    if refs.is_empty() || refs.len() > 20 || refs.iter().collect::<HashSet<_>>().len() != refs.len()
    {
        return Err(Error::Invalid(
            "supply 1..20 unique case evidence digests".into(),
        ));
    }
    let mut has_reported = false;
    for digest in refs {
        if !crate::retention::usable(tx, digest)? {
            return Err(Error::Conflict(
                "evidence was redacted; collect new proof".into(),
            ));
        }
        let row:Option<(String,String)>=tx.query_row("SELECT e.provenance,e.content FROM evidence e JOIN case_evidence c ON c.digest=e.digest WHERE c.case_id=?1 AND c.digest=?2",params![case_id,digest],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((provenance, content)) = row else {
            return Err(Error::Invalid("evidence must belong to this case".into()));
        };
        if store::digest(&content) != *digest {
            return Err(Error::Conflict("evidence integrity check failed".into()));
        }
        has_reported |= provenance == "operator-reported-v1";
    }
    if reported && !has_reported {
        return Err(Error::Invalid("this decision requires operator-reported evidence; synthetic results alone cannot support it".into()));
    }
    Ok(())
}
fn disposition(db: &Connection, id: &str) -> Result<Disposition> {
    let (case_id,recorded_at,due_at,state,count,record):(String,String,String,String,u32,String)=db.query_row("SELECT case_id,recorded_at,due_at,state,occurrence_count,record FROM dispositions WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?.ok_or(Error::NotFound)?;
    Ok(Disposition {
        schema_version: V1,
        id: id.into(),
        case_id,
        recorded_at,
        due_at,
        state: serde_json::from_value(serde_json::json!(state))?,
        occurrence_count: count,
        record: serde_json::from_str(&record)?,
    })
}
pub(crate) fn snapshot(db: &Connection, case_id: &str) -> Result<QualificationSnapshot> {
    let mut stmt = db.prepare("SELECT id FROM dispositions WHERE case_id=?1 ORDER BY rowid")?;
    let ids = stmt
        .query_map([case_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let dispositions = ids
        .iter()
        .map(|id| disposition(db, id))
        .collect::<Result<Vec<_>>>()?;
    let mut stmt=db.prepare("SELECT o.id,o.disposition_id,o.owner,o.task,o.due_at,o.state,o.triggered_at FROM obligations o JOIN dispositions d ON d.id=o.disposition_id WHERE d.case_id=?1 ORDER BY o.rowid")?;
    let rows = stmt.query_map([case_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;
    let obligations = rows
        .map(|r| {
            let (id, disposition_id, owner, task, due_at, state, triggered_at) = r?;
            Ok(Obligation {
                schema_version: V1,
                id,
                disposition_id,
                owner,
                task,
                due_at,
                state: serde_json::from_value(serde_json::json!(state))?,
                triggered_at,
            })
        })
        .collect::<Result<_>>()?;
    let mut stmt=db.prepare("SELECT e.digest,e.provenance,e.recorded_at,CASE WHEN EXISTS(SELECT 1 FROM evidence_redactions r WHERE r.digest=e.digest) THEN json_object('statement','Evidence redacted; original proof unavailable') ELSE e.content END FROM evidence e JOIN case_evidence c ON c.digest=e.digest WHERE c.case_id=?1 ORDER BY e.recorded_at,e.digest")?;
    let evidence = stmt
        .query_map([case_id], |r| {
            Ok(EvidenceReference {
                digest: r.get(0)?,
                provenance: r.get(1)?,
                recorded_at: r.get(2)?,
                summary: serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?)
                    .ok()
                    .and_then(|v| {
                        v.get("statement")
                            .or_else(|| v.get("summary"))
                            .or_else(|| v.get("checkpoint"))
                            .or_else(|| v.get("details").and_then(|d| d.get("checkpoint")))
                            .or_else(|| v.get("result").and_then(|r| r.get("summary")))
                            .and_then(|s| s.as_str())
                            .map(str::to_owned)
                    })
                    .unwrap_or_default(),
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(QualificationSnapshot {
        dispositions,
        obligations,
        evidence,
    })
}
/// Reopening makes the existing review/collection obligation due, without queuing execution.
fn reopen(
    tx: &Transaction<'_>,
    d: &Disposition,
    actor: Actor,
    cause: &str,
    now: &str,
    refs: Vec<String>,
    reason: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE dispositions SET state='reopened' WHERE id=?1 AND state='active'",
        [&d.id],
    )?;
    tx.execute("UPDATE obligations SET state='due',triggered_at=?1 WHERE disposition_id=?2 AND state='pending'",params![now,d.id])?;
    store::emit(
        tx,
        &d.case_id,
        actor.clone(),
        cause,
        now,
        EventData::CaseReopened {
            disposition_id: d.id.clone(),
            evidence_refs: refs,
            reason: reason.into(),
        },
    )?;
    let obligation_id: String = tx.query_row(
        "SELECT id FROM obligations WHERE disposition_id=?1",
        [&d.id],
        |r| r.get(0),
    )?;
    store::emit(
        tx,
        &d.case_id,
        actor,
        cause,
        now,
        EventData::ObligationDue {
            obligation_id,
            owner: d.record.owner.clone(),
            reason: reason.into(),
        },
    )
}
impl Application {
    pub fn record_evidence(&self, case_id: &str, cmd: RecordEvidence) -> Result<RecordedEvidence> {
        metadata(&cmd.idempotency_key, &cmd.actor, &cmd.reason)?;
        for (value, name, max) in [
            (&cmd.scope, "scope", 2000),
            (&cmd.statement, "statement", 8000),
            (&cmd.collection_method, "collection_method", 2000),
            (&cmd.uncertainty, "uncertainty", 2000),
        ] {
            text(value, name, max)?;
        }
        if let Some(time) = &cmd.observed_at {
            let time = DateTime::parse_from_rfc3339(time)
                .map_err(|_| Error::Invalid("observed_at must be RFC3339 or null".into()))?;
            if time > self.clock.now() {
                return Err(Error::Invalid("observed_at cannot be in the future".into()));
            }
        }
        let content = serde_json::to_string(&cmd)?;
        let fingerprint = store::digest(&format!("evidence:{case_id}:{content}"));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.idempotency_key, &fingerprint)? {
            return Ok(response);
        }
        expected(&tx, case_id, cmd.expected_version)?;
        let digest = store::digest(&content);
        let now = self.now();
        tx.execute("INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,'operator-reported-v1',?3)",params![digest,content,now])?;
        tx.execute(
            "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
            params![case_id, digest],
        )?;
        store::emit(
            &tx,
            case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::EvidenceRecorded {
                evidence_refs: vec![digest.clone()],
                provenance: "operator-reported-v1".into(),
                scope: cmd.scope,
                reason: cmd.reason,
            },
        )?;
        let response = RecordedEvidence {
            case: store::case(&tx, case_id)?,
            evidence: EvidenceReference {
                digest,
                provenance: "operator-reported-v1".into(),
                recorded_at: now,
                summary: cmd.statement,
            },
        };
        save_receipt(&tx, &cmd.idempotency_key, &fingerprint, &response)?;
        tx.commit()?;
        Ok(response)
    }
    pub fn record_disposition(&self, case_id: &str, cmd: RecordDisposition) -> Result<Case> {
        metadata(&cmd.idempotency_key, &cmd.actor, &cmd.reason)?;
        for (value, name, max) in [
            (&cmd.owner, "owner", 128),
            (&cmd.scope, "scope", 2000),
            (&cmd.uncertainty, "uncertainty", 2000),
            (
                &cmd.reconsideration_conditions,
                "reconsideration_conditions",
                2000,
            ),
        ] {
            text(value, name, max)?;
        }
        if !(1..=MAX_REVIEW_SECONDS).contains(&cmd.plan.after_seconds()) {
            return Err(Error::Invalid(format!(
                "after_seconds must be 1..{MAX_REVIEW_SECONDS}"
            )));
        }
        if !(1..=1000).contains(&cmd.reopen_after_occurrences) {
            return Err(Error::Invalid(
                "reopen_after_occurrences must be 1..1000".into(),
            ));
        }
        match &cmd.plan {
            QualificationPlan::ApparentlyTransient {
                recovery_indicators,
                ..
            } => text(recovery_indicators, "recovery_indicators", 2000)?,
            QualificationPlan::InsufficientEvidence {
                collection_task, ..
            } => text(collection_task, "collection_task", 2000)?,
            QualificationPlan::Duplicate {
                target_case,
                relation_revision,
                ..
            } => {
                text(target_case, "target_case", 128)?;
                if *relation_revision == 0 {
                    return Err(Error::Invalid(
                        "duplicate requires an accepted correlation revision".into(),
                    ));
                }
            }
            QualificationPlan::AlreadyAddressed {
                change_reference,
                verification_task,
                ..
            } => {
                text(change_reference, "change_reference", 2000)?;
                text(verification_task, "verification_task", 2000)?;
            }
            QualificationPlan::ExternalDependency {
                dependency,
                dependency_owner,
                status,
                recovery_tracking,
                resilience_task,
                ..
            } => {
                for (v, n) in [
                    (dependency, "dependency"),
                    (dependency_owner, "dependency_owner"),
                    (status, "dependency status"),
                    (recovery_tracking, "recovery_tracking"),
                    (resilience_task, "resilience_task"),
                ] {
                    text(v, n, 2000)?;
                }
            }
            QualificationPlan::OperationalConfiguration {
                workflow,
                permitted_action_reference,
                ..
            } => {
                text(workflow, "workflow", 2000)?;
                text(
                    permitted_action_reference,
                    "permitted_action_reference",
                    2000,
                )?;
            }
            QualificationPlan::ApplicationDefect {
                basis,
                repair_assessment,
                ..
            } => {
                text(repair_assessment, "repair_assessment", 2000)?;
                match basis {
                    DefectBasis::EvidenceGap { collection_task } => {
                        text(collection_task, "defect collection_task", 2000)?
                    }
                    DefectBasis::AcceptedHypothesis { id, revision } => {
                        text(id, "hypothesis id", 128)?;
                        if *revision == 0 {
                            return Err(Error::Invalid("hypothesis revision required".into()));
                        }
                    }
                }
            }
            _ => {}
        }
        let record = serde_json::to_string(&cmd)?;
        let fingerprint = store::digest(&format!("disposition:{case_id}:{record}"));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.idempotency_key, &fingerprint)? {
            return Ok(response);
        }
        let case = expected(&tx, case_id, cmd.expected_version)?;
        if cmd.plan.after_seconds() > crate::policy::for_case(&tx, case_id)?.max_review_seconds {
            return Err(Error::Invalid(
                "review timer exceeds governing policy".into(),
            ));
        }
        crate::challenges::ensure_clear(&tx, case_id)?;
        if case.status != CaseStatus::AwaitingReview {
            return Err(Error::Conflict(
                "investigation must finish before an operator records a disposition".into(),
            ));
        }
        evidence_refs(
            &tx,
            case_id,
            &cmd.evidence_refs,
            cmd.plan.needs_reported_evidence(),
        )?;
        match &cmd.plan {
            QualificationPlan::Duplicate{target_case,relation_revision,..} if !crate::correlation::duplicate_current(&tx,case_id,target_case,*relation_revision)? =>return Err(Error::Conflict("duplicate disposition requires an exact current accepted duplicate relation; linking never closes either case".into())),
            QualificationPlan::ApplicationDefect{basis:DefectBasis::AcceptedHypothesis{id,revision},..}=>{
                let artifacts=crate::incident_artifacts::snapshot(&tx,case_id)?;
                let a=artifacts.iter().find(|a|a.id==*id && a.revision==*revision && a.approval_current).ok_or_else(||Error::Conflict("exact current independently accepted hypothesis required".into()))?;
                match &a.data {
                    crate::incident_artifacts::ArtifactData::Hypothesis{state:crate::incident_artifacts::Finding::Supported,supporting,..} if supporting.iter().all(|e|cmd.evidence_refs.contains(e))=>{},
                    _=>return Err(Error::Conflict("defect basis needs a supported hypothesis and its cited evidence".into())),
                }
            },
            _=>{}
        }
        let supersedes: Option<String> = tx
            .query_row(
                "SELECT id FROM dispositions WHERE case_id=?1 AND state!='superseded'",
                [case_id],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute("UPDATE obligations SET state='superseded' WHERE disposition_id IN (SELECT id FROM dispositions WHERE case_id=?1 AND state!='superseded')",[case_id])?;
        tx.execute(
            "UPDATE dispositions SET state='superseded' WHERE case_id=?1 AND state!='superseded'",
            [case_id],
        )?;
        let now = self.now();
        let due_at = (self.clock.now() + Duration::seconds(cmd.plan.after_seconds().into()))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let d = Disposition {
            schema_version: V1,
            id: store::id("dsp"),
            case_id: case_id.into(),
            recorded_at: now.clone(),
            due_at: due_at.clone(),
            state: DispositionState::Active,
            occurrence_count: 0,
            record: cmd.clone(),
        };
        let obligation = Obligation {
            schema_version: V1,
            id: store::id("obl"),
            disposition_id: d.id.clone(),
            owner: cmd.owner.clone(),
            task: cmd.plan.review_task(),
            due_at,
            state: ObligationState::Pending,
            triggered_at: None,
        };
        tx.execute("INSERT INTO dispositions(id,case_id,recorded_at,due_at,state,record) VALUES (?1,?2,?3,?4,'active',?5)",params![d.id,case_id,now,d.due_at,record])?;
        tx.execute(
            "INSERT INTO obligations VALUES (?1,?2,?3,?4,?5,'pending',NULL)",
            params![
                obligation.id,
                d.id,
                cmd.owner,
                obligation.task,
                obligation.due_at
            ],
        )?;
        tx.execute(
            "UPDATE cases SET owner=?1 WHERE id=?2",
            params![cmd.owner, case_id],
        )?;
        store::emit(
            &tx,
            case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::DispositionRecorded {
                disposition: Box::new(d),
                supersedes,
                reason: cmd.reason.clone(),
            },
        )?;
        store::emit(
            &tx,
            case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::ObligationScheduled {
                obligation,
                reason:
                    "An accountable operator must reassess this decision by the recorded deadline"
                        .into(),
            },
        )?;
        let response = store::case(&tx, case_id)?;
        save_receipt(&tx, &cmd.idempotency_key, &fingerprint, &response)?;
        tx.commit()?;
        Ok(response)
    }
    pub fn reconsider_case(&self, case_id: &str, cmd: ReconsiderCase) -> Result<Case> {
        metadata(&cmd.idempotency_key, &cmd.actor, &cmd.reason)?;
        let fingerprint = store::digest(&format!(
            "reconsider:{case_id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.idempotency_key, &fingerprint)? {
            return Ok(response);
        }
        expected(&tx, case_id, cmd.expected_version)?;
        let mut d = disposition(&tx, &cmd.disposition_id)?;
        if d.case_id != case_id || d.state != DispositionState::Active {
            return Err(Error::Conflict(
                "disposition is not the current active decision".into(),
            ));
        }
        evidence_refs(
            &tx,
            case_id,
            &cmd.evidence_refs,
            cmd.trigger != ReconsiderTrigger::OperatorReview,
        )?;
        if cmd.trigger == ReconsiderTrigger::Recurrence {
            // The same evidence cannot count as a new recurrence under another command key.
            for digest in &cmd.evidence_refs {
                let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM reconsideration_evidence WHERE disposition_id=?1 AND digest=?2)",params![d.id,digest],|r|r.get(0))?;
                let fresh: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM evidence WHERE digest=?1 AND provenance='operator-reported-v1' AND json_extract(content,'$.expected_version')>=?2)",
                    params![digest,d.record.expected_version+2],|r|r.get(0))?;
                if used || !fresh || d.record.evidence_refs.contains(digest) {
                    return Err(Error::Conflict("recurrence requires operator evidence recorded after the decision, not already counted or used for this decision".into()));
                }
                tx.execute(
                    "INSERT INTO reconsideration_evidence VALUES (?1,?2)",
                    params![d.id, digest],
                )?;
            }
            d.occurrence_count += 1;
            tx.execute(
                "UPDATE dispositions SET occurrence_count=?1 WHERE id=?2",
                params![d.occurrence_count, d.id],
            )?;
        }
        let now = self.now();
        store::emit(
            &tx,
            case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::DispositionReconsidered {
                disposition_id: d.id.clone(),
                trigger: cmd.trigger,
                occurrence_count: d.occurrence_count,
                evidence_refs: cmd.evidence_refs.clone(),
                reason: cmd.reason.clone(),
            },
        )?;
        if cmd
            .trigger
            .reopens(d.occurrence_count, d.record.reopen_after_occurrences)
        {
            reopen(
                &tx,
                &d,
                operator(&cmd.actor),
                &cmd.idempotency_key,
                &now,
                cmd.evidence_refs,
                &cmd.reason,
            )?;
        }
        let response = store::case(&tx, case_id)?;
        save_receipt(&tx, &cmd.idempotency_key, &fingerprint, &response)?;
        tx.commit()?;
        Ok(response)
    }
    /// A bounded deterministic supervisor tick. Persisted obligations survive all process loss.
    pub fn process_due_obligations(&self) -> Result<usize> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = self.now();
        let ids: Vec<String> = {
            let mut stmt=tx.prepare("SELECT d.id FROM dispositions d JOIN obligations o ON o.disposition_id=d.id WHERE d.state='active' AND o.state='pending' AND (o.due_at<=?1 OR (json_extract(d.record,'$.plan.kind')='duplicate' AND NOT EXISTS (SELECT 1 FROM case_correlations r WHERE r.left_case=MIN(d.case_id,json_extract(d.record,'$.plan.target_case')) AND r.right_case=MAX(d.case_id,json_extract(d.record,'$.plan.target_case')) AND r.revision=json_extract(d.record,'$.plan.relation_revision') AND r.revision=(SELECT MAX(x.revision) FROM case_correlations x WHERE x.left_case=r.left_case AND x.right_case=r.right_case) AND json_extract(r.record,'$.active')=1 AND json_extract(r.record,'$.kind')='duplicate' AND json_extract(r.record,'$.source_digest')=(SELECT digest FROM source_cases WHERE case_id=json_extract(r.record,'$.source_case')) AND json_extract(r.record,'$.target_digest')=(SELECT digest FROM source_cases WHERE case_id=json_extract(r.record,'$.target_case'))))) ORDER BY o.due_at,o.id LIMIT 100")?;
            stmt.query_map([&now], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        for id in &ids {
            let d = disposition(&tx, id)?;
            reopen(
                &tx,
                &d,
                Actor {
                    kind: "supervisor".into(),
                    id: "qualification-timer".into(),
                },
                id,
                &now,
                d.record.evidence_refs.clone(),
                if d.due_at <= now {
                    "Disposition deadline reached; evidence and current scope require operator reassessment. Recovery remains unknown"
                } else {
                    "Accepted duplicate relation was revoked, superseded or its source evidence changed; reassess without transferring or closing either case"
                },
            )?;
        }
        tx.commit()?;
        Ok(ids.len())
    }
}
