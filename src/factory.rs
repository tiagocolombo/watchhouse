//! Deterministic factory admission/control; observation and owned duties remain active.
use crate::{
    application::{Application, receipt, save_receipt, text},
    contracts::{CommandMeta, V1},
    error::{Error, Result},
    store,
};
use chrono::DateTime;
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Global,
    Service,
    Repository,
    Incident,
    Case,
}
impl Scope {
    fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Service => "service",
            Self::Repository => "repository",
            Self::Incident => "incident",
            Self::Case => "case",
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    All,
    Fake,
    Container,
    Model,
    Publication,
    Merge,
    Operation,
}
impl Effect {
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Fake => "fake",
            Self::Container => "container",
            Self::Model => "model",
            Self::Publication => "publication",
            Self::Merge => "merge",
            Self::Operation => "operation",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Pause {
        scope: Scope,
        target: Option<String>,
        effect: Effect,
        cancel_running: bool,
    },
    Resume {
        pause_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub meta: CommandMeta,
    pub expected_version: i64,
    pub action: Action,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: V1,
    pub version: i64,
    pub pause_id: String,
    pub affected_jobs: usize,
    pub reason: String,
}
pub(crate) fn heartbeat(tx: &Transaction<'_>, now: &str) -> Result<()> {
    tx.execute("UPDATE factory_state SET last_tick=?1,high_at=CASE WHEN high_at IS NULL OR high_at<?1 THEN ?1 ELSE high_at END WHERE id=1",[now])?;
    Ok(())
}
pub(crate) fn admitted(db: &Connection, case: &str, effect: &str, now: &str) -> Result<bool> {
    Ok(db.query_row("SELECT NOT EXISTS(SELECT 1 FROM factory_holds WHERE case_id=?1 AND effect IN ('all',?2)) AND NOT EXISTS(SELECT 1 FROM factory_state WHERE high_at>?3)",params![case,effect,now],|r|r.get(0))?)
}
pub(crate) fn gate(db: &Connection, case: &str, effect: &str, now: &str) -> Result<()> {
    if admitted(db, case, effect, now)? {
        Ok(())
    } else {
        Err(Error::Conflict("factory admission paused or clock is behind its retained high watermark; inspect factory status".into()))
    }
}
#[derive(Serialize)]
pub struct Status {
    pub restoration: Option<crate::restore::Record>,
    pub remote_access: crate::access::Health,
    pub version: i64,
    pub capacity: crate::capacity::Status,
    pub state: String,
    pub now: String,
    pub last_tick: Option<String>,
    pub clock_high_at: Option<String>,
    pub scheduler_lag_seconds: Option<i64>,
    pub clock_behind: bool,
    pub pauses: Vec<Value>,
    pub queues: Vec<Value>,
    pub uncertain_resources: i64,
    pub open_escalations: i64,
    pub source_reads_unavailable: i64,
    pub audit: Vec<Value>,
    pub active_while_paused: Vec<String>,
    pub limitation: String,
}
impl Application {
    pub fn control_factory(&self, cmd: Command) -> Result<Receipt> {
        text(&cmd.meta.idempotency_key, "idempotency_key", 128)?;
        text(&cmd.meta.actor, "actor", 128)?;
        text(&cmd.meta.reason, "reason", 2000)?;
        if cmd.meta.actor.starts_with("fake-") || cmd.meta.actor.starts_with("model-") {
            return Err(Error::Invalid("operator control required".into()));
        }
        let fp = store::digest(&format!("factory-control:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        let version: i64 =
            tx.query_row("SELECT version FROM factory_state WHERE id=1", [], |r| {
                r.get(0)
            })?;
        if version != cmd.expected_version {
            return Err(Error::Conflict(
                "factory version changed; reload controls".into(),
            ));
        }
        let now = self.now();
        let mut affected = 0;
        let pause_id = match &cmd.action {
            Action::Pause {
                scope,
                target,
                effect,
                cancel_running,
            } => {
                match scope {
                    Scope::Global if target.is_some() => {
                        return Err(Error::Invalid("global pause has no target".into()));
                    }
                    Scope::Global => {}
                    _ => text(target.as_deref().unwrap_or(""), "scope target", 256)?,
                }
                if matches!(scope, Scope::Case) {
                    store::case(&tx, target.as_deref().unwrap())?;
                }
                if matches!(scope, Scope::Incident) {
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM incidents WHERE id=?1)",
                        [target],
                        |r| r.get(0),
                    )?;
                    if !exists {
                        return Err(Error::NotFound);
                    }
                }
                let id = store::id("pause");
                tx.execute(
                    "INSERT INTO factory_pauses VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8,NULL)",
                    params![
                        id,
                        scope.name(),
                        target,
                        effect.name(),
                        cmd.meta.actor,
                        cmd.meta.reason,
                        cancel_running,
                        now
                    ],
                )?;
                if *cancel_running {
                    let cases: Vec<String> = {
                        let mut q = tx.prepare(
                            "SELECT DISTINCT case_id FROM factory_holds WHERE pause_id=?1",
                        )?;
                        q.query_map([&id], |r| r.get(0))?
                            .collect::<std::result::Result<_, _>>()?
                    };
                    for case in cases {
                        if matches!(effect, Effect::All | Effect::Fake) {
                            affected+=tx.execute("UPDATE jobs SET lease_until=?1 WHERE state='running' AND assignment_id IN (SELECT id FROM assignments WHERE case_id=?2)",params![now,case])?;
                            tx.execute("UPDATE run_control SET progress_deadline=?1 WHERE run_id IN (SELECT r.id FROM runs r JOIN assignments a ON a.id=r.assignment_id WHERE a.case_id=?2 AND r.state='running')",params![now,case])?;
                        }
                        if matches!(effect, Effect::All | Effect::Model) {
                            affected +=
                                crate::model_runtime::ops::factory_stop(&tx, &case, &id, &now)?;
                        }
                        if matches!(effect, Effect::All | Effect::Container) {
                            affected +=
                                crate::real_execution::worker::factory_stop(&tx, &case, &id, &now)?;
                        }
                        if matches!(effect, Effect::All | Effect::Publication) {
                            affected +=
                                crate::delivery::lifecycle::factory_stop(&tx, &case, &id, &now)?;
                        }
                        if matches!(effect, Effect::All | Effect::Merge) {
                            affected += crate::merging::factory_stop(&tx, &case, &id, &now)?;
                        }
                        if matches!(effect, Effect::All | Effect::Operation) {
                            affected += crate::operations::factory_stop(&tx, &case, &id, &now)?;
                        }
                    }
                }
                id
            }
            Action::Resume { pause_id } => {
                text(pause_id, "pause_id", 128)?;
                if tx.execute("UPDATE factory_pauses SET state='resumed',resumed_at=?1 WHERE id=?2 AND state='active'",params![now,pause_id])?!=1{return Err(Error::Conflict("active pause not found; reload controls".into()));}
                pause_id.clone()
            }
        };
        let response = Receipt {
            schema_version: V1,
            version: version + 1,
            pause_id,
            affected_jobs: affected,
            reason: cmd.meta.reason.clone(),
        };
        tx.execute("UPDATE factory_state SET version=version+1 WHERE id=1", [])?;
        tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[serde_json::to_string(&json!({"recorded_at":now,"command":cmd,"result":response,"remaining_active":["timers","source reads","notification delivery","scoped reconciliation","cleanup"]}))?])?;
        save_receipt(&tx, &cmd.meta.idempotency_key, &fp, &response)?;
        tx.commit()?;
        Ok(response)
    }
    pub fn factory_status(&self) -> Result<Status> {
        let db = self.store.connect()?;
        let (version, last_tick, high): (i64, Option<String>, Option<String>) = db.query_row(
            "SELECT version,last_tick,high_at FROM factory_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let now = self.now();
        let lag = last_tick
            .as_ref()
            .map(|t| {
                DateTime::parse_from_rfc3339(t).map(|v| {
                    (self.clock.now() - v.with_timezone(&chrono::Utc))
                        .num_seconds()
                        .max(0)
                })
            })
            .transpose()
            .map_err(|_| Error::Conflict("invalid factory tick timestamp".into()))?;
        let clock_behind = high.as_ref().is_some_and(|t| t > &now);
        let pauses = {
            let mut q=db.prepare("SELECT id,scope,target,effect,owner,reason,cancel_running,created_at FROM factory_pauses WHERE state='active' ORDER BY rowid LIMIT 200")?;
            q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"scope":r.get::<_,String>(1)?,"target":r.get::<_,Option<String>>(2)?,"effect":r.get::<_,String>(3)?,"owner":r.get::<_,String>(4)?,"reason":r.get::<_,String>(5)?,"cancel_running":r.get::<_,bool>(6)?,"created_at":r.get::<_,String>(7)?})))?.collect::<std::result::Result<Vec<_>,_>>()?
        };
        let mut queues = vec![];
        for (kind, table) in [
            ("model", "model_jobs"),
            ("container", "container_jobs"),
            ("publication", "publication_jobs"),
            ("merge", "merge_jobs"),
            ("operation", "operation_jobs"),
        ] {
            let mut q=db.prepare(&format!("SELECT state,COUNT(*),MIN(json_extract(record,'$.deadline')) FROM {table} GROUP BY state"))?;
            queues.extend(q.query_map([],|r|Ok(json!({"kind":kind,"state":r.get::<_,String>(0)?,"count":r.get::<_,i64>(1)?,"earliest_deadline":r.get::<_,Option<String>>(2)?})))?.collect::<std::result::Result<Vec<_>,_>>()?);
        }
        let uncertain:i64=db.query_row("SELECT (SELECT COUNT(*) FROM model_jobs WHERE state='unknown' OR json_extract(record,'$.quarantined')=1)+(SELECT COUNT(*) FROM container_jobs WHERE state='unknown' OR json_extract(record,'$.quarantined')=1)+(SELECT COUNT(*) FROM publication_jobs WHERE state IN ('unknown','quarantined'))+(SELECT COUNT(*) FROM merge_jobs WHERE state='unknown')+(SELECT COUNT(*) FROM operation_jobs WHERE state='unknown')",[],|r|r.get(0))?;
        let escalations = db.query_row(
            "SELECT COUNT(*) FROM escalations WHERE state='open'",
            [],
            |r| r.get(0),
        )?;
        let unavailable = db.query_row(
            "SELECT COUNT(*) FROM source_reads WHERE error IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let audit = {
            let mut q =
                db.prepare("SELECT record FROM factory_events ORDER BY sequence DESC LIMIT 100")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .map(|r| Ok(serde_json::from_str(&r?)?))
                .collect::<Result<_>>()?
        };
        let capacity = self.capacity_status()?;
        let remote_access = self.access_health()?;
        let state = if clock_behind
            || lag.is_some_and(|s| s > 5)
            || uncertain > 0
            || unavailable > 0
            || remote_access.audit_stale_incomplete > 0
        {
            "degraded"
        } else if !pauses.is_empty() {
            "paused"
        } else if capacity.intake_at_capacity
            || capacity.evidence_at_capacity
            || (remote_access.configured_revision.is_some() && remote_access.audit_remaining == 0)
        {
            "limited"
        } else if last_tick.is_none() {
            "unknown"
        } else {
            "running"
        };
        Ok(Status{remote_access,restoration:self.restore_status()?,capacity,version,state:state.into(),now,last_tick,clock_high_at:high,scheduler_lag_seconds:lag,clock_behind,pauses,queues,uncertain_resources:uncertain,open_escalations:escalations,source_reads_unavailable:unavailable,audit,active_while_paused:vec!["source observation".into(),"timers and escalation".into(),"notification delivery".into(),"read-only reconciliation".into(),"original-worker cleanup (restored old handles remain held)".into()],limitation:"Factory health is separate from service recovery. Pause removes admission, not an already dispatched effect. Stop fences active results and requests scoped cleanup; unknown outcomes require reconciliation. External gateway stop/rollback and credential revocation require their own configured authority. Resume does not reset attempts, deadlines, uncertainty, credentials or native qualification. Status lists at most 200 active pauses and 100 audit entries; admission checks all pauses.".into()})
    }
}
