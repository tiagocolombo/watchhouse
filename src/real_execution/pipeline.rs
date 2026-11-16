//! Frozen candidate and independent regression/review gates; publication is a separate effect.
use super::{
    protocol::*,
    worker::{self, ContainerJob},
};
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextChange {
    pub path: String,
    pub before_digest: Option<String>,
    pub after: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub repair_id: String,
    pub author: String,
    pub producer: String,
    pub recorded_at: String,
    pub recorded_version: i64,
    pub source_revision: i64,
    pub source_digest: Option<String>,
    pub policy_revision: i64,
    pub repository: String,
    pub base_commit: String,
    pub base_tree: String,
    pub digest: String,
    pub files: Vec<InputFile>,
    pub root: PathBuf,
    pub changes: Vec<TextChange>,
    pub deadline: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FreezeCandidate {
    pub command: OperatorCommand,
    pub repair_id: String,
    pub policy_revision: i64,
    pub changes: Vec<TextChange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestCheck {
    pub command: OperatorCommand,
    pub candidate_id: String,
    pub side: Side,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Base,
    Candidate,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCandidate {
    pub command: OperatorCommand,
    pub candidate_id: String,
    pub base_check: String,
    pub candidate_check: String,
    pub approve: bool,
    pub findings: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub candidate_id: String,
    pub case_id: String,
    pub author: String,
    pub candidate_digest: String,
    pub base_commit: String,
    pub profile_digest: String,
    pub source_digest: Option<String>,
    pub base_check: String,
    pub candidate_check: String,
    pub approve: bool,
    pub findings: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
    pub current: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepairProgress {
    pub candidates: Vec<Candidate>,
    pub reviews: Vec<Review>,
}
pub(crate) fn get(db: &Connection, id: &str) -> Result<Candidate> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM repair_candidates WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
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
pub(crate) fn current(db: &Connection, c: &Candidate, now: &str) -> Result<()> {
    if c.deadline.as_str() <= now || c.source_digest != source_digest(db, &c.case_id)? {
        return Err(Error::Conflict(
            "repair budget/source context changed; new validation required".into(),
        ));
    }
    let latest: String = db.query_row(
        "SELECT id FROM repair_candidates WHERE repair_id=?1 ORDER BY rowid DESC LIMIT 1",
        [&c.repair_id],
        |r| r.get(0),
    )?;
    if latest != c.id {
        return Err(Error::Conflict(
            "candidate changed; prior checks/review cannot authorize it".into(),
        ));
    }
    let source_revision:Option<i64>=db.query_row("SELECT MAX(revision) FROM repair_sources WHERE json_extract(record,'$.config.repository')=?1",[&c.repository],|r|r.get(0))?;
    if source_revision != Some(c.source_revision) {
        return Err(Error::Conflict(
            "relevant repository base/config changed; revalidation required".into(),
        ));
    }
    crate::challenges::ensure_clear(db, &c.case_id)?;
    let p = worker::policy(db, c.policy_revision)?;
    worker::admission(db, &p, now)?;
    let source = crate::repair::source(db, c.source_revision)?;
    if source.snapshot.base_commit != c.base_commit || source.snapshot.tree != c.base_tree {
        return Err(Error::Conflict(
            "frozen repair source identity changed".into(),
        ));
    }
    Ok(())
}
pub(crate) fn review(db: &Connection, id: &str, now: &str) -> Result<Review> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM candidate_reviews WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let mut r: Review = serde_json::from_str(&raw.ok_or(Error::NotFound)?)?;
    let c = get(db, &r.candidate_id)?;
    r.current = r.approve
        && current(db, &c, now).is_ok()
        && c.digest == r.candidate_digest
        && c.source_digest == r.source_digest;
    let latest: Option<String> = db
        .query_row(
            "SELECT id FROM candidate_reviews WHERE candidate_id=?1 ORDER BY rowid DESC LIMIT 1",
            [&r.candidate_id],
            |r| r.get(0),
        )
        .optional()?;
    r.current &= latest.as_deref() == Some(&r.id);
    if r.current {
        let base = worker::get(db, &r.base_check)?;
        let candidate = worker::get(db, &r.candidate_check)?;
        r.current = checks_match(db, &c, &base, &candidate).is_ok();
    }
    Ok(r)
}
pub(crate) fn snapshot(db: &Connection, case: &str, now: &str) -> Result<RepairProgress> {
    let candidates = {
        let mut q = db.prepare(
            "SELECT id FROM repair_candidates WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
        )?;
        q.query_map([case], |r| r.get::<_, String>(0))?
            .map(|r| get(db, &r?))
            .collect::<Result<_>>()?
    };
    let reviews = {
        let mut q = db.prepare(
            "SELECT id FROM candidate_reviews WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
        )?;
        q.query_map([case], |r| r.get::<_, String>(0))?
            .map(|r| review(db, &r?, now))
            .collect::<Result<_>>()?
    };
    Ok(RepairProgress {
        candidates,
        reviews,
    })
}
fn check_case(db: &Connection, case: &str, cmd: &OperatorCommand) -> Result<String> {
    let c = store::case(db, case)?;
    if c.version != cmd.expected_version || c.owner != cmd.meta.actor {
        return Err(Error::Conflict(
            "reload the owned case before repair command".into(),
        ));
    }
    crate::challenges::ensure_clear(db, case)?;
    Ok(c.owner)
}
fn meta(m: &CommandMeta) -> Result<()> {
    crate::incident_ops::human(&m.actor)?;
    text(&m.idempotency_key, "key", 128)?;
    text(&m.reason, "reason", 2000)
}
pub(crate) fn checks_match(
    db: &Connection,
    c: &Candidate,
    base: &ContainerJob,
    candidate: &ContainerJob,
) -> Result<()> {
    let source = crate::repair::source(db, c.source_revision)?;
    let base_files: Vec<InputFile> = source
        .snapshot
        .files
        .iter()
        .map(|f| InputFile {
            path: f.path.clone(),
            digest: f.digest.clone(),
            bytes: f.bytes,
            executable: f.git_mode == "100755",
        })
        .collect();
    for job in [base, candidate] {
        let digest = job
            .evidence_digest
            .as_ref()
            .ok_or_else(|| Error::Conflict("native regression evidence missing".into()))?;
        let record: Option<(String, String)> = db
            .query_row(
                "SELECT content,provenance FROM evidence WHERE digest=?1",
                [digest],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if record
            .is_none_or(|(raw, p)| store::digest(&raw) != *digest || p != "native-container-v1")
        {
            return Err(Error::Conflict(
                "regression evidence is missing or corrupt".into(),
            ));
        }
    }
    if base.kind != "regression_base"
        || candidate.kind != "regression_candidate"
        || base.candidate_id.as_deref() != Some(&c.id)
        || candidate.candidate_id.as_deref() != Some(&c.id)
        || base.case_id != c.case_id
        || candidate.case_id != c.case_id
        || base.state != "passed"
        || candidate.state != "passed"
        || !base.native
        || !candidate.native
        || base.envelope.files != base_files
        || base.policy_revision != c.policy_revision
        || candidate.policy_revision != c.policy_revision
        || base.envelope.profile_digest != candidate.envelope.profile_digest
        || candidate.envelope.files != c.files
        || base
            .result
            .as_ref()
            .is_none_or(|r| r.exit_code != Some(1) || r.failure_code.is_some())
        || candidate
            .result
            .as_ref()
            .is_none_or(|r| r.exit_code != Some(0) || r.failure_code.is_some())
    {
        return Err(Error::Conflict(
            "exact native fail-on-base/pass-on-candidate evidence is missing or stale".into(),
        ));
    }
    Ok(())
}
impl Application {
    pub fn freeze_candidate(&self, case: &str, cmd: FreezeCandidate) -> Result<Candidate> {
        meta(&cmd.command.meta)?;
        let fp = store::digest(&format!(
            "freeze-candidate:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let db = self.store.connect()?;
        let tx = db.unchecked_transaction()?;
        if let Some(id) = receipt::<String>(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return get(&tx, &id);
        }
        check_case(&tx, case, &cmd.command)?;
        let raw: Option<String> = tx
            .query_row(
                "SELECT record FROM repair_requests WHERE id=?1 AND case_id=?2",
                params![cmd.repair_id, case],
                |r| r.get(0),
            )
            .optional()?;
        let repair: crate::repair::RepairRequest =
            serde_json::from_str(&raw.ok_or(Error::NotFound)?)?;
        let source = crate::repair::source(&tx, repair.source_revision)?;
        let p = worker::policy(&tx, cmd.policy_revision)?;
        let governing = worker::trusted_case_policy(&tx, case)?;
        let ctx = source_digest(&tx, case)?;
        drop(tx);
        drop(db);
        if cmd.changes.is_empty()
            || cmd.changes.len() > 20
            || serde_json::to_vec(&cmd.changes)?.len() > 1024 * 1024
        {
            return Err(Error::Invalid(
                "candidate needs 1..20 bounded text changes".into(),
            ));
        }
        crate::repair::snapshot::verify(
            &source.snapshot_path,
            &source.snapshot,
            source.config.preparation_seconds,
        )?;
        let mut files: BTreeMap<String, InputFile> = source
            .snapshot
            .files
            .iter()
            .map(|f| {
                (
                    f.path.clone(),
                    InputFile {
                        executable: f.git_mode == "100755",
                        path: f.path.clone(),
                        digest: f.digest.clone(),
                        bytes: f.bytes,
                    },
                )
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        for change in &cmd.changes {
            if !portable(&change.path) || !seen.insert(change.path.to_ascii_lowercase()) {
                return Err(Error::Invalid("unsafe/duplicate candidate path".into()));
            }
            let old = files.get(&change.path);
            if change.before_digest.as_deref() != old.map(|f| f.digest.as_str())
                || change.after.is_none() && old.is_none()
            {
                return Err(Error::Conflict(
                    "patch does not match frozen base bytes".into(),
                ));
            }
            if let Some(after) = &change.after {
                if after.len() as u64 > source.config.max_blob_bytes {
                    return Err(Error::Invalid(
                        "candidate blob exceeds source policy".into(),
                    ));
                }
                files.insert(
                    change.path.clone(),
                    InputFile {
                        executable: old.is_some_and(|f| f.executable),
                        path: change.path.clone(),
                        digest: store::digest(after),
                        bytes: after.len() as u64,
                    },
                );
            } else {
                files.remove(&change.path);
            }
        }
        let mut portable_names = std::collections::HashSet::new();
        for name in files.keys() {
            if !portable_names.insert(name.to_ascii_lowercase()) {
                return Err(Error::Invalid("candidate has a case-colliding path".into()));
            }
        }
        let list: Vec<InputFile> = files.into_values().collect();
        let total: u64 = list.iter().map(|f| f.bytes).sum();
        if list.is_empty()
            || list.len() > source.config.max_files as usize
            || total > source.config.max_total_bytes
            || total > p.work_bytes / 2
        {
            return Err(Error::Invalid(
                "candidate exceeds frozen input/work limits".into(),
            ));
        }
        let root = tempfile::Builder::new()
            .prefix("candidate-")
            .tempdir_in(&p.staging_root)
            .map_err(|_| Error::Invalid("candidate staging unavailable".into()))?;
        for file in &list {
            let path = root.path().join(&file.path);
            std::fs::create_dir_all(path.parent().unwrap())
                .map_err(|e| Error::Invalid(e.to_string()))?;
            let bytes = if let Some(ch) = cmd.changes.iter().find(|c| c.path == file.path) {
                ch.after
                    .as_ref()
                    .ok_or_else(|| Error::Invalid("deleted file remained in manifest".into()))?
                    .as_bytes()
                    .to_vec()
            } else {
                std::fs::read(source.snapshot_path.join(&file.path))
                    .map_err(|_| Error::Invalid("base input disappeared".into()))?
            };
            if bytes_digest(&bytes) != file.digest {
                return Err(Error::Conflict(
                    "base changed during candidate freeze".into(),
                ));
            }
            std::fs::write(path, bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        }
        let now = self.now();
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(id) = receipt::<String>(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return get(&tx, &id);
        }
        let author = check_case(&tx, case, &cmd.command)?;
        let budget: Option<String> = tx
            .query_row(
                "SELECT deadline FROM repair_execution_budgets WHERE repair_id=?1",
                [&cmd.repair_id],
                |r| r.get(0),
            )
            .optional()?;
        let deadline = budget.unwrap_or_else(|| {
            (self.clock.now() + Duration::seconds(governing.assignment_seconds))
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        });
        if deadline <= now {
            return Err(Error::Conflict(
                "repair's shared execution deadline exhausted".into(),
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO repair_execution_budgets VALUES (?1,?2,?3)",
            params![cmd.repair_id, now, deadline],
        )?;
        let c = Candidate {
            schema_version: V1,
            id: store::id("candidate"),
            case_id: case.into(),
            repair_id: cmd.repair_id,
            author,
            producer: "operator-provided-text-v1".into(),
            recorded_at: now.clone(),
            recorded_version: store::case(&tx, case)?.version + 1,
            source_revision: repair.source_revision,
            source_digest: ctx,
            policy_revision: p.revision,
            repository: repair.repository,
            base_commit: repair.base_commit,
            base_tree: repair.base_tree,
            digest: store::digest(&serde_json::to_string(&list)?),
            files: list,
            root: root.path().to_owned(),
            changes: cmd.changes,
            deadline,
        };
        tx.execute(
            "INSERT INTO repair_candidates VALUES (?1,?2,?3,?4)",
            params![c.id, case, c.repair_id, serde_json::to_string(&c)?],
        )?;
        store::emit(
            &tx,
            case,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &now,
            EventData::CandidateChanged {
                candidate_id: c.id.clone(),
                kind: "frozen".into(),
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &c.id)?;
        tx.commit()?;
        let _ = root.keep();
        Ok(c)
    }
    pub fn request_candidate_check(&self, case: &str, cmd: RequestCheck) -> Result<ContainerJob> {
        meta(&cmd.command.meta)?;
        let fp = store::digest(&format!(
            "request-candidate-check:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let db = self.store.connect()?;
        let tx = db.unchecked_transaction()?;
        if let Some(id) = receipt::<String>(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return worker::get(&tx, &id);
        }
        check_case(&tx, case, &cmd.command)?;
        let c = get(&tx, &cmd.candidate_id)?;
        if c.case_id != case {
            return Err(Error::NotFound);
        }
        current(&tx, &c, &self.now())?;
        worker::ensure_clear(&tx, case)?;
        let p = worker::policy(&tx, c.policy_revision)?;
        let source = crate::repair::source(&tx, c.source_revision)?;
        let governing = worker::trusted_case_policy(&tx, case)?;
        let kind = match cmd.side {
            Side::Base => "regression_base",
            Side::Candidate => "regression_candidate",
        };
        let count:i64=tx.query_row("SELECT COUNT(*) FROM container_jobs j JOIN repair_candidates c ON c.id=json_extract(j.record,'$.candidate_id') WHERE c.repair_id=?1 AND j.state!='blocked' AND json_extract(j.record,'$.kind')=?2",params![c.repair_id,kind],|r|r.get(0))?;
        if count >= governing.max_attempts
            || (self.clock.now()
                + Duration::seconds(
                    i64::from(p.wall_seconds) + i64::from(p.dispatch_grace_seconds),
                ))
            .to_rfc3339_opts(SecondsFormat::Millis, true)
                > c.deadline
        {
            return Err(Error::Conflict(
                "shared repair attempt/time budget exhausted".into(),
            ));
        }
        if matches!(cmd.side, Side::Candidate) {
            let base:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM container_jobs WHERE json_extract(record,'$.candidate_id')=?1 AND json_extract(record,'$.kind')='regression_base' AND state='passed' AND json_extract(record,'$.native')=1)",[&c.id],|r|r.get(0))?;
            if !base {
                return Err(Error::Conflict(
                    "prove regression failure on this exact base/context before checking candidate"
                        .into(),
                ));
            }
        }
        let (files, path) = match cmd.side {
            Side::Base => (
                source
                    .snapshot
                    .files
                    .iter()
                    .map(|f| InputFile {
                        executable: f.git_mode == "100755",
                        path: f.path.clone(),
                        digest: f.digest.clone(),
                        bytes: f.bytes,
                    })
                    .collect(),
                source.snapshot_path,
            ),
            Side::Candidate => (c.files.clone(), c.root.clone()),
        };
        drop(tx);
        drop(db);
        let envelope = p.job(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            files,
        );
        let (root, envelope) = worker::stage(&p, envelope, &path)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(id) = receipt::<String>(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return worker::get(&tx, &id);
        }
        check_case(&tx, case, &cmd.command)?;
        current(&tx, &c, &self.now())?;
        worker::ensure_clear(&tx, case)?;
        let mut j = worker::new_job(
            self,
            case,
            &p,
            kind,
            envelope,
            root.path().to_owned(),
            &cmd.command.meta,
        );
        j.candidate_id = Some(c.id);
        j.deadline = j.deadline.min(c.deadline);
        j.recorded_version = store::case(&tx, case)?.version + 1;
        worker::save(
            &tx,
            &j,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &self.now(),
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &j.id)?;
        tx.commit()?;
        let _ = root.keep();
        Ok(j)
    }
    pub fn review_candidate(&self, case: &str, cmd: ReviewCandidate) -> Result<Review> {
        meta(&cmd.command.meta)?;
        text(&cmd.findings, "review findings", 8000)?;
        let fp = store::digest(&format!(
            "review-candidate:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(id) = receipt::<String>(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return review(&tx, &id, &self.now());
        }
        if store::case(&tx, case)?.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale candidate review".into()));
        }
        let c = get(&tx, &cmd.candidate_id)?;
        if c.case_id != case {
            return Err(Error::NotFound);
        }
        current(&tx, &c, &self.now())?;
        if c.author == cmd.command.meta.actor {
            return Err(Error::Invalid(
                "candidate author cannot approve their own candidate".into(),
            ));
        }
        let base = worker::get(&tx, &cmd.base_check)?;
        let candidate = worker::get(&tx, &cmd.candidate_check)?;
        checks_match(&tx, &c, &base, &candidate)?;
        crate::recovery::independent_evidence(
            &tx,
            case,
            &cmd.evidence_refs,
            &cmd.command.meta.actor,
            &c.recorded_at,
            &[],
            c.recorded_version,
        )?;
        let r = Review {
            id: store::id("review"),
            candidate_id: c.id.clone(),
            case_id: case.into(),
            author: cmd.command.meta.actor.clone(),
            candidate_digest: c.digest,
            base_commit: c.base_commit,
            profile_digest: candidate.envelope.profile_digest,
            source_digest: c.source_digest,
            base_check: base.id,
            candidate_check: candidate.id,
            approve: cmd.approve,
            findings: cmd.findings,
            evidence_refs: cmd.evidence_refs,
            recorded_at: self.now(),
            current: cmd.approve,
        };
        tx.execute(
            "INSERT INTO candidate_reviews VALUES (?1,?2,?3,?4)",
            params![r.id, r.candidate_id, case, serde_json::to_string(&r)?],
        )?;
        store::emit(
            &tx,
            case,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &self.now(),
            EventData::CandidateChanged {
                candidate_id: c.id,
                kind: if r.approve {
                    "review_approved"
                } else {
                    "changes_required"
                }
                .into(),
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &r.id)?;
        tx.commit()?;
        Ok(r)
    }
}
