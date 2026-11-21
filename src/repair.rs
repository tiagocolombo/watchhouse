//! First M4 slice: trusted frozen inputs and durable denied admission, never real execution.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub(crate) mod snapshot;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairSourceConfig {
    pub schema_version: V1,
    pub revision: i64,
    pub repository: String,
    pub source_path: PathBuf,
    pub base_commit: String,
    pub workspace_root: PathBuf,
    pub runtime_path: PathBuf,
    pub runtime_digest: String,
    pub max_files: u32,
    pub max_blob_bytes: u64,
    pub max_total_bytes: u64,
    pub preparation_seconds: u32,
    pub probe_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub path: String,
    pub git_mode: String,
    pub git_blob: String,
    pub bytes: u64,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseSnapshot {
    pub base_commit: String,
    pub tree: String,
    pub files: Vec<SnapshotFile>,
    pub total_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub adapter: String,
    pub binary_digest: Option<String>,
    pub version: Option<String>,
    pub version_output_digest: Option<String>,
    pub help_output_digest: Option<String>,
    pub advertised: Vec<String>,
    pub verified: Vec<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairSource {
    pub schema_version: V1,
    pub config: RepairSourceConfig,
    pub config_digest: String,
    pub manifest_digest: String,
    pub snapshot: BaseSnapshot,
    pub snapshot_path: PathBuf,
    pub probe: RuntimeProbe,
    pub operator: String,
    pub recorded_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestRepair {
    pub command: OperatorCommand,
    pub source_revision: i64,
    pub objective: String,
    pub expected_behavior: String,
    pub regression_plan: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRequest {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub owner: String,
    pub state: String,
    pub source_revision: i64,
    pub config_digest: String,
    pub manifest_digest: String,
    pub repository: String,
    pub base_commit: String,
    pub base_tree: String,
    pub policy_provenance: String,
    pub objective: String,
    pub expected_behavior: String,
    pub regression_plan: String,
    pub evidence_refs: Vec<String>,
    pub recorded_at: String,
    pub denials: Vec<String>,
    pub next_action: String,
}
pub(crate) fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn human(s: &str) -> Result<()> {
    text(s, "operator", 128)?;
    if s.starts_with("fake-") {
        return Err(Error::Invalid(
            "agents cannot configure or authorize real execution".into(),
        ));
    }
    Ok(())
}
fn invalid(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("repair configuration: {e}"))
}
impl RepairSourceConfig {
    fn validate(&mut self) -> Result<()> {
        if self.revision < 1
            || !oid(&self.base_commit)
            || !(1..=5000).contains(&self.max_files)
            || !(1..=8 * 1024 * 1024).contains(&self.max_blob_bytes)
            || self.max_total_bytes < self.max_blob_bytes
            || self.max_total_bytes > 128 * 1024 * 1024
            || !(1..=60).contains(&self.preparation_seconds)
            || !(1..=10).contains(&self.probe_seconds)
            || self.runtime_digest.len() != 71
            || !self.runtime_digest.starts_with("sha256:")
            || !self.runtime_digest[7..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Invalid(
                "invalid repair limits or pinned identities".into(),
            ));
        }
        if !self.repository.split_once('/').is_some_and(|(o, r)| {
            [o, r].iter().all(|s| {
                !s.is_empty()
                    && s.len() <= 128
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            })
        }) {
            return Err(Error::Invalid(
                "explicit repository mapping required".into(),
            ));
        }
        self.base_commit.make_ascii_lowercase();
        for p in [&self.source_path, &self.workspace_root, &self.runtime_path] {
            if !p.is_absolute() {
                return Err(Error::Invalid(
                    "repair paths must be explicit absolute operator paths".into(),
                ));
            }
        }
        self.source_path = fs::canonicalize(&self.source_path).map_err(invalid)?;
        self.workspace_root = fs::canonicalize(&self.workspace_root).map_err(invalid)?;
        if self.runtime_path.exists() {
            self.runtime_path = fs::canonicalize(&self.runtime_path).map_err(invalid)?;
        }
        if self.runtime_path.starts_with(&self.source_path)
            || self.runtime_path.starts_with(&self.workspace_root)
        {
            return Err(Error::Invalid(
                "runtime must be outside source and snapshot roots".into(),
            ));
        }
        // Roots are created by the operator, never by an imported command.
        if !self.source_path.is_dir()
            || !self.workspace_root.is_dir()
            || self.workspace_root.starts_with(&self.source_path)
            || self.source_path.starts_with(&self.workspace_root)
        {
            return Err(Error::Invalid(
                "snapshot and source roots must be separate, non-overlapping directories".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn source(db: &Connection, revision: i64) -> Result<RepairSource> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM repair_sources WHERE revision=?1",
            [revision],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("repair source integrity failure".into()));
    }
    let mut s: RepairSource = serde_json::from_str(&raw)?;
    if store::digest(&serde_json::to_string(&s.config)?) != s.config_digest
        || store::digest(&serde_json::to_string(&s.snapshot)?) != s.manifest_digest
    {
        return Err(Error::Conflict("repair manifest integrity failure".into()));
    }
    if let Some(path) = crate::restore::materialization(db, "source", revision)? {
        s.snapshot_path = path;
    }
    Ok(s)
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<Vec<RepairRequest>> {
    let mut stmt = db.prepare(
        "SELECT record FROM repair_requests WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    stmt.query_map([id], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
impl Application {
    /// OS operator control plane only: snapshots repository objects and inspects version/help.
    pub async fn install_repair_source(
        &self,
        mut cfg: RepairSourceConfig,
        who: &str,
    ) -> Result<RepairSource> {
        human(who)?;
        cfg.validate()?;
        let config_digest = store::digest(&serde_json::to_string(&cfg)?);
        match source(&self.store.connect()?, cfg.revision) {
            Ok(old) if old.config_digest == config_digest => return Ok(old),
            Ok(_) => {
                return Err(Error::Conflict(
                    "repair source revisions are immutable".into(),
                ));
            }
            Err(Error::NotFound) => {}
            Err(error) => return Err(error),
        }
        let probe = snapshot::probe(&cfg).await;
        let (directory, manifest) = snapshot::prepare(&cfg).await?;
        let s = RepairSource {
            schema_version: V1,
            config: cfg.clone(),
            config_digest,
            manifest_digest: store::digest(&serde_json::to_string(&manifest)?),
            snapshot: manifest,
            snapshot_path: directory.path().to_path_buf(),
            probe,
            operator: who.into(),
            recorded_at: self.now(),
        };
        let raw = serde_json::to_string(&s)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM repair_sources",
            [],
            |r| r.get(0),
        )?;
        if cfg.revision != latest + 1 {
            return Err(Error::Conflict(
                "repair source revision must advance by one; reload before retrying".into(),
            ));
        }
        tx.execute(
            "INSERT INTO repair_sources VALUES (?1,?2,?3)",
            params![cfg.revision, store::digest(&raw), raw],
        )?;
        tx.commit()?;
        let _ = directory.keep();
        Ok(s)
    }
    pub fn repair_source_status(&self) -> Result<Option<RepairSource>> {
        let db = self.store.connect()?;
        let revision: Option<i64> =
            db.query_row("SELECT MAX(revision) FROM repair_sources", [], |r| r.get(0))?;
        revision.map(|r| source(&db, r)).transpose()
    }
    pub fn repair_markdown(&self, id: &str) -> Result<String> {
        let s = self.snapshot(id)?;
        let mut report = format!(
            "# Repair preparation {}\n\nPreparation, native checks, candidates and attributed review are separate records. Inspect each result and its provenance; this report does not establish model execution, PR delivery, merge, deployment or recovery.\n\n",
            id
        );
        if !s
            .container_jobs
            .iter()
            .any(|j| j.native && j.state == "passed")
        {
            report.push_str(
                "No real worker or native regression has been verified for this case.\n\n",
            );
        }
        for r in s.repairs {
            let raw = serde_json::to_string_pretty(&r)?
                .replace('`', "\\u0060")
                .replace('<', "\\u003c")
                .replace('>', "\\u003e");
            report.push_str(&format!(
                "## Frozen request {}\n\n```json\n{raw}\n```\n\n",
                r.id
            ));
        }
        let raw = serde_json::to_string_pretty(&serde_json::json!({"container_jobs":s.container_jobs,"repair_progress":s.repair_progress}))?.replace('`', "\\u0060").replace('<', "\\u003c").replace('>', "\\u003e");
        report.push_str(&format!(
            "## Candidate validation and review\n\n```json\n{raw}\n```\n"
        ));
        Ok(report)
    }
    pub fn request_repair(&self, id: &str, cmd: RequestRepair) -> Result<RepairRequest> {
        let m = &cmd.command.meta;
        human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "idempotency_key", 128)?;
        for (name, value, max) in [
            ("objective", &cmd.objective, 4000),
            ("expected_behavior", &cmd.expected_behavior, 4000),
            ("regression_plan", &cmd.regression_plan, 4000),
        ] {
            text(value, name, max)?;
        }
        let fp = store::digest(&format!(
            "request-repair:{id}:{}",
            serde_json::to_string(&cmd)?
        ));
        // Verify filesystem integrity without holding the canonical writer lock.
        let preflight_db = self.store.connect()?;
        let preflight_tx = preflight_db.unchecked_transaction()?;
        if let Some(r) = receipt(&preflight_tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let preflight_case = store::case(&preflight_tx, id)?;
        if preflight_case.owner != m.actor || preflight_case.version != cmd.command.expected_version
        {
            return Err(Error::Conflict(
                "reload the owned case before repair preparation".into(),
            ));
        }
        let prepared = source(&preflight_tx, cmd.source_revision)?;
        drop(preflight_tx);
        drop(preflight_db);
        let healthy = snapshot::verify(
            &prepared.snapshot_path,
            &prepared.snapshot,
            prepared.config.preparation_seconds,
        )
        .is_ok();
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let case = store::case(&tx, id)?;
        if case.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale repair request; reload case".into()));
        }
        if case.owner != m.actor {
            return Err(Error::Invalid(
                "accountable operator must request repair preparation".into(),
            ));
        }
        crate::challenges::ensure_clear(&tx, id)?;
        crate::qualification::evidence_refs(&tx, id, &cmd.evidence_refs, true)?;
        crate::restore::require_fresh(&tx, "repair_source", cmd.source_revision)?;
        let s = source(&tx, cmd.source_revision)?;
        if let Some(observation) = crate::sentry::snapshot(&tx, id)?
            && (observation.repository != s.config.repository
                || observation.deployed_revision.as_deref()
                    != Some(s.snapshot.base_commit.as_str()))
        {
            return Err(Error::Conflict(
                "repair base does not match the case's explicitly mapped deployed revision".into(),
            ));
        }
        let mut denials=vec!["No admitted isolated worker: filesystem/network containment, scoped credentials, process-tree cancellation and lost-worker reconciliation have not been demonstrated".into(),"Codex help/version advertisements grant no execution authority; no model invocation, candidate check, PR publication or production effect is enabled".into()];
        if let Some(error) = &s.probe.error {
            denials.push(error.clone());
        }
        if !healthy {
            denials.push("Frozen base snapshot is missing, altered or exceeds verification bounds; recreate through a new trusted revision".into());
        }
        let r=RepairRequest {schema_version:V1,id:store::id("repair"),case_id:id.into(),owner:case.owner,state:"blocked".into(),source_revision:cmd.source_revision,config_digest:s.config_digest,manifest_digest:s.manifest_digest,repository:s.config.repository,base_commit:s.snapshot.base_commit,base_tree:s.snapshot.tree,policy_provenance:crate::policy::for_case(&tx,id)?.provenance(),objective:cmd.objective,expected_behavior:cmd.expected_behavior,regression_plan:cmd.regression_plan,evidence_refs:cmd.evidence_refs,recorded_at:self.now(),denials,next_action:"The accountable operator must qualify an isolated worker and scoped runtime adapter, then submit a fresh versioned request. This record never enters the fake-worker queue or waives the escalation.".into()};
        tx.execute(
            "INSERT INTO repair_requests VALUES (?1,?2,?3)",
            params![r.id, id, serde_json::to_string(&r)?],
        )?;
        store::emit(
            &tx,
            id,
            operator(&m.actor),
            &m.idempotency_key,
            &self.now(),
            EventData::RepairRequested {
                request: Box::new(r.clone()),
                reason: m.reason.clone(),
            },
        )?;
        crate::team::escalate(
            &tx,
            id,
            "repair_admission",
            &r.id,
            "Repair is blocked on worker capability/isolation admission; the accountable operator owns the next action",
            &self.now(),
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
}
