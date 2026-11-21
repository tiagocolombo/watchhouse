//! Controlled, separately authorized operations gateway. Never executes model-provided commands.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
pub mod gateway;
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runbook {
    pub id: String,
    pub target: String,
    pub preconditions: String,
    pub stop_conditions: String,
    pub rollback: String,
    pub verification: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub service: String,
    pub environment: String,
    pub repository: String,
    pub gateway: String,
    pub token_env: String,
    pub paused: bool,
    pub request_seconds: u32,
    pub action_seconds: u32,
    pub max_actions: u32,
    pub runbooks: Vec<Runbook>,
    pub observation_ids: Vec<String>,
}
pub(crate) fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.gateway)
            .map_err(|_| Error::Invalid("invalid gateway".into()))?;
        if self.revision < 1
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.port().is_some()
            || !self.token_env.starts_with("WATCHHOUSE_")
            || !self
                .token_env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || self.token_env.len() > 128
            || !(1..=30).contains(&self.request_seconds)
            || !(1..=3600).contains(&self.action_seconds)
            || self.request_seconds > self.action_seconds
            || !(1..=10).contains(&self.max_actions)
            || self.runbooks.len() > 30
            || self.observation_ids.len() > 30
        {
            return Err(Error::Invalid(
                "operation policy exceeds trusted bounds".into(),
            ));
        }
        for (s, n) in [
            (&self.service, "service"),
            (&self.environment, "environment"),
            (&self.repository, "repository"),
        ] {
            text(s, n, 128)?;
        }
        let mut ids = std::collections::HashSet::new();
        for r in &self.runbooks {
            if !ident(&r.id) || !ident(&r.target) || !ids.insert(&r.id) {
                return Err(Error::Invalid("unique scoped runbook ids required".into()));
            }
            for s in [
                &r.preconditions,
                &r.stop_conditions,
                &r.rollback,
                &r.verification,
            ] {
                text(s, "runbook requirement", 2000)?;
            }
        }
        let mut ids = std::collections::HashSet::new();
        if self
            .observation_ids
            .iter()
            .any(|s| !ident(s) || !ids.insert(s))
        {
            return Err(Error::Invalid("unique observation ids required".into()));
        }
        Ok(())
    }
}
fn policy(db: &Connection, rev: Option<i64>) -> Result<Policy> {
    let row:Option<(String,String)>=db.query_row("SELECT record,digest FROM operation_policies WHERE revision=COALESCE(?1,(SELECT MAX(revision) FROM operation_policies))",[rev],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (raw, d) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != d {
        return Err(Error::Conflict("operation policy integrity failure".into()));
    }
    let p: Policy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(p)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub case_id: String,
    pub owner: String,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub source_digest: String,
    pub incident_revision: u64,
    pub runbook: Runbook,
    pub author: String,
    pub reviewer: Option<String>,
    pub evidence_refs: Vec<String>,
    pub review_refs: Vec<String>,
    pub created_at: String,
    pub created_version: i64,
    pub deadline: String,
    pub state: String,
    pub reason: String,
    pub native: bool,
    pub fence: String,
    pub result: Option<gateway::ActionResult>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub command: OperatorCommand,
    pub action: Action,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Propose {
        policy_revision: i64,
        runbook_id: String,
        evidence_refs: Vec<String>,
    },
    Approve {
        job_id: String,
        evidence_refs: Vec<String>,
    },
    Cancel {
        job_id: String,
    },
    Collect {
        policy_revision: i64,
        observation_id: String,
    },
    Problem {
        id: Option<String>,
        expected_revision: u32,
        title: String,
        known_error: bool,
        cause_status: String,
        hypothesis_id: Option<String>,
        hypothesis_revision: Option<u32>,
        workaround: String,
        invalidation: String,
        owner: String,
        review_seconds: u32,
        related_cases: Vec<String>,
        evidence_refs: Vec<String>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub case_id: String,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub source_digest: String,
    pub recorded_at: String,
    pub native: bool,
    pub data: gateway::Observation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub id: String,
    pub revision: u32,
    pub case_id: String,
    pub title: String,
    pub known_error: bool,
    pub cause_status: String,
    pub source_digest: String,
    pub context_current: bool,
    pub hypothesis_id: Option<String>,
    pub hypothesis_revision: Option<u32>,
    pub workaround: String,
    pub invalidation: String,
    pub owner: String,
    pub review_at: String,
    pub related_cases: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub escalated: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub jobs: Vec<Job>,
    pub observations: Vec<Observation>,
    pub problems: Vec<Problem>,
}
fn job(db: &Connection, id: &str) -> Result<Job> {
    let raw: Option<String> = db
        .query_row("SELECT record FROM operation_jobs WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
fn save(tx: &Transaction<'_>, j: &Job, now: &str) -> Result<()> {
    tx.execute("INSERT INTO operation_jobs VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![j.id,j.case_id,j.state,j.deadline,serde_json::to_string(j)?])?;
    event(tx, &j.case_id, &j.id, &j.state, &j.reason, now)?;
    if ["unknown", "blocked", "failed", "stopped", "succeeded_stale"].contains(&j.state.as_str()) {
        crate::team::escalate(
            tx,
            &j.case_id,
            "operation",
            &j.id,
            &format!(
                "{} owns operation {}: {}. Reconcile before any further action",
                j.owner, j.id, j.reason
            ),
            now,
        )?;
    }
    Ok(())
}
fn event(
    tx: &Transaction<'_>,
    case: &str,
    id: &str,
    kind: &str,
    reason: &str,
    now: &str,
) -> Result<()> {
    store::emit(
        tx,
        case,
        operator("operations-supervisor"),
        id,
        now,
        EventData::OperationChanged {
            record_id: id.into(),
            kind: kind.into(),
            reason: reason.into(),
        },
    )?;
    Ok(())
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Snapshot> {
    fn rows<T: serde::de::DeserializeOwned>(
        db: &Connection,
        sql: &str,
        case: &str,
    ) -> Result<Vec<T>> {
        let mut q = db.prepare(sql)?;
        q.query_map([case], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect()
    }
    let jobs = rows(
        db,
        "SELECT record FROM operation_jobs WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
        case,
    )?;
    let observations = rows(
        db,
        "SELECT record FROM operation_observations WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
        case,
    )?;
    let mut q=db.prepare("SELECT record,escalated FROM problem_revisions p WHERE case_id=?1 AND revision=(SELECT MAX(revision) FROM problem_revisions v WHERE v.id=p.id) ORDER BY rowid DESC LIMIT 100")?;
    let problems = q
        .query_map([case], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })?
        .map(|r| {
            let (raw, escalated) = r?;
            let mut p: Problem = serde_json::from_str(&raw)?;
            p.escalated = escalated;
            p.context_current = crate::recovery::source_digest(db, case)? == p.source_digest
                && crate::sentry_projects::context_current(db, case)?
                && crate::qualification::evidence_refs(db, case, &p.evidence_refs, true).is_ok();
            if p.cause_status == "independently_supported" {
                p.context_current &=
                    crate::incident_artifacts::snapshot(db, case)?
                        .iter()
                        .any(|a| {
                            Some(&a.id) == p.hypothesis_id.as_ref()
                                && Some(a.revision) == p.hypothesis_revision
                                && a.approval_current
                        });
            }

            Ok(p)
        })
        .collect::<Result<_>>()?;
    Ok(Snapshot {
        jobs,
        observations,
        problems,
    })
}
fn current(db: &Connection, j: &Job, now: &str) -> Result<Policy> {
    let p = policy(db, None)?;
    crate::restore::require_fresh(db, "operation", p.revision)?;
    if p.revision != j.policy_revision
        || p.paused
        || store::digest(&serde_json::to_string(&p)?) != j.policy_digest
        || now >= j.deadline.as_str()
        || crate::recovery::source_digest(db, &j.case_id)? != j.source_digest
        || !crate::sentry_projects::context_current(db, &j.case_id)?
        || !crate::incident_ops::snapshot(db, &j.case_id)?
            .is_some_and(|o| o.revision == j.incident_revision && o.roles.commander == j.owner)
    {
        return Err(Error::Conflict(
            "operation authority/context expired, changed or paused".into(),
        ));
    }
    crate::challenges::ensure_clear(db, &j.case_id)?;
    crate::qualification::evidence_refs(db, &j.case_id, &j.evidence_refs, true)?;
    if j.reviewer.is_some() {
        crate::qualification::evidence_refs(db, &j.case_id, &j.review_refs, true)?;
    }
    Ok(p)
}
impl Application {
    pub fn install_operation_policy(&self, p: Policy, who: &str, source: &str) -> Result<Policy> {
        crate::incident_ops::human(who)?;
        text(source, "configuration source", 4096)?;
        p.validate()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM operation_policies",
            [],
            |r| r.get(0),
        )?;
        let raw = serde_json::to_string(&p)?;
        if p.revision <= latest {
            if serde_json::to_string(&policy(&tx, Some(p.revision))?)? == raw {
                return Ok(p);
            }
            return Err(Error::Conflict("operation policy immutable".into()));
        }
        if p.revision != latest + 1 {
            return Err(Error::Conflict(
                "operation policy must advance one revision".into(),
            ));
        }
        tx.execute(
            "INSERT INTO operation_policies VALUES (?1,?2,?3,?4,?5,?6)",
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
    pub fn operation_policy(&self) -> Result<Policy> {
        policy(&self.store.connect()?, None)
    }
    pub fn operation_command(&self, case: &str, cmd: Command) -> Result<Snapshot> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        if matches!(cmd.action, Action::Collect { .. }) {
            return Err(Error::Invalid(
                "collect uses the separately invoked observation worker".into(),
            ));
        }
        let fp = store::digest(&format!(
            "operation:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let now = self.now();
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, case)?.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale operation command".into()));
        }
        match cmd.action {
            Action::Propose {
                policy_revision,
                runbook_id,
                evidence_refs,
            } => {
                let p = policy(&tx, None)?;
                let ops = crate::incident_ops::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
                let source = crate::sentry::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
                if p.revision != policy_revision
                    || p.paused
                    || m.actor != ops.roles.commander
                    || p.service != source.service
                    || p.environment != source.environment
                    || p.repository != source.repository
                {
                    return Err(Error::Conflict(
                        "current scoped policy and commander required".into(),
                    ));
                }
                let runbook = p
                    .runbooks
                    .iter()
                    .find(|r| r.id == runbook_id)
                    .ok_or(Error::NotFound)?
                    .clone();
                let count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM operation_jobs WHERE case_id=?1",
                    [case],
                    |r| r.get(0),
                )?;
                let unsafe_pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM operation_jobs WHERE case_id=?1 AND state IN ('reserved','unknown','approved','proposed'))",[case],|r|r.get(0))?;
                if count >= i64::from(p.max_actions) || unsafe_pending {
                    return Err(Error::Conflict(
                        "shared operation budget or unresolved prior action".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, &evidence_refs, true)?;
                let j = Job {
                    id: store::id("op"),
                    case_id: case.into(),
                    owner: ops.roles.commander,
                    policy_revision,
                    policy_digest: store::digest(&serde_json::to_string(&p)?),
                    source_digest: crate::recovery::source_digest(&tx, case)?,
                    incident_revision: ops.revision,
                    runbook,
                    author: m.actor.clone(),
                    reviewer: None,
                    evidence_refs,
                    review_refs: vec![],
                    created_at: now.clone(),
                    created_version: cmd.command.expected_version + 1,
                    deadline: (self.clock.now() + Duration::seconds(p.action_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    state: "proposed".into(),
                    reason: m.reason.clone(),
                    native: false,
                    fence: store::id("op-fence"),
                    result: None,
                };
                current(&tx, &j, &now)?;
                save(&tx, &j, &now)?;
            }
            Action::Approve {
                job_id,
                evidence_refs,
            } => {
                let mut j = job(&tx, &job_id)?;
                if j.case_id != case || j.state != "proposed" || j.author == m.actor {
                    return Err(Error::Conflict(
                        "independent current proposal review required".into(),
                    ));
                }
                current(&tx, &j, &now)?;
                crate::recovery::independent_evidence(
                    &tx,
                    case,
                    &evidence_refs,
                    &m.actor,
                    &j.created_at,
                    &j.evidence_refs,
                    j.created_version,
                )?;
                j.reviewer = Some(m.actor.clone());
                j.review_refs = evidence_refs;
                j.state = "approved".into();
                j.reason = m.reason.clone();
                save(&tx, &j, &now)?;
            }
            Action::Cancel { job_id } => {
                let mut j = job(&tx, &job_id)?;
                if j.case_id != case
                    || m.actor != j.owner
                    || !["proposed", "approved"].contains(&j.state.as_str())
                {
                    return Err(Error::Conflict("only undispatched owned actions can be cancelled; dispatched effects need reconciliation".into()));
                }
                j.state = "cancelled".into();
                j.reason = m.reason.clone();
                save(&tx, &j, &now)?;
            }
            Action::Problem {
                id,
                expected_revision,
                title,
                known_error,
                cause_status,
                hypothesis_id,
                hypothesis_revision,
                workaround,
                invalidation,
                owner,
                review_seconds,
                related_cases,
                evidence_refs,
            } => {
                for (v, n) in [
                    (&title, "title"),
                    (&workaround, "workaround"),
                    (&invalidation, "invalidation"),
                    (&owner, "owner"),
                ] {
                    text(v, n, 2000)?;
                }
                crate::incident_ops::human(&owner)?;
                // Cause remains uncertain unless the existing independent hypothesis gate is used.
                if !["unknown", "hypothesis", "independently_supported"]
                    .contains(&cause_status.as_str())
                    || !(1..=2592000).contains(&review_seconds)
                    || related_cases.len() > 30
                {
                    return Err(Error::Invalid(
                        "bounded problem scope and explicit cause status required".into(),
                    ));
                }
                if cause_status == "independently_supported" {
                    if !crate::incident_artifacts::snapshot(&tx, case)?
                        .iter()
                        .any(|a| {
                            Some(&a.id) == hypothesis_id.as_ref()
                                && Some(a.revision) == hypothesis_revision
                                && a.approval_current
                                && matches!(
                                    a.data,
                                    crate::incident_artifacts::ArtifactData::Hypothesis {
                                        state: crate::incident_artifacts::Finding::Supported,
                                        ..
                                    }
                                )
                        })
                    {
                        return Err(Error::Conflict(
                            "exact independently accepted current hypothesis required".into(),
                        ));
                    }
                } else if hypothesis_id.is_some() || hypothesis_revision.is_some() {
                    return Err(Error::Invalid(
                        "uncertain causes cannot carry an accepted-cause reference".into(),
                    ));
                }
                crate::qualification::evidence_refs(&tx, case, &evidence_refs, true)?;
                let scope = crate::sentry::snapshot(&tx, case)?.ok_or(Error::NotFound)?;
                for related in &related_cases {
                    let other = crate::sentry::snapshot(&tx, related)?.ok_or(Error::NotFound)?;
                    if scope.service != other.service
                        || scope.environment != other.environment
                        || scope.repository != other.repository
                    {
                        return Err(Error::Invalid(
                            "problem links require exact service/environment/repository".into(),
                        ));
                    }
                }
                let id = id.unwrap_or_else(|| store::id("problem"));
                let rev: u32 = tx.query_row(
                    "SELECT COALESCE(MAX(revision),0) FROM problem_revisions WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )?;
                if rev!=expected_revision || (rev>0 && !tx.query_row("SELECT EXISTS(SELECT 1 FROM problem_revisions WHERE id=?1 AND case_id=?2)",params![id,case],|r|r.get::<_,bool>(0))?){return Err(Error::Conflict("stale/foreign problem revision".into()));}
                let p = Problem {
                    id,
                    revision: rev + 1,
                    case_id: case.into(),
                    title,
                    known_error,
                    cause_status,
                    source_digest: crate::recovery::source_digest(&tx, case)?,
                    context_current: true,
                    hypothesis_id,
                    hypothesis_revision,
                    workaround,
                    invalidation,
                    owner,
                    review_at: (self.clock.now() + Duration::seconds(review_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    related_cases,
                    evidence_refs,
                    escalated: false,
                };
                tx.execute(
                    "INSERT INTO problem_revisions VALUES (?1,?2,?3,?4,0,?5)",
                    params![
                        p.id,
                        p.revision,
                        case,
                        p.review_at,
                        serde_json::to_string(&p)?
                    ],
                )?;
                event(&tx, case, &p.id, "problem", &m.reason, &now)?;
            }
            Action::Collect { .. } => unreachable!(),
        }
        let result = snapshot(&tx, case)?;
        save_receipt(&tx, &m.idempotency_key, &fp, &result)?;
        tx.commit()?;
        Ok(result)
    }
    /// Each action is reserved once. Restart/lost acknowledgments can only GET the same identity.
    pub fn operation_tick(&self, backend: &dyn gateway::Backend) -> Result<bool> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id: Option<String> = tx
            .query_row(
                "SELECT id FROM operation_jobs j WHERE state='approved' AND NOT EXISTS(SELECT 1 FROM factory_holds h WHERE h.case_id=j.case_id AND h.effect IN ('all','operation')) AND NOT EXISTS(SELECT 1 FROM factory_state WHERE high_at>?1) ORDER BY j.rowid LIMIT 1",
                [self.now()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(false) };
        let mut j = job(&tx, &id)?;
        let now = self.now();
        let p = match current(&tx, &j, &now) {
            Ok(p) => p,
            Err(_) => {
                j.state = "blocked".into();
                j.reason = "stale, paused, challenged or expired operation authority".into();
                save(&tx, &j, &now)?;
                tx.commit()?;
                return Ok(true);
            }
        };
        if j.reviewer.is_none() {
            return Err(Error::Conflict("independent approval missing".into()));
        }
        j.state = "reserved".into();
        j.native = backend.native();
        j.reason = "action reserved before gateway I/O; no write replay".into();
        save(&tx, &j, &now)?;
        tx.commit()?;
        let result = backend.execute(&p, &j);
        self.finish_operation(&j, result, false)?;
        Ok(true)
    }
    pub fn reconcile_operation(
        &self,
        case: &str,
        id: &str,
        backend: &dyn gateway::Backend,
    ) -> Result<Job> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = job(&tx, id)?;
        if j.case_id != case || !["unknown", "reserved"].contains(&j.state.as_str()) {
            return Err(Error::Conflict(
                "only uncertain owned effects can be reconciled".into(),
            ));
        }
        let p = policy(&tx, Some(j.policy_revision))?;
        j.native = backend.native();
        j.fence = store::id("read-fence");
        j.state = "unknown".into();
        j.reason = "Read-only gateway reconciliation reserved; no action replay".into();
        save(&tx, &j, &self.now())?;
        tx.commit()?;
        let result = backend.lookup(&p, &j);
        self.finish_operation(&j, result, true)
    }
    fn finish_operation(
        &self,
        reserved: &Job,
        result: std::result::Result<gateway::ActionResult, gateway::Failure>,
        reconcile: bool,
    ) -> Result<Job> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = job(&tx, &reserved.id)?;
        let now = self.now();
        if j.fence != reserved.fence || !["reserved", "unknown"].contains(&j.state.as_str()) {
            return Err(Error::Conflict("late operation result".into()));
        }
        match result {
            Ok(r)
                if r.operation_id == j.id
                    && r.runbook_id == j.runbook.id
                    && r.target == j.runbook.target
                    && ["succeeded", "failed", "stopped", "running"]
                        .contains(&r.state.as_str()) =>
            {
                j.state = if r.state == "running" {
                    "unknown".into()
                } else if current(&tx, &j, &now).is_err() && r.state == "succeeded" {
                    "succeeded_stale".into()
                } else {
                    r.state.clone()
                };
                j.reason =
                    "gateway-reported action status; independent service recovery still required"
                        .into();
                j.result = Some(r);
            }
            Err(gateway::Failure::MissingCredentials | gateway::Failure::Denied) if !reconcile => {
                j.state = "blocked".into();
                j.reason = "gateway denied action or dedicated credential unavailable".into();
            }
            _ => {
                j.state = "unknown".into();
                j.reason =
                    "effect outcome unknown; inspect stable gateway identity; never resubmit"
                        .into();
            }
        }
        save(&tx, &j, &now)?;
        tx.commit()?;
        Ok(j)
    }
    pub fn collect_operation_observation(
        &self,
        case: &str,
        cmd: Command,
        backend: &dyn gateway::Backend,
    ) -> Result<Observation> {
        let Action::Collect {
            policy_revision,
            observation_id,
        } = &cmd.action
        else {
            return Err(Error::Invalid("collect command required".into()));
        };
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!(
            "operation-observation:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        {
            let tx = db.transaction()?;
            if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
                return Ok(r);
            }
        }
        let p = policy(&db, None)?;
        crate::restore::require_fresh(&db, "operation", p.revision)?;
        let source = crate::sentry::snapshot(&db, case)?.ok_or(Error::NotFound)?;
        if p.revision != *policy_revision
            || !p.observation_ids.contains(observation_id)
            || p.service != source.service
            || p.environment != source.environment
            || p.repository != source.repository
        {
            return Err(Error::Conflict("scoped observation policy required".into()));
        }
        let digest = crate::recovery::source_digest(&db, case)?;
        let data = backend.observe(&p, observation_id).map_err(|_| {
            Error::Conflict("gateway observation unavailable; coverage unknown".into())
        })?;
        data.validate(&p, self.clock.now())?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        if store::case(&tx, case)?.version != cmd.command.expected_version
            || policy(&tx, None)?.revision != p.revision
            || crate::recovery::source_digest(&tx, case)? != digest
            || !crate::sentry_projects::context_current(&tx, case)?
        {
            return Err(Error::Conflict(
                "observation configuration or context changed during read".into(),
            ));
        }
        let r = Observation {
            id: store::id("observation"),
            case_id: case.into(),
            policy_revision: p.revision,
            policy_digest: store::digest(&serde_json::to_string(&p)?),
            source_digest: digest,
            recorded_at: self.now(),
            native: backend.native(),
            data,
        };
        tx.execute(
            "INSERT INTO operation_observations VALUES (?1,?2,?3)",
            params![r.id, case, serde_json::to_string(&r)?],
        )?;
        event(
            &tx,
            case,
            &r.id,
            "observation",
            "Authenticated gateway report; no automatic recovery or deployment inference",
            &r.recorded_at,
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut q=tx.prepare("SELECT id FROM operation_jobs WHERE state IN ('proposed','approved','reserved') AND deadline<=?1 ORDER BY deadline LIMIT 100")?;
    let ids = q
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        let mut j = job(tx, id)?;
        j.state = if j.state == "reserved" {
            "unknown"
        } else {
            "blocked"
        }
        .into();
        j.reason = "operation deadline exceeded; owner must reconcile any reserved effect".into();
        save(tx, &j, now)?;
    }
    let mut q=tx.prepare("SELECT record FROM problem_revisions p WHERE escalated=0 AND review_at<=?1 AND revision=(SELECT MAX(revision) FROM problem_revisions v WHERE v.id=p.id) ORDER BY review_at LIMIT 100")?;
    let problems = q
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for raw in &problems {
        let p: Problem = serde_json::from_str(raw)?;
        tx.execute(
            "UPDATE problem_revisions SET escalated=1 WHERE id=?1 AND revision=?2",
            params![p.id, p.revision],
        )?;
        crate::team::escalate(
            tx,
            &p.case_id,
            "problem_review",
            &p.id,
            &format!(
                "{} owns known-error/problem review: {}. {}",
                p.owner, p.title, p.invalidation
            ),
            now,
        )?;
        event(
            tx,
            &p.case_id,
            &p.id,
            "problem_due",
            "Problem review remains due independently of incident recovery",
            now,
        )?;
    }
    Ok(ids.len() + problems.len())
}

pub(crate) fn factory_stop(
    tx: &Transaction<'_>,
    case: &str,
    pause: &str,
    now: &str,
) -> Result<usize> {
    let ids: Vec<String> = {
        let mut q=tx.prepare("SELECT id FROM operation_jobs WHERE case_id=?1 AND state IN ('proposed','approved','reserved','unknown')")?;
        q.query_map([case], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?
    };
    for id in &ids {
        let mut j = job(tx, id)?;
        j.state = if matches!(j.state.as_str(), "proposed" | "approved") {
            "blocked"
        } else {
            "unknown"
        }
        .into();
        j.fence = store::id("factory-stop-fence");
        j.reason = format!(
            "Factory stop {pause}: admission fenced, not proof of target rollback/stop; reconcile the original gateway operation"
        );
        save(tx, &j, now)?;
    }
    Ok(ids.len())
}
