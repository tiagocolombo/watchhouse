//! Durable reservation/dispatch/reconciliation for external containers, independent of models.
use super::{
    docker::{ContainerPolicy, Docker, EngineIdentity, Observation},
    protocol::*,
};
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};
pub type Effect<T> = Pin<Box<dyn Future<Output = Result<T>> + Send>>;
#[derive(Clone)]
pub struct Request {
    pub policy: ContainerPolicy,
    pub job: WorkerJob,
    pub input: PathBuf,
    pub control: PathBuf,
    pub probe: bool,
}
pub trait Backend: Send + Sync {
    fn native(&self) -> bool {
        false
    }
    fn engine(&self, p: ContainerPolicy) -> Effect<EngineIdentity>;
    fn create(&self, r: Request) -> Effect<String>;
    fn inspect(&self, r: Request) -> Effect<Observation>;
    fn start(&self, r: Request) -> Effect<WorkerOutput>;
    fn logs(&self, r: Request) -> Effect<WorkerOutput>;
    fn cancel(&self, r: Request) -> Effect<()>;
}
pub struct Native;
impl Backend for Native {
    fn native(&self) -> bool {
        true
    }
    fn engine(&self, p: ContainerPolicy) -> Effect<EngineIdentity> {
        Box::pin(async move { Docker::new(p)?.engine().await })
    }
    fn create(&self, r: Request) -> Effect<String> {
        Box::pin(async move {
            Docker::new(r.policy)?
                .create(&r.job, &r.input, &r.control, r.probe)
                .await
        })
    }
    fn inspect(&self, r: Request) -> Effect<Observation> {
        Box::pin(async move {
            Docker::new(r.policy)?
                .inspect(&r.job, &r.input, &r.control, r.probe)
                .await
        })
    }
    fn start(&self, r: Request) -> Effect<WorkerOutput> {
        Box::pin(async move {
            Docker::new(r.policy)?
                .start(&r.job, &r.input, &r.control, r.probe)
                .await
        })
    }
    fn logs(&self, r: Request) -> Effect<WorkerOutput> {
        Box::pin(async move {
            Docker::new(r.policy)?
                .logs(&r.job, &r.input, &r.control, r.probe)
                .await
        })
    }
    fn cancel(&self, r: Request) -> Effect<()> {
        Box::pin(async move {
            Docker::new(r.policy)?
                .cancel(&r.job, &r.input, &r.control, r.probe)
                .await
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerJob {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub owner: String,
    pub kind: String,
    pub candidate_id: Option<String>,
    pub policy_revision: i64,
    pub envelope: WorkerJob,
    pub root: PathBuf,
    pub state: String,
    pub deadline: String,
    pub recorded_at: String,
    pub reason: String,
    pub engine: Option<EngineIdentity>,
    pub container_id: Option<String>,
    pub create_dispatched: bool,
    pub start_dispatched: bool,
    pub cancel_requested: bool,
    pub native: bool,
    pub result: Option<WorkerOutput>,
    pub evidence_digest: Option<String>,
    pub quarantined: bool,
    pub cleanup_attempts: u32,
    pub cleanup_next_at: String,
    pub recorded_version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Admission {
    pub qualification_id: String,
    pub engine: EngineIdentity,
    pub policy_digest: String,
    pub expires_at: String,
    pub evidence_digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualify {
    pub command: OperatorCommand,
    pub policy_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manage {
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub command: OperatorCommand,
    pub job_id: String,
    pub action: ManageAction,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManageAction {
    Reconcile,
    Cancel,
    ReportCleanup,
}
pub(crate) const CHECKS: &[&str] = &[
    "input_verified",
    "runner_identity",
    "non_root",
    "trusted_init_uid",
    "minimal_init_capabilities",
    "no_capabilities",
    "protocol_protected",
    "descendant_started",
    "removal_acknowledged",
    "host_canary_held",
    "no_new_privileges",
    "seccomp_enforced",
    "private_network",
    "private_pid",
    "memory_limit",
    "pids_limit",
    "cpu_limit",
    "input_read_only",
    "policy_read_only",
    "root_read_only",
    "scratch_writable",
    "no_credentials",
    "scratch_quota",
];
pub(crate) fn policy(db: &Connection, revision: i64) -> Result<ContainerPolicy> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM container_policies WHERE revision=?1",
            [revision],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("container policy integrity failure".into()));
    }
    Ok(serde_json::from_str(&raw)?)
}
pub(crate) fn get(db: &Connection, id: &str) -> Result<ContainerJob> {
    let raw: Option<String> = db
        .query_row("SELECT record FROM container_jobs WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
pub(crate) fn trusted_case_policy(db: &Connection, case: &str) -> Result<crate::policy::Policy> {
    let p = crate::policy::for_case(db, case)?;
    let installer: String = db.query_row(
        "SELECT installed_by FROM policy_versions WHERE revision=?1",
        [p.revision],
        |r| r.get(0),
    )?;
    if installer == "compiled-bootstrap" {
        return Err(Error::Conflict("native work requires a case pinned to an explicitly installed operator policy; bootstrap policy authorizes synthetic work only".into()));
    }
    Ok(p)
}
pub(crate) fn admission(db: &Connection, p: &ContainerPolicy, now: &str) -> Result<Admission> {
    let active: i64 = db.query_row("SELECT MAX(revision) FROM container_policies", [], |r| {
        r.get(0)
    })?;
    if active != p.revision {
        return Err(Error::Conflict(
            "new execution requires active container policy".into(),
        ));
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM container_admissions WHERE revision=?1",
            [p.revision],
            |r| r.get(0),
        )
        .optional()?;
    let a: Admission = serde_json::from_str(&raw.ok_or_else(|| {
        Error::Conflict("container worker has no measured native admission".into())
    })?)?;
    let q = get(db, &a.qualification_id)?;
    let latest: String = db.query_row("SELECT id FROM container_jobs WHERE policy_revision=?1 AND json_extract(record,'$.kind')='qualification' ORDER BY rowid DESC LIMIT 1",[p.revision],|r|r.get(0))?;
    let evidence: Option<(String, String)> = db
        .query_row(
            "SELECT content,provenance FROM evidence WHERE digest=?1",
            [&a.evidence_digest],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if latest != q.id
        || q.state != "passed"
        || !q.native
        || q.kind != "qualification"
        || q.quarantined
        || q.evidence_digest.as_deref() != Some(&a.evidence_digest)
        || evidence.is_none_or(|(raw, origin)| {
            store::digest(&raw) != a.evidence_digest || origin != "native-container-v1"
        })
    {
        return Err(Error::Conflict(
            "native admission evidence missing, stale or corrupt".into(),
        ));
    }
    if a.policy_digest != p.digest()? || a.expires_at.as_str() <= now {
        return Err(Error::Conflict(
            "container admission expired or policy changed".into(),
        ));
    }
    Ok(a)
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<ContainerJob>> {
    let mut q = db.prepare(
        "SELECT record FROM container_jobs WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    q.query_map([case], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
fn diagnostic(e: Error) -> String {
    match e {
        Error::Invalid(message) | Error::Conflict(message) => message.chars().take(1000).collect(),
        _ => "protocol/persistence validation failed".into(),
    }
}
fn supervisor() -> Actor {
    Actor {
        kind: "supervisor".into(),
        id: "native-execution".into(),
    }
}
pub(crate) fn save(
    tx: &Transaction<'_>,
    j: &ContainerJob,
    actor: Actor,
    cause: &str,
    now: &str,
) -> Result<()> {
    tx.execute("INSERT INTO container_jobs VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![j.id,j.case_id,j.policy_revision,j.state,j.deadline,serde_json::to_string(j)?])?;
    store::emit(
        tx,
        &j.case_id,
        actor,
        cause,
        now,
        EventData::ContainerJobChanged {
            job_id: j.id.clone(),
            kind: j.kind.clone(),
            state: j.state.clone(),
            reason: j.reason.clone(),
        },
    )
}
fn escalate(tx: &Transaction<'_>, j: &ContainerJob, now: &str) -> Result<()> {
    crate::team::escalate(
        tx,
        &j.case_id,
        "native_execution",
        &j.id,
        &format!(
            "{} owns native execution {}: {}. Reconcile the exact container identity; do not repeat dispatch",
            j.owner, j.id, j.reason
        ),
        now,
    )?;
    Ok(())
}
pub(super) fn request(p: ContainerPolicy, j: &ContainerJob) -> Request {
    Request {
        policy: p,
        job: j.envelope.clone(),
        input: j.root.join("input"),
        control: j.root.join("policy"),
        probe: j.kind == "qualification",
    }
}
pub(crate) fn stage(
    p: &ContainerPolicy,
    mut job: WorkerJob,
    source: &Path,
) -> Result<(tempfile::TempDir, WorkerJob)> {
    use std::fs;
    job.validate()?;
    let root = tempfile::Builder::new()
        .prefix("execution-")
        .tempdir_in(&p.staging_root)
        .map_err(|_| Error::Invalid("worker staging unavailable".into()))?;
    fs::create_dir(root.path().join("input")).map_err(|e| Error::Invalid(e.to_string()))?;
    fs::create_dir(root.path().join("policy")).map_err(|e| Error::Invalid(e.to_string()))?;
    for f in &job.files {
        let path = source.join(&f.path);
        let meta = fs::symlink_metadata(&path)
            .map_err(|_| Error::Invalid("source input unavailable".into()))?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != f.bytes {
            return Err(Error::Invalid("source manifest changed".into()));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|_| Error::Invalid("source path unavailable".into()))?;
        if !canonical.starts_with(
            fs::canonicalize(source)
                .map_err(|_| Error::Invalid("source root unavailable".into()))?,
        ) {
            return Err(Error::Invalid("source input escaped its root".into()));
        }
        let bytes = fs::read(path).map_err(|_| Error::Invalid("source input unreadable".into()))?;
        if bytes_digest(&bytes) != f.digest {
            return Err(Error::Conflict("source input digest mismatch".into()));
        }
        let target = root.path().join("input").join(&f.path);
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| Error::Invalid(e.to_string()))?;
        fs::write(target, bytes).map_err(|e| Error::Invalid(e.to_string()))?;
    }
    job.host_net_namespace = fs::read_link("/proc/self/ns/net")
        .map_err(|_| Error::Invalid("native Linux namespaces unavailable".into()))?
        .to_string_lossy()
        .into_owned();
    job.host_pid_namespace = fs::read_link("/proc/self/ns/pid")
        .map_err(|_| Error::Invalid("native Linux namespaces unavailable".into()))?
        .to_string_lossy()
        .into_owned();
    let raw = serde_json::to_vec(&job)?;
    fs::write(root.path().join("policy/job.json"), raw)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok((root, job))
}
pub(crate) fn new_job(
    app: &Application,
    case: &str,
    p: &ContainerPolicy,
    kind: &str,
    job: WorkerJob,
    root: PathBuf,
    meta: &CommandMeta,
) -> ContainerJob {
    ContainerJob {
        schema_version: V1,
        id: job.operation.clone(),
        case_id: case.into(),
        owner: meta.actor.clone(),
        kind: kind.into(),
        candidate_id: None,
        policy_revision: p.revision,
        envelope: job,
        root,
        state: "queued".into(),
        deadline: (app.clock.now()
            + Duration::seconds(i64::from(p.wall_seconds) + i64::from(p.dispatch_grace_seconds)))
        .to_rfc3339_opts(SecondsFormat::Millis, true),
        recorded_at: app.now(),
        reason: meta.reason.clone(),
        engine: None,
        container_id: None,
        create_dispatched: false,
        start_dispatched: false,
        cancel_requested: false,
        native: false,
        result: None,
        evidence_digest: None,
        quarantined: false,
        cleanup_attempts: 0,
        cleanup_next_at: app.now(),
        recorded_version: 0,
    }
}
pub(crate) fn ensure_clear(db: &Connection, case: &str) -> Result<()> {
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM container_jobs WHERE case_id=?1 AND (state IN ('queued','created','creating','starting','running','unknown','cancelling') OR json_extract(record,'$.quarantined')=1))",[case],|r|r.get(0))?;
    if pending {
        return Err(Error::Conflict(
            "native resources require completion or reconciliation before another attempt".into(),
        ));
    }
    Ok(())
}
impl Application {
    pub fn install_container_policy(
        &self,
        mut p: ContainerPolicy,
        who: &str,
    ) -> Result<ContainerPolicy> {
        crate::incident_ops::human(who)?;
        p.staging_root = std::fs::canonicalize(&p.staging_root)
            .map_err(|_| Error::Invalid("operator must create container staging root".into()))?;
        p.docker_path = std::fs::canonicalize(&p.docker_path)
            .map_err(|_| Error::Invalid("Docker client missing".into()))?;
        p.validate()?;
        if crate::repair::snapshot::file_digest(&p.docker_path)? != p.docker_digest {
            return Err(Error::Conflict("pinned Docker client differs".into()));
        }
        let raw = serde_json::to_string(&p)?;
        let digest = store::digest(&raw);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM container_policies",
            [],
            |r| r.get(0),
        )?;
        if p.revision <= latest {
            if policy(&tx, p.revision)?.digest()? == digest {
                return Ok(p);
            }
            return Err(Error::Conflict(
                "container policy revisions are immutable".into(),
            ));
        }
        if p.revision != latest + 1 {
            return Err(Error::Conflict(
                "container policy must advance by one".into(),
            ));
        }
        tx.execute(
            "INSERT INTO container_policies VALUES (?1,?2,?3,?4,?5)",
            params![p.revision, digest, raw, who, self.now()],
        )?;
        tx.commit()?;
        Ok(p)
    }
    pub fn qualify_container(&self, case: &str, cmd: Qualify) -> Result<ContainerJob> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        let fp = store::digest(&format!(
            "qualify-container:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(id) = receipt::<String>(&tx, &m.idempotency_key, &fp)? {
            return get(&tx, &id);
        }
        let c = store::case(&tx, case)?;
        if c.version != cmd.command.expected_version || c.owner != m.actor {
            return Err(Error::Conflict(
                "reload the owned case before qualification".into(),
            ));
        }
        crate::challenges::ensure_clear(&tx, case)?;
        ensure_clear(&tx, case)?;
        let p = policy(&tx, cmd.policy_revision)?;
        let active: i64 =
            tx.query_row("SELECT MAX(revision) FROM container_policies", [], |r| {
                r.get(0)
            })?;
        if active != p.revision {
            return Err(Error::Conflict(
                "qualification requires active policy".into(),
            ));
        }
        let governing = trusted_case_policy(&tx, case)?;
        if i64::from(p.wall_seconds) + i64::from(p.dispatch_grace_seconds)
            > governing.assignment_seconds
        {
            return Err(Error::Conflict(
                "qualification exceeds the case's governing time budget".into(),
            ));
        }
        let source = tempfile::tempdir()
            .map_err(|_| Error::Invalid("qualification canary unavailable".into()))?;
        std::fs::write(source.path().join("value"), "synthetic")
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let mut job = p.job(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            vec![InputFile {
                executable: false,
                path: "value".into(),
                digest: store::digest("synthetic"),
                bytes: 9,
            }],
        );
        let canary = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| Error::Invalid("host network canary unavailable".into()))?;
        job.canary_port = canary
            .local_addr()
            .map_err(|_| Error::Invalid("canary address unavailable".into()))?
            .port();
        drop(canary);
        let (root, job) = stage(&p, job, source.path())?;
        let mut j = new_job(
            self,
            case,
            &p,
            "qualification",
            job,
            root.path().to_owned(),
            m,
        );
        j.recorded_version = c.version + 1;
        tx.execute(
            "DELETE FROM container_admissions WHERE revision=?1",
            [p.revision],
        )?;
        save(&tx, &j, operator(&m.actor), &m.idempotency_key, &self.now())?;
        save_receipt(&tx, &m.idempotency_key, &fp, &j.id)?;
        tx.commit()?;
        let _ = root.keep();
        Ok(j)
    }
    pub fn container_job(&self, case: &str, id: &str) -> Result<ContainerJob> {
        let j = get(&self.store.connect()?, id)?;
        if j.case_id != case {
            return Err(Error::NotFound);
        }
        Ok(j)
    }
    pub async fn run_container_once<B: Backend>(&self, backend: &B) -> Result<bool> {
        self.run_container_on(backend, None).await
    }
    pub async fn run_container_on<B: Backend>(
        &self,
        backend: &B,
        worker: Option<&str>,
    ) -> Result<bool> {
        let now = self.now();
        if self.cleanup_container_once(backend, worker).await? {
            return Ok(true);
        }
        let (mut j, p) = {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let id = crate::routing::select(
                &tx,
                crate::routing::Kind::Container,
                worker,
                "j.state IN ('queued','created')",
                "j.rowid",
                &now,
                false,
            )?;
            let Some(id) = id else {
                return Ok(false);
            };
            let mut j = get(&tx, &id)?;
            let p = policy(&tx, j.policy_revision)?;
            if j.deadline <= now {
                j.cancel_requested = true;
                j.state = if j.create_dispatched {
                    "unknown"
                } else {
                    "cancelled"
                }
                .into();
                j.quarantined = j.create_dispatched;
                j.reason =
                    "Queue/dispatch deadline expired; reconcile any dispatched resources".into();
                save(&tx, &j, supervisor(), &id, &now)?;
                escalate(&tx, &j, &now)?;
                tx.commit()?;
                return Ok(true);
            }
            if j.kind != "qualification" {
                match admission(&tx, &p, &now) {
                    Ok(a) => j.engine = Some(a.engine),
                    Err(_) => {
                        j.cancel_requested = j.create_dispatched;
                        j.quarantined = j.create_dispatched;
                        j.state = if j.create_dispatched {
                            "unknown"
                        } else {
                            "blocked"
                        }
                        .into();
                        j.reason="Container admission expired or changed before dispatch; no new work is authorized".into();
                        save(&tx, &j, supervisor(), &id, &now)?;
                        escalate(&tx, &j, &now)?;
                        tx.commit()?;
                        return Ok(true);
                    }
                }
            }
            crate::routing::bind(&tx, crate::routing::Kind::Container, &id, worker, &now)?;
            if j.state == "queued" {
                j.state = "creating".into();
                j.create_dispatched = true;
            } else {
                j.state = "starting".into();
                j.start_dispatched = true;
            }
            j.native = backend.native();
            j.reason =
                "External effect reserved before I/O; retries must inspect this operation".into();
            save(&tx, &j, supervisor(), &id, &now)?;
            tx.commit()?;
            (j, p)
        };
        let engine = match backend.engine(p.clone()).await {
            Ok(e) => e,
            Err(_) => {
                self.container_preflight_failed(&j.id,"Worker engine/pinned image unavailable before effect dispatch; repair configuration or qualify the worker")?;
                return Ok(true);
            }
        };
        if let Some(expected) = &j.engine
            && serde_json::to_string(expected)? != serde_json::to_string(&engine)?
        {
            self.container_preflight_failed(
                &j.id,
                "Worker engine identity/version changed; requalification required",
            )?;
            return Ok(true);
        }
        j.engine = Some(engine);
        let r = request(p, &j);
        if j.state == "creating" {
            match backend.create(r).await {Ok(id)=>{let mut db=self.store.connect()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;let mut current=get(&tx,&j.id)?;if current.state=="creating" && !current.cancel_requested {current.state="created".into();current.container_id=Some(id);current.engine=j.engine;current.reason="Created exact reserved container; no start replay".into();save(&tx,&current,supervisor(),&j.id,&self.now())?;}tx.commit()?;},Err(e)=>self.container_unknown(&j.id,&format!("Container creation outcome unknown; reconcile reserved name without recreating: {}",diagnostic(e)))?}
        } else {
            let canary = if j.kind == "qualification" && backend.native() {
                match std::net::TcpListener::bind((
                    std::net::Ipv4Addr::LOCALHOST,
                    j.envelope.canary_port,
                )) {
                    Ok(listener) => {
                        listener
                            .set_nonblocking(true)
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        Some(listener)
                    }
                    Err(_) => {
                        self.container_unknown(&j.id, "Host canary could not be held; qualification cannot establish network isolation")?;
                        return Ok(true);
                    }
                }
            } else {
                None
            };
            match backend.start(r).await {
                Ok(mut output) => {
                    if let Some(listener) = &canary {
                        output.checks.insert("host_canary_held".into(), matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
                    }
                    if self.finish_container(&j, output, backend.native()).is_err() {
                        self.container_unknown(&j.id,"Worker returned invalid or unaccepted output; reconcile the reserved resource")?;
                    }
                },
                Err(e) => self.container_unknown(&j.id,&format!("Container start/result outcome unknown; inspect existing container without repeating start: {}",diagnostic(e)))?
            }
        }
        Ok(true)
    }
    async fn cleanup_container_once<B: Backend>(
        &self,
        backend: &B,
        worker: Option<&str>,
    ) -> Result<bool> {
        let now = self.now();
        let pair = {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let id = crate::routing::select(
                &tx,
                crate::routing::Kind::Container,
                worker,
                "j.state='unknown' AND json_extract(j.record,'$.cancel_requested')=1 AND json_extract(j.record,'$.quarantined')=1 AND json_extract(j.record,'$.cleanup_attempts')<(SELECT json_extract(p.record,'$.cleanup_attempts') FROM container_policies p WHERE p.revision=j.policy_revision) AND json_extract(j.record,'$.cleanup_next_at')<=:now",
                "j.deadline,j.rowid",
                &now,
                true,
            )?;
            if let Some(id) = id {
                let mut j = get(&tx, &id)?;
                let p = policy(&tx, j.policy_revision)?;
                j.cleanup_attempts += 1;
                j.cleanup_next_at = (self.clock.now()
                    + Duration::seconds(p.cleanup_retry_seconds.into()))
                .to_rfc3339_opts(SecondsFormat::Millis, true);
                j.state = "cancelling".into();
                j.reason="Scoped cleanup attempt reserved before external removal; no start/create retry".into();
                save(&tx, &j, supervisor(), &id, &now)?;
                tx.commit()?;
                Some((j, p))
            } else {
                None
            }
        };
        let Some((mut j, p)) = pair else {
            return Ok(false);
        };
        match backend.cancel(request(p, &j)).await {
            Ok(()) => {
                let mut db = self.store.connect()?;
                let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                j = get(&tx, &j.id)?;
                if j.state == "cancelling" {
                    j.state = "cancelled".into();
                    j.quarantined = false;
                    j.reason = "Exact owned container removal positively acknowledged".into();
                    save(&tx, &j, supervisor(), &j.id, &self.now())?;
                }
                tx.commit()?;
            }
            Err(_) => self.container_unknown(
                &j.id,
                "Scoped cleanup acknowledgment unavailable; resources remain quarantined",
            )?,
        }
        Ok(true)
    }
    fn container_preflight_failed(&self, id: &str, reason: &str) -> Result<()> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, id)?;
        if j.state == "creating" && !j.cancel_requested {
            j.state = "blocked".into();
            j.create_dispatched = false;
            j.quarantined = false;
            j.reason = reason.into();
            save(&tx, &j, supervisor(), id, &self.now())?;
            escalate(&tx, &j, &self.now())?;
            tx.commit()?;
            Ok(())
        } else {
            drop(tx);
            drop(db);
            self.container_unknown(id, reason)
        }
    }
    fn container_unknown(&self, id: &str, reason: &str) -> Result<()> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, id)?;
        if [
            "passed",
            "failed",
            "cancelled",
            "simulated",
            "reported_cleaned",
            "blocked",
        ]
        .contains(&j.state.as_str())
        {
            return Ok(());
        }
        j.state = "unknown".into();
        j.quarantined = true;
        j.reason = reason.into();
        save(&tx, &j, supervisor(), id, &self.now())?;
        escalate(&tx, &j, &self.now())?;
        tx.commit()?;
        Ok(())
    }
    pub(super) fn finish_container(
        &self,
        original: &ContainerJob,
        output: WorkerOutput,
        native: bool,
    ) -> Result<()> {
        super::docker::validate_output(&original.envelope, &output)?;
        let now = self.now();
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut j = get(&tx, &original.id)?;
        let raw = serde_json::to_string(&json_output(&j, &output, native))?;
        let digest = store::digest(&raw);
        tx.execute(
            "INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,?3,?4)",
            params![
                digest,
                raw,
                if native {
                    "native-container-v1"
                } else {
                    "container-fixture-v1"
                },
                now
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
            params![j.case_id, digest],
        )?;
        if j.cancel_requested
            || j.deadline <= now
            || !["starting", "running", "unknown"].contains(&j.state.as_str())
        {
            store::emit(
                &tx,
                &j.case_id,
                supervisor(),
                &j.id,
                &now,
                EventData::ContainerJobChanged {
                    job_id: j.id.clone(),
                    kind: j.kind.clone(),
                    state: "late_unaccepted".into(),
                    reason:
                        "Late/cancelled output retained as evidence and cannot advance workflow"
                            .into(),
                },
            )?;
            tx.commit()?;
            return Ok(());
        }
        j.result = Some(output.clone());
        j.evidence_digest = Some(digest.clone());
        j.quarantined = false;
        j.engine = original.engine.clone().or(j.engine);
        let valid = output.failure_code.is_none()
            && if j.kind == "qualification" {
                output.exit_code == Some(0)
                    && CHECKS.iter().all(|s| output.checks.get(*s) == Some(&true))
            } else {
                output.checks.is_empty()
                    && output.exit_code == Some(if j.kind == "regression_base" { 1 } else { 0 })
            };
        j.state = if !native {
            "simulated"
        } else if valid {
            "passed"
        } else {
            "failed"
        }
        .into();
        j.reason=if valid{"Measured worker result bound to immutable input/profile; qualification or exact-side regression only"}else{"Worker measurement/regression did not satisfy the required outcome"}.into();
        let latest_qualification: Option<String> = tx.query_row("SELECT id FROM container_jobs WHERE policy_revision=?1 AND json_extract(record,'$.kind')='qualification' ORDER BY rowid DESC LIMIT 1",[j.policy_revision],|r|r.get(0)).optional()?;
        if native
            && valid
            && j.kind == "qualification"
            && latest_qualification.as_deref() == Some(&j.id)
        {
            let p = policy(&tx, j.policy_revision)?;
            let a = Admission {
                qualification_id: j.id.clone(),
                engine: j.engine.clone().ok_or_else(|| {
                    Error::Conflict("qualification engine identity missing".into())
                })?,
                policy_digest: p.digest()?,
                expires_at: (self.clock.now() + Duration::seconds(p.proof_seconds.into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                evidence_digest: digest,
            };
            tx.execute("INSERT INTO container_admissions VALUES (?1,?2) ON CONFLICT(revision) DO UPDATE SET record=excluded.record",params![j.policy_revision,serde_json::to_string(&a)?])?;
        }
        save(&tx, &j, supervisor(), &j.id, &now)?;
        if j.state == "failed" {
            escalate(&tx, &j, &now)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub async fn manage_container<B: Backend>(
        &self,
        case: &str,
        cmd: Manage,
        backend: &B,
    ) -> Result<ContainerJob> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!(
            "manage-container:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let (mut j, p) = {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(id) = receipt::<String>(&tx, &m.idempotency_key, &fp)? {
                return get(&tx, &id);
            }
            let c = store::case(&tx, case)?;
            if c.owner != m.actor || c.version != cmd.command.expected_version {
                return Err(Error::Conflict(
                    "reload owned case before resource reconciliation".into(),
                ));
            }
            let mut j = get(&tx, &cmd.job_id)?;
            if j.case_id != case {
                return Err(Error::NotFound);
            }
            let p = policy(&tx, j.policy_revision)?;
            if matches!(cmd.action, ManageAction::ReportCleanup) {
                if j.state != "unknown" || !j.quarantined {
                    return Err(Error::Conflict(
                        "only quarantined unknown resources need reported reconciliation".into(),
                    ));
                }
                crate::recovery::independent_evidence(
                    &tx,
                    case,
                    &cmd.evidence_refs,
                    &m.actor,
                    &j.recorded_at,
                    &[],
                    j.recorded_version,
                )?;
                j.state = "reported_cleaned".into();
                j.quarantined = false;
                j.native = false;
                j.cancel_requested = true;
                j.reason = format!(
                    "Operator-reported resource inspection: {}. No result/capability accepted",
                    m.reason
                );
                save(&tx, &j, operator(&m.actor), &m.idempotency_key, &self.now())?;
                save_receipt(&tx, &m.idempotency_key, &fp, &j.id)?;
                tx.commit()?;
                return Ok(j);
            }
            if [
                "passed",
                "failed",
                "simulated",
                "cancelled",
                "blocked",
                "reported_cleaned",
            ]
            .contains(&j.state.as_str())
            {
                return Err(Error::Conflict("terminal execution cannot be reopened; results and cleanup history are retained".into()));
            }
            if matches!(cmd.action, ManageAction::Cancel)
                || j.cancel_requested
                || j.deadline <= self.now()
            {
                if j.create_dispatched && j.cleanup_attempts >= p.cleanup_attempts {
                    return Err(Error::Conflict("cleanup attempt budget exhausted; report scoped operator inspection or repair the worker".into()));
                }
                if j.create_dispatched {
                    j.cleanup_attempts += 1;
                }
                j.cleanup_next_at = (self.clock.now()
                    + Duration::seconds(p.cleanup_retry_seconds.into()))
                .to_rfc3339_opts(SecondsFormat::Millis, true);
                j.cancel_requested = true;
                j.state = if j.create_dispatched {
                    "cancelling"
                } else {
                    "cancelled"
                }
                .into();
                j.reason = m.reason.clone();
                save(&tx, &j, operator(&m.actor), &m.idempotency_key, &self.now())?;
            }
            save_receipt(&tx, &m.idempotency_key, &fp, &j.id)?;
            tx.commit()?;
            (j, p)
        };
        if j.state == "cancelled" {
            return Ok(j);
        }
        let r = request(p, &j);
        if j.cancel_requested || j.deadline <= self.now() {
            match backend.cancel(r).await {
                Ok(()) => {
                    let mut db = self.store.connect()?;
                    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    j = get(&tx, &j.id)?;
                    j.cancel_requested = true;
                    j.quarantined = false;
                    j.state = "cancelled".into();
                    j.reason="Exact scoped container removal acknowledged; prior evidence/history retained".into();
                    save(&tx, &j, operator(&m.actor), &m.idempotency_key, &self.now())?;
                    tx.commit()?;
                }
                Err(_) => self.container_unknown(
                    &j.id,
                    "Cancellation/removal acknowledgment missing; absence is not proof",
                )?,
            }
        } else {
            match backend.inspect(r.clone()).await {Ok(o)=>{
            if o.status=="created" && !j.start_dispatched {let mut db=self.store.connect()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;let mut cur=get(&tx,&j.id)?;if !cur.cancel_requested {cur.state="created".into();cur.quarantined=false;cur.container_id=Some(o.id);cur.reason="Existing created handle recovered; create will not be repeated".into();save(&tx,&cur,operator(&m.actor),&m.idempotency_key,&self.now())?;}tx.commit()?;}
            else if o.status=="exited" && !o.running && j.start_dispatched {match backend.logs(r).await {Ok(mut out)=>{if j.kind=="qualification" {out.checks.insert("host_canary_held".into(),false);out.checks.insert("removal_acknowledged".into(),false);}self.finish_container(&j,out,backend.native())?},Err(_)=>self.container_unknown(&j.id,"Terminal container lacks accepted complete worker output")?}}
            else {self.container_unknown(&j.id,"Existing worker remains running or outcome is uncertain; retain owned resources")?;}
        },Err(_)=>self.container_unknown(&j.id,"Worker lookup failed; missing container is not a cancellation/result acknowledgment")?}
        }
        get(&self.store.connect()?, &j.id)
    }
}
fn json_output(j: &ContainerJob, o: &WorkerOutput, native: bool) -> serde_json::Value {
    serde_json::json!({"schema_version":1,"job_id":j.id,"kind":j.kind,"input_digest":j.envelope.input_digest().ok(),"native":native,"output":o})
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let ids: Vec<String> = {
        let mut q=tx.prepare("SELECT id FROM container_jobs WHERE (state IN ('queued','created','creating','starting','running','cancelling') OR (state='unknown' AND json_extract(record,'$.cancel_requested')=0)) AND deadline<=?1 ORDER BY deadline,rowid LIMIT 100")?;
        q.query_map([now], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?
    };
    for id in &ids {
        let mut j = get(tx, id)?;
        j.cancel_requested = true;
        j.quarantined = j.create_dispatched;
        j.state = if j.create_dispatched {
            "unknown"
        } else {
            "cancelled"
        }
        .into();
        j.reason =
            "Native execution deadline expired; worker must reconcile/remove exact owned container"
                .into();
        save(tx, &j, supervisor(), id, now)?;
        escalate(tx, &j, now)?;
    }
    Ok(ids.len())
}
