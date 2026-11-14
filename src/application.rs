use crate::error::{Error, Result};
use crate::{
    contracts::*,
    domain::*,
    execution::{ExecutionAdapter, Task},
    store::{self, Store},
};
use chrono::SecondsFormat;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;

#[derive(Clone)]
pub struct Application {
    pub(crate) store: Store,
    pub(crate) clock: Arc<dyn Clock>,
}
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub schema_version: V1,
    pub executor: String,
    pub briefing: Briefing,
    pub job_id: String,
    pub case_id: String,
    pub run_id: String,
    pub fence: String,
    pub task: Task,
}
pub(crate) fn operator(id: &str) -> Actor {
    Actor {
        kind: "operator".into(),
        id: id.into(),
    }
}
pub(crate) fn text(value: &str, name: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(Error::Invalid(format!(
            "{name} must contain 1..{max} bytes of text"
        )));
    }
    Ok(())
}
pub(crate) fn receipt<T: DeserializeOwned>(
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
pub(crate) fn save_receipt<T: Serialize>(
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
    pub(crate) fn now(&self) -> String {
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
                assignment_id: assignment_id.clone(),
                role: "investigator".into(),
                reason: "Manual intake requests a bounded fake investigation".into(),
            },
        )?;
        crate::team::initialize(&tx, &case_id, &assignment_id)?;
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
            qualification: crate::qualification::snapshot(&tx, id)?,
            team: crate::team::snapshot(&tx, id)?,
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
    pub fn claim(&self) -> Result<Option<Claim>> {
        self.claim_for("fake-investigator")
    }
    pub fn complete(&self, claim: &Claim, result: ExecutionResult) -> Result<()> {
        self.complete_team_result(claim, result)
    }
    pub fn run_once(&self, adapter: &dyn ExecutionAdapter) -> Result<bool> {
        let due = self.process_due_obligations()? + self.supervise_team()?;
        let Some(claim) = self.claim()? else {
            return Ok(due > 0);
        };
        self.complete(&claim, adapter.execute(&claim.task))?;
        Ok(true)
    }
}
