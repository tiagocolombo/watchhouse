use crate::{
    application::Claim,
    contracts::*,
    error::{Error, Result},
    execution::Task,
    store,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::de::DeserializeOwned;

pub(super) fn records<T: DeserializeOwned>(db: &Connection, sql: &str, id: &str) -> Result<Vec<T>> {
    let mut stmt = db.prepare(sql)?;
    let rows = stmt.query_map([id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (record, state) = row?;
        let mut value: serde_json::Value = serde_json::from_str(&record)?;
        value["state"] = state.into();
        Ok(serde_json::from_value(value)?)
    })
    .collect()
}
pub(super) fn block(db: &Connection, id: &str) -> Result<Block> {
    records(db, "SELECT record,state FROM blocks WHERE id=?1", id)?
        .pop()
        .ok_or(Error::NotFound)
}
pub(super) fn handoff(db: &Connection, id: &str) -> Result<Handoff> {
    records(db, "SELECT record,state FROM handoffs WHERE id=?1", id)?
        .pop()
        .ok_or(Error::NotFound)
}
pub(super) fn message(db: &Connection, id: &str) -> Result<Message> {
    let raw: String = db
        .query_row("SELECT record FROM team_messages WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or(Error::NotFound)?;
    let mut m: Message = serde_json::from_str(&raw)?;
    let mut stmt=db.prepare("SELECT recipient FROM message_ack WHERE message_id=?1 AND acknowledged_at IS NOT NULL ORDER BY recipient")?;
    m.acknowledged_by = stmt
        .query_map([id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(m)
}
pub(super) fn claim(db: &Connection, proof: &RunProof) -> Result<Claim> {
    let row:Option<(String,String,String,String,String,String)>=db.query_row(
        "SELECT j.id,a.case_id,a.id,c.executor,c.fence,c.briefing FROM run_control c JOIN runs r ON r.id=c.run_id JOIN assignments a ON a.id=r.assignment_id JOIN jobs j ON j.assignment_id=a.id WHERE c.run_id=?1",[&proof.run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let Some((job_id, case_id, assignment_id, executor, fence, briefing)) = row else {
        return Err(Error::Conflict("unknown run proof".into()));
    };
    if fence != proof.fence {
        return Err(Error::Conflict("run proof does not match".into()));
    }
    let briefing: Briefing = serde_json::from_str(&briefing)?;
    Ok(Claim {
        schema_version: V1,
        job_id,
        case_id,
        run_id: proof.run_id.clone(),
        executor,
        fence,
        task: Task {
            schema_version: V1,
            assignment_id,
            objective: briefing.objective.clone(),
        },
        briefing,
    })
}
pub(super) fn live(db: &Connection, c: &Claim, now: &str) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM jobs j JOIN assignment_control a ON a.assignment_id=j.assignment_id JOIN runs r ON r.assignment_id=a.assignment_id AND r.attempt=j.attempt JOIN run_control rc ON rc.run_id=r.id WHERE j.id=?1 AND j.fence=?2 AND j.state='running' AND j.lease_until>?3 AND a.state='running' AND a.overall_deadline>?3 AND rc.progress_deadline>?3 AND r.id=?4 AND r.state='running' AND a.executor=?5)",params![c.job_id,c.fence,now,c.run_id,c.executor],|r|r.get(0))?)
}
pub(crate) fn snapshot(db: &Connection, case_id: &str) -> Result<TeamSnapshot> {
    let thread_id: String = db.query_row(
        "SELECT id FROM team_threads WHERE case_id=?1",
        [case_id],
        |r| r.get(0),
    )?;
    let control=Some(db.query_row("SELECT c.executor,c.state,c.overall_deadline,c.max_attempts FROM assignment_control c JOIN assignments a ON a.id=c.assignment_id WHERE a.case_id=?1",[case_id],|r|Ok(AssignmentControl{executor:r.get(0)?,state:r.get(1)?,overall_deadline:r.get(2)?,max_attempts:r.get(3)?}))?);
    let mut stmt = db.prepare("SELECT id FROM team_messages WHERE thread_id=?1 ORDER BY rowid")?;
    let ids = stmt
        .query_map([&thread_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let messages = ids
        .iter()
        .map(|id| message(db, id))
        .collect::<Result<_>>()?;
    let blocks = records(
        db,
        "SELECT b.record,b.state FROM blocks b JOIN assignments a ON a.id=b.assignment_id WHERE a.case_id=?1 ORDER BY b.rowid",
        case_id,
    )?;
    let handoffs = records(
        db,
        "SELECT h.record,h.state FROM handoffs h JOIN blocks b ON b.id=h.block_id JOIN assignments a ON a.id=b.assignment_id WHERE a.case_id=?1 ORDER BY h.rowid",
        case_id,
    )?;
    let mut escalations: Vec<Escalation> = records(
        db,
        "SELECT record,state FROM escalations WHERE case_id=?1 ORDER BY rowid",
        case_id,
    )?;
    for e in &mut escalations {
        crate::notifications::resolution_state(db, e)?;
        e.acknowledged_at = db.query_row(
            "SELECT acknowledged_at FROM escalations WHERE id=?1",
            [&e.id],
            |r| r.get(0),
        )?;
    }
    let mut stmt=db.prepare("SELECT r.id,c.executor,c.heartbeat_at,c.progress_at,c.progress_deadline,c.checkpoint_digest,r.state FROM run_control c JOIN runs r ON r.id=c.run_id JOIN assignments a ON a.id=r.assignment_id WHERE a.case_id=?1 ORDER BY r.attempt")?;
    let runs = stmt
        .query_map([case_id], |r| {
            Ok(RunStatus {
                run_id: r.get(0)?,
                executor: r.get(1)?,
                heartbeat_at: r.get(2)?,
                progress_at: r.get(3)?,
                progress_deadline: r.get(4)?,
                checkpoint_digest: r.get(5)?,
                state: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt=db.prepare("SELECT l.run_id,l.digest,l.recorded_at FROM late_results l JOIN runs r ON r.id=l.run_id JOIN assignments a ON a.id=r.assignment_id WHERE a.case_id=?1 ORDER BY l.rowid")?;
    let late_results = stmt
        .query_map([case_id], |r| {
            Ok(LateResult {
                run_id: r.get(0)?,
                digest: r.get(1)?,
                recorded_at: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(TeamSnapshot {
        thread_id,
        control,
        messages,
        blocks,
        handoffs,
        escalations,
        runs,
        late_results,
    })
}
pub(super) fn expected(db: &Connection, id: &str, version: i64) -> Result<()> {
    let current = store::case(db, id)?.version;
    if current != version {
        return Err(Error::Conflict(format!(
            "expected version {version}, current version {current}; reload before retrying"
        )));
    }
    Ok(())
}
