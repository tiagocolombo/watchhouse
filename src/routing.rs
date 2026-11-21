//! OS-controlled local worker routing. Selection never substitutes for physical admission.
use crate::{
    application::{Application, text},
    contracts::V1,
    error::{Error, Result},
    store,
};
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, named_params, params,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Model,
    Container,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Container => "container",
        }
    }
    fn table(self) -> &'static str {
        match self {
            Self::Model => "model_jobs",
            Self::Container => "container_jobs",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: Kind,
    pub policy_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    pub id: String,
    pub platform: String,
    pub architecture: String,
    pub paused: bool,
    pub targets: Vec<Target>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub workers: Vec<Worker>,
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
impl Policy {
    fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        if self.revision < 1 || self.workers.len() > 64 {
            return Err(Error::Invalid("worker routing limits exceeded".into()));
        }
        for w in &self.workers {
            let mut targets = BTreeSet::new();
            if !identifier(&w.id)
                || w.id == "local-legacy"
                || !ids.insert(&w.id)
                || !matches!(w.platform.as_str(), "linux" | "macos" | "windows")
                || !matches!(w.architecture.as_str(), "x86_64" | "aarch64")
                || w.targets.len() > 64
                || w.targets
                    .iter()
                    .any(|t| t.policy_revision < 1 || !targets.insert((t.kind, t.policy_revision)))
            {
                return Err(Error::Invalid(
                    "invalid or duplicate worker identity/platform/target".into(),
                ));
            }
        }
        Ok(())
    }
}
fn active(db: &Connection) -> Result<Option<(Policy, String)>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM worker_routes ORDER BY revision DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(raw, digest)| {
        if store::digest(&raw) != digest {
            return Err(Error::Conflict("worker routing integrity failure".into()));
        }
        let p: Policy = serde_json::from_str(&raw)?;
        p.validate()?;
        Ok((p, digest))
    })
    .transpose()
}
// This is a local shared-database scheduler, not a remote worker credential or an OS sandbox.
// Actual container and model protocols currently implement Linux boundaries only.
fn supported(w: &Worker) -> bool {
    w.platform == "linux"
        && w.platform == std::env::consts::OS
        && w.architecture == std::env::consts::ARCH
}
pub(crate) fn select(
    db: &Connection,
    kind: Kind,
    worker: Option<&str>,
    condition: &str,
    order: &str,
    now: &str,
    cleanup: bool,
) -> Result<Option<String>> {
    let id = worker.unwrap_or("local-legacy");
    if !identifier(id) {
        return Err(Error::Invalid("invalid local worker identity".into()));
    }
    let route = if cleanup { None } else { active(db)? };
    let revisions = if cleanup {
        String::new()
    } else {
        let admitted = match &route {
            None if worker.is_none() => "1=1".into(),
            Some((p, _)) => p
                .workers
                .iter()
                .find(|w| w.id == id && !w.paused && supported(w))
                .map_or_else(
                    || "0".into(),
                    |w| {
                        let revs: Vec<String> = w
                            .targets
                            .iter()
                            .filter(|t| t.kind == kind)
                            .map(|t| t.policy_revision.to_string())
                            .collect();
                        if revs.is_empty() {
                            "0".into()
                        } else {
                            format!("j.policy_revision IN ({})", revs.join(","))
                        }
                    },
                ),
            _ => "0".into(),
        };
        // Already running model observation creates no client call. Continue reading only
        // on the retained original host/worker while create/start admission is paused.
        if kind == Kind::Model {
            format!(
                " AND ({admitted} OR (j.state='running' AND EXISTS(SELECT 1 FROM worker_bindings b WHERE b.kind=:kind AND b.job_id=j.id AND b.worker_id=:worker AND b.platform=:platform AND b.architecture=:arch)))"
            )
        } else {
            format!(" AND ({admitted})")
        }
    };
    let binding = if cleanup {
        // Removing/pausing a route never strands cleanup on the original local host.
        "(EXISTS(SELECT 1 FROM worker_bindings b WHERE b.kind=:kind AND b.job_id=j.id AND b.worker_id=:worker AND b.platform=:platform AND b.architecture=:arch) OR (:worker='local-legacy' AND NOT EXISTS(SELECT 1 FROM worker_bindings b WHERE b.kind=:kind AND b.job_id=j.id)))"
    } else {
        "NOT EXISTS(SELECT 1 FROM worker_bindings b WHERE b.kind=:kind AND b.job_id=j.id AND (b.worker_id!=:worker OR b.platform!=:platform OR b.architecture!=:arch))"
    };
    let factory = if cleanup {
        String::new()
    } else {
        " AND (j.state='running' OR (NOT EXISTS(SELECT 1 FROM factory_holds h WHERE h.case_id=j.case_id AND h.effect IN ('all',:kind)) AND NOT EXISTS(SELECT 1 FROM factory_state WHERE high_at>:now)))".into()
    };
    let capacity = if cleanup {
        ""
    } else {
        " AND (j.state!='queued' OR EXISTS(SELECT 1 FROM factory_case_capacity c WHERE c.case_id=j.case_id AND c.available=1))"
    };
    let sql = format!(
        "SELECT j.id FROM {} j WHERE ({condition}) AND ({binding}) {revisions} {factory} {capacity} AND :now IS NOT NULL ORDER BY {order} LIMIT 1",
        kind.table()
    );
    Ok(db.query_row(&sql,named_params!{":kind":kind.name(),":worker":id,":platform":std::env::consts::OS,":arch":std::env::consts::ARCH,":now":now},|r|r.get(0)).optional()?)
}
pub(crate) fn bind(
    tx: &Transaction<'_>,
    kind: Kind,
    job: &str,
    worker: Option<&str>,
    now: &str,
) -> Result<()> {
    let revision = active(tx)?.map(|(p, _)| p.revision);
    tx.execute(
        "INSERT OR IGNORE INTO worker_bindings VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            kind.name(),
            job,
            worker.unwrap_or("local-legacy"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            revision,
            now
        ],
    )?;
    crate::capacity::bind_scope(tx, kind.name(), job, now)?;
    Ok(())
}
#[derive(Serialize)]
pub struct Status {
    pub policy: Option<Policy>,
    pub digest: Option<String>,
    pub installed_by: Option<String>,
    pub source: Option<String>,
    pub platform: String,
    pub architecture: String,
    pub workers: Vec<WorkerStatus>,
    pub bindings: Vec<serde_json::Value>,
    pub limitation: String,
}
#[derive(Serialize)]
pub struct WorkerStatus {
    pub id: String,
    pub dispatch_supported_here: bool,
    pub reason: String,
}
impl Application {
    pub fn install_worker_routes(&self, p: Policy, who: &str, source: &str) -> Result<String> {
        text(who, "operator", 128)?;
        text(source, "source", 4096)?;
        if who.starts_with("fake-") || who.starts_with("model-") {
            return Err(Error::Invalid(
                "OS operator required for worker routing".into(),
            ));
        }
        p.validate()?;
        let raw = serde_json::to_string(&p)?;
        let digest = store::digest(&raw);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT digest FROM worker_routes WHERE revision=?1",
                [p.revision],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old == digest {
                return Ok(digest);
            }
            return Err(Error::Conflict("worker route revision is immutable".into()));
        }
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0)+1 FROM worker_routes",
            [],
            |r| r.get(0),
        )?;
        if p.revision != next {
            return Err(Error::Conflict(
                "worker route revisions must be consecutive".into(),
            ));
        }
        for w in &p.workers {
            for t in &w.targets {
                let table = match t.kind {
                    Kind::Model => "model_policies",
                    Kind::Container => "container_policies",
                };
                let exists: bool = tx.query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE revision=?1)"),
                    [t.policy_revision],
                    |r| r.get(0),
                )?;
                if !exists {
                    return Err(Error::Invalid(
                        "route references an uninstalled execution policy".into(),
                    ));
                }
            }
        }
        tx.execute(
            "INSERT INTO worker_routes VALUES (?1,?2,?3,?4,?5,?6)",
            params![p.revision, raw, digest, who, source, self.now()],
        )?;
        tx.commit()?;
        Ok(digest)
    }
    pub fn worker_routes(&self) -> Result<Status> {
        let db = self.store.connect()?;
        let active = active(&db)?;
        let (policy, digest) = active.map_or((None, None), |(p, d)| (Some(p), Some(d)));
        let provenance: Option<(String, String)> = db
            .query_row(
                "SELECT operator,source FROM worker_routes ORDER BY revision DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let workers=policy.as_ref().map_or(vec![],|p|p.workers.iter().map(|w|WorkerStatus{id:w.id.clone(),dispatch_supported_here:supported(w)&&!w.paused,reason:if w.paused {"OS route paused"}else if supported(w){"Local route eligible; existing policy/native admission gates still apply"}else{"No matching implemented native boundary on this host; work remains owned and expires normally"}.into()}).collect());
        let mut q=db.prepare("SELECT b.kind,b.job_id,b.worker_id,b.route_revision,b.recorded_at FROM worker_bindings b ORDER BY b.rowid DESC LIMIT 100")?;
        let bindings=q.query_map([],|r|Ok(serde_json::json!({"kind":r.get::<_,String>(0)?,"job_id":r.get::<_,String>(1)?,"worker_id":r.get::<_,String>(2)?,"route_revision":r.get::<_,Option<i64>>(3)?,"recorded_at":r.get::<_,String>(4)?})))?.collect::<std::result::Result<_,_>>()?;
        Ok(Status{policy,digest,installed_by:provenance.as_ref().map(|p|p.0.clone()),source:provenance.map(|p|p.1),platform:std::env::consts::OS.into(),architecture:std::env::consts::ARCH.into(),workers,bindings,limitation:"Local OS-installed routing only; no remote registration, worker authentication, native macOS/Windows candidate boundary or transfer of isolation proof. Route labels never grant execution capability. Uncertain resources remain bound to their original worker; cleanup remains available after route removal.".into()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch(include_str!("../migrations/029_worker_routing.sql"))
            .unwrap();
        db.execute_batch("CREATE TABLE model_jobs(id TEXT PRIMARY KEY,policy_revision INTEGER,state TEXT); CREATE TABLE container_jobs(id TEXT PRIMARY KEY,policy_revision INTEGER,state TEXT); CREATE TABLE factory_case_scopes(case_id TEXT,repository TEXT,service TEXT); INSERT INTO factory_case_scopes VALUES ('route-test',NULL,NULL); CREATE TABLE factory_resource_scopes(kind TEXT,job_id TEXT,repository TEXT,service TEXT,recorded_at TEXT,PRIMARY KEY(kind,job_id)); CREATE TABLE factory_case_capacity(case_id TEXT,available INTEGER); INSERT INTO factory_case_capacity VALUES ('route-test',1); CREATE TABLE factory_holds(case_id TEXT,effect TEXT); CREATE TABLE factory_state(high_at TEXT); ALTER TABLE model_jobs ADD COLUMN case_id TEXT DEFAULT 'route-test'; ALTER TABLE container_jobs ADD COLUMN case_id TEXT DEFAULT 'route-test';").unwrap();
        db.execute_batch("CREATE TABLE factory_limits(revision INTEGER PRIMARY KEY,record TEXT,digest TEXT,operator TEXT,source TEXT,installed_at TEXT);").unwrap();
        let tx = db.transaction().unwrap();
        crate::capacity::bootstrap(&tx).unwrap();
        tx.commit().unwrap();
        db
    }
    fn put(db: &Connection, revision: i64, paused: bool, platform: &str) {
        let p = Policy {
            schema_version: V1,
            revision,
            workers: vec![Worker {
                id: "worker-a".into(),
                platform: platform.into(),
                architecture: std::env::consts::ARCH.into(),
                paused,
                targets: vec![Target {
                    kind: Kind::Model,
                    policy_revision: 2,
                }],
            }],
        };
        p.validate().unwrap();
        let raw = serde_json::to_string(&p).unwrap();
        db.execute(
            "INSERT INTO worker_routes VALUES (?1,?2,?3,'operator','fixture','now')",
            params![revision, raw, store::digest(&raw)],
        )
        .unwrap();
    }
    fn choose(db: &Connection, worker: Option<&str>, cleanup: bool) -> Result<Option<String>> {
        select(db, Kind::Model, worker, "1=1", "j.rowid", "now", cleanup)
    }
    #[test]
    fn legacy_work_stays_compatible_but_arbitrary_names_cannot_claim() {
        let mut db = db();
        db.execute(
            "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('old',1,'queued')",
            [],
        )
        .unwrap();
        assert_eq!(choose(&db, None, false).unwrap().as_deref(), Some("old"));
        assert!(choose(&db, Some("worker-a"), false).unwrap().is_none());
        let tx = db.transaction().unwrap();
        bind(&tx, Kind::Model, "old", None, "now").unwrap();
        tx.commit().unwrap();
        put(&db, 1, true, "linux");
        assert!(choose(&db, None, false).unwrap().is_none());
        assert_eq!(choose(&db, None, true).unwrap().as_deref(), Some("old"));
    }
    #[test]
    fn platform_and_architecture_never_become_capability_proof() {
        for platform in ["linux", "windows", "macos"] {
            let db = db();
            db.execute(
                "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('j',2,'queued')",
                [],
            )
            .unwrap();
            put(&db, 1, false, platform);
            let selected = choose(&db, Some("worker-a"), false).unwrap();
            assert_eq!(
                selected.is_some(),
                platform == "linux" && std::env::consts::OS == "linux"
            );
        }
        let mut p = Policy {
            schema_version: V1,
            revision: 1,
            workers: vec![],
        };
        p.workers.push(Worker {
            id: "worker-a".into(),
            platform: "linux".into(),
            architecture: "invented".into(),
            paused: false,
            targets: vec![],
        });
        assert!(p.validate().is_err());
        p.workers[0].architecture = "x86_64".into();
        p.workers.push(p.workers[0].clone());
        assert!(p.validate().is_err());
    }
    #[test]
    fn cleanup_survives_corrupt_or_paused_routing_without_stealing_bindings() {
        let mut db = db();
        db.execute(
            "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('reserved',2,'unknown')",
            [],
        )
        .unwrap();
        put(&db, 1, true, "linux");
        let tx = db.transaction().unwrap();
        bind(&tx, Kind::Model, "reserved", Some("worker-a"), "now").unwrap();
        tx.commit().unwrap();
        assert_eq!(
            choose(&db, Some("worker-a"), true).unwrap().as_deref(),
            Some("reserved")
        );
        assert!(choose(&db, Some("worker-b"), true).unwrap().is_none());
        assert!(choose(&db, None, true).unwrap().is_none());
        db.execute_batch(
            "DROP TRIGGER worker_routes_no_update; UPDATE worker_routes SET digest='corrupt';",
        )
        .unwrap();
        assert!(choose(&db, Some("worker-a"), false).is_err());
        assert!(choose(&db, Some("worker-a"), true).unwrap().is_some());
        assert!(db.execute("DELETE FROM worker_bindings", []).is_err());
    }
    #[test]
    fn paused_route_keeps_original_running_observation_available() {
        let mut db = db();
        db.execute(
            "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('running',2,'running')",
            [],
        )
        .unwrap();
        put(&db, 1, true, "linux");
        let tx = db.transaction().unwrap();
        bind(&tx, Kind::Model, "running", Some("worker-a"), "now").unwrap();
        tx.commit().unwrap();
        assert_eq!(
            choose(&db, Some("worker-a"), false).unwrap().as_deref(),
            Some("running")
        );
        assert!(choose(&db, Some("worker-b"), false).unwrap().is_none());
    }
    #[test]
    fn route_binding_is_atomic_and_namespace_isolated() {
        let mut db = db();
        db.execute(
            "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('same',1,'queued')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO container_jobs(id,policy_revision,state) VALUES ('same',1,'queued')",
            [],
        )
        .unwrap();
        {
            let tx = db.transaction().unwrap();
            bind(&tx, Kind::Model, "same", None, "now").unwrap();
        }
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM worker_bindings", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let tx = db.transaction().unwrap();
        bind(&tx, Kind::Model, "same", Some("worker-a"), "now").unwrap();
        bind(&tx, Kind::Container, "same", Some("worker-b"), "now").unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM worker_bindings", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn unmatched_queue_prefix_does_not_starve_matching_policy_and_binding_cannot_move() {
        let mut db = db();
        for i in 0..150 {
            db.execute(
                "INSERT INTO model_jobs(id,policy_revision,state) VALUES (?1,1,'queued')",
                [format!("foreign-{i}")],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO model_jobs(id,policy_revision,state) VALUES ('matching',2,'queued')",
            [],
        )
        .unwrap();
        put(&db, 1, false, "linux");
        assert_eq!(
            choose(&db, Some("worker-a"), false).unwrap().as_deref(),
            Some("matching")
        );
        let tx = db.transaction().unwrap();
        bind(&tx, Kind::Model, "matching", Some("worker-b"), "now").unwrap();
        tx.commit().unwrap();
        assert!(choose(&db, Some("worker-a"), false).unwrap().is_none());
        assert!(choose(&db, Some("worker-b"), false).unwrap().is_none());
    }
}
