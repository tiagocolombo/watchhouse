//! Transactional bounded intake/storage and native reservation capacity; duties have separate headroom.
use crate::{
    application::{Application, text},
    contracts::V1,
    error::{Error, Result},
    store,
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub schema_version: V1,
    pub revision: i64,
    pub max_queued_cases: u32,
    pub max_queued_effects: u32,
    pub max_pending_deliveries: u32,
    pub max_evidence_bytes: u64,
    pub supervisor_evidence_bytes: u64,
    pub max_native_resources: u32,
    pub max_native_per_repository: u32,
    pub max_native_per_service: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            schema_version: V1,
            revision: 1,
            max_queued_cases: 1000,
            max_queued_effects: 1000,
            max_pending_deliveries: 1000,
            max_evidence_bytes: 256 * 1024 * 1024,
            supervisor_evidence_bytes: 16 * 1024 * 1024,
            max_native_resources: 2,
            max_native_per_repository: 2,
            max_native_per_service: 2,
        }
    }
}
impl Limits {
    fn validate(&self) -> Result<()> {
        if self.revision < 1
            || [
                self.max_queued_cases,
                self.max_queued_effects,
                self.max_pending_deliveries,
            ]
            .iter()
            .any(|n| !(1..=100_000).contains(n))
            || !(1024..=16 * 1024 * 1024 * 1024).contains(&self.max_evidence_bytes)
            || !(65536..=256 * 1024 * 1024).contains(&self.supervisor_evidence_bytes)
            || !(1..=64).contains(&self.max_native_resources)
            || [self.max_native_per_repository, self.max_native_per_service]
                .iter()
                .any(|n| *n == 0 || *n > self.max_native_resources)
        {
            return Err(Error::Invalid(
                "factory capacity limits exceed supported bounds".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn bootstrap(tx: &Transaction<'_>) -> Result<()> {
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM factory_limits", [], |r| r.get(0))?;
    if count == 0 {
        let p = Limits::default();
        let raw = serde_json::to_string(&p)?;
        tx.execute("INSERT INTO factory_limits VALUES (1,?1,?2,'builtin','bounded-default-v1','1970-01-01T00:00:00.000Z')",params![raw,store::digest(&raw)])?;
    }
    Ok(())
}
fn active(db: &Connection) -> Result<Limits> {
    let (raw, digest): (String, String) = db.query_row(
        "SELECT record,digest FROM factory_limits ORDER BY revision DESC LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("factory limit integrity failure".into()));
    }
    let p: Limits = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(p)
}
pub(crate) fn bind_scope(tx: &Transaction<'_>, kind: &str, job: &str, now: &str) -> Result<()> {
    let bound: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM factory_resource_scopes WHERE kind=?1 AND job_id=?2)",
        params![kind, job],
        |r| r.get(0),
    )?;
    if !bound {
        active(tx)?;
    }
    let table = match kind {
        "model" => "model_jobs",
        "container" => "container_jobs",
        _ => return Err(Error::Invalid("invalid resource kind".into())),
    };
    tx.execute(&format!("INSERT OR IGNORE INTO factory_resource_scopes SELECT ?1,j.id,s.repository,s.service,?3 FROM {table} j JOIN factory_case_scopes s ON s.case_id=j.case_id WHERE j.id=?2"),params![kind,job,now])?;
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub limits: Limits,
    pub installed_by: String,
    pub source: String,
    pub queued_cases: i64,
    pub queued_effects: i64,
    pub pending_deliveries: i64,
    pub ordinary_evidence_bytes: i64,
    pub supervisor_evidence_bytes: i64,
    pub native_resources: i64,
    pub scoped_usage: Vec<serde_json::Value>,
    pub database_and_journal_bytes: u64,
    pub intake_at_capacity: bool,
    pub evidence_at_capacity: bool,
    pub resources_at_capacity: bool,
    pub limitation: String,
}
impl Application {
    pub fn install_factory_limits(&self, p: Limits, who: &str, source: &str) -> Result<String> {
        text(who, "operator", 128)?;
        text(source, "source", 4096)?;
        if who.starts_with("fake-") || who.starts_with("model-") {
            return Err(Error::Invalid(
                "OS operator required for capacity policy".into(),
            ));
        }
        p.validate()?;
        let raw = serde_json::to_string(&p)?;
        let digest = store::digest(&raw);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT digest FROM factory_limits WHERE revision=?1",
                [p.revision],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old == digest {
                return Ok(digest);
            }
            return Err(Error::Conflict(
                "factory limit revision is immutable".into(),
            ));
        }
        active(&tx)?;
        let next: i64 = tx.query_row("SELECT MAX(revision)+1 FROM factory_limits", [], |r| {
            r.get(0)
        })?;
        if p.revision != next {
            return Err(Error::Conflict(
                "factory limit revisions must be consecutive".into(),
            ));
        }
        tx.execute(
            "INSERT INTO factory_limits VALUES (?1,?2,?3,?4,?5,?6)",
            params![p.revision, raw, digest, who, source, self.now()],
        )?;
        tx.commit()?;
        Ok(digest)
    }
    pub fn capacity_status(&self) -> Result<Status> {
        let db = self.store.connect()?;
        let p = active(&db)?;
        let (installed_by, source) = db.query_row(
            "SELECT operator,source FROM factory_limits ORDER BY revision DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let cases = db.query_row(
            "SELECT COUNT(*) FROM cases WHERE status='queued'",
            [],
            |r| r.get::<_, i64>(0),
        )?;
        let effects = db.query_row("SELECT COUNT(*) FROM factory_queued_effects", [], |r| {
            r.get::<_, i64>(0)
        })?;
        let deliveries = db.query_row(
            "SELECT COUNT(*) FROM source_deliveries WHERE state='pending'",
            [],
            |r| r.get::<_, i64>(0),
        )?;
        let (ordinary, reserved): (i64, i64) = db.query_row(
            "SELECT ordinary_bytes,supervisor_bytes FROM factory_evidence_usage WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let resources = db.query_row("SELECT COUNT(*) FROM factory_native_resources", [], |r| {
            r.get::<_, i64>(0)
        })?;
        let mut scoped_usage = vec![];
        for (column, limit) in [
            ("repository", p.max_native_per_repository),
            ("service", p.max_native_per_service),
        ] {
            let mut q=db.prepare(&format!("SELECT {column},COUNT(*) FROM factory_scoped_resources WHERE {column} IS NOT NULL GROUP BY {column} ORDER BY {column} LIMIT 100"))?;
            scoped_usage.extend(q.query_map([],|r|Ok(serde_json::json!({"dimension":column,"scope":r.get::<_,String>(0)?,"reservations":r.get::<_,i64>(1)?,"limit":limit})))?.collect::<std::result::Result<Vec<_>,_>>()?);
        }
        let mut bytes = 0;
        for suffix in ["", "-wal", "-shm"] {
            let mut file = self.store.path().as_os_str().to_owned();
            file.push(suffix);
            match std::fs::metadata(std::path::PathBuf::from(file)) {
                Ok(meta) => bytes += meta.len(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Invalid("factory storage usage unavailable".into())),
            }
        }
        Ok(Status{installed_by,source,queued_cases:cases,queued_effects:effects,pending_deliveries:deliveries,ordinary_evidence_bytes:ordinary,supervisor_evidence_bytes:reserved,native_resources:resources,scoped_usage,database_and_journal_bytes:bytes,intake_at_capacity:cases>=i64::from(p.max_queued_cases)||effects>=i64::from(p.max_queued_effects)||deliveries>=i64::from(p.max_pending_deliveries),evidence_at_capacity:ordinary>=p.max_evidence_bytes as i64||reserved>=p.supervisor_evidence_bytes as i64,resources_at_capacity:resources>=i64::from(p.max_native_resources),limits:p,limitation:"Queue limits reject new work before acknowledgment; exact receipts/deduplication and existing reconciliation continue. Supervisor checkpoints have separate bounded evidence headroom. Counts are resource reservations, including simulated fixtures, not native proof; uncertain/quarantined reservations count until cleanup is acknowledged. Capacity waits do not extend deadlines. Repository/service scope is pinned at first reservation; later remapping cannot free old scoped capacity. Global capacity always applies. Scoped displays cap each dimension at 100; admission counts all reservations. Evidence byte limits do not cap audit history, filesystem candidate artifacts or the entire database. Disk usage reports the database/WAL/SHM only; retention and free-space handling remain separate.".into()})
    }
}
