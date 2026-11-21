//! Observed qualification of a fixed harmless probe, never an agent admission bypass.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
mod linux;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedTool {
    pub path: PathBuf,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerPolicy {
    pub schema_version: V1,
    pub revision: i64,
    pub boundary: PinnedTool,
    pub limiter: PinnedTool,
    pub probe: PinnedTool,
    pub scratch_root: PathBuf,
    pub wall_seconds: u32,
    pub cpu_seconds: u32,
    pub memory_bytes: u64,
    pub file_bytes: u64,
    pub descriptors: u32,
    pub valid_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifyWorker {
    pub command: OperatorCommand,
    pub policy_revision: i64,
    pub source_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileQualification {
    pub command: OperatorCommand,
    pub qualification_id: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reconciliation {
    pub operator: String,
    pub reason: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qualification {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub owner: String,
    pub state: String,
    pub scope: String,
    pub real_execution_admitted: bool,
    pub policy_revision: i64,
    pub policy_digest: String,
    pub source_revision: i64,
    pub manifest_digest: String,
    pub nonce: String,
    pub requested_at: String,
    pub deadline: String,
    pub finished_at: Option<String>,
    pub expires_at: Option<String>,
    pub checks: BTreeMap<String, bool>,
    pub verified: Vec<String>,
    pub output_digest: Option<String>,
    pub evidence_digest: Option<String>,
    pub reason: String,
    pub request_reason: String,
    pub case_policy_provenance: String,
    pub quarantined: bool,
    pub time_current: bool,
    pub reconciliation: Option<Reconciliation>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Evidence {
    checks: BTreeMap<String, bool>,
    details: Option<serde_json::Value>,
    output_digest: Option<String>,
    reason: String,
    quarantined: bool,
    origin: String,
}
fn supervisor() -> Actor {
    Actor {
        kind: "supervisor".into(),
        id: "worker-qualification".into(),
    }
}
fn human(actor: &str) -> Result<()> {
    text(actor, "operator", 128)?;
    if actor.starts_with("fake-") {
        return Err(Error::Invalid(
            "worker qualification is an OS-operator action".into(),
        ));
    }
    Ok(())
}
fn digest_valid(s: &str) -> bool {
    s.len() == 71 && s.starts_with("sha256:") && s[7..].bytes().all(|b| b.is_ascii_hexdigit())
}
impl WorkerPolicy {
    fn validate(&mut self) -> Result<()> {
        if self.revision < 1
            || !(2..=30).contains(&self.wall_seconds)
            || !(1..=10).contains(&self.cpu_seconds)
            || self.cpu_seconds > self.wall_seconds
            || !(64 * 1024 * 1024..=512 * 1024 * 1024).contains(&self.memory_bytes)
            || !(4096..=1048576).contains(&self.file_bytes)
            || !(16..=256).contains(&self.descriptors)
            || !(1..=3600).contains(&self.valid_seconds)
        {
            return Err(Error::Invalid(
                "worker policy limits outside protocol bounds".into(),
            ));
        }
        if !self.scratch_root.is_absolute() {
            return Err(Error::Invalid(
                "explicit operator scratch root required".into(),
            ));
        }
        self.scratch_root = std::fs::canonicalize(&self.scratch_root)
            .map_err(|_| Error::Invalid("scratch root must already exist".into()))?;
        if !self.scratch_root.is_dir() {
            return Err(Error::Invalid("scratch root must be a directory".into()));
        }
        for t in [&mut self.boundary, &mut self.limiter, &mut self.probe] {
            if !t.path.is_absolute() || !digest_valid(&t.digest) {
                return Err(Error::Invalid(
                    "absolute pinned worker tool required".into(),
                ));
            }
            if t.path.exists() {
                t.path = std::fs::canonicalize(&t.path)
                    .map_err(|_| Error::Invalid("worker tool cannot be resolved".into()))?;
            }
            if t.path.starts_with(&self.scratch_root) {
                return Err(Error::Invalid(
                    "worker tools must be outside their scratch root".into(),
                ));
            }
        }
        Ok(())
    }
}
fn policy(db: &Connection, rev: i64) -> Result<(WorkerPolicy, String)> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM worker_policies WHERE revision=?1",
            [rev],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("worker policy integrity failure".into()));
    }
    Ok((serde_json::from_str(&raw)?, digest))
}
fn get(db: &Connection, id: &str) -> Result<Qualification> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM worker_qualifications WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(serde_json::from_str(&raw.ok_or(Error::NotFound)?)?)
}
fn save(
    tx: &Transaction<'_>,
    q: &Qualification,
    cause: &str,
    actor: Actor,
    now: &str,
) -> Result<()> {
    tx.execute("INSERT INTO worker_qualifications VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![q.id,q.case_id,q.state,q.deadline,serde_json::to_string(q)?])?;
    store::emit(
        tx,
        &q.case_id,
        actor,
        cause,
        now,
        EventData::WorkerQualificationChanged {
            qualification: Box::new(q.clone()),
            reason: q.reason.clone(),
        },
    )
}
fn finish(
    tx: &Transaction<'_>,
    mut q: Qualification,
    e: Evidence,
    now: &str,
    valid: u32,
) -> Result<Qualification> {
    let raw = serde_json::to_string(
        &serde_json::json!({"schema_version":1,"qualification_id":&q.id,"nonce":&q.nonce,"policy_digest":&q.policy_digest,"manifest_digest":&q.manifest_digest,"outcome":&e}),
    )?;
    let evidence_digest = store::digest(&raw);
    let late = q.state != "probing" || q.deadline.as_str() <= now;
    let provenance = if late {
        "worker-qualification-v1:unaccepted-late-result"
    } else if e.origin == "native" {
        "worker-qualification-native-v1"
    } else {
        "worker-qualification-fixture-v1"
    };
    tx.execute(
        "INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,?3,?4)",
        params![evidence_digest, raw, provenance, now],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
        params![q.case_id, evidence_digest],
    )?;
    if q.state != "probing" || q.deadline.as_str() <= now {
        if q.state == "probing" {
            interrupt(tx, &mut q, now)?;
        }
        let digest = evidence_digest;
        if tx.execute(
            "INSERT OR IGNORE INTO worker_qualification_late VALUES (?1,?2)",
            params![q.id, digest],
        )? > 0
        {
            store::emit(
                tx,
                &q.case_id,
                supervisor(),
                &q.id,
                now,
                EventData::WorkerQualificationLate {
                    qualification_id: q.id.clone(),
                    digest,
                    reason: "Late probe output cannot restore expired admission evidence".into(),
                },
            )?;
        }
        return Ok(q);
    }
    let passed = e.origin == "native"
        && linux::REQUIRED
            .iter()
            .all(|name| e.checks.get(*name) == Some(&true))
        && !e.quarantined;
    q.state = if e.origin == "fixture" {
        "simulated"
    } else if passed {
        "probe_passed"
    } else {
        "failed"
    }
    .into();
    q.checks = e.checks;
    q.verified = if passed {
        linux::REQUIRED.iter().map(|s| (*s).into()).collect()
    } else {
        vec![]
    };
    q.output_digest = e.output_digest;
    q.evidence_digest = Some(evidence_digest);
    q.reason = e.reason;
    q.quarantined = e.quarantined;
    q.finished_at = Some(now.into());
    let active: i64 = tx.query_row("SELECT MAX(revision) FROM worker_policies", [], |r| {
        r.get(0)
    })?;
    q.time_current = passed && q.policy_revision == active;
    if passed {
        let t = DateTime::parse_from_rfc3339(now)
            .map_err(|_| Error::Invalid("invalid qualification time".into()))?;
        q.expires_at = Some(
            (t + Duration::seconds(valid.into())).to_rfc3339_opts(SecondsFormat::Millis, true),
        );
    }
    save(tx, &q, &q.id, supervisor(), now)?;
    if !passed {
        crate::team::escalate(
            tx,
            &q.case_id,
            "worker_qualification",
            &q.id,
            "Worker qualification failed or is simulated; the accountable operator must inspect the boundary before any real execution",
            now,
        )?;
    }
    Ok(q)
}
fn interrupt(tx: &Transaction<'_>, q: &mut Qualification, now: &str) -> Result<()> {
    q.state = "interrupted".into();
    q.quarantined = true;
    q.verified.clear();
    q.reason="Qualification deadline expired or coordinator was lost; probe resources require operator reconciliation. No automatic retry or execution admission.".into();
    q.finished_at = Some(now.into());
    save(tx, q, &q.id, supervisor(), now)?;
    crate::team::escalate(
        tx,
        &q.case_id,
        "worker_qualification",
        &q.id,
        "Interrupted worker qualification requires operator reconciliation; all real execution remains denied",
        now,
    )?;
    Ok(())
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    let mut stmt=tx.prepare("SELECT id FROM worker_qualifications WHERE state='probing' AND deadline<=?1 ORDER BY deadline LIMIT 100")?;
    let ids = stmt
        .query_map([now], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for id in &ids {
        let mut q = get(tx, id)?;
        interrupt(tx, &mut q, now)?;
    }
    Ok(ids.len())
}
pub(crate) fn snapshot(
    db: &Connection,
    id: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Qualification>> {
    let latest: i64 = db.query_row(
        "SELECT COALESCE(MAX(revision),0) FROM worker_policies",
        [],
        |r| r.get(0),
    )?;
    let mut stmt = db.prepare(
        "SELECT record FROM worker_qualifications WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    let mut records = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str::<Qualification>(&r?)?))
        .collect::<Result<Vec<_>>>()?;
    for q in &mut records {
        q.time_current = q.state == "probe_passed"
            && q.policy_revision == latest
            && q.expires_at
                .as_deref()
                .is_some_and(|t| t > now.to_rfc3339_opts(SecondsFormat::Millis, true).as_str());
    }
    Ok(records)
}
impl Application {
    pub fn install_worker_policy(&self, mut p: WorkerPolicy, who: &str) -> Result<WorkerPolicy> {
        human(who)?;
        p.validate()?;
        let raw = serde_json::to_string(&p)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT record FROM worker_policies WHERE revision=?1",
                [p.revision],
                |r| r.get(0),
            )
            .optional()?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM worker_policies",
            [],
            |r| r.get(0),
        )?;
        if let Some(old) = previous {
            if old == raw && p.revision == latest {
                return Ok(p);
            }
            return Err(Error::Conflict(
                "worker policies are immutable and cannot roll back".into(),
            ));
        }
        if p.revision != latest + 1 {
            return Err(Error::Conflict("worker policy must advance by one".into()));
        }
        tx.execute(
            "INSERT INTO worker_policies VALUES (?1,?2,?3,?4,?5)",
            params![p.revision, store::digest(&raw), raw, who, self.now()],
        )?;
        tx.commit()?;
        Ok(p)
    }
    fn begin_qualification(
        &self,
        id: &str,
        cmd: &QualifyWorker,
    ) -> Result<(Qualification, WorkerPolicy, bool)> {
        let m = &cmd.command.meta;
        human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "idempotency_key", 128)?;
        let fp = store::digest(&format!("qualify:{id}:{}", serde_json::to_string(cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = receipt::<String>(&tx, &m.idempotency_key, &fp)? {
            let q = get(&tx, &key)?;
            let (p, _) = policy(&tx, q.policy_revision)?;
            return Ok((q, p, false));
        }
        let case = store::case(&tx, id)?;
        if case.version != cmd.command.expected_version || case.owner != m.actor {
            return Err(Error::Conflict(
                "reload the owned case before qualifying a worker".into(),
            ));
        }
        crate::challenges::ensure_clear(&tx, id)?;
        let unresolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM worker_qualifications WHERE case_id=?1 AND json_extract(record,'$.quarantined')=1)",[id],|r|r.get(0))?;
        if unresolved {
            return Err(Error::Conflict("quarantined qualification resources require explicit operator reconciliation before a new probe".into()));
        }
        crate::restore::require_fresh(&tx, "worker", cmd.policy_revision)?;
        crate::restore::require_fresh(&tx, "repair_source", cmd.source_revision)?;
        let source = crate::repair::source(&tx, cmd.source_revision)?;
        let (p, digest) = policy(&tx, cmd.policy_revision)?;
        let active: i64 = tx.query_row("SELECT MAX(revision) FROM worker_policies", [], |r| {
            r.get(0)
        })?;
        if cmd.policy_revision != active {
            return Err(Error::Conflict(
                "new qualification requires the active worker policy".into(),
            ));
        }
        if p.scratch_root.starts_with(&source.config.source_path)
            || p.scratch_root.starts_with(&source.config.workspace_root)
            || source.config.source_path.starts_with(&p.scratch_root)
            || source.config.workspace_root.starts_with(&p.scratch_root)
        {
            return Err(Error::Invalid(
                "qualification scratch, source and frozen-base roots must not overlap".into(),
            ));
        }
        for t in [&p.boundary, &p.limiter, &p.probe] {
            if t.path.starts_with(&source.config.source_path)
                || t.path.starts_with(&source.config.workspace_root)
            {
                return Err(Error::Invalid(
                    "qualification tools cannot be candidate/base files".into(),
                ));
            }
        }
        let q=Qualification {schema_version:V1,id:store::id("qual"),case_id:id.into(),owner:case.owner,state:"probing".into(),scope:"hermetic-probe-v1".into(),real_execution_admitted:false,policy_revision:cmd.policy_revision,policy_digest:digest,source_revision:cmd.source_revision,manifest_digest:source.manifest_digest,nonce:uuid::Uuid::new_v4().to_string(),requested_at:self.now(),deadline:(self.clock.now()+Duration::seconds(i64::from(p.wall_seconds)+5)).to_rfc3339_opts(SecondsFormat::Millis,true),finished_at:None,expires_at:None,checks:BTreeMap::new(),verified:vec![],output_digest:None,evidence_digest:None,reason:"Controlled qualification probe requested; no candidate/model/production execution admitted".into(),request_reason:m.reason.clone(),case_policy_provenance:crate::policy::for_case(&tx,id)?.provenance(),quarantined:false,time_current:false,reconciliation:None};
        save(&tx, &q, &m.idempotency_key, operator(&m.actor), &self.now())?;
        save_receipt(&tx, &m.idempotency_key, &fp, &q.id)?;
        tx.commit()?;
        Ok((q, p, true))
    }
    pub async fn qualify_worker(&self, id: &str, cmd: QualifyWorker) -> Result<Qualification> {
        let (q, p, run) = self.begin_qualification(id, &cmd)?;
        if !run {
            return self.current_worker_qualification(&q.id);
        }
        let evidence = linux::run(&p, &q.nonce).await;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let q = get(&tx, &q.id)?;
        let result = finish(&tx, q, evidence, &self.now(), p.valid_seconds)?;
        tx.commit()?;
        self.current_worker_qualification(&result.id)
    }
    fn current_worker_qualification(&self, id: &str) -> Result<Qualification> {
        let db = self.store.connect()?;
        let mut q = get(&db, id)?;
        let latest: i64 = db.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM worker_policies",
            [],
            |r| r.get(0),
        )?;
        q.time_current = q.state == "probe_passed"
            && q.policy_revision == latest
            && q.expires_at
                .as_deref()
                .is_some_and(|t| t > self.now().as_str());
        Ok(q)
    }
    pub fn worker_policy_status(&self) -> Result<serde_json::Value> {
        let db = self.store.connect()?;
        let row:Option<(i64,String,String,String)>=db.query_row("SELECT revision,digest,operator,installed_at FROM worker_policies ORDER BY revision DESC LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        match row {
            None => Ok(serde_json::Value::Null),
            Some((rev, digest, who, time)) => {
                let (p, _) = policy(&db, rev)?;
                Ok(
                    serde_json::json!({"policy":p,"digest":digest,"operator":who,"installed_at":time}),
                )
            }
        }
    }
    pub fn reconcile_worker_qualification(
        &self,
        id: &str,
        cmd: ReconcileQualification,
    ) -> Result<Qualification> {
        let m = &cmd.command.meta;
        human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "idempotency_key", 128)?;
        let fp = store::digest(&format!(
            "qualification-reconcile:{id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = receipt::<String>(&tx, &m.idempotency_key, &fp)? {
            return get(&tx, &key);
        }
        let case = store::case(&tx, id)?;
        if case.version != cmd.command.expected_version || case.owner != m.actor {
            return Err(Error::Conflict(
                "reload the owned case before reconciliation".into(),
            ));
        }
        let mut q = get(&tx, &cmd.qualification_id)?;
        if q.case_id != id {
            return Err(Error::NotFound);
        }
        if !q.quarantined || q.state == "probing" {
            return Err(Error::Conflict(
                "only terminal quarantined resources can be reconciled".into(),
            ));
        }
        crate::qualification::evidence_refs(&tx, id, &cmd.evidence_refs, true)?;
        q.quarantined = false;
        q.reconciliation = Some(Reconciliation {
            operator: m.actor.clone(),
            reason: m.reason.clone(),
            evidence_refs: cmd.evidence_refs.clone(),
            recorded_at: self.now(),
        });
        tx.execute(
            "UPDATE worker_qualifications SET record=?1 WHERE id=?2",
            params![serde_json::to_string(&q)?, q.id],
        )?;
        store::emit(
            &tx,
            id,
            operator(&m.actor),
            &m.idempotency_key,
            &self.now(),
            EventData::WorkerQualificationReconciled {
                qualification_id: q.id.clone(),
                evidence_refs: cmd.evidence_refs,
                reason: m.reason.clone(),
            },
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &q.id)?;
        tx.commit()?;
        Ok(q)
    }
    pub fn worker_qualification_markdown(&self, id: &str) -> Result<String> {
        let s = self.snapshot(id)?;
        let raw = serde_json::to_string_pretty(&s.worker_qualifications)?
            .replace('`', "\\u0060")
            .replace('<', "\\u003c");
        Ok(format!(
            "# Worker qualification {id}\n\nScope: controlled hermetic probe only. Real harness execution remains denied. Time currency is not admission and tool identities must be rechecked before future use.\n\n```json\n{raw}\n```\n"
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::ManualClock, intake::FakeSource, repair::RepairSourceConfig, store::Store,
    };
    use chrono::TimeZone;
    use std::{fs, path::Path, process::Command, sync::Arc};
    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().into()
    }
    async fn fixture() -> (
        tempfile::TempDir,
        Application,
        ManualClock,
        String,
        WorkerPolicy,
    ) {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        let base = d.path().join("base");
        let scratch = d.path().join("scratch");
        for path in [&repo, &base, &scratch] {
            fs::create_dir(path).unwrap();
        }
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("fixture.txt"), "synthetic qualification source").unwrap();
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "Synthetic source",
            ],
        );
        let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap());
        let a = Application::new(
            Store::open(d.path().join("db")).unwrap(),
            Arc::new(t.clone()),
        );
        let id = a
            .create_case(FakeSource.manual_command("worker-qualification"))
            .unwrap()
            .id;
        let runtime = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join(if cfg!(windows) { "git.exe" } else { "git" }))
            .find(|p| p.is_file())
            .unwrap()
            .canonicalize()
            .unwrap();
        let digest = crate::repair::snapshot::file_digest(&runtime).unwrap();
        a.install_repair_source(
            RepairSourceConfig {
                schema_version: V1,
                revision: 1,
                repository: "fixture/qualification".into(),
                source_path: repo.canonicalize().unwrap(),
                base_commit: git(&repo, &["rev-parse", "HEAD"]),
                workspace_root: base.canonicalize().unwrap(),
                runtime_path: runtime.clone(),
                runtime_digest: digest.clone(),
                max_files: 10,
                max_blob_bytes: 4096,
                max_total_bytes: 8192,
                preparation_seconds: 30,
                probe_seconds: 5,
            },
            "operator",
        )
        .await
        .unwrap();
        let tool = PinnedTool {
            path: runtime,
            digest,
        };
        let policy = WorkerPolicy {
            schema_version: V1,
            revision: 1,
            boundary: tool.clone(),
            limiter: tool.clone(),
            probe: tool,
            scratch_root: scratch.canonicalize().unwrap(),
            wall_seconds: 2,
            cpu_seconds: 1,
            memory_bytes: 134217728,
            file_bytes: 8192,
            descriptors: 64,
            valid_seconds: 10,
        };
        a.install_worker_policy(policy.clone(), "operator").unwrap();
        (d, a, t, id, policy)
    }
    fn command(a: &Application, id: &str, key: &str) -> QualifyWorker {
        QualifyWorker {
            command: OperatorCommand {
                meta: CommandMeta {
                    schema_version: V1,
                    idempotency_key: key.into(),
                    actor: "operator".into(),
                    reason: "Synthetic qualification lifecycle test".into(),
                },
                expected_version: a.snapshot(id).unwrap().case.version,
            },
            policy_revision: 1,
            source_revision: 1,
        }
    }
    fn evidence(origin: &str) -> Evidence {
        Evidence {
            details: Some(serde_json::json!({"synthetic":true,"scope":"controller-test-only"})),
            checks: linux::REQUIRED
                .iter()
                .map(|k| ((*k).into(), true))
                .collect(),
            output_digest: Some(store::digest(
                "synthetic outcome, not a native isolation drill",
            )),
            reason: "Synthetic controller fixture, not live worker verification".into(),
            quarantined: false,
            origin: origin.into(),
        }
    }
    fn complete(a: &Application, id: &str, e: Evidence, valid: u32) -> Qualification {
        let mut db = a.store.connect().unwrap();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let q = get(&tx, id).unwrap();
        let out = finish(&tx, q, e, &a.now(), valid).unwrap();
        tx.commit().unwrap();
        out
    }
    #[tokio::test]
    async fn policy_is_immutable_bounded_and_keeps_installation_provenance() {
        let (_d, a, _t, _id, p) = fixture().await;
        let s = a.worker_policy_status().unwrap();
        assert_eq!(s["operator"], "operator");
        assert!(s["installed_at"].as_str().is_some());
        assert_eq!(
            a.install_worker_policy(p.clone(), "operator")
                .unwrap()
                .revision,
            1
        );
        let mut changed = p.clone();
        changed.cpu_seconds = 2;
        assert!(a.install_worker_policy(changed, "operator").is_err());
        let mut invalid = p.clone();
        invalid.revision = 2;
        invalid.wall_seconds = 0;
        assert!(a.install_worker_policy(invalid, "operator").is_err());
        assert!(a.install_worker_policy(p.clone(), "fake-engineer").is_err());
        let mut unsafe_root = p;
        unsafe_root.revision = 2;
        unsafe_root.probe.path = unsafe_root.scratch_root.join("agent-tool");
        assert!(a.install_worker_policy(unsafe_root, "operator").is_err());
        let mut v = serde_json::to_value(s["policy"].clone()).unwrap();
        v["admit_real_execution"] = true.into();
        assert!(serde_json::from_value::<WorkerPolicy>(v).is_err());
    }
    #[tokio::test]
    async fn native_failure_is_owned_idempotent_and_never_grants_execution() {
        let (_d, a, _t, id, _) = fixture().await;
        let cmd = command(&a, &id, "attempt");
        let q = a.qualify_worker(&id, cmd.clone()).await.unwrap();
        assert_eq!(q.state, "failed");
        assert!(q.verified.is_empty());
        let data = a.evidence(q.evidence_digest.as_deref().unwrap()).unwrap();
        assert!(data.contains("native"));
        assert!(!q.real_execution_admitted);
        let retry = a.qualify_worker(&id, cmd).await.unwrap();
        assert_eq!(retry.id, q.id);
        assert_eq!(retry.state, "failed");
        assert_eq!(a.snapshot(&id).unwrap().worker_qualifications.len(), 1);
        assert_eq!(
            a.escalation_inbox()
                .unwrap()
                .iter()
                .filter(|e| e.source_type == "worker_qualification")
                .count(),
            1
        );
    }
    #[tokio::test]
    async fn coordinator_loss_is_supervised_without_models_and_late_output_cannot_resurrect_proof()
    {
        let (d, a, t, id, _) = fixture().await;
        let cmd = command(&a, &id, "lost");
        let (q, _, run) = a.begin_qualification(&id, &cmd).unwrap();
        assert!(run);
        let restarted = Application::new(
            Store::open(d.path().join("db")).unwrap(),
            Arc::new(t.clone()),
        );
        assert_eq!(
            restarted.snapshot(&id).unwrap().worker_qualifications[0].state,
            "probing"
        );
        t.advance(Duration::seconds(8));
        assert!(restarted.supervise_team().unwrap() > 0);
        assert_eq!(restarted.supervise_team().unwrap(), 0);
        let out = complete(&restarted, &q.id, evidence("native"), 10);
        assert_eq!(out.state, "interrupted");
        assert!(out.quarantined);
        assert!(out.verified.is_empty());
        assert!(!out.real_execution_admitted);
        let events = restarted.timeline(&id, 0).unwrap().len();
        complete(&restarted, &q.id, evidence("native"), 10);
        assert_eq!(restarted.timeline(&id, 0).unwrap().len(), events);
        assert_eq!(
            restarted.qualify_worker(&id, cmd).await.unwrap().state,
            "interrupted"
        );
    }
    #[tokio::test]
    async fn fixture_evidence_remains_simulated_even_when_all_fields_claim_success() {
        let (_d, a, _t, id, _) = fixture().await;
        let (q, _, _) = a
            .begin_qualification(&id, &command(&a, &id, "fixture"))
            .unwrap();
        let out = complete(&a, &q.id, evidence("fixture"), 10);
        assert_eq!(out.state, "simulated");
        assert!(out.verified.is_empty());
        assert!(!out.real_execution_admitted);
    }
    #[tokio::test]
    async fn synthetic_controller_success_tests_currency_and_policy_invalidation_not_native_isolation()
     {
        // Private synthetic outcome exercises the controller branch only; it is not a live drill.
        let (_d, a, t, id, mut p) = fixture().await;
        let cmd = command(&a, &id, "controller");
        let (q, _, _) = a.begin_qualification(&id, &cmd).unwrap();
        let out = complete(&a, &q.id, evidence("native"), 10);
        assert_eq!(out.state, "probe_passed");
        assert!(!out.real_execution_admitted);
        assert!(a.snapshot(&id).unwrap().worker_qualifications[0].time_current);
        t.advance(Duration::seconds(11));
        assert!(!a.qualify_worker(&id, cmd).await.unwrap().time_current);
        p.revision = 2;
        a.install_worker_policy(p, "operator").unwrap();
        assert!(!a.snapshot(&id).unwrap().worker_qualifications[0].time_current);
        assert!(
            a.begin_qualification(&id, &command(&a, &id, "retired-policy"))
                .is_err()
        );
    }
    #[tokio::test]
    async fn missing_measurement_cannot_pass_and_changed_or_stale_commands_conflict() {
        let (_d, a, _t, id, _) = fixture().await;
        let cmd = command(&a, &id, "complete-set");
        let (q, _, _) = a.begin_qualification(&id, &cmd).unwrap();
        let mut e = evidence("native");
        e.checks.remove("descendant_termination");
        assert_eq!(complete(&a, &q.id, e, 10).state, "failed");
        let mut changed = cmd.clone();
        changed.source_revision = 2;
        assert!(a.begin_qualification(&id, &changed).is_err());
        let mut stale = command(&a, &id, "stale");
        stale.command.expected_version -= 1;
        assert!(a.begin_qualification(&id, &stale).is_err());
        let mut fake = command(&a, &id, "fake");
        fake.command.meta.actor = "fake-investigator".into();
        assert!(a.begin_qualification(&id, &fake).is_err());
    }
    #[tokio::test]
    async fn canonical_failures_rollback_reservation_and_terminal_evidence_without_restarting_a_probe()
     {
        let (_d, a, t, id, _) = fixture().await;
        let db = a.store.connect().unwrap();
        db.execute_batch("CREATE TRIGGER fail_qualification BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='worker.qualification_changed' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        let cmd = command(&a, &id, "atomic");
        assert!(a.begin_qualification(&id, &cmd).is_err());
        assert!(a.snapshot(&id).unwrap().worker_qualifications.is_empty());
        db.execute_batch("DROP TRIGGER fail_qualification").unwrap();
        let (q, _, _) = a.begin_qualification(&id, &cmd).unwrap();
        db.execute_batch("CREATE TRIGGER fail_outcome BEFORE INSERT ON escalations WHEN NEW.source_type='worker_qualification' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        let mut c = a.store.connect().unwrap();
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut e = evidence("native");
        e.checks.clear();
        assert!(finish(&tx, get(&tx, &q.id).unwrap(), e, &a.now(), 10).is_err());
        drop(tx);
        assert_eq!(get(&db, &q.id).unwrap().state, "probing");
        assert!(!a.begin_qualification(&id, &cmd).unwrap().2);
        db.execute_batch("DROP TRIGGER fail_outcome").unwrap();
        t.advance(Duration::seconds(8));
        a.supervise_team().unwrap();
        assert_eq!(get(&db, &q.id).unwrap().state, "interrupted");
    }
    #[tokio::test]
    async fn concurrent_reservations_have_one_winner_and_markdown_remains_inert() {
        let (_d, a, _t, id, _) = fixture().await;
        let mut x = command(&a, &id, "one");
        x.command.meta.reason = "<script>window.workerInjected=true</script> fixture".into();
        let y = command(&a, &id, "two");
        let b = a.clone();
        let c = a.clone();
        let i = id.clone();
        let j = id.clone();
        let one = std::thread::spawn(move || b.begin_qualification(&i, &x));
        let two = std::thread::spawn(move || c.begin_qualification(&j, &y));
        assert_eq!(
            [one.join().unwrap().is_ok(), two.join().unwrap().is_ok()]
                .iter()
                .filter(|v| **v)
                .count(),
            1
        );
        let report = a.worker_qualification_markdown(&id).unwrap();
        assert!(!report.contains("<script>"));
        assert!(report.contains("Real harness execution remains denied"));
    }
    fn report_cleanup(a: &Application, id: &str) -> String {
        a.record_evidence(id,RecordEvidence{schema_version:V1,idempotency_key:"cleanup-observation".into(),actor:"operator".into(),reason:"Record controlled fixture inspection".into(),expected_version:a.snapshot(id).unwrap().case.version,scope:"qualification scratch resources".into(),statement:"Operator reports the fixture resources are no longer active and cleanup was inspected".into(),collection_method:"Manual synthetic fixture inspection".into(),observed_at:None,uncertainty:"Operator report, not an isolation pass".into()}).unwrap().evidence.digest
    }
    #[tokio::test]
    async fn quarantine_blocks_fresh_attempts_until_explicit_evidence_backed_reconciliation() {
        let (_d, a, _t, id, _) = fixture().await;
        let (q, _, _) = a
            .begin_qualification(&id, &command(&a, &id, "quarantined"))
            .unwrap();
        let mut e = evidence("native");
        e.quarantined = true;
        complete(&a, &q.id, e, 10);
        assert!(
            a.begin_qualification(&id, &command(&a, &id, "blind-retry"))
                .is_err()
        );
        let digest = report_cleanup(&a, &id);
        let op = ReconcileQualification {
            command: command(&a, &id, "cleanup").command,
            qualification_id: q.id.clone(),
            evidence_refs: vec![digest],
        };
        let out = a.reconcile_worker_qualification(&id, op.clone()).unwrap();
        assert!(!out.quarantined);
        assert_eq!(out.state, "failed");
        assert!(out.verified.is_empty());
        assert!(!out.real_execution_admitted);
        assert!(out.reconciliation.is_some());
        assert_eq!(a.reconcile_worker_qualification(&id, op).unwrap().id, q.id);
        assert!(
            a.begin_qualification(&id, &command(&a, &id, "fresh-probe"))
                .unwrap()
                .2
        );
        assert!(
            a.escalation_inbox()
                .unwrap()
                .iter()
                .any(|e| e.source_type == "worker_qualification" && e.state == "open")
        );
    }
    #[tokio::test]
    async fn reconciliation_requires_reported_case_evidence_and_rolls_back_with_its_audit_event() {
        let (_d, a, _t, id, _) = fixture().await;
        let (q, _, _) = a
            .begin_qualification(&id, &command(&a, &id, "unknown-resources"))
            .unwrap();
        let mut e = evidence("native");
        e.quarantined = true;
        complete(&a, &q.id, e, 10);
        let mut op = ReconcileQualification {
            command: command(&a, &id, "reconcile").command,
            qualification_id: q.id.clone(),
            evidence_refs: vec![
                a.snapshot(&id).unwrap().worker_qualifications[0]
                    .evidence_digest
                    .clone()
                    .unwrap(),
            ],
        };
        assert!(a.reconcile_worker_qualification(&id, op.clone()).is_err());
        op.evidence_refs = vec![report_cleanup(&a, &id)];
        op.command.expected_version = a.snapshot(&id).unwrap().case.version;
        let db = a.store.connect().unwrap();
        db.execute_batch("CREATE TRIGGER fail_reconciliation BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='worker.qualification_reconciled' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(a.reconcile_worker_qualification(&id, op.clone()).is_err());
        assert!(get(&db, &q.id).unwrap().quarantined);
        db.execute_batch("DROP TRIGGER fail_reconciliation")
            .unwrap();
        assert!(
            !a.reconcile_worker_qualification(&id, op)
                .unwrap()
                .quarantined
        );
    }
    #[tokio::test]
    async fn reconciliation_http_enforces_authentication_and_never_promotes_capabilities() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (_d, a, _t, id, _) = fixture().await;
        let (q, _, _) = a
            .begin_qualification(&id, &command(&a, &id, "http-unknown"))
            .unwrap();
        let mut e = evidence("native");
        e.quarantined = true;
        complete(&a, &q.id, e, 10);
        let digest = report_cleanup(&a, &id);
        let cmd = ReconcileQualification {
            command: command(&a, &id, "http-reconcile").command,
            qualification_id: q.id,
            evidence_refs: vec![digest],
        };
        let token = "qualification-test-token-32-bytes-long";
        let router = crate::http::router(a.clone(), token.into(), "127.0.0.1:8787".into());
        let request = |auth: bool| {
            let mut r = Request::builder()
                .method("POST")
                .uri(format!("/api/v1/cases/{id}/worker-qualification/reconcile"))
                .header("Host", "127.0.0.1:8787")
                .header("Content-Type", "application/json");
            if auth {
                r = r.header("Authorization", format!("Bearer {token}"));
            }
            r.body(Body::from(serde_json::to_vec(&cmd).unwrap()))
                .unwrap()
        };
        assert_eq!(
            router
                .clone()
                .oneshot(request(false))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            router.oneshot(request(true)).await.unwrap().status(),
            StatusCode::OK
        );
        let result = &a.snapshot(&id).unwrap().worker_qualifications[0];
        assert!(!result.quarantined);
        assert!(!result.real_execution_admitted);
        assert!(result.verified.is_empty());
    }
}
