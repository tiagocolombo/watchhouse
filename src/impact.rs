//! Explicit reported impact/denominators; zero traffic and unknown coverage cannot prove health.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub start: String,
    pub end: String,
    pub affected_requests: Option<u64>,
    pub total_requests: Option<u64>,
    pub affected_users: Option<u64>,
    pub total_users: Option<u64>,
    pub instrumentation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assess {
    pub command: OperatorCommand,
    pub expected_revision: u32,
    pub service: String,
    pub environment: String,
    pub scope: String,
    pub window: Option<Window>,
    pub critical_workflow: String,
    pub impact_statement: String,
    pub uncertainty: String,
    pub collection_owner: String,
    pub collection_task: String,
    pub review_seconds: u32,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub revision: u32,
    pub recorded_at: String,
    pub review_at: String,
    pub due: bool,
    pub provenance: String,
    pub source_digest: Option<String>,
    pub context_current: bool,
    pub observation: String,
    pub record: Assess,
}
fn source_digest(db: &Connection, case: &str) -> Result<Option<String>> {
    Ok(db
        .query_row(
            "SELECT digest FROM source_cases WHERE case_id=?1",
            [case],
            |r| r.get(0),
        )
        .optional()?)
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<Assessment>> {
    let mut q=db.prepare("SELECT record,due FROM impact_assessments WHERE case_id=?1 ORDER BY revision DESC LIMIT 100")?;
    let digest = source_digest(db, case)?;
    q.query_map([case], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
    })?
    .map(|r| {
        let (raw, due) = r?;
        let mut a: Assessment = serde_json::from_str(&raw)?;
        a.due = due;
        a.context_current = a.source_digest == digest;
        Ok(a)
    })
    .collect()
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut q=tx.prepare("SELECT record FROM impact_assessments a WHERE a.due=0 AND a.revision=(SELECT MAX(b.revision) FROM impact_assessments b WHERE b.case_id=a.case_id) AND (a.review_at<=?1 OR COALESCE(json_extract(a.record,'$.source_digest'),'')!=COALESCE((SELECT digest FROM source_cases WHERE case_id=a.case_id),'')) ORDER BY a.review_at LIMIT 100")?;
    let rows = q
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for raw in &rows {
        let mut a: Assessment = serde_json::from_str(raw)?;
        a.due = true;
        tx.execute("UPDATE impact_assessments SET due=1 WHERE id=?1", [&a.id])?;
        let reason = format!(
            "{} owns impact collection: {}. Review deadline or source context changed; unknown/zero traffic does not prove recovery",
            a.record.collection_owner, a.record.collection_task
        );
        crate::team::escalate(tx, &a.case_id, "impact_review", &a.id, &reason, now)?;
        store::emit(
            tx,
            &a.case_id,
            operator("impact-supervisor"),
            &a.id,
            now,
            EventData::ImpactChanged {
                assessment_id: a.id.clone(),
                revision: a.revision,
                reason,
            },
        )?;
    }
    Ok(rows.len())
}
impl Application {
    pub fn assess_impact(&self, case: &str, cmd: Assess) -> Result<Assessment> {
        crate::incident_ops::human(&cmd.command.meta.actor)?;
        for (v, n, max) in [
            (&cmd.command.meta.idempotency_key, "idempotency key", 128),
            (&cmd.command.meta.reason, "reason", 2000),
            (&cmd.service, "service", 128),
            (&cmd.environment, "environment", 128),
            (&cmd.scope, "impact scope", 2000),
            (&cmd.critical_workflow, "critical_workflow", 2000),
            (&cmd.impact_statement, "impact_statement", 4000),
            (&cmd.uncertainty, "uncertainty", 2000),
            (&cmd.collection_owner, "collection_owner", 128),
            (&cmd.collection_task, "collection_task", 2000),
        ] {
            text(v, n, max)?;
        }
        if !(1..=crate::domain::qualification::MAX_REVIEW_SECONDS).contains(&cmd.review_seconds) {
            return Err(Error::Invalid(
                "bounded impact review interval required".into(),
            ));
        }
        if let Some(w) = &cmd.window {
            text(&w.instrumentation, "instrumentation coverage", 2000)?;
            let start = DateTime::parse_from_rfc3339(&w.start)
                .map_err(|_| Error::Invalid("impact start must be RFC3339".into()))?;
            let end = DateTime::parse_from_rfc3339(&w.end)
                .map_err(|_| Error::Invalid("impact end must be RFC3339".into()))?;
            if start >= end || end > self.clock.now() || end - start > Duration::days(30) {
                return Err(Error::Invalid(
                    "impact window must be ordered, historical and no longer than 30 days".into(),
                ));
            }
            for (affected, total) in [
                (w.affected_requests, w.total_requests),
                (w.affected_users, w.total_users),
            ] {
                if affected.is_some_and(|n| n > 9_007_199_254_740_991)
                    || total.is_some_and(|n| n > 9_007_199_254_740_991)
                    || affected.zip(total).is_some_and(|(a, t)| a > t)
                {
                    return Err(Error::Invalid(
                        "impact count exceeds its denominator or supported bound".into(),
                    ));
                }
            }
        }
        let fingerprint = store::digest(&format!("impact:{case}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(a) = receipt(&tx, &cmd.command.meta.idempotency_key, &fingerprint)? {
            return Ok(a);
        }
        let c = store::case(&tx, case)?;
        let commander = crate::incident_ops::snapshot(&tx, case)?.is_some_and(|i| {
            i.roles.commander == cmd.command.meta.actor
                || i.roles.operations == cmd.command.meta.actor
        });
        if c.version != cmd.command.expected_version
            || (c.owner != cmd.command.meta.actor && !commander)
        {
            return Err(Error::Conflict(
                "current case owner or incident operations owner/version required".into(),
            ));
        }
        let latest: u32 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM impact_assessments WHERE case_id=?1",
            [case],
            |r| r.get(0),
        )?;
        if latest != cmd.expected_revision {
            return Err(Error::Conflict("stale impact assessment revision".into()));
        }
        let revision = latest
            .checked_add(1)
            .ok_or_else(|| Error::Conflict("impact revision exhausted".into()))?;
        if cmd.review_seconds > crate::policy::for_case(&tx, case)?.max_review_seconds {
            return Err(Error::Invalid(
                "impact review exceeds governing policy".into(),
            ));
        }
        crate::qualification::evidence_refs(&tx, case, &cmd.evidence_refs, true)?;
        crate::challenges::ensure_clear(&tx, case)?;
        if let Some(s) = crate::sentry::snapshot(&tx, case)?
            && (s.service != cmd.service || s.environment != cmd.environment)
        {
            return Err(Error::Conflict(
                "impact scope differs from configured source mapping".into(),
            ));
        }
        let observation = match &cmd.window {
            None => "unknown_window",
            Some(w) if w.total_requests == Some(0) => "no_request_traffic",
            Some(w) if w.total_requests.is_none() || w.affected_requests.is_none() => {
                "unknown_request_denominator_or_impact"
            }
            Some(_) => "reported_request_counts",
        }
        .to_owned();
        let a = Assessment {
            schema_version: V1,
            id: store::id("impact"),
            case_id: case.into(),
            revision,
            recorded_at: self.now(),
            review_at: (self.clock.now() + Duration::seconds(cmd.review_seconds.into()))
                .to_rfc3339_opts(SecondsFormat::Millis, true),
            due: false,
            provenance: "operator-reported-impact-v1".into(),
            source_digest: source_digest(&tx, case)?,
            context_current: true,
            observation,
            record: cmd,
        };
        tx.execute("INSERT INTO impact_assessments(id,case_id,revision,review_at,record) VALUES (?1,?2,?3,?4,?5)",params![a.id,case,revision,a.review_at,serde_json::to_string(&a)?])?;
        store::emit(
            &tx,
            case,
            operator(&a.record.command.meta.actor),
            &a.record.command.meta.idempotency_key,
            &a.recorded_at,
            EventData::ImpactChanged {
                assessment_id: a.id.clone(),
                revision,
                reason: a.record.command.meta.reason.clone(),
            },
        )?;
        save_receipt(
            &tx,
            &a.record.command.meta.idempotency_key,
            &fingerprint,
            &a,
        )?;
        tx.commit()?;
        Ok(a)
    }
}
