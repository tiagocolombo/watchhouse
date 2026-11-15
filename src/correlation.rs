//! Reversible evidence-backed links; suggestions never merge cases or transfer duties.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    sentry::SourceObservation,
    store,
};
use chrono::DateTime;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Related,
    Duplicate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RelationAction {
    Link {
        kind: RelationKind,
        source_evidence: Vec<String>,
        target_evidence: Vec<String>,
        uncertainty: String,
    },
    Revoke,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationCommand {
    pub command: OperatorCommand,
    pub target_case: String,
    pub expected_target_version: i64,
    pub expected_revision: u32,
    pub action: RelationAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub schema_version: V1,
    pub source_case: String,
    pub target_case: String,
    pub revision: u32,
    pub kind: RelationKind,
    pub active: bool,
    pub context_current: bool,
    pub source_digest: String,
    pub target_digest: String,
    pub source_evidence: Vec<String>,
    pub target_evidence: Vec<String>,
    pub actor: String,
    pub reason: String,
    pub uncertainty: String,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub case_id: String,
    pub case_version: i64,
    pub source_digest: String,
    pub target_digest: String,
    pub signature: String,
    pub reason: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Correlations {
    pub relations: Vec<Relation>,
    pub candidates: Vec<Candidate>,
    pub limitation: String,
}
fn pair<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    if a < b { (a, b) } else { (b, a) }
}
fn latest(db: &Connection, a: &str, b: &str) -> Result<Option<Relation>> {
    let (left, right) = pair(a, b);
    let raw:Option<String>=db.query_row("SELECT record FROM case_correlations WHERE left_case=?1 AND right_case=?2 ORDER BY revision DESC LIMIT 1",params![left,right],|r|r.get(0)).optional()?;
    raw.map(|v| Ok(serde_json::from_str(&v)?)).transpose()
}
fn digest(db: &Connection, id: &str) -> Result<String> {
    db.query_row(
        "SELECT digest FROM source_cases WHERE case_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn scope(a: &SourceObservation, b: &SourceObservation) -> bool {
    a.service == b.service && a.environment == b.environment && a.repository == b.repository
}
fn signature(o: &SourceObservation) -> Option<String> {
    let d = o.diagnostic.as_ref()?;
    if !o.authoritative
        || o.exceptions.is_empty()
        || d.frames.is_empty()
        || o.release.is_none()
        || o.deployed_revision.is_none()
    {
        return None;
    }
    // Frame order is meaningful. Issue title, HTTP status, count and provider grouping are not signatures.
    Some(store::digest(
        &serde_json::to_string(&(&o.exceptions, &d.frames)).ok()?,
    ))
}
fn current(db: &Connection, r: &Relation) -> Result<bool> {
    Ok(r.active
        && digest(db, &r.source_case)? == r.source_digest
        && digest(db, &r.target_case)? == r.target_digest)
}
pub(crate) fn snapshot(
    db: &Connection,
    id: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Correlations> {
    let mut stmt=db.prepare("SELECT c.record FROM case_correlations c WHERE (c.left_case=?1 OR c.right_case=?1) AND c.revision=(SELECT MAX(x.revision) FROM case_correlations x WHERE x.left_case=c.left_case AND x.right_case=c.right_case) ORDER BY c.rowid DESC LIMIT 100")?;
    let mut relations = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str::<Relation>(&r?)?))
        .collect::<Result<Vec<_>>>()?;
    for r in &mut relations {
        r.context_current = current(db, r)?;
    }
    let mut result=Correlations {relations,candidates:vec![],limitation:"Suggestions only: at most 20 matches from the newest 200 same-scope source cases; exact mapped release/revision, diagnostic signature and occurrence within one hour required. Current successful read required for both. Not exhaustive, causal evidence, authorization or recovery. Links keep all cases, incidents and duties separate; changed facts require renewed review. Latest 100 links shown; full revisions remain in events.".into()};
    let Some(source) = crate::sentry::snapshot(db, id)? else {
        return Ok(result);
    };
    let Some(sig) = signature(&source) else {
        return Ok(result);
    };
    if crate::sentry_refresh::snapshot(db, id, now)?.is_none_or(|h| h.state != "current_snapshot") {
        return Ok(result);
    }
    let source_digest = digest(db, id)?;
    let mut stmt=db.prepare("SELECT case_id,record,digest FROM source_cases WHERE case_id!=?1 AND json_extract(record,'$.service')=?2 AND json_extract(record,'$.environment')=?3 AND json_extract(record,'$.repository')=?4 ORDER BY rowid DESC LIMIT 200")?;
    let rows = stmt
        .query_map(
            params![id, source.service, source.environment, source.repository],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (target, raw, target_digest) in rows {
        let other: SourceObservation = serde_json::from_str(&raw)?;
        if !scope(&source, &other)
            || source.release != other.release
            || source.deployed_revision != other.deployed_revision
            || signature(&other).as_ref() != Some(&sig)
            || result
                .relations
                .iter()
                .any(|r| r.active && (r.source_case == target || r.target_case == target))
        {
            continue;
        }
        let gap = (DateTime::parse_from_rfc3339(&source.signal.last_seen)
            .map_err(|_| Error::Invalid("invalid source time".into()))?
            - DateTime::parse_from_rfc3339(&other.signal.last_seen)
                .map_err(|_| Error::Invalid("invalid source time".into()))?)
        .num_seconds()
        .abs();
        if gap > 3600
            || crate::sentry_refresh::snapshot(db, &target, now)?
                .is_none_or(|h| h.state != "current_snapshot")
        {
            continue;
        }
        result.candidates.push(Candidate {case_version:store::case(db,&target)?.version,case_id:target,source_digest:source_digest.clone(),target_digest,signature:sig.clone(),reason:"Same service/environment/repository, mapped release/revision and bounded exception/frame signature; last occurrences within one hour. Operator must assess whether these represent the same failure.".into()});
        if result.candidates.len() == 20 {
            break;
        }
    }
    Ok(result)
}
impl Application {
    pub fn correlate_case(&self, id: &str, cmd: RelationCommand) -> Result<Relation> {
        let m = &cmd.command.meta;
        text(&m.actor, "actor", 128)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "idempotency_key", 128)?;
        text(&cmd.target_case, "target_case", 128)?;
        if id == cmd.target_case {
            return Err(Error::Invalid("cannot correlate a case with itself".into()));
        }
        let fp = store::digest(&format!("correlate:{id}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let source = store::case(&tx, id)?;
        let target = store::case(&tx, &cmd.target_case)?;
        if source.version != cmd.command.expected_version
            || target.version != cmd.expected_target_version
        {
            return Err(Error::Conflict(
                "reload both cases before correlating".into(),
            ));
        }
        if source.owner != m.actor || target.owner != m.actor || m.actor.starts_with("fake-") {
            return Err(Error::Invalid(
                "accountable trusted operator must own both cases".into(),
            ));
        }
        let previous = latest(&tx, id, &target.id)?;
        if previous.as_ref().map_or(0, |r| r.revision) != cmd.expected_revision {
            return Err(Error::Conflict("stale correlation revision".into()));
        }
        let revision = cmd
            .expected_revision
            .checked_add(1)
            .ok_or_else(|| Error::Conflict("correlation revision exhausted".into()))?;
        let mut record = match &cmd.action {
            RelationAction::Link {
                kind,
                source_evidence,
                target_evidence,
                uncertainty,
            } => {
                text(uncertainty, "uncertainty", 2000)?;
                crate::challenges::ensure_clear(&tx, id)?;
                crate::challenges::ensure_clear(&tx, &target.id)?;
                crate::qualification::evidence_refs(
                    &tx,
                    id,
                    source_evidence,
                    *kind == RelationKind::Duplicate,
                )?;
                crate::qualification::evidence_refs(
                    &tx,
                    &target.id,
                    target_evidence,
                    *kind == RelationKind::Duplicate,
                )?;
                let a = crate::sentry::snapshot(&tx, id)?
                    .ok_or_else(|| Error::Invalid("source case requires enrichment".into()))?;
                let b = crate::sentry::snapshot(&tx, &target.id)?
                    .ok_or_else(|| Error::Invalid("target case requires enrichment".into()))?;
                if !a.authoritative || !b.authoritative || !scope(&a, &b) {
                    return Err(Error::Invalid("link requires API evidence in the same configured service/environment/repository scope".into()));
                }
                let ad = digest(&tx, id)?;
                let bd = digest(&tx, &target.id)?;
                if !source_evidence.contains(&ad) || !target_evidence.contains(&bd) {
                    return Err(Error::Conflict(
                        "link must cite both current source snapshots".into(),
                    ));
                }
                Relation {
                    schema_version: V1,
                    source_case: id.into(),
                    target_case: target.id.clone(),
                    revision,
                    kind: *kind,
                    active: true,
                    context_current: true,
                    source_digest: ad,
                    target_digest: bd,
                    source_evidence: source_evidence.clone(),
                    target_evidence: target_evidence.clone(),
                    actor: m.actor.clone(),
                    reason: m.reason.clone(),
                    uncertainty: uncertainty.clone(),
                    recorded_at: self.now(),
                }
            }
            RelationAction::Revoke => {
                let mut r = previous.ok_or_else(|| Error::Conflict("no link to revoke".into()))?;
                if !r.active {
                    return Err(Error::Conflict("link already revoked".into()));
                }
                r.active = false;
                r.context_current = false;
                r.revision = revision;
                r.actor = m.actor.clone();
                r.reason = m.reason.clone();
                r.recorded_at = self.now();
                r
            }
        };
        record.context_current = current(&tx, &record)?;
        let (left, right) = pair(id, &target.id);
        tx.execute(
            "INSERT INTO case_correlations VALUES (?1,?2,?3,?4)",
            params![left, right, revision, serde_json::to_string(&record)?],
        )?;
        for case in [id, target.id.as_str()] {
            store::emit(
                &tx,
                case,
                operator(&m.actor),
                &m.idempotency_key,
                &self.now(),
                EventData::CaseCorrelated {
                    relation: Box::new(record.clone()),
                    reason: m.reason.clone(),
                },
            )?;
        }
        save_receipt(&tx, &m.idempotency_key, &fp, &record)?;
        tx.commit()?;
        Ok(record)
    }
    pub fn qualification_markdown(&self, id: &str) -> Result<String> {
        let s = self.snapshot(id)?;
        let mut report = format!(
            "# Case qualification {}\n\nAll classifications and links require scoped evidence. Source status and duplicate links do not prove recovery or close duties.\n\n## Source context\n\n",
            id
        );
        // Escape code-fence delimiters and markup; fields remain untrusted literal text.
        for (name, value) in [
            ("Source", serde_json::to_value(&s.source)?),
            ("Freshness", serde_json::to_value(&s.source_freshness)?),
            ("Qualification", serde_json::to_value(&s.qualification)?),
            ("Correlations", serde_json::to_value(&s.correlations)?),
        ] {
            let raw = serde_json::to_string_pretty(&value)?
                .replace('`', "\\u0060")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e");
            report.push_str(&format!("## {name}\n\n```json\n{raw}\n```\n\n"));
        }
        Ok(report)
    }
}
