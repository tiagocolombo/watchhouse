//! Separately authorized publisher. Reserve every effect; uncertain writes are only inspected.
use super::{
    PublicationPlan,
    github::{Failure, Github},
    policy::{self, PublisherPolicy},
};
use crate::{
    application::{Application, operator, receipt, save_receipt},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub trait Backend {
    fn native(&self) -> bool {
        false
    }
    fn get(&self, p: &PublisherPolicy, path: &str) -> super::github::Result<Option<Value>>;
    fn write(
        &self,
        p: &PublisherPolicy,
        path: &str,
        body: &Value,
        update: bool,
    ) -> super::github::Result<Value>;
}
pub struct Native;
impl Backend for Native {
    fn native(&self) -> bool {
        true
    }
    fn get(&self, p: &PublisherPolicy, path: &str) -> super::github::Result<Option<Value>> {
        Github::new(p.clone())?.get(path)
    }
    fn write(
        &self,
        p: &PublisherPolicy,
        path: &str,
        body: &Value,
        update: bool,
    ) -> super::github::Result<Value> {
        Github::new(p.clone())?.write(path, body, update)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enqueue {
    pub command: OperatorCommand,
    pub candidate_id: String,
    pub review_id: String,
    pub policy_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub case_id: String,
    pub repair_id: String,
    pub owner: String,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub native: bool,
    pub plan: PublicationPlan,
    pub branch: String,
    pub previous_head: Option<String>,
    pub pr: Option<u64>,
    pub commit: String,
    pub commit_request: Value,
    pub marker: String,
    pub state: String,
    pub stage: String,
    pub fence: String,
    pub reads: u32,
    pub deadline: String,
    pub retry_at: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub id: String,
    pub case_id: String,
    pub repair_id: String,
    pub owner: String,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub native: bool,
    pub candidate_id: String,
    pub review_id: String,
    pub repository: String,
    pub base_commit: String,
    pub tree: String,
    pub commit: String,
    pub branch: String,
    pub pr: Option<u64>,
    pub state: String,
    pub stage: String,
    pub deadline: String,
    pub reason: String,
}
impl Job {
    pub fn status(&self) -> Status {
        Status {
            id: self.id.clone(),
            case_id: self.case_id.clone(),
            repair_id: self.repair_id.clone(),
            owner: self.owner.clone(),
            policy_revision: self.policy_revision,
            policy_digest: self.policy_digest.clone(),
            native: self.native,
            candidate_id: self.plan.candidate_id.clone(),
            review_id: self.plan.review_id.clone(),
            repository: self.plan.repository.clone(),
            base_commit: self.plan.base_commit.clone(),
            tree: self.plan.tree.sha.clone(),
            commit: self.commit.clone(),
            branch: self.branch.clone(),
            pr: self.pr,
            state: self.state.clone(),
            stage: self.stage.clone(),
            deadline: self.deadline.clone(),
            reason: self.reason.clone(),
        }
    }
}
pub(crate) fn statuses(db: &Connection, case: &str) -> Result<Vec<Status>> {
    Ok(snapshot(db, case)?.iter().map(Job::status).collect())
}
fn get(db: &Connection, id: &str) -> Result<Job> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM publication_jobs WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<Job>> {
    let mut q = db.prepare(
        "SELECT record FROM publication_jobs WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    q.query_map([case], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
fn save(tx: &Transaction<'_>, j: &Job, now: &str) -> Result<()> {
    tx.execute("INSERT INTO publication_jobs VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![j.id,j.case_id,j.repair_id,j.state,serde_json::to_string(j)?])?;
    store::emit(
        tx,
        &j.case_id,
        operator("publisher-supervisor"),
        &j.id,
        now,
        EventData::PublicationChanged {
            job_id: j.id.clone(),
            state: j.state.clone(),
            stage: j.stage.clone(),
            reason: j.reason.clone(),
        },
    )?;
    if ["blocked", "unknown", "quarantined", "published_stale"].contains(&j.state.as_str()) {
        crate::team::escalate(
            tx,
            &j.case_id,
            "publication",
            &j.id,
            &format!(
                "{} owns publication {}: {}. Inspect the reserved effect; do not repeat an uncertain write",
                j.owner, j.id, j.reason
            ),
            now,
        )?;
    }
    Ok(())
}
/// Runs with the ordinary model-independent supervisor. Expiry never permits another write.
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut q=tx.prepare("SELECT id FROM publication_jobs WHERE (state='unknown' OR (state IN ('reading','writing','ready') AND json_extract(record,'$.deadline')<=?1)) AND (NOT EXISTS(SELECT 1 FROM restore_fences f WHERE f.kind='publication_job' AND f.identity=publication_jobs.id) OR EXISTS(SELECT 1 FROM restore_read_permits p WHERE p.kind='publication_job' AND p.identity=publication_jobs.id AND p.restore_id=(SELECT id FROM restore_runs ORDER BY rowid DESC LIMIT 1) AND p.expires_at>?1)) ORDER BY rowid LIMIT 100")?;
    let ids = q
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        let mut j = get(tx, id)?;
        j.state = if j.state == "ready" {
            "blocked"
        } else {
            "reconcile"
        }
        .into();
        j.fence = store::id("fence");
        j.reason="Publisher acknowledgment lost or deadline expired; inspect exact effects without replay".into();
        save(tx, &j, now)?;
        crate::team::escalate(
            tx,
            &j.case_id,
            "publication",
            &j.id,
            &format!("{} owns publication {}: {}", j.owner, j.id, j.reason),
            now,
        )?;
    }
    Ok(ids.len())
}
fn active(db: &Connection, j: &Job, now: &str) -> Result<PublisherPolicy> {
    let latest: i64 = db.query_row("SELECT MAX(revision) FROM publisher_policies", [], |r| {
        r.get(0)
    })?;
    let p = policy::policy(db, j.policy_revision)?;
    crate::restore::require_fresh(db, "publisher", p.revision)?;
    let owner: String = db.query_row("SELECT owner FROM cases WHERE id=?1", [&j.case_id], |r| {
        r.get(0)
    })?;
    if latest != p.revision
        || p.paused
        || store::digest(&serde_json::to_string(&p)?) != j.policy_digest
        || j.deadline.as_str() <= now
        || owner != j.owner
    {
        return Err(Error::Conflict(
            "publisher policy, owner or shared deadline changed".into(),
        ));
    }
    Ok(p)
}
impl Application {
    pub fn enqueue_publication(&self, case: &str, cmd: Enqueue) -> Result<Job> {
        crate::incident_ops::human(&cmd.command.meta.actor)?;
        crate::application::text(&cmd.command.meta.reason, "publication reason", 2000)?;
        crate::application::text(&cmd.command.meta.idempotency_key, "idempotency key", 128)?;
        let fingerprint = store::digest(&format!(
            "publication:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(j) = receipt(&tx, &cmd.command.meta.idempotency_key, &fingerprint)? {
            return Ok(j);
        }
        let (owner, version): (String, i64) =
            tx.query_row("SELECT owner,version FROM cases WHERE id=?1", [case], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        if owner != cmd.command.meta.actor || version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "publication requires the current owner/version".into(),
            ));
        }
        let c = crate::real_execution::pipeline::get(&tx, &cmd.candidate_id)?;
        let plan = self.prepare_publication(case, &cmd.candidate_id, &cmd.review_id)?;
        let p = policy::policy(&tx, cmd.policy_revision)?;
        if plan.repository != p.repository {
            return Err(Error::Conflict(
                "publisher repository differs from reviewed source".into(),
            ));
        }
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM publication_jobs WHERE repair_id=?1 AND state NOT IN ('published','published_stale','simulated'))",[&c.repair_id],|r|r.get(0))?;
        if pending {
            return Err(Error::Conflict(
                "repair has an unresolved publication; inspect it before a new revision".into(),
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO publication_slots(repair_id,branch) VALUES (?1,?2)",
            params![
                c.repair_id,
                format!("watchhouse/repair-{}", uuid::Uuid::new_v4())
            ],
        )?;
        let (branch, previous_head, pr, attempts): (String, Option<String>, Option<u64>, u32) = tx
            .query_row(
                "SELECT branch,head,pr,attempts FROM publication_slots WHERE repair_id=?1",
                [&c.repair_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
        if attempts >= p.max_revisions {
            return Err(Error::Conflict(
                "shared publication revision budget exhausted".into(),
            ));
        }
        let now = self.now();
        let id = store::id("pub");
        let marker = format!(
            "Watchhouse publication {id}; candidate {}; review {}",
            plan.candidate_id, plan.review_id
        );
        let mut parents = vec![
            previous_head
                .clone()
                .unwrap_or_else(|| plan.base_commit.clone()),
        ];
        if parents[0] != plan.base_commit {
            parents.push(plan.base_commit.clone());
        }
        let (commit, commit_request) = super::commit::prepare(
            &p,
            &plan.tree.sha,
            &parents,
            &now,
            &format!("Repair {}\n\n{marker}\n", c.repair_id),
        )?;
        let deadline = std::cmp::min(
            c.deadline,
            (self.clock.now() + Duration::seconds(p.job_seconds.into()))
                .to_rfc3339_opts(SecondsFormat::Millis, true),
        );
        let j = Job {
            id,
            case_id: case.into(),
            repair_id: c.repair_id,
            owner,
            policy_revision: p.revision,
            policy_digest: store::digest(&serde_json::to_string(&p)?),
            native: false,
            plan,
            branch,
            previous_head,
            pr,
            commit,
            commit_request,
            marker,
            state: "ready".into(),
            stage: "tree".into(),
            fence: store::id("fence"),
            reads: 0,
            deadline,
            retry_at: now.clone(),
            reason: "Owned publication queued; no external effect dispatched".into(),
        };
        active(&tx, &j, &now)?;
        tx.execute(
            "UPDATE publication_slots SET attempts=attempts+1 WHERE repair_id=?1",
            [&j.repair_id],
        )?;
        save(&tx, &j, &now)?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fingerprint, &j)?;
        tx.commit()?;
        Ok(j)
    }
    pub fn publication_jobs(&self, case: &str) -> Result<Vec<Job>> {
        snapshot(&self.store.connect()?, case)
    }
    pub fn publication_statuses(&self, case: &str) -> Result<Vec<Status>> {
        let db = self.store.connect()?;
        store::case(&db, case)?;
        statuses(&db, case)
    }
    /// An OS operator may recover a lost coordinator. This grants reads only, never replay.
    pub fn reconcile_publication(&self, case: &str, id: &str, who: &str) -> Result<Job> {
        crate::incident_ops::human(who)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, id)?;
        if j.case_id != case {
            return Err(Error::NotFound);
        }
        if !["reading", "writing", "unknown", "quarantined", "blocked"].contains(&j.state.as_str())
        {
            return Err(Error::Conflict(
                "publication is not awaiting reconciliation".into(),
            ));
        }
        crate::restore::permit_read(&tx, "publication_job", id, who, &self.now())?;
        // A dead read may be repeated; a write or blocked job may only prove its effect.
        j.state = if j.state == "reading" {
            "ready"
        } else {
            "reconcile"
        }
        .into();
        j.fence = store::id("fence");
        j.retry_at = self.now();
        j.reason = format!("{who} requested scoped reconciliation without write replay");
        save(&tx, &j, &self.now())?;
        tx.commit()?;
        Ok(j)
    }
    fn publication_current(&self, j: &Job) -> Result<PublisherPolicy> {
        let p = active(&self.store.connect()?, j, &self.now())?;
        let plan = self.prepare_publication(&j.case_id, &j.plan.candidate_id, &j.plan.review_id)?;
        if serde_json::to_string(&plan)? != serde_json::to_string(&j.plan)? {
            return Err(Error::Conflict("publication inputs changed".into()));
        }
        Ok(p)
    }
    pub fn run_publisher_once(&self, b: &impl Backend) -> Result<bool> {
        {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            supervise(&tx, &self.now())?;
            tx.commit()?;
        }
        let (mut j, was_reconcile) = {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let id:Option<String>=tx.query_row("SELECT id FROM publication_jobs WHERE state IN ('ready','reconcile') AND (NOT EXISTS(SELECT 1 FROM restore_fences f WHERE f.kind='publication_job' AND f.identity=publication_jobs.id) OR EXISTS(SELECT 1 FROM restore_read_permits p WHERE p.kind='publication_job' AND p.identity=publication_jobs.id AND p.restore_id=(SELECT id FROM restore_runs ORDER BY rowid DESC LIMIT 1) AND p.expires_at>?1)) AND json_extract(record,'$.retry_at')<=?1 AND (state='reconcile' OR (NOT EXISTS(SELECT 1 FROM factory_holds h WHERE h.case_id=publication_jobs.case_id AND h.effect IN ('all','publication')) AND NOT EXISTS(SELECT 1 FROM factory_state WHERE high_at>?1))) ORDER BY rowid LIMIT 1",[self.now()],|r|r.get(0)).optional()?;
            let Some(id) = id else { return Ok(false) };
            let mut j = get(&tx, &id)?;
            let reconcile = j.state == "reconcile";
            j.native = b.native();
            j.state = "reading".into();
            j.fence = store::id("fence");
            j.reason = "Read reservation acquired before provider I/O".into();
            save(&tx, &j, &self.now())?;
            tx.commit()?;
            (j, reconcile)
        };
        let authority = if was_reconcile {
            policy::policy(&self.store.connect()?, j.policy_revision)
        } else {
            self.publication_current(&j)
        };
        let p = match authority {
            Ok(p) => p,
            Err(_) => {
                self.finish_publication(
                    &mut j,
                    "blocked",
                    "Current review, owner, policy or deadline no longer authorizes publication",
                )?;
                return Ok(true);
            }
        };
        let effect = match inspect(b, &p, &j, !was_reconcile) {
            Ok(effect) => effect,
            Err(Failure::Unavailable) => {
                self.retry_publication(&mut j, &p, was_reconcile)?;
                return Ok(true);
            }
            Err(Failure::MissingCredentials) => {
                self.finish_publication(
                    &mut j,
                    "blocked",
                    "Configured publisher credential unavailable; operator owns activation",
                )?;
                return Ok(true);
            }
            Err(_) => {
                self.finish_publication(&mut j,"quarantined","Remote base, branch, PR or object could not be verified against the reserved identity")?;
                return Ok(true);
            }
        };
        match effect {
            Observation::Present(pr) => {
                if let Some(pr) = pr {
                    j.pr = Some(pr);
                }
                let read_only = was_reconcile
                    && crate::restore::fenced(
                        &self.store.connect()?,
                        "publisher",
                        j.policy_revision,
                    )?;
                self.advance_publication(&mut j, read_only)?;
            }
            Observation::Absent if was_reconcile => self.retry_publication(&mut j, &p, true)?,
            Observation::Absent => {
                if !crate::factory::admitted(
                    &self.store.connect()?,
                    &j.case_id,
                    "publication",
                    &self.now(),
                )? {
                    self.finish_publication(
                        &mut j,
                        "ready",
                        "Factory pause holds the next write; observation retained without mutation",
                    )?;
                    return Ok(true);
                }
                // Recheck currency after reads, before reserving the actual write.
                if self.publication_current(&j).is_err() {
                    self.finish_publication(
                        &mut j,
                        "blocked",
                        "Authority changed before effect dispatch",
                    )?;
                    return Ok(true);
                }
                let (path, body, update) = write_request(&j, &p)?;
                self.finish_publication(&mut j,"writing","External write reserved; lost acknowledgment requires read-only reconciliation")?;
                // A concurrent recovery changed the fence: do not dispatch its superseded write.
                if get(&self.store.connect()?, &j.id)?.fence != j.fence {
                    return Ok(true);
                }
                match b.write(&p, &path, &body, update) {
                    Ok(value) if confirms(&j, &p, &value) => {
                        if j.stage == "pr" {
                            j.pr = value["number"].as_u64();
                        }
                        self.advance_publication(&mut j, false)?;
                    }
                    _ => self.finish_publication(
                        &mut j,
                        "unknown",
                        "Write acknowledgment missing or inconsistent; never repeat the write",
                    )?,
                }
            }
        }
        Ok(true)
    }
    fn finish_publication(&self, j: &mut Job, state: &str, reason: &str) -> Result<()> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get(&tx, &j.id)?;
        if current.fence != j.fence {
            return Err(Error::Conflict("publication reservation superseded".into()));
        }
        j.state = state.into();
        j.reason = reason.into();
        save(&tx, j, &self.now())?;
        tx.commit()?;
        Ok(())
    }
    fn retry_publication(&self, j: &mut Job, p: &PublisherPolicy, reconcile: bool) -> Result<()> {
        j.reads += 1;
        j.retry_at = (self.clock.now() + Duration::seconds(p.retry_seconds.into()))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let state = if j.reads >= p.reconciliation_attempts {
            "quarantined"
        } else if reconcile {
            "reconcile"
        } else {
            "ready"
        };
        self.finish_publication(
            j,
            state,
            "Read unavailable or uncertain write not yet visible; bounded inspection only",
        )
    }
    fn advance_publication(&self, j: &mut Job, read_only: bool) -> Result<()> {
        let next = match j.stage.as_str() {
            "tree" => "commit",
            "commit" => "ref",
            "ref" => "pr",
            "pr" => "done",
            _ => return Err(Error::Conflict("unknown publication stage".into())),
        };
        j.stage = next.into();
        j.reads = 0;
        j.retry_at = self.now();
        if next != "done" {
            return self.finish_publication(
                j,
                if read_only { "reconcile" } else { "ready" },
                if read_only {
                    "Restored publication: inspect the next existing stage without write authority"
                } else {
                    "Exact reserved effect observed; next stage awaits fresh authorization"
                },
            );
        }
        let current = self.publication_current(j).is_ok();
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if get(&tx, &j.id)?.fence != j.fence {
            return Err(Error::Conflict("publication completion superseded".into()));
        }
        j.state = if !j.native {
            "simulated"
        } else if current {
            "published"
        } else {
            "published_stale"
        }
        .into();
        j.reason =
            "Exact PR effect retained; publication is not merge, deployment or recovery".into();
        tx.execute(
            "UPDATE publication_slots SET head=?1,pr=?2 WHERE repair_id=?3",
            params![j.commit, j.pr, j.repair_id],
        )?;
        save(&tx, j, &self.now())?;
        tx.commit()?;
        Ok(())
    }
}
enum Observation {
    Absent,
    Present(Option<u64>),
}
fn inspect(
    b: &impl Backend,
    p: &PublisherPolicy,
    j: &Job,
    check_base: bool,
) -> super::github::Result<Observation> {
    if check_base {
        let base = b
            .get(p, &format!("/git/ref/heads/{}", p.base_branch))?
            .ok_or(Failure::Denied)?;
        if base["object"]["sha"] != j.plan.base_commit {
            return Err(Failure::Denied);
        }
    }
    match j.stage.as_str() {
        "tree" | "commit" => {
            let (path, sha) = if j.stage == "tree" {
                ("trees", &j.plan.tree.sha)
            } else {
                ("commits", &j.commit)
            };
            match b.get(p, &format!("/git/{path}/{sha}"))? {
                Some(v) if v["sha"] == *sha => Ok(Observation::Present(None)),
                None => Ok(Observation::Absent),
                _ => Err(Failure::InvalidResponse),
            }
        }
        "ref" => {
            let ref_path = format!("/git/ref/heads/{}", j.branch);
            match b.get(p, &ref_path)? {
                Some(v) if v["object"]["sha"] == j.commit => Ok(Observation::Present(None)),
                Some(v)
                    if j.previous_head
                        .as_ref()
                        .is_some_and(|s| v["object"]["sha"] == *s) =>
                {
                    Ok(Observation::Absent)
                }
                None if j.previous_head.is_none() => Ok(Observation::Absent),
                _ => Err(Failure::Denied),
            }
        }
        "pr" => {
            let head = b
                .get(p, &format!("/git/ref/heads/{}", j.branch))?
                .ok_or(Failure::Denied)?;
            if head["object"]["sha"] != j.commit {
                return Err(Failure::Denied);
            }
            let owner = p
                .repository
                .split('/')
                .next()
                .ok_or(Failure::InvalidConfiguration)?;
            let v = b
                .get(
                    p,
                    &format!(
                        "/pulls?state=all&head={owner}:{}&base={}&per_page=100",
                        j.branch, p.base_branch
                    ),
                )?
                .ok_or(Failure::InvalidResponse)?;
            let pulls = v.as_array().ok_or(Failure::InvalidResponse)?;
            // Zero or one is a complete bounded result; saturated/duplicate responses fail closed.
            if pulls.len() > 1 {
                return Err(Failure::Denied);
            }
            let Some(pr) = pulls.first() else {
                return if j.pr.is_none() {
                    Ok(Observation::Absent)
                } else {
                    Err(Failure::Denied)
                };
            };
            let number = pr["number"].as_u64().ok_or(Failure::InvalidResponse)?;
            if j.pr.is_some_and(|old| old != number)
                || pr["state"] != "open"
                || pr["head"]["ref"] != j.branch
                || pr["head"]["sha"] != j.commit
                || pr["head"]["repo"]["full_name"] != p.repository
                || pr["base"]["ref"] != p.base_branch
                || pr["base"]["repo"]["full_name"] != p.repository
            {
                return Err(Failure::Denied);
            }
            if pr["body"] == pr_body(j) {
                Ok(Observation::Present(Some(number)))
            } else if j.pr == Some(number) {
                Ok(Observation::Absent)
            } else {
                Err(Failure::Denied)
            }
        }
        _ => Err(Failure::InvalidConfiguration),
    }
}
pub(super) fn pr_body(j: &Job) -> String {
    format!(
        "{}\n\nFrozen base: {}\nCandidate digest: {}\nExpected tree: {}\n\nIndependent regression and review are retained in Watchhouse. Publication grants no merge or deployment authority.",
        j.marker, j.plan.base_commit, j.plan.candidate_digest, j.plan.tree.sha
    )
}
fn write_request(j: &Job, p: &PublisherPolicy) -> Result<(String, Value, bool)> {
    Ok(match j.stage.as_str() {
        "tree" => ("/git/trees".into(), j.plan.tree.request.clone(), false),
        "commit" => ("/git/commits".into(), j.commit_request.clone(), false),
        "ref" => {
            if j.previous_head.is_some() {
                (
                    format!("/git/refs/heads/{}", j.branch),
                    Github::update_body(&j.commit)
                        .map_err(|_| Error::Invalid("invalid scoped ref".into()))?,
                    true,
                )
            } else {
                (
                    "/git/refs".into(),
                    Github::branch_body(&j.branch, &j.commit)
                        .map_err(|_| Error::Invalid("invalid scoped ref".into()))?,
                    false,
                )
            }
        }
        "pr" => {
            let body =
                json!({"title":format!("Watchhouse repair {}",j.repair_id),"body":pr_body(j)});
            if let Some(n) = j.pr {
                (format!("/pulls/{n}"), body, true)
            } else {
                (
                    "/pulls".into(),
                    json!({"title":body["title"],"body":body["body"],"head":j.branch,"base":p.base_branch,"draft":p.draft}),
                    false,
                )
            }
        }
        _ => return Err(Error::Conflict("unknown publication stage".into())),
    })
}
fn confirms(j: &Job, p: &PublisherPolicy, v: &Value) -> bool {
    match j.stage.as_str() {
        "tree" => v["sha"] == j.plan.tree.sha,
        "commit" => v["sha"] == j.commit,
        "ref" => v["object"]["sha"] == j.commit,
        "pr" => {
            v["number"]
                .as_u64()
                .is_some_and(|n| j.pr.is_none_or(|old| old == n))
                && v["body"] == pr_body(j)
                && v["head"]["sha"] == j.commit
                && v["head"]["ref"] == j.branch
                && v["state"] == "open"
                && v["base"]["ref"] == p.base_branch
                && v["head"]["repo"]["full_name"] == j.plan.repository
                && v["base"]["repo"]["full_name"] == j.plan.repository
        }
        _ => false,
    }
}

pub(crate) fn factory_stop(
    tx: &Transaction<'_>,
    case: &str,
    pause: &str,
    now: &str,
) -> Result<usize> {
    let ids: Vec<String> = {
        let mut q=tx.prepare("SELECT id FROM publication_jobs WHERE case_id=?1 AND state IN ('ready','reading','writing','reconcile')")?;
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
            "Factory stop {pause}: mutation admission fenced; reconcile stable GitHub identity without replay"
        );
        save(tx, &j, now)?;
    }
    Ok(ids.len())
}
