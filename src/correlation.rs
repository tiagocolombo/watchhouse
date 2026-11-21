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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_topology_revision: Option<i64>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology_digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub case_id: String,
    pub case_version: i64,
    pub source_digest: String,
    pub target_digest: String,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology_revision: Option<i64>,
    pub reason: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Correlations {
    pub relations: Vec<Relation>,
    pub candidates: Vec<Candidate>,
    pub limitation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<crate::topology::View>,
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
        && crate::sentry_projects::context_current(db, &r.source_case)?
        && crate::sentry_projects::context_current(db, &r.target_case)?
        && crate::qualification::evidence_refs(db, &r.source_case, &r.source_evidence, false)
            .is_ok()
        && crate::qualification::evidence_refs(db, &r.target_case, &r.target_evidence, false)
            .is_ok()
        && digest(db, &r.source_case)? == r.source_digest
        && digest(db, &r.target_case)? == r.target_digest
        && match r.topology_revision {
            None => true,
            Some(rev) => crate::topology::active(db)?.is_some_and(|p| {
                !p.paused && p.revision == rev && r.topology_digest.as_ref() == Some(&p.digest)
            }),
        })
}
pub(crate) fn duplicate_current(
    db: &Connection,
    case: &str,
    target: &str,
    revision: u32,
) -> Result<bool> {
    let Some(r) = latest(db, case, target)? else {
        return Ok(false);
    };
    Ok(case != target
        && r.revision == revision
        && r.kind == RelationKind::Duplicate
        && current(db, &r)?)
}
fn append_cross_candidates(
    db: &Connection,
    id: &str,
    source: &SourceObservation,
    now: chrono::DateTime<chrono::Utc>,
    result: &mut Correlations,
) -> Result<()> {
    let Some(p) = result.topology.as_ref().filter(|p| !p.paused) else {
        return Ok(());
    };
    if !source.authoritative
        || source.release.is_none()
        || source.deployed_revision.is_none()
        || result.candidates.len() >= 20
        || crate::sentry_refresh::snapshot(db, id, now)?
            .is_none_or(|s| s.state != "current_snapshot")
    {
        return Ok(());
    }
    let neighbors: Vec<_> = p
        .dependencies
        .iter()
        .filter_map(|d| {
            if d.caller.matches(source) {
                Some(&d.dependency.service)
            } else if d.dependency.matches(source) {
                Some(&d.caller.service)
            } else {
                None
            }
        })
        .collect();
    if neighbors.is_empty() {
        return Ok(());
    }
    let mut q=db.prepare("SELECT case_id,record,digest FROM source_cases WHERE case_id!=?1 AND json_extract(record,'$.service') IN (SELECT value FROM json_each(?2)) ORDER BY rowid DESC LIMIT 200")?;
    let rows = q
        .query_map(params![id, serde_json::to_string(&neighbors)?], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let source_digest = digest(db, id)?;
    for (target, raw, target_digest) in rows {
        let other: SourceObservation = serde_json::from_str(&raw)?;
        let Some(edge) = crate::topology::edge(p, source, &other) else {
            continue;
        };
        if !other.authoritative
            || other.release.is_none()
            || other.deployed_revision.is_none()
            || latest(db, id, &target)?.is_some_and(|r| r.active)
            || crate::sentry_refresh::snapshot(db, &target, now)?
                .is_none_or(|s| s.state != "current_snapshot")
        {
            continue;
        }
        let times = DateTime::parse_from_rfc3339(&source.signal.last_seen)
            .and_then(|a| {
                DateTime::parse_from_rfc3339(&other.signal.last_seen)
                    .map(|b| (a - b).num_seconds().abs())
            })
            .map_err(|_| Error::Invalid("invalid cross-service occurrence time".into()))?;
        if times > i64::from(p.max_gap_seconds) {
            continue;
        }
        result.candidates.push(Candidate{case_id:target.clone(),case_version:store::case(db,&target)?.version,source_digest:source_digest.clone(),target_digest,signature:store::digest(&serde_json::to_string(&(&edge.caller,&edge.dependency,p.revision))?),topology_revision:Some(p.revision),reason:format!("Configured dependency: {} calls {}. Nearby observations within {} seconds have independently mapped releases/revisions and current reads. This is a review hint, not a shared stack signature, duplicate judgment or causal claim. Mapping rationale: {}",edge.caller.service,edge.dependency.service,p.max_gap_seconds,edge.rationale)});
        if result.candidates.len() >= 20 {
            break;
        }
    }
    Ok(())
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
    let mut result=Correlations {relations,candidates:vec![],topology:crate::topology::active(db)?,limitation:"Suggestions only: at most 20 matches from the newest 200 same-scope source cases; exact mapped release/revision, diagnostic signature and occurrence within one hour required. Current successful read required for both. Not exhaustive, causal evidence, authorization or recovery. Links keep all cases, incidents and duties separate; changed facts require renewed review. Cross-service hints require an installed unpaused dependency mapping, mapped revisions and fresh reads within its configured time gap; at most 200 neighbor-service cases scanned and 20 total hints. They do not establish a shared signature or causality. Latest 100 links shown; full revisions remain in events.".into()};
    let Some(source) = crate::sentry::snapshot(db, id)? else {
        return Ok(result);
    };
    let Some(sig) = signature(&source) else {
        append_cross_candidates(db, id, &source, now, &mut result)?;
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
            || latest(db, id, &target)?.is_some_and(|r| r.active)
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
        result.candidates.push(Candidate {case_version:store::case(db,&target)?.version,case_id:target,source_digest:source_digest.clone(),target_digest,signature:sig.clone(),topology_revision:None,reason:"Same service/environment/repository, mapped release/revision and bounded exception/frame signature; last occurrences within one hour. Operator must assess whether these represent the same failure.".into()});
        if result.candidates.len() >= 20 {
            break;
        }
    }
    append_cross_candidates(db, id, &source, now, &mut result)?;
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
                if !a.authoritative || !b.authoritative {
                    return Err(Error::Invalid(
                        "link requires API evidence from authoritative configured source snapshots"
                            .into(),
                    ));
                }
                let cross = !scope(&a, &b);
                let topology = if cross {
                    let p = crate::topology::active(&tx)?.ok_or_else(|| {
                        Error::Invalid(
                            "cross-service review requires installed dependency mapping".into(),
                        )
                    })?;
                    if *kind != RelationKind::Related
                        || cmd.expected_topology_revision != Some(p.revision)
                        || crate::topology::edge(&p, &a, &b).is_none()
                        || a.release.is_none()
                        || a.deployed_revision.is_none()
                        || b.release.is_none()
                        || b.deployed_revision.is_none()
                        || [id, target.id.as_str()].iter().any(|case| {
                            crate::sentry_refresh::snapshot(&tx, case, self.clock.now())
                                .ok()
                                .flatten()
                                .is_none_or(|s| s.state != "current_snapshot")
                        })
                    {
                        return Err(Error::Conflict("cross-service review requires exact active topology revision, a mapped dependency and fresh mapped source evidence; only related links are permitted".into()));
                    }
                    Some(p)
                } else {
                    None
                };
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
                    topology_revision: topology.as_ref().map(|p| p.revision),
                    topology_digest: topology.map(|p| p.digest),
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
        render_qualification(&self.snapshot(id)?)
    }
}
pub(crate) fn render_qualification(s: &Snapshot) -> Result<String> {
    let id = &s.case.id;
    let mut report = format!(
        "# Case qualification {}\n\nAll classifications and links require scoped evidence. Source status and duplicate links do not prove recovery or close duties.\n\n## Source context\n\n",
        id
    );
    // Escape code-fence delimiters and markup; fields remain untrusted literal text.
    for (name, value) in [
        ("Source", serde_json::to_value(&s.source)?),
        ("Freshness", serde_json::to_value(&s.source_freshness)?),
        ("Qualification", serde_json::to_value(&s.qualification)?),
        ("Impact", serde_json::to_value(&s.impact)?),
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
