use crate::error::{Error, Result};
use crate::{
    contracts::*,
    domain::*,
    execution::{ExecutionAdapter, Task},
    store::{self, Store},
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;

#[derive(Clone)]
pub struct Application {
    store: Store,
    clock: Arc<dyn Clock>,
}
#[derive(Debug, Clone)]
pub struct Claim {
    pub job_id: String,
    pub case_id: String,
    pub run_id: String,
    pub fence: String,
    pub task: Task,
}
fn operator(id: &str) -> Actor {
    Actor {
        kind: "operator".into(),
        id: id.into(),
    }
}
fn worker() -> Actor {
    Actor {
        kind: "worker".into(),
        id: "fake-investigator".into(),
    }
}
fn text(value: &str, name: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(Error::Invalid(format!(
            "{name} must contain 1..{max} bytes of text"
        )));
    }
    Ok(())
}
fn receipt<T: DeserializeOwned>(
    tx: &rusqlite::Transaction<'_>,
    key: &str,
    fingerprint: &str,
) -> Result<Option<T>> {
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT fingerprint,response FROM commands WHERE idempotency_key=?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        Some((old, response)) if old == fingerprint => Ok(Some(serde_json::from_str(&response)?)),
        Some(_) => Err(Error::Conflict(
            "idempotency key already used for a different command".into(),
        )),
        None => Ok(None),
    }
}
fn save_receipt<T: Serialize>(
    tx: &rusqlite::Transaction<'_>,
    key: &str,
    fingerprint: &str,
    response: &T,
) -> Result<()> {
    tx.execute(
        "INSERT INTO commands VALUES (?1,?2,?3)",
        params![key, fingerprint, serde_json::to_string(response)?],
    )?;
    Ok(())
}
impl Application {
    pub fn new(store: Store, clock: Arc<dyn Clock>) -> Self {
        Self { store, clock }
    }
    fn now(&self) -> String {
        self.clock
            .now()
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }
    pub fn create_case(&self, cmd: CreateCase) -> Result<Case> {
        text(&cmd.idempotency_key, "idempotency_key", 128)?;
        text(&cmd.actor, "actor", 128)?;
        text(&cmd.reason, "reason", 2000)?;
        text(&cmd.title, "title", 200)?;
        if cmd.description.len() > 16000 {
            return Err(Error::Invalid("description exceeds 16000 bytes".into()));
        }
        let fingerprint = store::digest(&format!("create:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.idempotency_key, &fingerprint)? {
            return Ok(response);
        }
        let case_id = store::id("case");
        let assignment_id = store::id("asn");
        let now = self.now();
        tx.execute(
            "INSERT INTO cases VALUES (?1,?2,?3,?4,'queued',0,?5)",
            params![case_id, cmd.title, cmd.description, cmd.actor, now],
        )?;
        store::emit(
            &tx,
            &case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::CaseCreated {
                title: cmd.title.clone(),
                reason: cmd.reason,
                owner: cmd.actor.clone(),
            },
        )?;
        tx.execute(
            "INSERT INTO assignments VALUES (?1,?2,'investigator',?3,?4,'queued',1)",
            params![assignment_id, case_id, cmd.actor, cmd.title],
        )?;
        tx.execute(
            "INSERT INTO jobs(id,assignment_id,state) VALUES (?1,?2,'queued')",
            params![store::id("job"), assignment_id],
        )?;
        store::emit(
            &tx,
            &case_id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &now,
            EventData::AssignmentQueued {
                assignment_id,
                role: "investigator".into(),
                reason: "Manual intake requests a bounded fake investigation".into(),
            },
        )?;
        let response = store::case(&tx, &case_id)?;
        save_receipt(&tx, &cmd.idempotency_key, &fingerprint, &response)?;
        tx.commit()?;
        Ok(response)
    }
    pub fn rename_case(&self, id: &str, cmd: RenameCase) -> Result<Case> {
        text(&cmd.idempotency_key, "idempotency_key", 128)?;
        text(&cmd.actor, "actor", 128)?;
        text(&cmd.reason, "reason", 2000)?;
        text(&cmd.title, "title", 200)?;
        let fingerprint = store::digest(&format!("rename:{id}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.idempotency_key, &fingerprint)? {
            return Ok(response);
        }
        let current = store::case(&tx, id)?;
        if current.version != cmd.expected_version {
            return Err(Error::Conflict(format!(
                "expected version {}, current version {}; reload before retrying",
                cmd.expected_version, current.version
            )));
        }
        tx.execute(
            "UPDATE cases SET title=?1 WHERE id=?2",
            params![cmd.title, id],
        )?;
        store::emit(
            &tx,
            id,
            operator(&cmd.actor),
            &cmd.idempotency_key,
            &self.now(),
            EventData::CaseRenamed {
                title: cmd.title,
                reason: cmd.reason,
            },
        )?;
        let response = store::case(&tx, id)?;
        save_receipt(&tx, &cmd.idempotency_key, &fingerprint, &response)?;
        tx.commit()?;
        Ok(response)
    }
    pub fn list_cases(&self) -> Result<Vec<Case>> {
        let db = self.store.connect()?;
        let mut stmt =
            db.prepare("SELECT id FROM cases ORDER BY created_at DESC,id DESC LIMIT 200")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| store::case(&db, id)).collect()
    }
    pub fn snapshot(&self, id: &str) -> Result<Snapshot> {
        let mut db = self.store.connect()?;
        let tx = db.transaction()?;
        let case = store::case(&tx, id)?;
        let assignment = tx.query_row(
            "SELECT id,case_id,role,owner,objective,state FROM assignments WHERE case_id=?1",
            [id],
            |r| {
                Ok(Assignment {
                    schema_version: V1,
                    id: r.get(0)?,
                    case_id: r.get(1)?,
                    role: r.get(2)?,
                    owner: r.get(3)?,
                    objective: r.get(4)?,
                    state: r.get(5)?,
                })
            },
        )?;
        let events = store::events(&tx, id, 0, i64::MAX)?;
        let cursor = events.last().map_or(0, |e| e.cursor);
        let mut stmt = tx.prepare(
            "SELECT result FROM runs WHERE assignment_id=?1 AND state='completed' ORDER BY attempt",
        )?;
        let results = stmt
            .query_map([&assignment.id], |r| r.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str(&row?)?))
            .collect::<Result<Vec<_>>>()?;
        Ok(Snapshot {
            schema_version: V1,
            case,
            assignment,
            events,
            cursor,
            results,
        })
    }
    pub fn timeline(&self, id: &str, after: i64) -> Result<Vec<Event>> {
        if after < 0 {
            return Err(Error::Invalid("cursor must be nonnegative".into()));
        }
        let db = self.store.connect()?;
        store::case(&db, id)?;
        store::events(&db, id, after, 200)
    }
    pub fn evidence(&self, digest: &str) -> Result<String> {
        self.store
            .connect()?
            .query_row(
                "SELECT content FROM evidence WHERE digest=?1",
                [digest],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::NotFound)
    }
    /// Claim and run.started commit before adapter invocation. Expired claims are fenced.
    pub fn claim(&self) -> Result<Option<Claim>> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = self.now();
        let job: Option<(String,String,String,String,i64,String)> = tx.query_row(
            "SELECT j.id,a.id,a.case_id,a.objective,j.attempt,j.state FROM jobs j JOIN assignments a ON a.id=j.assignment_id WHERE j.state='queued' OR (j.state='running' AND j.lease_until<=?1) ORDER BY j.rowid LIMIT 1",[&now],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let Some((job_id, assignment_id, case_id, objective, attempt, state)) = job else {
            return Ok(None);
        };
        if state == "running" {
            let old_run: String = tx.query_row(
                "SELECT id FROM runs WHERE assignment_id=?1 AND attempt=?2",
                params![assignment_id, attempt],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE runs SET state='expired',finished_at=?1 WHERE id=?2",
                params![now, old_run],
            )?;
            store::emit(
                &tx,
                &case_id,
                worker(),
                &job_id,
                &now,
                EventData::RunLeaseExpired {
                    run_id: old_run,
                    reason: "Lease expired; side-effect-free fake execution may be retried".into(),
                },
            )?;
        }
        let fence = store::id("fence");
        let run_id = store::id("run");
        let until =
            (self.clock.now() + Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Millis, true);
        tx.execute(
            "UPDATE jobs SET state='running',attempt=attempt+1,lease_until=?1,fence=?2 WHERE id=?3",
            params![until, fence, job_id],
        )?;
        tx.execute(
            "UPDATE assignments SET state='running' WHERE id=?1",
            [&assignment_id],
        )?;
        tx.execute("UPDATE cases SET status='running' WHERE id=?1", [&case_id])?;
        tx.execute("INSERT INTO runs(id,assignment_id,attempt,adapter,started_at,state) VALUES (?1,?2,?3,'fake-v1',?4,'running')",params![run_id,assignment_id,attempt+1,now])?;
        store::emit(
            &tx,
            &case_id,
            worker(),
            &job_id,
            &now,
            EventData::RunStarted {
                assignment_id: assignment_id.clone(),
                run_id: run_id.clone(),
                attempt: attempt + 1,
                reason: "Claimed queued investigation under fake-only policy".into(),
            },
        )?;
        tx.commit()?;
        Ok(Some(Claim {
            job_id,
            case_id,
            run_id,
            fence,
            task: Task {
                schema_version: V1,
                assignment_id,
                objective,
            },
        }))
    }
    pub fn complete(&self, claim: &Claim, result: ExecutionResult) -> Result<()> {
        if result.assignment_id != claim.task.assignment_id
            || result.conclusion != "insufficient_evidence"
            || result.actual_model.is_some()
            || result.usage.is_some()
        {
            return Err(Error::Invalid("result violates fake-v1 contract".into()));
        }
        text(&result.summary, "summary", 16000)?;
        let content = serde_json::to_string(&result)?;
        let digest = store::digest(&content);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let accepted: Option<String> = tx
            .query_row(
                "SELECT result FROM runs WHERE id=?1 AND state='completed'",
                [&claim.run_id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = accepted {
            return if old == content {
                Ok(())
            } else {
                Err(Error::Conflict(
                    "completed run has a different result".into(),
                ))
            };
        }
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs j JOIN assignments a ON a.id=j.assignment_id JOIN runs r ON r.assignment_id=a.id AND r.attempt=j.attempt WHERE j.id=?1 AND j.state='running' AND j.fence=?2 AND j.lease_until>?3 AND r.id=?4 AND r.state='running' AND a.id=?5 AND a.case_id=?6)",params![claim.job_id,claim.fence,self.now(),claim.run_id,claim.task.assignment_id,claim.case_id],|r|r.get(0))?;
        if !valid {
            return Err(Error::Conflict(
                "expired or superseded execution fence".into(),
            ));
        }
        let now = self.now();
        tx.execute("INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,'fake-v1:synthetic-result',?3)",params![digest,content,now])?;
        tx.execute("UPDATE runs SET state='completed',finished_at=?1,result=?2,evidence_digest=?3 WHERE id=?4",params![now,content,digest,claim.run_id])?;
        tx.execute(
            "UPDATE assignments SET state='completed' WHERE id=?1",
            [&claim.task.assignment_id],
        )?;
        tx.execute(
            "UPDATE jobs SET state='completed',lease_until=NULL WHERE id=?1",
            [&claim.job_id],
        )?;
        tx.execute(
            "UPDATE cases SET status='awaiting_review' WHERE id=?1",
            [&claim.case_id],
        )?;
        store::emit(&tx,&claim.case_id,worker(),&claim.run_id,&now,EventData::AssignmentCompleted { assignment_id:claim.task.assignment_id.clone(),run_id:claim.run_id.clone(),evidence_refs:vec![digest],reason:"Synthetic result persisted; operator owns review, no recovery or disposition asserted".into() })?;
        tx.commit()?;
        Ok(())
    }
    pub fn run_once(&self, adapter: &dyn ExecutionAdapter) -> Result<bool> {
        let Some(claim) = self.claim()? else {
            return Ok(false);
        };
        let result = adapter.execute(&claim.task);
        self.complete(&claim, result)?;
        Ok(true)
    }
}
