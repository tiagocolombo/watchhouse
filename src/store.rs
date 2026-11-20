//! SQLite owns state, events, evidence bytes, command receipts, and runnable work.
use crate::contracts::{Actor, Event, EventData, V1};
use crate::domain::{Case, CaseStatus};
use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let store = Self {
            path: path.as_ref().to_path_buf(),
        };
        let mut db = store.connect()?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, digest TEXT NOT NULL);")?;
        let migrations = [
            (1, include_str!("../migrations/001_initial.sql")),
            (2, include_str!("../migrations/002_qualification.sql")),
            (3, include_str!("../migrations/003_team.sql")),
            (4, include_str!("../migrations/004_challenges.sql")),
            (5, include_str!("../migrations/005_policy.sql")),
            (6, include_str!("../migrations/006_notifications.sql")),
            (7, include_str!("../migrations/007_sentry_incidents.sql")),
            (8, include_str!("../migrations/008_incident_operations.sql")),
            (9, include_str!("../migrations/009_incident_artifacts.sql")),
            (
                10,
                include_str!("../migrations/010_source_read_reliability.sql"),
            ),
            (11, include_str!("../migrations/011_case_correlations.sql")),
            (12, include_str!("../migrations/012_repair_preflight.sql")),
            (
                13,
                include_str!("../migrations/013_worker_qualification.sql"),
            ),
            (14, include_str!("../migrations/014_adapter_trials.sql")),
            (15, include_str!("../migrations/015_incident_recovery.sql")),
            (16, include_str!("../migrations/016_learning.sql")),
            (17, include_str!("../migrations/017_native_execution.sql")),
            (18, include_str!("../migrations/018_knowledge_imports.sql")),
            (19, include_str!("../migrations/019_model_runtime.sql")),
            (20, include_str!("../migrations/020_publisher_policy.sql")),
            (21, include_str!("../migrations/021_publications.sql")),
            (22, include_str!("../migrations/022_impact_assessments.sql")),
            (23, include_str!("../migrations/023_service_topology.sql")),
            (
                24,
                include_str!("../migrations/024_hypothesis_acceptance.sql"),
            ),
            (25, include_str!("../migrations/025_sentry_projects.sql")),
            (26, include_str!("../migrations/026_operations.sql")),
            (27, include_str!("../migrations/027_merging.sql")),
            (28, include_str!("../migrations/028_datadog.sql")),
        ];
        let applied: Vec<(i64, String)> = {
            let mut stmt =
                tx.prepare("SELECT version,digest FROM schema_migrations ORDER BY version")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?
        };
        // Validate the complete applied history before running any new migration.
        for (index, (version, checksum)) in applied.iter().enumerate() {
            let Some((expected, sql)) = migrations.get(index) else {
                return Err(Error::Conflict(
                    "database is newer than this application".into(),
                ));
            };
            if version != expected
                || checksum != &migration_digest(sql)
                    && checksum != &digest(&sql.replace("\r\n", "\n").replace('\n', "\r\n"))
            {
                return Err(Error::Conflict(format!(
                    "migration {version} history or checksum changed"
                )));
            }
        }
        for (version, sql) in migrations.iter().skip(applied.len()) {
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations VALUES (?1,?2)",
                params![version, migration_digest(sql)],
            )?;
        }
        crate::policy::bootstrap(&tx)?;
        tx.commit()?;
        Ok(store)
    }
    pub fn connect(&self) -> Result<Connection> {
        let db = Connection::open(&self.path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        Ok(db)
    }
}
// Git may check out CRLF on Windows. New checksums use LF; exact legacy CRLF
// checksums are accepted without rewriting applied history.
fn migration_digest(sql: &str) -> String {
    digest(&sql.replace("\r\n", "\n"))
}
pub fn digest(content: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
}
pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}
pub fn case(db: &Connection, id: &str) -> Result<Case> {
    db.query_row(
        "SELECT id,title,description,owner,status,version,created_at FROM cases WHERE id=?1",
        [id],
        |row| {
            let status: String = row.get(4)?;
            Ok(Case {
                schema_version: V1,
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                owner: row.get(3)?,
                status: match status.as_str() {
                    "queued" => CaseStatus::Queued,
                    "running" => CaseStatus::Running,
                    _ => CaseStatus::AwaitingReview,
                },
                version: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or(Error::NotFound)
}
/// All child records use the case aggregate's serialization/version boundary in v1.
pub fn emit(
    tx: &Transaction<'_>,
    case_id: &str,
    actor: Actor,
    cause: &str,
    time: &str,
    data: EventData,
) -> Result<()> {
    tx.execute("UPDATE cases SET version=version+1 WHERE id=?1", [case_id])?;
    let version = case(tx, case_id)?.version;
    let event = Event {
        cursor: 0,
        event_id: id("evt"),
        schema_version: V1,
        aggregate_type: "case".into(),
        aggregate_id: case_id.into(),
        aggregate_version: version,
        recorded_at: time.into(),
        occurred_at: None,
        actor,
        correlation_id: case_id.into(),
        causation_id: cause.into(),
        policy_version: crate::policy::for_case(tx, case_id)?.provenance(),
        data,
    };
    tx.execute(
        "INSERT INTO events(event_id,case_id,aggregate_version,envelope) VALUES (?1,?2,?3,?4)",
        params![
            event.event_id,
            case_id,
            version,
            serde_json::to_string(&event)?
        ],
    )?;
    Ok(())
}
pub fn events(db: &Connection, case_id: &str, after: i64, limit: i64) -> Result<Vec<Event>> {
    let mut stmt = db.prepare("SELECT cursor,envelope FROM events WHERE case_id=?1 AND cursor>?2 ORDER BY cursor LIMIT ?3")?;
    let rows = stmt.query_map(params![case_id, after, limit], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (cursor, json) = row?;
        let mut event: Event = serde_json::from_str(&json)?;
        event.cursor = cursor;
        Ok(event)
    })
    .collect()
}
