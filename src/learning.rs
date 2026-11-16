//! Reviewed operational learning. Knowledge is scoped data, never execution/approval policy.
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
pub struct Impact {
    pub requests: Option<u64>,
    pub affected: Option<u64>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub method: String,
    pub uncertainty: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Cause {
    Unknown {
        question: String,
    },
    Supported {
        hypothesis_id: String,
        revision: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostmortemDraft {
    pub title: String,
    pub summary: String,
    pub chronology: String,
    pub impact: Impact,
    pub cause: Cause,
    pub response_assessment: String,
    pub action_ids: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub review_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeDraft {
    pub title: String,
    pub kind: String,
    pub body: String,
    pub applicability: String,
    pub invalidation_conditions: String,
    pub postmortem_id: String,
    pub postmortem_revision: u32,
    pub evidence_refs: Vec<String>,
    pub expires_seconds: u32,
    pub imported_from: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Postmortem { draft: PostmortemDraft },
    Knowledge { draft: KnowledgeDraft },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub recorded_version: i64,
    pub schema_version: V1,
    pub id: String,
    pub revision: u32,
    pub case_id: String,
    pub author: String,
    pub recorded_at: String,
    pub service: String,
    pub environment: String,
    pub repository: String,
    pub release: Option<String>,
    pub context_digest: String,
    pub content: Content,
    pub review_by: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acceptance {
    pub reviewer: String,
    pub digest: String,
    pub context_digest: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub document: Document,
    pub digest: String,
    pub acceptance: Option<Acceptance>,
    pub accepted_current: bool,
    pub retired: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearningSnapshot {
    pub entries: Vec<Entry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningCommand {
    pub command: OperatorCommand,
    pub action: LearningAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LearningAction {
    DraftPostmortem {
        id: Option<String>,
        expected_revision: Option<u32>,
        draft: PostmortemDraft,
    },
    ProposeKnowledge {
        id: Option<String>,
        expected_revision: Option<u32>,
        draft: KnowledgeDraft,
    },
    Accept {
        id: String,
        revision: u32,
        evidence_refs: Vec<String>,
    },
    Retire {
        id: String,
        revision: u32,
        evidence_refs: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReference {
    pub id: String,
    pub revision: u32,
    pub digest: String,
    pub title: String,
    pub body: String,
    pub applicability: String,
    pub invalidation_conditions: String,
    pub expires_at: String,
}
pub(crate) fn context(db: &Connection, case: &str) -> Result<String> {
    let recovery = crate::recovery::state(db, case)?;
    let ops = crate::incident_ops::snapshot(db, case)?.ok_or(Error::NotFound)?;
    Ok(store::digest(&serde_json::to_string(
        &serde_json::json!({"source":crate::recovery::source_digest(db,case)?,"incident_revision":ops.revision,"plan":recovery.plan.as_ref().map(|p|&p.id),"observation":recovery.observation.as_ref().map(|o|&o.id),"resolved_at":recovery.resolved_at,"resolved":recovery.response_resolved}),
    )?))
}
fn raw_document(db: &Connection, id: &str, revision: u32) -> Result<(Document, String)> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM learning_documents WHERE id=?1 AND revision=?2",
            params![id, revision],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict(
            "learning document integrity failure".into(),
        ));
    }
    Ok((serde_json::from_str(&raw)?, digest))
}
fn accepted(db: &Connection, id: &str, revision: u32, now: &str) -> Result<bool> {
    let (doc, digest) = raw_document(db, id, revision)?;
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM learning_acceptances WHERE id=?1 AND revision=?2",
            params![id, revision],
            |r| r.get(0),
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(false) };
    let a: Acceptance = serde_json::from_str(&raw)?;
    let latest: Option<u32> = db.query_row(
        "SELECT MAX(revision) FROM learning_acceptances WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    if latest != Some(revision)
        || a.digest != digest
        || a.context_digest != context(db, &doc.case_id)?
        || doc.context_digest != a.context_digest
        || crate::challenges::ensure_clear(db, &doc.case_id).is_err()
    {
        return Ok(false);
    }
    let original = match &doc.content {
        Content::Postmortem { draft } => &draft.evidence_refs,
        Content::Knowledge { draft } => &draft.evidence_refs,
    };
    if !refs_intact(db, &doc.case_id, original)?
        || !refs_intact(db, &doc.case_id, &a.evidence_refs)?
    {
        return Ok(false);
    }
    match &doc.content {
        Content::Postmortem { draft } => {
            if !crate::recovery::resolved(db, &doc.case_id)? {
                return Ok(false);
            }
            if validate_cause(db, &doc.case_id, &draft.cause).is_err() {
                return Ok(false);
            }
        }
        Content::Knowledge { draft } => {
            let retired: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM knowledge_retirements WHERE id=?1 AND revision=?2)",
                params![id, revision],
                |r| r.get(0),
            )?;
            if retired
                || crate::hallmark::validate_origin(
                    db,
                    &doc.case_id,
                    draft.imported_from.as_deref(),
                )
                .is_err()
                || doc.review_by.as_str() <= now
                || !accepted(db, &draft.postmortem_id, draft.postmortem_revision, now)?
            {
                return Ok(false);
            }
            let (pm, _) = raw_document(db, &draft.postmortem_id, draft.postmortem_revision)?;
            if pm.case_id != doc.case_id || !matches!(pm.content, Content::Postmortem { .. }) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
pub(crate) fn entry(db: &Connection, id: &str, revision: u32, now: &str) -> Result<Entry> {
    let (document, digest) = raw_document(db, id, revision)?;
    let acceptance: Option<String> = db
        .query_row(
            "SELECT record FROM learning_acceptances WHERE id=?1 AND revision=?2",
            params![id, revision],
            |r| r.get(0),
        )
        .optional()?;
    let retired = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_retirements WHERE id=?1 AND revision=?2)",
        params![id, revision],
        |r| r.get(0),
    )?;
    Ok(Entry {
        document,
        digest,
        acceptance: acceptance.map(|s| serde_json::from_str(&s)).transpose()?,
        accepted_current: accepted(db, id, revision, now)?,
        retired,
    })
}
pub(crate) fn snapshot(db: &Connection, case: &str, now: &str) -> Result<LearningSnapshot> {
    let mut q = db.prepare(
        "SELECT id,revision FROM learning_documents WHERE case_id=?1 ORDER BY rowid DESC LIMIT 200",
    )?;
    let entries = q
        .query_map([case], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
        })?
        .map(|r| {
            let (id, rev) = r?;
            entry(db, &id, rev, now)
        })
        .collect::<Result<_>>()?;
    Ok(LearningSnapshot { entries })
}
fn validate_cause(db: &Connection, case: &str, cause: &Cause) -> Result<()> {
    match cause {
        Cause::Unknown { question } => text(question, "unresolved cause question", 2000),
        Cause::Supported {
            hypothesis_id,
            revision,
        } => {
            let row:Option<(u32,String)>=db.query_row("SELECT revision,record FROM incident_artifacts WHERE id=?1 AND case_id=?2 ORDER BY revision DESC LIMIT 1",params![hypothesis_id,case],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (rev, raw) = row.ok_or(Error::NotFound)?;
            let a: crate::incident_artifacts::Artifact = serde_json::from_str(&raw)?;
            if rev != *revision
                || !matches!(
                    a.data,
                    crate::incident_artifacts::ArtifactData::Hypothesis {
                        state: crate::incident_artifacts::Finding::Supported,
                        ..
                    }
                )
            {
                return Err(Error::Conflict(
                    "established cause requires current evidence-supported hypothesis revision"
                        .into(),
                ));
            }
            Ok(())
        }
    }
}
fn validate_impact(impact: &Impact, now: &str) -> Result<()> {
    text(&impact.method, "impact method", 2000)?;
    text(&impact.uncertainty, "impact uncertainty", 2000)?;
    if impact.requests.is_some_and(|n| n > 1_000_000_000_000)
        || impact.affected.is_some_and(|n| n > 1_000_000_000_000)
        || impact
            .affected
            .zip(impact.requests)
            .is_some_and(|(a, n)| a > n)
        || impact.affected.is_some() && impact.requests.is_none()
    {
        return Err(Error::Invalid("impact needs a bounded denominator".into()));
    }
    match (&impact.started_at, &impact.ended_at) {
        (None, None) => {
            if impact.affected.is_some() || impact.requests.is_some() {
                return Err(Error::Invalid(
                    "quantified impact requires an observation window".into(),
                ));
            }
        }
        (Some(start), Some(end)) => {
            let start = chrono::DateTime::parse_from_rfc3339(start)
                .map_err(|_| Error::Invalid("impact start must be RFC3339".into()))?;
            let end = chrono::DateTime::parse_from_rfc3339(end)
                .map_err(|_| Error::Invalid("impact end must be RFC3339".into()))?;
            let at = chrono::DateTime::parse_from_rfc3339(now).unwrap();
            if start >= end || end > at {
                return Err(Error::Invalid("invalid impact window".into()));
            }
        }
        _ => {
            return Err(Error::Invalid(
                "impact requires both window boundaries or neither".into(),
            ));
        }
    };
    Ok(())
}
fn validate_actions(db: &Connection, case: &str, ids: &[String]) -> Result<()> {
    if ids.is_empty()
        || ids.len() > 20
        || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
    {
        return Err(Error::Invalid(
            "postmortem needs 1..20 distinct owned corrective actions".into(),
        ));
    }
    for id in ids {
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM corrective_actions WHERE id=?1 AND case_id=?2)",
            params![id, case],
            |r| r.get(0),
        )?;
        if !valid {
            return Err(Error::Invalid(
                "postmortem action must belong to this case".into(),
            ));
        }
    }
    Ok(())
}
fn next_revision(
    db: &Connection,
    case: &str,
    id: &Option<String>,
    expected: Option<u32>,
    kind: &str,
    who: &str,
    commander: &str,
) -> Result<(String, u32)> {
    match (id, expected) {
        (None, None) => Ok((
            store::id(if kind == "knowledge" {
                "knowledge"
            } else {
                "postmortem"
            }),
            1,
        )),
        (Some(id), Some(rev)) => {
            let latest: Option<u32> = db.query_row(
                "SELECT MAX(revision) FROM learning_documents WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            let (old, _) = raw_document(db, id, rev)?;
            let old_kind = if matches!(old.content, Content::Knowledge { .. }) {
                "knowledge"
            } else {
                "postmortem"
            };
            if latest != Some(rev)
                || old.case_id != case
                || old_kind != kind
                || (who != old.author && who != commander)
            {
                return Err(Error::Conflict(
                    "stale/foreign learning revision or wrong author".into(),
                ));
            }
            Ok((id.clone(), rev + 1))
        }
        _ => Err(Error::Invalid(
            "id and expected_revision must be supplied together".into(),
        )),
    }
}
fn event(
    tx: &Transaction<'_>,
    case: &str,
    id: &str,
    revision: u32,
    kind: &str,
    m: &CommandMeta,
    now: &str,
) -> Result<()> {
    store::emit(
        tx,
        case,
        operator(&m.actor),
        &m.idempotency_key,
        now,
        EventData::LearningChanged {
            id: id.into(),
            revision,
            kind: kind.into(),
            reason: m.reason.clone(),
        },
    )
}
impl Application {
    pub fn learning_command(&self, case: &str, cmd: LearningCommand) -> Result<LearningSnapshot> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        let fp = store::digest(&format!("learning:{case}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, case)?.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale learning command".into()));
        }
        let ops = crate::incident_ops::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
        let source = crate::sentry::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
        let now = self.now();
        let policy = crate::policy::for_case(&tx, case)?;
        match &cmd.action {
            LearningAction::DraftPostmortem {
                id,
                expected_revision,
                draft,
            } => {
                if m.actor != ops.roles.commander && m.actor != ops.roles.scribe {
                    return Err(Error::Invalid(
                        "commander or scribe must draft postmortem".into(),
                    ));
                }
                text(&draft.title, "title", 500)?;
                text(&draft.summary, "summary", 4000)?;
                text(&draft.chronology, "chronology", 8000)?;
                text(&draft.response_assessment, "response assessment", 4000)?;
                validate_impact(&draft.impact, &now)?;
                validate_cause(&tx, case, &draft.cause)?;
                validate_actions(&tx, case, &draft.action_ids)?;
                crate::qualification::evidence_refs(&tx, case, &draft.evidence_refs, false)?;
                if draft.review_seconds == 0 || draft.review_seconds > policy.max_review_seconds {
                    return Err(Error::Invalid(
                        "postmortem review deadline outside policy".into(),
                    ));
                }
                let (id, revision) = next_revision(
                    &tx,
                    case,
                    id,
                    *expected_revision,
                    "postmortem",
                    &m.actor,
                    &ops.roles.commander,
                )?;
                let doc = Document {
                    recorded_version: store::case(&tx, case)?.version + 1,
                    schema_version: V1,
                    id,
                    revision,
                    case_id: case.into(),
                    author: m.actor.clone(),
                    recorded_at: now.clone(),
                    service: source.service,
                    environment: source.environment,
                    repository: source.repository,
                    release: source.release,
                    context_digest: context(&tx, case)?,
                    content: Content::Postmortem {
                        draft: draft.clone(),
                    },
                    review_by: (self.clock.now() + Duration::seconds(draft.review_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                };
                save_document(&tx, &doc, m, &now)?;
            }
            LearningAction::ProposeKnowledge {
                id,
                expected_revision,
                draft,
            } => {
                if m.actor != ops.roles.commander && m.actor != ops.roles.scribe {
                    return Err(Error::Invalid(
                        "commander or scribe must propose knowledge".into(),
                    ));
                }
                for (name, value, max) in [
                    ("title", &draft.title, 500),
                    ("body", &draft.body, 8000),
                    ("applicability", &draft.applicability, 2000),
                    (
                        "invalidation conditions",
                        &draft.invalidation_conditions,
                        2000,
                    ),
                ] {
                    text(value, name, max)?;
                }
                if let Some(origin) = &draft.imported_from {
                    text(origin, "import provenance", 1000)?;
                    crate::hallmark::validate_origin(&tx, case, Some(origin))?;
                }
                if ![
                    "lesson",
                    "known_error",
                    "runbook",
                    "service_map",
                    "decision",
                    "process",
                ]
                .contains(&draft.kind.as_str())
                    || draft.expires_seconds == 0
                    || draft.expires_seconds > policy.max_review_seconds
                {
                    return Err(Error::Invalid("invalid knowledge kind/expiry".into()));
                }
                let (pm, _) = raw_document(&tx, &draft.postmortem_id, draft.postmortem_revision)?;
                if pm.case_id != case || !matches!(pm.content, Content::Postmortem { .. }) {
                    return Err(Error::Invalid(
                        "knowledge requires a same-case postmortem revision".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, &draft.evidence_refs, false)?;
                let (id, revision) = next_revision(
                    &tx,
                    case,
                    id,
                    *expected_revision,
                    "knowledge",
                    &m.actor,
                    &ops.roles.commander,
                )?;
                let doc = Document {
                    recorded_version: store::case(&tx, case)?.version + 1,
                    schema_version: V1,
                    id,
                    revision,
                    case_id: case.into(),
                    author: m.actor.clone(),
                    recorded_at: now.clone(),
                    service: source.service,
                    environment: source.environment,
                    repository: source.repository,
                    release: source.release,
                    context_digest: context(&tx, case)?,
                    content: Content::Knowledge {
                        draft: draft.clone(),
                    },
                    review_by: (self.clock.now() + Duration::seconds(draft.expires_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                };
                save_document(&tx, &doc, m, &now)?;
            }
            LearningAction::Accept {
                id,
                revision,
                evidence_refs,
            } => {
                crate::challenges::ensure_clear(&tx, case)?;
                let (doc, digest) = raw_document(&tx, id, *revision)?;
                let latest: Option<u32> = tx.query_row(
                    "SELECT MAX(revision) FROM learning_documents WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                if doc.case_id != case
                    || latest != Some(*revision)
                    || doc.context_digest != context(&tx, case)?
                    || doc.review_by <= now
                {
                    return Err(Error::Conflict(
                        "learning revision/context is stale or expired".into(),
                    ));
                }
                if doc.author == m.actor {
                    return Err(Error::Invalid(
                        "learning author cannot accept their own revision".into(),
                    ));
                }
                let original_refs = match &doc.content {
                    Content::Postmortem { draft } => {
                        validate_cause(&tx, case, &draft.cause)?;
                        validate_actions(&tx, case, &draft.action_ids)?;
                        if !crate::recovery::resolved(&tx, case)? {
                            return Err(Error::Conflict("postmortem acceptance requires resolved response with current recovery context".into()));
                        }
                        &draft.evidence_refs
                    }
                    Content::Knowledge { draft } => {
                        if !accepted(&tx, &draft.postmortem_id, draft.postmortem_revision, &now)? {
                            return Err(Error::Conflict(
                                "source postmortem acceptance is missing/stale".into(),
                            ));
                        }
                        &draft.evidence_refs
                    }
                };
                crate::qualification::evidence_refs(&tx, case, original_refs, false)?;
                crate::recovery::independent_evidence(
                    &tx,
                    case,
                    evidence_refs,
                    &m.actor,
                    &doc.recorded_at,
                    original_refs,
                    doc.recorded_version,
                )?;
                let retired:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM knowledge_retirements WHERE id=?1 AND revision=?2)",params![id,revision],|r|r.get(0))?;
                if retired {
                    return Err(Error::Conflict(
                        "retired revision cannot be reaccepted; propose a new revision".into(),
                    ));
                }
                let a = Acceptance {
                    reviewer: m.actor.clone(),
                    digest,
                    context_digest: doc.context_digest,
                    evidence_refs: evidence_refs.clone(),
                    recorded_at: now.clone(),
                    reason: m.reason.clone(),
                };
                tx.execute(
                    "INSERT INTO learning_acceptances VALUES (?1,?2,?3)",
                    params![id, revision, serde_json::to_string(&a)?],
                )?;
                event(&tx, case, id, *revision, "accepted", m, &now)?;
            }
            LearningAction::Retire {
                id,
                revision,
                evidence_refs,
            } => {
                let (doc, _) = raw_document(&tx, id, *revision)?;
                if doc.case_id != case
                    || !matches!(doc.content, Content::Knowledge { .. })
                    || m.actor != ops.roles.commander
                {
                    return Err(Error::Invalid(
                        "current commander may retire same-case knowledge".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, evidence_refs, false)?;
                tx.execute("INSERT INTO knowledge_retirements VALUES (?1,?2,?3)",params![id,revision,serde_json::to_string(&serde_json::json!({"actor":m.actor,"reason":m.reason,"evidence_refs":evidence_refs,"recorded_at":now}))?])?;
                event(&tx, case, id, *revision, "retired", m, &now)?;
            }
        }
        let out = snapshot(&tx, case, &now)?;
        save_receipt(&tx, &m.idempotency_key, &fp, &out)?;
        tx.commit()?;
        Ok(out)
    }
    pub fn knowledge_search(
        &self,
        service: &str,
        environment: &str,
        repository: &str,
        release: Option<&str>,
        query: &str,
    ) -> Result<Vec<KnowledgeReference>> {
        for (name, value) in [
            ("service", service),
            ("environment", environment),
            ("repository", repository),
        ] {
            text(value, name, 256)?;
        }
        if query.len() > 200 {
            return Err(Error::Invalid("query too long".into()));
        }
        let mut db = self.store.connect()?;
        let tx = db.transaction()?;
        search(
            &tx,
            service,
            environment,
            repository,
            release,
            query,
            &self.now(),
        )
    }
    pub fn learning_markdown(&self, case: &str) -> Result<String> {
        let db = self.store.connect()?;
        let s = snapshot(&db, case, &self.now())?;
        let mut report = String::from(
            "# Postmortems and knowledge\n\nOperator-authored and independently attributed review. Only current accepted scoped knowledge is eligible for briefings; prose never grants policy or execution authority.\n",
        );
        for e in &s.entries {
            render_entry(&db, e, &mut report)?;
        }
        let raw = serde_json::to_string_pretty(&s)?
            .replace('`', "\\u0060")
            .replace('<', "\\u003c");
        report.push_str(&format!(
            "\n## Authoritative structured records\n\n```json\n{raw}\n```\n"
        ));
        Ok(report)
    }
}
fn save_document(tx: &Transaction<'_>, d: &Document, m: &CommandMeta, now: &str) -> Result<()> {
    let raw = serde_json::to_string(d)?;
    let kind = if matches!(d.content, Content::Knowledge { .. }) {
        "knowledge"
    } else {
        "postmortem"
    };
    tx.execute(
        "INSERT INTO learning_documents VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
        params![
            d.id,
            d.revision,
            d.case_id,
            kind,
            store::digest(&raw),
            raw,
            d.review_by
        ],
    )?;
    event(tx, &d.case_id, &d.id, d.revision, "proposed", m, now)
}
fn search(
    db: &Connection,
    service: &str,
    environment: &str,
    repository: &str,
    release: Option<&str>,
    query: &str,
    now: &str,
) -> Result<Vec<KnowledgeReference>> {
    let mut q=db.prepare("SELECT d.id,MAX(a.revision) FROM learning_documents d JOIN learning_acceptances a ON a.id=d.id AND a.revision=d.revision WHERE d.kind='knowledge' AND json_extract(d.record,'$.service')=?1 AND json_extract(d.record,'$.environment')=?2 AND json_extract(d.record,'$.repository')=?3 AND json_extract(d.record,'$.release') IS ?4 GROUP BY d.id ORDER BY MAX(d.rowid) DESC LIMIT 200")?;
    let ids = q
        .query_map(params![service, environment, repository, release], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut out = vec![];
    for (id, revision) in ids {
        if !accepted(db, &id, revision, now)? {
            continue;
        }
        let (doc, digest) = raw_document(db, &id, revision)?;
        if let Content::Knowledge { draft } = &doc.content {
            let haystack =
                format!("{} {} {}", draft.title, draft.body, draft.applicability).to_lowercase();
            if terms.iter().all(|s| haystack.contains(s)) {
                out.push(KnowledgeReference {
                    id,
                    revision,
                    digest,
                    title: draft.title.clone(),
                    body: draft.body.clone(),
                    applicability: draft.applicability.clone(),
                    invalidation_conditions: draft.invalidation_conditions.clone(),
                    expires_at: doc.review_by,
                });
                if out.len() == 20 {
                    break;
                }
            }
        }
    }
    Ok(out)
}
pub(crate) fn for_case(db: &Connection, case: &str, now: &str) -> Result<Vec<KnowledgeReference>> {
    let Some(s) = crate::sentry::snapshot(db, case)? else {
        return Ok(vec![]);
    };
    search(
        db,
        &s.service,
        &s.environment,
        &s.repository,
        s.release.as_deref(),
        "",
        now,
    )
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let ids: Vec<(String, u32, String)> = {
        let mut q=tx.prepare("SELECT d.id,d.revision,d.case_id FROM learning_documents d WHERE d.escalated=0 AND d.review_by<=?1 AND (d.revision=(SELECT MAX(revision) FROM learning_documents WHERE id=d.id) OR d.revision=(SELECT MAX(revision) FROM learning_acceptances WHERE id=d.id)) AND NOT EXISTS(SELECT 1 FROM knowledge_retirements k WHERE k.id=d.id AND k.revision=d.revision) AND (d.kind='knowledge' OR NOT EXISTS(SELECT 1 FROM learning_acceptances a WHERE a.id=d.id AND a.revision=d.revision)) ORDER BY d.review_by,d.rowid LIMIT 100")?;
        q.query_map([now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?
    };
    for (id, revision, case) in &ids {
        crate::team::escalate(
            tx,
            case,
            "learning_review",
            &format!("{id}@{revision}"),
            "Postmortem/knowledge review or expiry deadline reached; commander must review, revise or retire the scoped record",
            now,
        )?;
        tx.execute(
            "UPDATE learning_documents SET escalated=1 WHERE id=?1 AND revision=?2",
            params![id, revision],
        )?;
    }
    Ok(ids.len())
}

fn refs_intact(db: &Connection, case: &str, refs: &[String]) -> Result<bool> {
    if refs.is_empty() {
        return Ok(false);
    }
    for digest in refs {
        let raw:Option<String>=db.query_row("SELECT e.content FROM evidence e JOIN case_evidence c ON c.digest=e.digest WHERE c.case_id=?1 AND c.digest=?2",params![case,digest],|r|r.get(0)).optional()?;
        if raw.is_none_or(|s| store::digest(&s) != *digest) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn render_entry(db: &Connection, e: &Entry, out: &mut String) -> Result<()> {
    use crate::incidents::safe;
    let d = &e.document;
    let status = if e.retired {
        "retired"
    } else if e.accepted_current {
        "accepted in current context"
    } else {
        "proposed / acceptance missing or stale"
    };
    out.push_str(&format!(
        "\n## {} @ {}\n\nStatus: {}. Author: {}. Scope: {} / {} / {} / {}.\n",
        safe(&d.id),
        d.revision,
        status,
        safe(&d.author),
        safe(&d.service),
        safe(&d.environment),
        safe(&d.repository),
        safe(d.release.as_deref().unwrap_or("unknown release"))
    ));
    match &d.content {
        Content::Postmortem{draft}=>{
            out.push_str(&format!("\n### Executive summary: {}\n\n{}\n\n### Impact and chronology\n\nReported affected requests: {}. Denominator: {}. Window: {} to {}. Method: {}. Uncertainty: {}.\n\n{}\n\n### Causal analysis\n\n",safe(&draft.title),safe(&draft.summary),draft.impact.affected.map_or("unknown".into(),|n|n.to_string()),draft.impact.requests.map_or("unknown".into(),|n|n.to_string()),safe(draft.impact.started_at.as_deref().unwrap_or("unknown")),safe(draft.impact.ended_at.as_deref().unwrap_or("unknown")),safe(&draft.impact.method),safe(&draft.impact.uncertainty),safe(&draft.chronology)));
            match &draft.cause {Cause::Unknown{question}=>out.push_str(&format!("Unknown: {}.\n",safe(question))),Cause::Supported{hypothesis_id,revision}=>out.push_str(&format!("Evidence-supported hypothesis: {} @ {}.\n",safe(hypothesis_id),revision))};
            out.push_str(&format!("\n### Response assessment\n\n{}\n\n### Corrective actions\n\n| Action | Owner | Verifier | Acceptance | Due | State |\n| --- | --- | --- | --- | --- | --- |\n",safe(&draft.response_assessment)));
            for id in &draft.action_ids {let raw:String=db.query_row("SELECT record FROM corrective_actions WHERE id=?1 AND case_id=?2",params![id,d.case_id],|r|r.get(0))?;let a:crate::recovery::CorrectiveAction=serde_json::from_str(&raw)?;out.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",safe(&a.title),safe(&a.owner),safe(&a.verifier),safe(&a.acceptance),safe(&a.due_at),safe(&a.state)));}
        }
        Content::Knowledge{draft}=>out.push_str(&format!("\n### {}\n\n{}\n\nApplicability: {}. Reassess when: {}. Expires: {}. Import provenance: {}.\n",safe(&draft.title),safe(&draft.body),safe(&draft.applicability),safe(&draft.invalidation_conditions),safe(&d.review_by),safe(draft.imported_from.as_deref().unwrap_or("native proposal")))),
    }
    Ok(())
}
