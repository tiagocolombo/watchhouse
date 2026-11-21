//! Automatic eligible merging uses separate OS authority and never bypasses repository protection.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
pub mod github;
#[cfg(all(test, target_os = "linux"))]
mod tests;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub name: String,
    pub app_id: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub repository: String,
    pub base_branch: String,
    pub token_env: String,
    pub paused: bool,
    pub allowed_paths: Vec<String>,
    pub denied_paths: Vec<String>,
    pub required_checks: Vec<Check>,
    pub required_approvals: u32,
    pub max_changed_bytes: usize,
    pub request_seconds: u32,
    pub job_seconds: u32,
}
fn path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && !s.starts_with('/')
        && !s.contains('\\')
        && !s.chars().any(|c| c.is_control())
        && s.trim_end_matches('/')
            .split('/')
            .all(|p| !p.is_empty() && ![".", ".."].contains(&p))
}
fn includes(rule: &str, value: &str) -> bool {
    if rule.ends_with('/') {
        value.starts_with(rule)
    } else {
        rule == value
    }
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        let repo = self.repository.split_once('/').is_some_and(|(a, b)| {
            [a, b].iter().all(|s| {
                crate::operations::ident(s)
                    || s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                        && !s.is_empty()
                        && ![".", ".."].contains(s)
            })
        });
        if self.revision < 1
            || !repo
            || self.repository.len() > 256
            || !crate::delivery::policy::branch(&self.base_branch)
            || self.allowed_paths.is_empty()
            || self.allowed_paths.len() > 50
            || self.denied_paths.len() > 50
            || self
                .allowed_paths
                .iter()
                .chain(&self.denied_paths)
                .any(|s| !path(s))
            || self.required_checks.is_empty()
            || self.required_checks.len() > 30
            || self
                .required_checks
                .iter()
                .any(|c| c.name.trim().is_empty() || c.name.len() > 128 || c.app_id == 0)
            || !(1..=10).contains(&self.required_approvals)
            || !(1..=1024 * 1024).contains(&self.max_changed_bytes)
            || !(1..=10).contains(&self.request_seconds)
            || !(30..=600).contains(&self.job_seconds)
            || !self.token_env.starts_with("WATCHHOUSE_")
            || self.token_env.len() > 128
            || !self
                .token_env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(Error::Invalid(
                "merge policy exceeds explicit trusted bounds".into(),
            ));
        }
        let mut names = std::collections::HashSet::new();
        if self.required_checks.iter().any(|c| !names.insert(&c.name)) {
            return Err(Error::Invalid("unique required checks needed".into()));
        }
        Ok(())
    }
    pub fn permits(&self, s: &str) -> bool {
        path(s)
            && ![
                ".github/",
                ".watchhouse/",
                "policy/",
                "implementation/",
                "migrations/",
                "AGENTS.md",
            ]
            .iter()
            .any(|r| includes(r, s))
            && !s.split('/').any(|c| {
                c == ".env" || c.starts_with(".env.") || c.ends_with(".pem") || c.ends_with(".key")
            })
            && self.allowed_paths.iter().any(|r| includes(r, s))
            && !self.denied_paths.iter().any(|r| includes(r, s))
    }
}
fn policy(db: &Connection, rev: Option<i64>) -> Result<Policy> {
    let row:Option<(String,String)>=db.query_row("SELECT record,digest FROM merge_policies WHERE revision=COALESCE(?1,(SELECT MAX(revision) FROM merge_policies))",[rev],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("merge policy integrity failure".into()));
    }
    let p: Policy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(p)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enqueue {
    pub command: OperatorCommand,
    pub publication_id: String,
    pub policy_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retry {
    pub command: OperatorCommand,
    pub job_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub case_id: String,
    pub publication_id: String,
    pub candidate_id: String,
    pub review_id: String,
    pub owner: String,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub repository: String,
    pub base_branch: String,
    pub head: String,
    pub base: String,
    pub tree: String,
    pub branch: String,
    pub pr: u64,
    pub paths: Vec<String>,
    pub state: String,
    pub reason: String,
    pub deadline: String,
    pub fence: String,
    pub native: bool,
    pub attempts: u32,
    pub observation: Option<github::Observation>,
    pub merged_commit: Option<String>,
}
fn get(db: &Connection, id: &str) -> Result<Job> {
    let raw: Option<String> = db
        .query_row("SELECT record FROM merge_jobs WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<Job>> {
    let mut q =
        db.prepare("SELECT record FROM merge_jobs WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100")?;
    q.query_map([case], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
fn save(tx: &Transaction<'_>, j: &Job, now: &str) -> Result<()> {
    tx.execute("INSERT INTO merge_jobs VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![j.id,j.case_id,j.publication_id,j.state,j.deadline,serde_json::to_string(j)?])?;
    store::emit(
        tx,
        &j.case_id,
        operator("merge-supervisor"),
        &j.id,
        now,
        EventData::MergeChanged {
            job_id: j.id.clone(),
            state: j.state.clone(),
            reason: j.reason.clone(),
        },
    )?;
    if ["blocked", "unknown", "merged_stale"].contains(&j.state.as_str()) {
        crate::team::escalate(
            tx,
            &j.case_id,
            "merge",
            &j.id,
            &format!(
                "{} owns merge {}: {}. Reconcile the exact PR before any further action",
                j.owner, j.id, j.reason
            ),
            now,
        )?;
    }
    Ok(())
}
fn current(db: &Connection, j: &Job, now: &str) -> Result<Policy> {
    let p = policy(db, None)?;
    crate::restore::require_fresh(db, "merge", p.revision)?;
    if p.revision != j.policy_revision
        || p.paused
        || store::digest(&serde_json::to_string(&p)?) != j.policy_digest
        || now >= j.deadline.as_str()
        || store::case(db, &j.case_id)?.owner != j.owner
    {
        return Err(Error::Conflict(
            "merge authority, owner or deadline changed".into(),
        ));
    }
    let c = crate::real_execution::pipeline::get(db, &j.candidate_id)?;
    crate::real_execution::pipeline::current(db, &c, now)?;
    let r = crate::real_execution::pipeline::review(db, &j.review_id, now)?;
    if !r.current
        || r.candidate_id != c.id
        || c.repository != j.repository
        || c.base_commit != j.base
        || c.changes.iter().any(|c| !p.permits(&c.path))
        || c.changes
            .iter()
            .map(|c| c.after.as_ref().map_or(0, String::len))
            .sum::<usize>()
            > p.max_changed_bytes
    {
        return Err(Error::Conflict(
            "exact candidate review or permitted change scope invalid".into(),
        ));
    }
    let pubs = crate::delivery::lifecycle::snapshot(db, &j.case_id)?;
    let published = pubs
        .iter()
        .find(|v| v.id == j.publication_id)
        .ok_or(Error::NotFound)?;
    if !published.native
        || published.state != "published"
        || published.commit != j.head
        || published.plan.tree.sha != j.tree
        || published.pr != Some(j.pr)
        || published.plan.candidate_id != c.id
        || published.plan.review_id != r.id
    {
        return Err(Error::Conflict(
            "current native publication lineage required".into(),
        ));
    }
    Ok(p)
}
impl Application {
    pub fn install_merge_policy(&self, p: Policy, who: &str, source: &str) -> Result<Policy> {
        crate::incident_ops::human(who)?;
        text(source, "merge policy source", 4096)?;
        p.validate()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let last: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM merge_policies",
            [],
            |r| r.get(0),
        )?;
        let raw = serde_json::to_string(&p)?;
        if p.revision <= last {
            if serde_json::to_string(&policy(&tx, Some(p.revision))?)? == raw {
                return Ok(p);
            }
            return Err(Error::Conflict("merge policy immutable".into()));
        }
        if p.revision != last + 1 {
            return Err(Error::Conflict(
                "consecutive merge policy revision required".into(),
            ));
        }
        tx.execute(
            "INSERT INTO merge_policies VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                p.revision,
                store::digest(&raw),
                raw,
                who,
                source,
                self.now()
            ],
        )?;
        tx.commit()?;
        Ok(p)
    }
    pub fn merge_policy(&self) -> Result<Policy> {
        policy(&self.store.connect()?, None)
    }
    pub fn enqueue_merge(&self, case: &str, cmd: Enqueue) -> Result<Job> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.reason, "merge reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!("merge:{case}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = store::case(&tx, case)?;
        if c.owner != m.actor || c.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "current owned case/version required".into(),
            ));
        }
        let p = policy(&tx, None)?;
        if p.revision != cmd.policy_revision {
            return Err(Error::Conflict("current merge policy required".into()));
        }
        let pubs = crate::delivery::lifecycle::snapshot(&tx, case)?;
        let published = pubs
            .iter()
            .find(|v| v.id == cmd.publication_id)
            .ok_or(Error::NotFound)?;
        let candidate = crate::real_execution::pipeline::get(&tx, &published.plan.candidate_id)?;
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM merge_jobs WHERE json_extract(record,'$.repository')=?1 AND json_extract(record,'$.base_branch')=?2 AND state IN ('ready','reserved','unknown'))",params![p.repository,p.base_branch],|r|r.get(0))?;
        if pending || p.repository != published.plan.repository {
            return Err(Error::Conflict(
                "merge repository has conflicting pending work or different scope".into(),
            ));
        }
        let mut paths = candidate
            .changes
            .iter()
            .map(|c| c.path.clone())
            .collect::<Vec<_>>();
        paths.sort();
        let j = Job {
            id: store::id("merge"),
            case_id: case.into(),
            publication_id: published.id.clone(),
            candidate_id: published.plan.candidate_id.clone(),
            review_id: published.plan.review_id.clone(),
            owner: m.actor.clone(),
            policy_revision: p.revision,
            policy_digest: store::digest(&serde_json::to_string(&p)?),
            repository: p.repository.clone(),
            base_branch: p.base_branch.clone(),
            head: published.commit.clone(),
            base: published.plan.base_commit.clone(),
            tree: published.plan.tree.sha.clone(),
            branch: published.branch.clone(),
            pr: published.pr.ok_or(Error::NotFound)?,
            paths,
            state: "ready".into(),
            reason: m.reason.clone(),
            deadline: (self.clock.now() + Duration::seconds(p.job_seconds.into()))
                .to_rfc3339_opts(SecondsFormat::Millis, true),
            fence: store::id("merge-fence"),
            native: false,
            attempts: 0,
            observation: None,
            merged_commit: None,
        };
        current(&tx, &j, &self.now())?;
        save(&tx, &j, &self.now())?;
        save_receipt(&tx, &m.idempotency_key, &fp, &j)?;
        tx.commit()?;
        Ok(j)
    }
    pub fn run_merge_once(&self, b: &dyn github::Backend) -> Result<bool> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<String> = tx
            .query_row(
                "SELECT id FROM merge_jobs j WHERE state='ready' AND NOT EXISTS(SELECT 1 FROM factory_holds h WHERE h.case_id=j.case_id AND h.effect IN ('all','merge')) AND NOT EXISTS(SELECT 1 FROM factory_state WHERE high_at>?1) ORDER BY j.rowid LIMIT 1",
                [self.now()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(false) };
        let mut j = get(&tx, &id)?;
        let p = match current(&tx, &j, &self.now()) {
            Ok(p) => p,
            Err(_) => {
                j.state = "blocked".into();
                j.reason = "local merge admission changed; revalidation required".into();
                save(&tx, &j, &self.now())?;
                tx.commit()?;
                return Ok(true);
            }
        };
        // Reserve even the preflight so competing coordinators cannot dispatch the same job.
        j.attempts += 1;
        j.state = "reserved".into();
        j.native = b.native();
        j.reason = "reserved; final read and at most one merge PUT".into();
        save(&tx, &j, &self.now())?;
        tx.commit()?;
        let observed = b.inspect(&p, &j);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut actual = get(&tx, &j.id)?;
        if actual.fence != j.fence || actual.state != "reserved" {
            return Err(Error::Conflict(
                "merge reservation changed during final read".into(),
            ));
        }
        if !crate::factory::admitted(&tx, &j.case_id, "merge", &self.now())? {
            actual.state = "ready".into();
            actual.reason = "Factory paused during final read; no merge mutation dispatched".into();
            save(&tx, &actual, &self.now())?;
            tx.commit()?;
            return Ok(true);
        }
        let eligible = observed
            .as_ref()
            .ok()
            .is_some_and(|o| github::eligible(&p, &j, o).is_ok());
        if current(&tx, &j, &self.now()).is_err() || !eligible {
            actual.state = "blocked".into();
            actual.reason="final remote head/base/checks/approvals/protection or local authority did not pass".into();
            actual.observation = observed.ok();
            save(&tx, &actual, &self.now())?;
            tx.commit()?;
            return Ok(true);
        }
        actual.observation = observed.ok();
        save(&tx, &actual, &self.now())?;
        tx.commit()?;
        // The backend must enforce exact head and protected strict up-to-date checks at effect time.
        let result = b.merge(&p, &actual);
        self.finish_merge(&actual, result)?;
        Ok(true)
    }
    fn finish_merge(
        &self,
        reserved: &Job,
        result: std::result::Result<github::Merged, github::Failure>,
    ) -> Result<Job> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, &reserved.id)?;
        if j.fence != reserved.fence || !["reserved", "unknown"].contains(&j.state.as_str()) {
            return Err(Error::Conflict(
                "late merge result retained remotely; reconcile current identity".into(),
            ));
        }
        match result {
            Ok(r)
                if r.head == j.head
                    && r.base == j.base
                    && r.tree == j.tree
                    && crate::delivery::objects::oid(&r.commit) =>
            {
                j.state = if !j.native {
                    "simulated"
                } else if current(&tx, &j, &self.now()).is_ok() {
                    "merged"
                } else {
                    "merged_stale"
                }
                .into();
                j.merged_commit = Some(r.commit);
                j.reason="exact merge commit/parents/tree observed; deployment and recovery remain separate".into();
            }
            Err(github::Failure::Denied) => {
                j.state = "blocked".into();
                j.reason =
                    "repository refused the exact merge; no bypass/rebase/update performed".into();
            }
            _ => {
                j.state = "unknown".into();
                j.reason =
                    "merge outcome unknown; only read-only exact PR/commit reconciliation allowed"
                        .into();
            }
        }
        save(&tx, &j, &self.now())?;
        tx.commit()?;
        Ok(j)
    }
    pub fn retry_merge(&self, case: &str, cmd: Retry) -> Result<Job> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.reason, "retry reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!(
            "merge-retry:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let mut j = get(&tx, &cmd.job_id)?;
        if j.case_id != case
            || m.actor != j.owner
            || store::case(&tx, case)?.version != cmd.command.expected_version
            || j.state != "blocked"
            || j.attempts >= 3
        {
            return Err(Error::Conflict("only a current owned definitively blocked merge can retry within its original three-attempt budget".into()));
        }
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM merge_jobs WHERE id!=?1 AND json_extract(record,'$.repository')=?2 AND json_extract(record,'$.base_branch')=?3 AND state IN ('ready','reserved','unknown'))",params![j.id,j.repository,j.base_branch],|r|r.get(0))?;
        if pending {
            return Err(Error::Conflict(
                "another merge owns this repository/base".into(),
            ));
        }
        current(&tx, &j, &self.now())?;
        j.state = "ready".into();
        j.fence = store::id("merge-fence");
        j.reason = m.reason.clone();
        save(&tx, &j, &self.now())?;
        save_receipt(&tx, &m.idempotency_key, &fp, &j)?;
        tx.commit()?;
        Ok(j)
    }
    pub fn reconcile_merge(&self, case: &str, id: &str, b: &dyn github::Backend) -> Result<Job> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, id)?;
        if j.case_id != case || !["reserved", "unknown"].contains(&j.state.as_str()) {
            return Err(Error::Conflict("uncertain merge identity required".into()));
        }
        let p = policy(&tx, Some(j.policy_revision))?;
        j.native = b.native();
        j.fence = store::id("read-fence");
        j.state = "unknown".into();
        j.reason = "Read-only exact merge reconciliation reserved; no merge replay".into();
        save(&tx, &j, &self.now())?;
        tx.commit()?;
        self.finish_merge(
            &j,
            b.reconcile(&p, &j)
                .map_err(|_| github::Failure::Unavailable),
        )
    }
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut q=tx.prepare("SELECT id FROM merge_jobs WHERE state IN ('ready','reserved') AND deadline<=?1 ORDER BY deadline LIMIT 100")?;
    let ids = q
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        let mut j = get(tx, id)?;
        j.state = if j.state == "reserved" {
            "unknown"
        } else {
            "blocked"
        }
        .into();
        j.reason = "merge deadline exceeded; uncertain effect must be reconciled".into();
        save(tx, &j, now)?;
    }
    Ok(ids.len())
}

pub(crate) fn factory_stop(
    tx: &Transaction<'_>,
    case: &str,
    pause: &str,
    now: &str,
) -> Result<usize> {
    let ids: Vec<String> = {
        let mut q=tx.prepare("SELECT id FROM merge_jobs WHERE case_id=?1 AND state IN ('ready','reserved','unknown')")?;
        q.query_map([case], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?
    };
    for id in &ids {
        let mut j = get(tx, id)?;
        j.state = if j.state == "ready" {
            "blocked"
        } else {
            "unknown"
        }
        .into();
        j.fence = store::id("factory-stop-fence");
        j.reason = format!(
            "Factory stop {pause}: late effects cannot advance this reservation; read-only reconciliation remains required"
        );
        save(tx, &j, now)?;
    }
    Ok(ids.len())
}
