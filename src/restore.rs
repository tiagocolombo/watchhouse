//! Restore preserves facts and deadlines, never a copied deployment's effect authority.
use crate::{
    application::{Application, text},
    backups::{self, Manifest},
    error::{Error, Result},
    store::{self, Store},
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn io(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("restore I/O: {e}"))
}
fn has_table(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )?)
}
pub(crate) fn fenced(db: &Connection, kind: &str, id: impl ToString) -> Result<bool> {
    if !has_table(db, "restore_fences")? {
        return Ok(false);
    }
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM restore_fences WHERE kind=?1 AND identity=?2)",
        params![kind, id.to_string()],
        |r| r.get(0),
    )?)
}
pub(crate) fn require_fresh(db: &Connection, kind: &str, id: impl ToString) -> Result<()> {
    if fenced(db, kind, id)? {
        return Err(Error::Conflict(
            "restored authority is fenced; install and qualify a fresh OS configuration revision"
                .into(),
        ));
    }
    Ok(())
}
pub(crate) fn permit_read(
    tx: &rusqlite::Transaction<'_>,
    kind: &str,
    id: &str,
    who: &str,
    now: &str,
) -> Result<()> {
    if fenced(tx, kind, id)? {
        let r = latest(tx)?.ok_or(Error::NotFound)?;
        let until = (chrono::DateTime::parse_from_rfc3339(now).map_err(io)?
            + chrono::Duration::minutes(5))
        .to_rfc3339_opts(SecondsFormat::Millis, true);
        tx.execute(
            "INSERT INTO restore_read_permits VALUES (?1,?2,?3,?4,?5,?6)",
            params![store::id("read-permit"), r.id, kind, id, who, until],
        )?;
    }
    Ok(())
}
pub(crate) fn materialization(
    db: &Connection,
    kind: &str,
    id: impl ToString,
) -> Result<Option<PathBuf>> {
    if !has_table(db, "restore_artifacts")? {
        return Ok(None);
    }
    Ok(db.query_row("SELECT path FROM restore_artifacts WHERE kind=?1 AND identity=?2 ORDER BY rowid DESC LIMIT 1",params![kind,id.to_string()],|r|r.get::<_,String>(0)).optional()?.map(PathBuf::from))
}
/// A reported cleanup of an old handle never touches an original worker root or process.
pub(crate) fn cleanup_evidence(
    db: &rusqlite::Transaction<'_>,
    kind: &str,
    id: &str,
    case: &str,
    refs: &[String],
    actor: &str,
) -> Result<bool> {
    if !fenced(db, kind, id)? {
        return Ok(false);
    }
    let raw:String=db.query_row("SELECT record FROM restore_fences WHERE kind=?1 AND identity=?2 ORDER BY rowid DESC LIMIT 1",params![kind,id],|r|r.get(0))?;
    let v: Value = serde_json::from_str(&raw)?;
    crate::recovery::independent_evidence(
        db,
        case,
        refs,
        actor,
        v["at"]
            .as_str()
            .ok_or_else(|| Error::Conflict("restore boundary missing".into()))?,
        &[],
        v["case_version"]
            .as_i64()
            .ok_or_else(|| Error::Conflict("restore case boundary missing".into()))?,
    )?;
    Ok(true)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub backup_id: String,
    pub manifest_digest: String,
    pub restored_at: String,
    pub operator: String,
    pub reason: String,
    pub pause_id: String,
    pub artifact_roots: usize,
    pub uncertain_native_resources: usize,
    pub uncertain_external_effects: usize,
    pub held_notification_routes: usize,
    pub authority: String,
}
pub(crate) fn latest(db: &Connection) -> Result<Option<Record>> {
    if !has_table(db, "restore_runs")? {
        return Ok(None);
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM restore_runs ORDER BY rowid DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
fn isolate(
    store: &Store,
    m: &Manifest,
    manifest_digest: &str,
    output: &Path,
    operator: &str,
    reason: &str,
) -> Result<Record> {
    let mut db = store.connect()?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut record=Record{id:store::id("restore"),backup_id:m.id.clone(),manifest_digest:manifest_digest.into(),restored_at:now.clone(),operator:operator.into(),reason:reason.into(),pause_id:store::id("pause"),artifact_roots:m.artifacts.len(),uncertain_native_resources:0,uncertain_external_effects:0,held_notification_routes:0,authority:"All copied external configuration revisions are permanently fenced. Install fresh OS revisions; qualify native workers anew. Old handles require new operator inspection, never automatic cleanup. Reconcile uncertain external effects without replay. Old notification routes remain held; newly authorized deliveries are globally paced at least 30 seconds apart. Resume alone grants no inherited authority.".into()};
    // Insert the immutable audit before its FK-linked fences. The final counts are
    // calculated first; no copied job is allowed to become newly dispatchable.
    record.uncertain_native_resources=tx.query_row("SELECT (SELECT COUNT(*) FROM model_jobs WHERE state NOT IN ('qualified','completed','failed','simulated','cancelled','reported_cleaned','blocked'))+(SELECT COUNT(*) FROM container_jobs WHERE state NOT IN ('passed','failed','simulated','cancelled','reported_cleaned','blocked'))",[],|r|r.get(0))?;
    record.uncertain_external_effects=tx.query_row("SELECT (SELECT COUNT(*) FROM publication_jobs WHERE state NOT IN ('published','published_stale','simulated','cancelled'))+(SELECT COUNT(*) FROM merge_jobs WHERE state NOT IN ('merged','merged_stale','simulated','cancelled'))+(SELECT COUNT(*) FROM operation_jobs WHERE state NOT IN ('succeeded','succeeded_stale','failed','stopped','simulated','denied','blocked','cancelled'))",[],|r|r.get(0))?;
    record.held_notification_routes =
        tx.query_row("SELECT COUNT(*) FROM escalation_routes", [], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO restore_runs VALUES (?1,?2)",
        params![record.id, serde_json::to_string(&record)?],
    )?;
    for (kind, table) in [
        ("source", "source_configs"),
        ("datadog", "datadog_configs"),
        ("notification", "notification_configs"),
        ("model", "model_policies"),
        ("container", "container_policies"),
        ("publisher", "publisher_policies"),
        ("merge", "merge_policies"),
        ("operation", "operation_policies"),
        ("repair_source", "repair_sources"),
        ("worker", "worker_policies"),
        ("api_access", "api_access_policies"),
    ] {
        let mut query = tx.prepare(&format!("SELECT revision FROM {table} ORDER BY revision"))?;
        let ids = query.query_map([], |r| r.get::<_, i64>(0))?;
        for id in ids {
            let id = id?;
            tx.execute(
                "INSERT INTO restore_fences VALUES (?1,?2,?3,?4)",
                params![
                    record.id,
                    kind,
                    id.to_string(),
                    json!({"at":now,"reason":"copied configuration requires fresh OS authority"})
                        .to_string()
                ],
            )?;
        }
    }
    for (kind, table, terminals) in [
        (
            "model_resource",
            "model_jobs",
            "'qualified','completed','failed','simulated','cancelled','reported_cleaned','blocked'",
        ),
        (
            "container_resource",
            "container_jobs",
            "'passed','failed','simulated','cancelled','reported_cleaned','blocked'",
        ),
    ] {
        let mut query=tx.prepare(&format!("SELECT j.id,j.record,c.version FROM {table} j JOIN cases c ON c.id=j.case_id ORDER BY j.rowid"))?;
        let rows = query.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (id, raw, version) = row?;
            tx.execute("INSERT INTO restore_fences VALUES (?1,?2,?3,?4)",params![record.id,kind,id,json!({"at":now,"case_version":version,"reason":"old native handle is forensic only; no implicit adoption or cleanup"}).to_string()])?;
            let mut v: Value = serde_json::from_str(&raw)?;
            if terminals
                .split(',')
                .any(|t| Some(t.trim_matches('\'')) == v["state"].as_str())
            {
                continue;
            }
            v["state"] = json!("unknown");
            v["cancel_requested"] = json!(true);
            v["quarantined"] = json!(true);
            v["reason"] = json!(
                "Restored old resource: original deployment may still own it; new operator inspection required; no automatic process or journal access"
            );
            tx.execute(
                &format!("UPDATE {table} SET state='unknown',record=?1 WHERE id=?2"),
                params![v.to_string(), id],
            )?;
            let case = v["case_id"]
                .as_str()
                .ok_or_else(|| Error::Conflict("restored job case identity missing".into()))?;
            crate::team::escalate(
                &tx,
                case,
                "restore-review",
                &format!("{}:{table}:{id}", record.id),
                "Restored uncertainty requires operator review of the original deployment, scoped reconciliation and fresh authority; no effect or cleanup replay is authorized",
                &now,
            )?;
        }
    }
    for (table, terminals) in [
        (
            "publication_jobs",
            "'published','published_stale','simulated','cancelled'",
        ),
        (
            "merge_jobs",
            "'merged','merged_stale','simulated','cancelled'",
        ),
        (
            "operation_jobs",
            "'succeeded','succeeded_stale','failed','stopped','simulated','denied','blocked','cancelled'",
        ),
    ] {
        let mut query = tx.prepare(&format!(
            "SELECT id,record FROM {table} WHERE state NOT IN ({terminals}) ORDER BY rowid"
        ))?;
        let rows = query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, raw) = row?;
            if table == "publication_jobs" {
                tx.execute("INSERT INTO restore_fences VALUES (?1,'publication_job',?2,?3)",params![record.id,id,json!({"at":now,"reason":"old publisher credentials require an explicit bounded read permit"}).to_string()])?;
            }
            let mut v: Value = serde_json::from_str(&raw)?;
            v["state"] = json!("unknown");
            v["fence"] = json!(store::id("restore-fence"));
            v["reason"] = json!(
                "Backup may precede an effect by the original coordinator; explicit read-only reconciliation required, never blind replay"
            );
            tx.execute(
                &format!("UPDATE {table} SET state='unknown',record=?1 WHERE id=?2"),
                params![v.to_string(), id],
            )?;
            let case = v["case_id"]
                .as_str()
                .ok_or_else(|| Error::Conflict("restored job case identity missing".into()))?;
            crate::team::escalate(
                &tx,
                case,
                "restore-review",
                &format!("{}:{table}:{id}", record.id),
                "Restored uncertainty requires operator review of the original deployment, scoped reconciliation and fresh authority; no effect or cleanup replay is authorized",
                &now,
            )?;
        }
    }
    tx.execute("INSERT OR IGNORE INTO factory_resource_scopes SELECT r.kind,r.id,s.repository,s.service,?1 FROM factory_native_resources r JOIN factory_case_scopes s ON s.case_id=r.case_id",[&now])?;
    tx.execute("INSERT INTO factory_evidence_usage SELECT 1,COALESCE(SUM(CASE WHEN provenance!='supervisor-checkpoint-v1' THEN length(CAST(content AS BLOB)) ELSE 0 END),0),COALESCE(SUM(CASE WHEN provenance='supervisor-checkpoint-v1' THEN length(CAST(content AS BLOB)) ELSE 0 END),0) FROM evidence WHERE 1 ON CONFLICT(id) DO UPDATE SET ordinary_bytes=excluded.ordinary_bytes,supervisor_bytes=excluded.supervisor_bytes",[])?;
    tx.execute("DELETE FROM model_admissions", [])?;
    tx.execute("DELETE FROM container_admissions", [])?;
    tx.execute(
        "UPDATE jobs SET lease_until=?1 WHERE state='running'",
        [&now],
    )?;
    tx.execute("UPDATE run_control SET progress_deadline=?1 WHERE run_id IN (SELECT id FROM runs WHERE state='running')",[&now])?;
    tx.execute("UPDATE notification_deliveries SET state='uncertain',fence=NULL,lease_until=NULL,detail='Restored sending process; external delivery may have occurred. Old route authority is held.' WHERE state='inflight'",[])?;
    {
        let mut query=tx.prepare("SELECT e.id,c.version FROM escalations e JOIN cases c ON c.id=e.case_id ORDER BY e.rowid")?;
        let old_escalations =
            query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in old_escalations {
            let (id, version) = row?;
            tx.execute("INSERT INTO restore_fences VALUES (?1,'escalation',?2,?3)",params![record.id,id,json!({"at":now,"case_version":version,"reason":"old notification requires explicit operator review"}).to_string()])?;
        }
    }
    tx.execute("UPDATE notification_deliveries SET state='restore_held',detail='Old notification authority held after restore; reconcile or explicitly authorize a fresh reminder' WHERE state IN ('pending','retry')",[])?;
    tx.execute("UPDATE notification_pacing SET interval_seconds=MAX(interval_seconds,30),next_at=?1 WHERE id=1",[&now])?;
    tx.execute("UPDATE source_read_state SET error='Restore holds old source authority; install a fresh OS configuration'",[])?;
    tx.execute("UPDATE datadog_reads SET error='Restore holds old source authority; install a fresh OS configuration'",[])?;
    tx.execute("INSERT INTO factory_pauses VALUES (?1,'global',NULL,'all',?2,?3,0,'active',?4,NULL)",params![record.pause_id,operator,"Restored coordinator: reconcile uncertainty and install fresh authority before explicit resume",now])?;
    tx.execute(
        "UPDATE factory_state SET version=version+1,last_tick=NULL",
        [],
    )?;
    for a in &m.artifacts {
        let path = output.join(&a.directory);
        fs::create_dir_all(&path).map_err(io)?;
        tx.execute(
            "INSERT INTO restore_artifacts VALUES (?1,?2,?3,?4,?5)",
            params![
                record.id,
                a.kind,
                a.identity,
                path.to_str()
                    .ok_or_else(|| Error::Invalid("restore path is not Unicode".into()))?,
                manifest_digest
            ],
        )?;
        #[cfg(unix)]
        for f in &a.files {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                path.join(&f.path),
                fs::Permissions::from_mode(if f.executable { 0o500 } else { 0o400 }),
            )
            .map_err(io)?;
        }
    }
    tx.execute(
        "INSERT INTO factory_events(record) VALUES (?1)",
        [json!({"type":"restore_completed","restore":record}).to_string()],
    )?;
    tx.commit()?;
    Ok(record)
}
/// Restore into a new private directory. The expected hash must come from trusted
/// retained metadata. No credential lookup, executor, provider or original path is used.
pub fn restore(
    backup: &Path,
    output: &Path,
    expected_digest: &str,
    operator: &str,
    reason: &str,
) -> Result<Record> {
    text(operator, "operator", 128)?;
    text(reason, "reason", 2000)?;
    if operator.starts_with("fake-") || operator.starts_with("model-") {
        return Err(Error::Invalid("OS operator required".into()));
    }
    let verified = backups::inspect(backup, Some(expected_digest))?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let proposed = fs::canonicalize(parent).map_err(io)?.join(
        output
            .file_name()
            .ok_or_else(|| Error::Invalid("restore requires a new directory name".into()))?,
    );
    if proposed.starts_with(fs::canonicalize(backup).map_err(io)?) {
        return Err(Error::Invalid(
            "restore destination must be outside its backup; preserve the input inventory".into(),
        ));
    }
    backups::private_directory(output)?;
    let output = fs::canonicalize(output).map_err(io)?;
    let mut incomplete = backups::create_file(&output.join("restore-incomplete"))?;
    incomplete
        .write_all(verified.manifest_digest.as_bytes())
        .map_err(io)?;
    incomplete.sync_all().map_err(io)?;
    drop(incomplete);
    let staging = output.join("staging");
    backups::private_directory(&staging)?;
    let raw = backups::manifest_text(backup)?;
    if store::digest(&raw) != expected_digest {
        return Err(Error::Conflict("backup changed during restore".into()));
    }
    let m: Manifest = serde_json::from_str(&raw)?;
    for (name, bytes) in m
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.bytes))
        .chain(std::iter::once(("manifest.json", raw.len() as u64)))
    {
        let source = backups::contained(backup, name)?;
        let target = staging.join(name);
        fs::create_dir_all(target.parent().unwrap()).map_err(io)?;
        let from = fs::File::open(source).map_err(io)?;
        let mut to = backups::create_file(&target)?;
        let copied =
            std::io::copy(&mut std::io::Read::take(from, bytes + 1), &mut to).map_err(io)?;
        if copied != bytes {
            return Err(Error::Conflict("backup changed during restore".into()));
        }
        to.sync_all().map_err(io)?;
    }
    backups::inspect(&staging, Some(expected_digest))?;
    let m: Manifest = serde_json::from_str(&backups::manifest_text(&staging)?)?;
    fs::rename(staging.join("watchhouse.db"), output.join("watchhouse.db")).map_err(io)?;
    if staging.join("artifacts").exists() {
        fs::rename(staging.join("artifacts"), output.join("artifacts")).map_err(io)?;
    }
    fs::rename(
        staging.join("manifest.json"),
        output.join("source-manifest.json"),
    )
    .map_err(io)?;
    fs::remove_dir(staging).map_err(io)?;
    let store = Store::open_for_restore(output.join("watchhouse.db"))?;
    let record = isolate(&store, &m, expected_digest, &output, operator, reason)?;
    let mut completed = backups::create_file(&output.join("restore-complete.json"))?;
    completed
        .write_all(serde_json::to_string_pretty(&record)?.as_bytes())
        .map_err(io)?;
    completed.sync_all().map_err(io)?;
    fs::remove_file(output.join("restore-incomplete")).map_err(io)?;
    Ok(record)
}

impl Application {
    pub fn restore_status(&self) -> Result<Option<Record>> {
        latest(&self.store.connect()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn known_v1_backup_migrates_from_schema31_before_activating_any_coordinator() {
        let d = tempfile::tempdir().unwrap();
        let backup = d.path().join("backup");
        backups::private_directory(&backup).unwrap();
        let path = backup.join("watchhouse.db");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,digest TEXT NOT NULL);",
        )
        .unwrap();
        for (version, sql) in store::MIGRATIONS.iter().take(31) {
            db.execute_batch(sql).unwrap();
            db.execute(
                "INSERT INTO schema_migrations VALUES (?1,?2)",
                params![version, store::migration_digest(sql)],
            )
            .unwrap();
        }
        let migrations = db
            .prepare("SELECT version,digest FROM schema_migrations ORDER BY version")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<Vec<(i64, String)>, _>>()
            .unwrap();
        drop(db);
        let bytes = fs::read(&path).unwrap();
        let m = Manifest {
            version: 1,
            id: "backup_schema31_fixture".into(),
            created_at: "2026-11-21T00:00:00.000Z".into(),
            operator: "operator".into(),
            reason: "Legacy consistent snapshot fixture".into(),
            migrations,
            entries: vec![backups::Entry {
                path: "watchhouse.db".into(),
                bytes: bytes.len() as u64,
                digest: format!("sha256:{:x}", Sha256::digest(bytes)),
            }],
            artifacts: vec![],
            excluded: vec![],
        };
        let raw = serde_json::to_string(&m).unwrap();
        fs::write(backup.join("manifest.json"), &raw).unwrap();
        let output = d.path().join("restored");
        restore(
            &backup,
            &output,
            &store::digest(&raw),
            "operator",
            "Known-prefix migration recovery",
        )
        .unwrap();
        let db = Store::open(output.join("watchhouse.db"))
            .unwrap()
            .connect()
            .unwrap();
        assert_eq!(
            db.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            34
        );
        assert!(latest(&db).unwrap().is_some());
        assert_eq!(
            db.query_row(
                "SELECT interval_seconds FROM notification_pacing",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            30
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reminder {
    pub command: crate::contracts::OperatorCommand,
    pub escalation_id: String,
    pub evidence_refs: Vec<String>,
}
impl Application {
    /// A new reminder is explicit authorization, never an automatic resend of an
    /// uncertain old delivery. Its source link and exact receipt prevent duplicates.
    pub fn reissue_restored_escalation(&self, case: &str, cmd: Reminder) -> Result<String> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.reason, "reason", 2000)?;
        text(&m.idempotency_key, "key", 128)?;
        let fp = store::digest(&format!(
            "restore-reminder:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = crate::application::receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let current = store::case(&tx, case)?;
        if current.owner != m.actor || current.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "reload owned case before authorizing a restored reminder".into(),
            ));
        }
        let belongs:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM escalations e WHERE e.id=?1 AND e.case_id=?2 AND e.id NOT IN (SELECT escalation_id FROM escalation_resolutions))",params![cmd.escalation_id,case],|r|r.get(0))?;
        if !belongs
            || !cleanup_evidence(
                &tx,
                "escalation",
                &cmd.escalation_id,
                case,
                &cmd.evidence_refs,
                &m.actor,
            )?
        {
            return Err(Error::Conflict(
                "only a restored unresolved escalation accepts a fresh reminder".into(),
            ));
        }
        let revision: Option<i64> =
            tx.query_row("SELECT MAX(revision) FROM notification_configs", [], |r| {
                r.get(0)
            })?;
        require_fresh(
            &tx,
            "notification",
            revision.ok_or_else(|| {
                Error::Conflict(
                    "install a fresh notification configuration before authorizing a reminder"
                        .into(),
                )
            })?,
        )?;
        let restoration = latest(&tx)?.ok_or(Error::NotFound)?;
        let key = format!("{}:{}", restoration.id, cmd.escalation_id);
        crate::team::escalate(
            &tx,
            case,
            "restore-reminder",
            &key,
            &format!(
                "Explicit restored escalation reminder for {}: {}. Prior delivery may have occurred; old record and obligations are retained.",
                cmd.escalation_id, m.reason
            ),
            &self.now(),
        )?;
        let id: String = tx.query_row(
            "SELECT id FROM escalations WHERE source_type='restore-reminder' AND source_id=?1",
            [key],
            |r| r.get(0),
        )?;
        tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[json!({"type":"restore_reminder_authorized","old_escalation_id":cmd.escalation_id,"reminder_id":id,"operator":m.actor,"reason":m.reason,"evidence_refs":cmd.evidence_refs,"at":self.now()}).to_string()])?;
        crate::application::save_receipt(&tx, &m.idempotency_key, &fp, &id)?;
        tx.commit()?;
        Ok(id)
    }
}
