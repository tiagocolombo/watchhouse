//! Local fake-team application services. No provider, production, or external-message effects.
mod store;
use self::store as read;
use crate::{
    application::{Application, Claim, operator, receipt, save_receipt, text},
    contracts::*,
    domain::Case,
    error::{Error, Result},
    execution::{ExecutionAdapter, FakeAdapter, Task},
    store::{self as db},
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
pub(crate) use read::snapshot;
use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use std::collections::HashSet;
const LEASE_SECONDS: i64 = 30;
const PROGRESS_SECONDS: i64 = 60;
const RESPONSE_SECONDS: i64 = 120;
const ASSIGNMENT_SECONDS: i64 = 300;
fn supervisor() -> Actor {
    Actor {
        kind: "supervisor".into(),
        id: "team-supervisor".into(),
    }
}
fn executor(id: &str) -> Actor {
    Actor {
        kind: "worker".into(),
        id: id.into(),
    }
}
fn validate_meta(m: &CommandMeta) -> Result<()> {
    text(&m.idempotency_key, "idempotency_key", 128)?;
    text(&m.actor, "actor", 128)?;
    text(&m.reason, "reason", 2000)
}
fn identity(id: &str) -> Result<()> {
    text(id, "executor", 128)?;
    if !id.starts_with("fake-")
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid(
            "only local fake-* executor identities are admitted".into(),
        ));
    }
    Ok(())
}
fn fingerprint<T: Serialize>(kind: &str, id: &str, cmd: &T) -> Result<String> {
    Ok(db::digest(&format!(
        "{kind}:{id}:{}",
        serde_json::to_string(cmd)?
    )))
}
fn bounded_time(now: DateTime<Utc>, seconds: i64, limit: &str) -> Result<String> {
    let proposed = (now + Duration::seconds(seconds)).to_rfc3339_opts(SecondsFormat::Millis, true);
    DateTime::parse_from_rfc3339(limit)
        .map_err(|_| Error::Invalid("invalid stored deadline".into()))?;
    Ok(proposed.min(limit.into()))
}
fn refs(tx: &Transaction<'_>, case_id: &str, values: &[String]) -> Result<()> {
    if !values.is_empty() {
        crate::qualification::evidence_refs(tx, case_id, values, false)?;
    }
    Ok(())
}
fn evidence(
    tx: &Transaction<'_>,
    case_id: &str,
    content: &str,
    provenance: &str,
    now: &str,
) -> Result<String> {
    let digest = db::digest(content);
    tx.execute(
        "INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,?3,?4)",
        params![digest, content, provenance, now],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
        params![case_id, digest],
    )?;
    Ok(digest)
}
pub(crate) fn initialize(tx: &Transaction<'_>, case_id: &str, assignment_id: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO team_threads VALUES (?1,?2)",
        params![format!("thread_{case_id}"), case_id],
    )?;
    tx.execute(
        "INSERT INTO assignment_control VALUES (?1,'fake-investigator','queued',NULL,3)",
        [assignment_id],
    )?;
    Ok(())
}
fn post(tx: &Transaction<'_>, m: Message, cause: &str, reason: &str) -> Result<Message> {
    tx.execute(
        "INSERT INTO team_messages(id,thread_id,record,ack_due_at) VALUES (?1,?2,?3,?4)",
        params![
            m.id,
            m.thread_id,
            serde_json::to_string(&m)?,
            m.acknowledge_by
        ],
    )?;
    for recipient in &m.recipients {
        tx.execute(
            "INSERT INTO message_ack VALUES (?1,?2,NULL)",
            params![m.id, recipient],
        )?;
    }
    db::emit(
        tx,
        &m.case_id,
        m.sender.clone(),
        cause,
        &m.recorded_at,
        EventData::MessagePosted {
            message: Box::new(m.clone()),
            reason: reason.into(),
        },
    )?;
    Ok(m)
}
struct Notification {
    actor: Actor,
    run: Option<String>,
    kind: MessageKind,
    body: String,
    recipients: Vec<String>,
    evidence_refs: Vec<String>,
}
fn system_message(
    tx: &Transaction<'_>,
    case_id: &str,
    n: Notification,
    cause: &str,
    now: &str,
) -> Result<()> {
    let reason = n.body.clone();
    let m = Message {
        schema_version: V1,
        id: db::id("msg"),
        thread_id: format!("thread_{case_id}"),
        case_id: case_id.into(),
        sender: n.actor,
        run_id: n.run,
        kind: n.kind,
        body: n.body,
        recipients: n.recipients,
        evidence_refs: n.evidence_refs,
        reply_to: None,
        referenced_version: db::case(tx, case_id)?.version,
        recorded_at: now.into(),
        acknowledge_by: None,
        acknowledged_by: vec![],
    };
    post(tx, m, cause, &reason)?;
    Ok(())
}
fn briefing(
    tx: &Transaction<'_>,
    case_id: &str,
    objective: &str,
    to: &str,
    owner: &str,
    previous: Option<&Block>,
    deadline: &str,
) -> Result<Briefing> {
    let mut stmt=tx.prepare("SELECT m.id FROM team_messages m JOIN message_ack a ON a.message_id=m.id WHERE m.thread_id=?1 AND a.recipient IN (?2,?3) GROUP BY m.id ORDER BY m.rowid DESC LIMIT 20")?;
    let mut messages = stmt
        .query_map(params![format!("thread_{case_id}"), to, owner], |r| {
            r.get(0)
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    messages.reverse();
    Ok(Briefing {
        schema_version: V1,
        objective: objective.into(),
        role: "investigator".into(),
        checkpoint_digest: previous.map(|b| b.checkpoint_digest.clone()),
        evidence_refs: previous.map_or_else(Vec::new, |b| b.details.evidence_refs.clone()),
        message_refs: messages,
        previous_run_id: previous.map(|b| b.run_id.clone()),
        overall_deadline: deadline.into(),
        remaining_attempts: tx.query_row("SELECT c.max_attempts-j.attempt FROM assignment_control c JOIN jobs j ON j.assignment_id=c.assignment_id JOIN assignments a ON a.id=c.assignment_id WHERE a.case_id=?1",[case_id],|r|r.get(0))?,
    })
}
fn start(
    tx: &Transaction<'_>,
    case_id: &str,
    assignment_id: &str,
    who: &str,
    mut manifest: Briefing,
    now: &str,
) -> Result<Claim> {
    let (job_id,attempt,max):(String,i64,i64)=tx.query_row("SELECT j.id,j.attempt,c.max_attempts FROM jobs j JOIN assignment_control c ON c.assignment_id=j.assignment_id WHERE j.assignment_id=?1",[assignment_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if attempt >= max || manifest.overall_deadline.as_str() <= now {
        return Err(Error::Conflict(
            "assignment deadline or shared attempt budget exhausted; operator escalation required"
                .into(),
        ));
    }
    manifest.remaining_attempts = max - attempt - 1;
    let run_id = db::id("run");
    let fence = db::id("fence");
    let time = DateTime::parse_from_rfc3339(now)
        .unwrap()
        .with_timezone(&Utc);
    let lease = bounded_time(time, LEASE_SECONDS, &manifest.overall_deadline)?;
    let progress = bounded_time(time, PROGRESS_SECONDS, &manifest.overall_deadline)?;
    tx.execute(
        "UPDATE jobs SET state='running',attempt=attempt+1,lease_until=?1,fence=?2 WHERE id=?3",
        params![lease, fence, job_id],
    )?;
    tx.execute(
        "UPDATE assignments SET state='running' WHERE id=?1",
        [assignment_id],
    )?;
    tx.execute("UPDATE assignment_control SET state='running',executor=?1,overall_deadline=?2 WHERE assignment_id=?3",params![who,manifest.overall_deadline,assignment_id])?;
    tx.execute("UPDATE cases SET status='running' WHERE id=?1", [case_id])?;
    tx.execute("INSERT INTO runs(id,assignment_id,attempt,adapter,started_at,state) VALUES (?1,?2,?3,'fake-v1',?4,'running')",params![run_id,assignment_id,attempt+1,now])?;
    tx.execute(
        "INSERT INTO run_control VALUES (?1,?2,?3,?4,?4,?5,?6,?7)",
        params![
            run_id,
            who,
            fence,
            now,
            progress,
            manifest.checkpoint_digest,
            serde_json::to_string(&manifest)?
        ],
    )?;
    db::emit(
        tx,
        case_id,
        executor(who),
        &job_id,
        now,
        EventData::RunStarted {
            assignment_id: assignment_id.into(),
            run_id: run_id.clone(),
            attempt: attempt + 1,
            reason: "Fake investigator accepted the bounded assignment and frozen briefing".into(),
        },
    )?;
    Ok(Claim {
        schema_version: V1,
        job_id,
        case_id: case_id.into(),
        run_id,
        fence,
        executor: who.into(),
        task: Task {
            schema_version: V1,
            assignment_id: assignment_id.into(),
            objective: manifest.objective.clone(),
        },
        briefing: manifest,
    })
}
fn revoke(tx: &Transaction<'_>, c: &Claim, now: &str) -> Result<()> {
    tx.execute(
        "UPDATE jobs SET fence=?1,lease_until=NULL WHERE id=?2",
        params![db::id("revoked"), c.job_id],
    )?;
    tx.execute(
        "UPDATE runs SET state='expired',finished_at=?1 WHERE id=?2 AND state='running'",
        params![now, c.run_id],
    )?;
    Ok(())
}
fn make_block(
    tx: &Transaction<'_>,
    c: &Claim,
    mut details: BlockDetails,
    actor: Actor,
    cause: &str,
    now: &str,
) -> Result<Block> {
    let owner = db::case(tx, &c.case_id)?.owner;
    let checkpoint = evidence(
        tx,
        &c.case_id,
        &serde_json::to_string(
            &serde_json::json!({"schema_version":1,"case_id":c.case_id,"run_id":c.run_id,"recorded_at":now,"details":details}),
        )?,
        "fake-worker-checkpoint-v1",
        now,
    )?;
    details.evidence_refs.push(checkpoint.clone());
    let due = bounded_time(
        DateTime::parse_from_rfc3339(now)
            .unwrap()
            .with_timezone(&Utc),
        RESPONSE_SECONDS,
        &c.briefing.overall_deadline,
    )?;
    let due = if c.briefing.remaining_attempts == 0 {
        now.to_owned()
    } else {
        due
    };
    let block = Block {
        schema_version: V1,
        id: db::id("blk"),
        case_id: c.case_id.clone(),
        assignment_id: c.task.assignment_id.clone(),
        run_id: c.run_id.clone(),
        owner: owner.clone(),
        reported_at: now.into(),
        response_due_at: due.clone(),
        state: "open".into(),
        escalation_path: "local-operator-inbox".into(),
        checkpoint_digest: checkpoint,
        details,
    };
    revoke(tx, c, now)?;
    tx.execute(
        "UPDATE assignment_control SET state='blocked' WHERE assignment_id=?1",
        [&c.task.assignment_id],
    )?;
    tx.execute(
        "INSERT INTO blocks VALUES (?1,?2,?3,?4,'open',?5)",
        params![
            block.id,
            block.assignment_id,
            block.run_id,
            serde_json::to_string(&block)?,
            due
        ],
    )?;
    db::emit(tx,&c.case_id,actor.clone(),cause,now,EventData::AssignmentBlocked{block:Box::new(block.clone()),reason:"Execution stopped in the side-effect-free fake boundary; owner must arrange help or escalation".into()})?;
    system_message(
        tx,
        &c.case_id,
        Notification {
            actor,
            run: Some(c.run_id.clone()),
            kind: MessageKind::Block,
            body: block.details.assistance_needed.clone(),
            recipients: vec![owner],
            evidence_refs: block.details.evidence_refs.clone(),
        },
        cause,
        now,
    )?;
    Ok(block)
}
pub(crate) fn escalate(
    tx: &Transaction<'_>,
    case_id: &str,
    kind: &str,
    id: &str,
    reason: &str,
    now: &str,
) -> Result<bool> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM escalations WHERE source_type=?1 AND source_id=?2)",
        params![kind, id],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(false);
    }
    let e=Escalation{schema_version:V1,id:db::id("esc"),case_id:case_id.into(),source_type:kind.into(),source_id:id.into(),owner:"operator".into(),reason:reason.into(),recorded_at:now.into(),state:"open".into(),acknowledged_at:None,next_action:"Operator must review the blocker and arrange an accepted handoff or explicit decision. No external notification was sent".into()};
    tx.execute(
        "INSERT INTO escalations VALUES (?1,?2,?3,?4,?5,'open',NULL)",
        params![e.id, case_id, kind, id, serde_json::to_string(&e)?],
    )?;
    db::emit(
        tx,
        case_id,
        supervisor(),
        id,
        now,
        EventData::EscalationOpened {
            escalation: Box::new(e),
            reason: reason.into(),
        },
    )?;
    Ok(true)
}
impl Application {
    /// One local fake responder can accept a pending handoff, execute, or report a block.
    pub fn run_fake_worker_once(&self, who: &str, blocked: bool) -> Result<bool> {
        identity(who)?;
        let transitions = self.process_due_obligations()? + self.supervise_team()?;
        let claimed = self.accept_or_claim_fake(who)?;
        let Some(c) = claimed else {
            return Ok(transitions > 0);
        };
        if blocked {
            self.report_block(ReportBlock{run:RunCommand{meta:CommandMeta{schema_version:V1,idempotency_key:format!("block-{}",c.run_id),actor:who.into(),reason:"Deterministic fake missing-evidence scenario".into()},proof:RunProof{run_id:c.run_id.clone(),fence:c.fence.clone()}},details:BlockDetails{reason_code:BlockReason::MissingEvidence,work_attempted:format!("Read bounded objective: {}",c.task.objective),last_useful_progress:"Objective read; no service inspected".into(),checkpoint:"No mutable workspace or external effects. Continue from the frozen assignment briefing".into(),assistance_needed:"A second fake investigator must obtain missing evidence or request operator help".into(),uncertainty:"No actual cause or recovery is established".into(),evidence_refs:vec![]}})?;
        } else {
            self.complete(&c, FakeAdapter.execute(&c.task))?;
        }
        Ok(true)
    }
    pub fn accept_or_claim_fake(&self, who: &str) -> Result<Option<Claim>> {
        identity(who)?;
        self.supervise_team()?;
        let db = self.store.connect()?;
        let mut stmt = db.prepare(
            "SELECT record,state FROM handoffs WHERE state='proposed' AND json_extract(record,'$.successor')=?1 AND acknowledge_by>?2 ORDER BY rowid LIMIT 1",
        )?;
        let candidates = stmt
            .query_map(params![who, self.now()], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str::<Handoff>(&r?)?))
            .collect::<Result<Vec<_>>>()?;
        let next = candidates
            .into_iter()
            .find(|h| h.successor == who && h.acknowledge_by > self.now());
        let claimed = if let Some(h) = next {
            let version = db::case(&db, &h.case_id)?.version;
            Some(self.accept_handoff(
                &h.case_id,
                AcceptHandoff {
                    command: OperatorCommand {
                        meta: CommandMeta {
                            schema_version: V1,
                            idempotency_key: format!("accept-{}", h.id),
                            actor: who.into(),
                            reason: "Fake responder accepted the persisted briefing".into(),
                        },
                        expected_version: version,
                    },
                    handoff_id: h.id,
                },
            )?)
        } else {
            self.claim_for(who)?
        };
        Ok(claimed)
    }
    pub fn claim_for(&self, who: &str) -> Result<Option<Claim>> {
        self.supervise_team()?;
        self.claim_internal(who, None)
    }
    pub fn claim_work(&self, cmd: ClaimWork) -> Result<Option<Claim>> {
        validate_meta(&cmd.meta)?;
        self.supervise_team()?;
        self.claim_internal(&cmd.executor, Some(&cmd))
    }
    fn claim_internal(&self, who: &str, command: Option<&ClaimWork>) -> Result<Option<Claim>> {
        identity(who)?;
        let fp = command.map(|c| fingerprint("claim", who, c)).transpose()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(cmd) = command
            && let Some(response) = receipt(&tx, &cmd.meta.idempotency_key, fp.as_ref().unwrap())?
        {
            return Ok(response);
        }
        let row:Option<(String,String,String)>=tx.query_row("SELECT a.id,a.case_id,a.objective FROM assignments a JOIN jobs j ON j.assignment_id=a.id JOIN assignment_control c ON c.assignment_id=a.id WHERE j.state='queued' AND c.state='queued' AND c.executor=?1 ORDER BY j.rowid LIMIT 1",[who],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let response = if let Some((assignment_id, case_id, objective)) = row {
            let now = self.now();
            let deadline = (self.clock.now() + Duration::seconds(ASSIGNMENT_SECONDS))
                .to_rfc3339_opts(SecondsFormat::Millis, true);
            let owner = db::case(&tx, &case_id)?.owner;
            let manifest = briefing(&tx, &case_id, &objective, who, &owner, None, &deadline)?;
            Some(start(&tx, &case_id, &assignment_id, who, manifest, &now)?)
        } else {
            None
        };
        if let Some(cmd) = command {
            save_receipt(
                &tx,
                &cmd.meta.idempotency_key,
                fp.as_ref().unwrap(),
                &response,
            )?;
        }
        tx.commit()?;
        Ok(response)
    }
    pub fn post_team_message(&self, case_id: &str, cmd: PostMessage) -> Result<Message> {
        validate_meta(&cmd.command.meta)?;
        text(&cmd.body, "body", 8000)?;
        if matches!(cmd.kind, MessageKind::Block | MessageKind::Handoff) {
            return Err(Error::Invalid(
                "block and handoff messages require their dedicated commands".into(),
            ));
        }
        if cmd.recipients.is_empty()
            || cmd.recipients.len() > 5
            || cmd.recipients.iter().collect::<HashSet<_>>().len() != cmd.recipients.len()
        {
            return Err(Error::Invalid(
                "supply 1..5 distinct local recipients".into(),
            ));
        }
        for r in &cmd.recipients {
            text(r, "recipient", 128)?;
        }
        if cmd
            .ack_within_seconds
            .is_some_and(|s| !(1..=86400).contains(&s))
        {
            return Err(Error::Invalid("ack_within_seconds must be 1..86400".into()));
        }
        let fp = fingerprint("message", case_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = receipt(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return Ok(response);
        }
        read::expected(&tx, case_id, cmd.command.expected_version)?;
        refs(&tx, case_id, &cmd.evidence_refs)?;
        let mut reply = cmd.reply_to.clone();
        let mut depth = 0;
        while let Some(id) = reply {
            let parent = read::message(&tx, &id)?;
            if parent.case_id != case_id {
                return Err(Error::Invalid("reply must stay in this case thread".into()));
            }
            depth += 1;
            if depth > 8 {
                return Err(Error::Invalid("reply depth exceeds 8".into()));
            }
            reply = parent.reply_to;
        }
        let now = self.now();
        let m = Message {
            schema_version: V1,
            id: db::id("msg"),
            thread_id: format!("thread_{case_id}"),
            case_id: case_id.into(),
            sender: operator(&cmd.command.meta.actor),
            run_id: None,
            kind: cmd.kind,
            body: cmd.body,
            recipients: cmd.recipients,
            evidence_refs: cmd.evidence_refs,
            reply_to: cmd.reply_to,
            referenced_version: cmd.command.expected_version,
            recorded_at: now,
            acknowledge_by: cmd.ack_within_seconds.map(|s| {
                (self.clock.now() + Duration::seconds(s.into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true)
            }),
            acknowledged_by: vec![],
        };
        let m = post(
            &tx,
            m,
            &cmd.command.meta.idempotency_key,
            &cmd.command.meta.reason,
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &m)?;
        tx.commit()?;
        Ok(m)
    }
    pub fn acknowledge_message(&self, case_id: &str, cmd: AcknowledgeMessage) -> Result<Case> {
        validate_meta(&cmd.command.meta)?;
        let fp = fingerprint("ack-message", case_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        read::expected(&tx, case_id, cmd.command.expected_version)?;
        let m = read::message(&tx, &cmd.message_id)?;
        if m.case_id != case_id || !m.recipients.contains(&cmd.command.meta.actor) {
            return Err(Error::Invalid(
                "only a listed recipient can acknowledge this case message".into(),
            ));
        }
        let changed=tx.execute("UPDATE message_ack SET acknowledged_at=?1 WHERE message_id=?2 AND recipient=?3 AND acknowledged_at IS NULL",params![self.now(),cmd.message_id,cmd.command.meta.actor])?;
        if changed != 1 {
            return Err(Error::Conflict("message already acknowledged".into()));
        }
        db::emit(
            &tx,
            case_id,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &self.now(),
            EventData::MessageAcknowledged {
                message_id: cmd.message_id,
                recipient: cmd.command.meta.actor.clone(),
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        let r = db::case(&tx, case_id)?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn heartbeat(&self, cmd: RunCommand) -> Result<Case> {
        self.activity(cmd, None)
    }
    pub fn report_progress(&self, cmd: ReportProgress) -> Result<Case> {
        text(&cmd.checkpoint, "checkpoint", 8000)?;
        self.activity(cmd.run, Some((cmd.checkpoint, cmd.evidence_refs)))
    }
    fn activity(&self, cmd: RunCommand, progress: Option<(String, Vec<String>)>) -> Result<Case> {
        validate_meta(&cmd.meta)?;
        let fp = fingerprint("activity", &cmd.proof.run_id, &(&cmd, &progress))?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = read::claim(&tx, &cmd.proof)?;
        let now = self.now();
        if cmd.meta.actor != c.executor || !read::live(&tx, &c, &now)? {
            return Err(Error::Conflict(
                "expired or superseded worker authority".into(),
            ));
        }
        let lease = bounded_time(
            self.clock.now(),
            LEASE_SECONDS,
            &c.briefing.overall_deadline,
        )?;
        tx.execute(
            "UPDATE jobs SET lease_until=?1 WHERE id=?2",
            params![lease, c.job_id],
        )?;
        tx.execute(
            "UPDATE run_control SET heartbeat_at=?1 WHERE run_id=?2",
            params![now, c.run_id],
        )?;
        if let Some((checkpoint, mut values)) = progress {
            values.sort();
            refs(&tx, &c.case_id, &values)?;
            let content = serde_json::to_string(
                &serde_json::json!({"schema_version":1,"assignment_id":c.task.assignment_id,"checkpoint":checkpoint,"evidence_refs":values}),
            )?;
            let digest = evidence(&tx, &c.case_id, &content, "fake-worker-progress-v1", &now)?;
            let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM progress_evidence WHERE assignment_id=?1 AND digest=?2)",params![c.task.assignment_id,digest],|r|r.get(0))?;
            if duplicate {
                return Err(Error::Conflict(
                    "repeated checkpoint is not new useful progress".into(),
                ));
            }
            tx.execute(
                "INSERT INTO progress_evidence VALUES (?1,?2)",
                params![c.task.assignment_id, digest],
            )?;
            let until = bounded_time(
                self.clock.now(),
                PROGRESS_SECONDS,
                &c.briefing.overall_deadline,
            )?;
            tx.execute("UPDATE run_control SET progress_at=?1,progress_deadline=?2,checkpoint_digest=?3 WHERE run_id=?4",params![now,until,digest,c.run_id])?;
            db::emit(
                &tx,
                &c.case_id,
                executor(&c.executor),
                &cmd.meta.idempotency_key,
                &now,
                EventData::RunProgressed {
                    run_id: c.run_id.clone(),
                    evidence_refs: vec![digest],
                    reason: cmd.meta.reason.clone(),
                },
            )?;
        } else {
            db::emit(
                &tx,
                &c.case_id,
                executor(&c.executor),
                &cmd.meta.idempotency_key,
                &now,
                EventData::RunHeartbeat {
                    run_id: c.run_id.clone(),
                    reason: cmd.meta.reason.clone(),
                },
            )?;
        }
        let r = db::case(&tx, &c.case_id)?;
        save_receipt(&tx, &cmd.meta.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn report_block(&self, cmd: ReportBlock) -> Result<Block> {
        validate_meta(&cmd.run.meta)?;
        for (value, name, max) in [
            (&cmd.details.work_attempted, "work_attempted", 2000),
            (
                &cmd.details.last_useful_progress,
                "last_useful_progress",
                2000,
            ),
            (&cmd.details.checkpoint, "checkpoint", 8000),
            (&cmd.details.assistance_needed, "assistance_needed", 2000),
            (&cmd.details.uncertainty, "uncertainty", 2000),
        ] {
            text(value, name, max)?;
        }
        if !matches!(
            cmd.details.reason_code,
            BlockReason::MissingEvidence | BlockReason::MissingCapability
        ) {
            return Err(Error::Invalid("workers report missing evidence/capability; timeout reasons belong to the supervisor".into()));
        }
        if cmd.details.evidence_refs.len() > 19 {
            return Err(Error::Invalid(
                "block accepts at most 19 supporting digests".into(),
            ));
        }
        let fp = fingerprint("block", &cmd.run.proof.run_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.run.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = read::claim(&tx, &cmd.run.proof)?;
        let now = self.now();
        if cmd.run.meta.actor != c.executor || !read::live(&tx, &c, &now)? {
            return Err(Error::Conflict(
                "expired or superseded worker authority".into(),
            ));
        }
        refs(&tx, &c.case_id, &cmd.details.evidence_refs)?;
        let b = make_block(
            &tx,
            &c,
            cmd.details,
            executor(&c.executor),
            &cmd.run.meta.idempotency_key,
            &now,
        )?;
        save_receipt(&tx, &cmd.run.meta.idempotency_key, &fp, &b)?;
        tx.commit()?;
        Ok(b)
    }
    pub fn offer_handoff(&self, case_id: &str, cmd: OfferHandoff) -> Result<Handoff> {
        validate_meta(&cmd.command.meta)?;
        identity(&cmd.successor)?;
        text(&cmd.next_owner, "next_owner", 128)?;
        if !(1..=120).contains(&cmd.acknowledge_within_seconds) {
            return Err(Error::Invalid("handoff acknowledgment must be 1..120 seconds and cannot extend the assignment deadline".into()));
        }
        let fp = fingerprint("handoff", case_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        read::expected(&tx, case_id, cmd.command.expected_version)?;
        let b = read::block(&tx, &cmd.block_id)?;
        refs(&tx, case_id, &b.details.evidence_refs)?;
        if b.case_id != case_id || !matches!(b.state.as_str(), "open" | "escalated") {
            return Err(Error::Conflict("block is not available for handoff".into()));
        }
        let (objective,deadline,attempt,max,who,state):(String,String,i64,i64,String,String)=tx.query_row("SELECT a.objective,c.overall_deadline,j.attempt,c.max_attempts,c.executor,c.state FROM assignments a JOIN assignment_control c ON c.assignment_id=a.id JOIN jobs j ON j.assignment_id=a.id WHERE a.id=?1",[&b.assignment_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        let now = self.now();
        if state != "blocked" || who == cmd.successor {
            return Err(Error::Conflict(
                "handoff needs a blocked assignment and a different successor".into(),
            ));
        }
        if attempt >= max || deadline <= now {
            return Err(Error::Conflict("assignment deadline or shared attempt budget exhausted; operator escalation required".into()));
        }
        let h = Handoff {
            schema_version: V1,
            id: db::id("hnd"),
            case_id: case_id.into(),
            block_id: b.id.clone(),
            from_run_id: b.run_id.clone(),
            previous_owner: db::case(&tx, case_id)?.owner,
            successor: cmd.successor.clone(),
            next_owner: cmd.next_owner.clone(),
            state: "proposed".into(),
            acknowledge_by: bounded_time(
                self.clock.now(),
                cmd.acknowledge_within_seconds.into(),
                &deadline,
            )?,
            briefing: briefing(
                &tx,
                case_id,
                &objective,
                &cmd.successor,
                &b.owner,
                Some(&b),
                &deadline,
            )?,
        };
        tx.execute(
            "INSERT INTO handoffs VALUES (?1,?2,?3,'proposed',?4)",
            params![h.id, b.id, serde_json::to_string(&h)?, h.acknowledge_by],
        )?;
        tx.execute(
            "UPDATE blocks SET state='handoff_pending' WHERE id=?1",
            [&b.id],
        )?;
        tx.execute(
            "UPDATE assignment_control SET state='handoff_pending' WHERE assignment_id=?1",
            [&b.assignment_id],
        )?;
        db::emit(
            &tx,
            case_id,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &now,
            EventData::HandoffProposed {
                handoff: Box::new(h.clone()),
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        system_message(
            &tx,
            case_id,
            Notification {
                actor: operator(&cmd.command.meta.actor),
                run: None,
                kind: MessageKind::Handoff,
                body: format!(
                    "{} must accept handoff {} by {}; {} remains accountable until acceptance",
                    h.successor, h.id, h.acknowledge_by, h.previous_owner
                ),
                recipients: vec![h.successor.clone()],
                evidence_refs: b.details.evidence_refs,
            },
            &h.id,
            &now,
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &h)?;
        tx.commit()?;
        Ok(h)
    }
    pub fn accept_handoff(&self, case_id: &str, cmd: AcceptHandoff) -> Result<Claim> {
        validate_meta(&cmd.command.meta)?;
        let fp = fingerprint("accept-handoff", case_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        read::expected(&tx, case_id, cmd.command.expected_version)?;
        let h = read::handoff(&tx, &cmd.handoff_id)?;
        if h.case_id != case_id
            || h.state != "proposed"
            || h.successor != cmd.command.meta.actor
            || h.acknowledge_by <= self.now()
        {
            return Err(Error::Conflict("handoff is not current, not addressed to this actor, or past acknowledgment deadline".into()));
        }
        let b = read::block(&tx, &h.block_id)?;
        let active: bool = tx.query_row(
            "SELECT state='handoff_pending' FROM assignment_control WHERE assignment_id=?1",
            [&b.assignment_id],
            |r| r.get(0),
        )?;
        if !active {
            return Err(Error::Conflict(
                "assignment no longer awaits this handoff".into(),
            ));
        }
        refs(&tx, case_id, &h.briefing.evidence_refs)?;
        let now = self.now();
        let c = start(
            &tx,
            case_id,
            &b.assignment_id,
            &h.successor,
            h.briefing.clone(),
            &now,
        )?;
        tx.execute("UPDATE handoffs SET state='accepted' WHERE id=?1", [&h.id])?;
        tx.execute("UPDATE blocks SET state='transferred' WHERE id=?1", [&b.id])?;
        tx.execute(
            "UPDATE cases SET owner=?1 WHERE id=?2",
            params![h.next_owner, case_id],
        )?;
        tx.execute(
            "UPDATE assignments SET owner=?1 WHERE id=?2",
            params![h.next_owner, b.assignment_id],
        )?;
        db::emit(
            &tx,
            case_id,
            executor(&h.successor),
            &cmd.command.meta.idempotency_key,
            &now,
            EventData::HandoffAccepted {
                handoff_id: h.id,
                run_id: c.run_id.clone(),
                owner: h.next_owner,
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &c)?;
        tx.commit()?;
        Ok(c)
    }
    pub fn acknowledge_escalation(
        &self,
        case_id: &str,
        cmd: AcknowledgeEscalation,
    ) -> Result<Case> {
        validate_meta(&cmd.command.meta)?;
        let fp = fingerprint("ack-escalation", case_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.command.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        read::expected(&tx, case_id, cmd.command.expected_version)?;
        let e: Escalation = read::records(
            &tx,
            "SELECT record,state FROM escalations WHERE id=?1",
            &cmd.escalation_id,
        )?
        .pop()
        .ok_or(Error::NotFound)?;
        if e.case_id != case_id || e.owner != cmd.command.meta.actor {
            return Err(Error::Invalid(
                "only the local inbox owner can acknowledge this escalation".into(),
            ));
        }
        if e.state != "open" {
            return Err(Error::Conflict("escalation already acknowledged".into()));
        }
        tx.execute(
            "UPDATE escalations SET state='acknowledged',acknowledged_at=?1 WHERE id=?2",
            params![self.now(), e.id],
        )?;
        db::emit(
            &tx,
            case_id,
            operator(&cmd.command.meta.actor),
            &cmd.command.meta.idempotency_key,
            &self.now(),
            EventData::EscalationAcknowledged {
                escalation_id: e.id,
                reason: cmd.command.meta.reason.clone(),
            },
        )?;
        let r = db::case(&tx, case_id)?;
        save_receipt(&tx, &cmd.command.meta.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub fn escalation_inbox(&self) -> Result<Vec<Escalation>> {
        let db = self.store.connect()?;
        let mut stmt = db.prepare(
            "SELECT record,state,acknowledged_at FROM escalations ORDER BY rowid DESC LIMIT 200",
        )?;
        stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .map(|row| {
            let (raw, state, acknowledged_at) = row?;
            let mut e: Escalation = serde_json::from_str(&raw)?;
            e.state = state;
            e.acknowledged_at = acknowledged_at;
            Ok(e)
        })
        .collect()
    }
    pub fn supervise_team(&self) -> Result<usize> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = self.now();
        let mut changed = crate::challenges::supervise(&tx, &now)?;
        let proofs: Vec<RunProof> = {
            let mut stmt=tx.prepare("SELECT r.id,rc.fence FROM runs r JOIN run_control rc ON rc.run_id=r.id JOIN jobs j ON j.assignment_id=r.assignment_id AND j.attempt=r.attempt JOIN assignment_control c ON c.assignment_id=r.assignment_id WHERE c.state='running' AND r.state='running' AND (j.lease_until<=?1 OR rc.progress_deadline<=?1 OR c.overall_deadline<=?1) ORDER BY r.rowid LIMIT 100")?;
            stmt.query_map([&now], |r| {
                Ok(RunProof {
                    run_id: r.get(0)?,
                    fence: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?
        };
        for proof in proofs {
            let c = read::claim(&tx, &proof)?;
            let(lease,progress,last,checkpoint):(String,String,String,Option<String>)=tx.query_row("SELECT j.lease_until,rc.progress_deadline,rc.progress_at,rc.checkpoint_digest FROM jobs j JOIN run_control rc ON rc.run_id=?1 WHERE j.id=?2",params![c.run_id,c.job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
            let reason = if c.briefing.overall_deadline <= now {
                BlockReason::AssignmentDeadline
            } else if progress <= now {
                BlockReason::ProgressDeadline
            } else {
                debug_assert!(lease <= now);
                BlockReason::HeartbeatLost
            };
            make_block(&tx,&c,BlockDetails{reason_code:reason,work_attempted:format!("Bounded investigation: {}",c.task.objective),last_useful_progress:last,checkpoint:checkpoint.as_ref().map_or_else(||"No checkpoint reported; resume from frozen briefing".into(),|d|format!("Resume from immutable checkpoint {d}")),assistance_needed:"Owner must inspect stalled fake execution, arrange acknowledged handoff within remaining budget, or review local escalation".into(),uncertainty:"Heartbeat or activity does not prove useful progress; no service recovery or external effect was observed".into(),evidence_refs:checkpoint.into_iter().collect()},supervisor(),&c.run_id,&now)?;
            changed += 1;
        }
        let expired: Vec<Handoff> = read::records(
            &tx,
            "SELECT record,state FROM handoffs WHERE state='proposed' AND acknowledge_by<=?1 ORDER BY rowid LIMIT 100",
            &now,
        )?;
        for h in expired {
            let b = read::block(&tx, &h.block_id)?;
            tx.execute("UPDATE handoffs SET state='expired' WHERE id=?1", [&h.id])?;
            tx.execute("UPDATE blocks SET state='escalated' WHERE id=?1", [&b.id])?;
            tx.execute(
                "UPDATE assignment_control SET state='blocked' WHERE assignment_id=?1",
                [&b.assignment_id],
            )?;
            db::emit(
                &tx,
                &h.case_id,
                supervisor(),
                &h.id,
                &now,
                EventData::HandoffExpired {
                    handoff_id: h.id.clone(),
                    reason: "Successor did not accept in time; prior owner remains accountable"
                        .into(),
                },
            )?;
            escalate(
                &tx,
                &h.case_id,
                "handoff",
                &h.id,
                "Handoff acknowledgment deadline missed; accountability was not transferred",
                &now,
            )?;
            changed += 1;
        }
        let due: Vec<Block> = read::records(
            &tx,
            "SELECT record,state FROM blocks WHERE state IN ('open','handoff_pending') AND response_due_at<=?1 ORDER BY rowid LIMIT 100",
            &now,
        )?;
        for b in due {
            tx.execute("UPDATE blocks SET state='escalated' WHERE id=?1", [&b.id])?;
            escalate(
                &tx,
                &b.case_id,
                "block",
                &b.id,
                "Block response deadline reached; operator intervention required",
                &now,
            )?;
            changed += 1;
        }
        let messages: Vec<(String, String)> = {
            let mut stmt=tx.prepare("SELECT m.id,t.case_id FROM team_messages m JOIN team_threads t ON t.id=m.thread_id WHERE m.escalated=0 AND m.ack_due_at<=?1 AND EXISTS(SELECT 1 FROM message_ack a WHERE a.message_id=m.id AND a.acknowledged_at IS NULL) ORDER BY m.rowid LIMIT 100")?;
            stmt.query_map([&now], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<std::result::Result<_, _>>()?
        };
        for (id, case_id) in messages {
            tx.execute("UPDATE team_messages SET escalated=1 WHERE id=?1", [&id])?;
            escalate(
                &tx,
                &case_id,
                "message",
                &id,
                "Required message acknowledgment missed",
                &now,
            )?;
            changed += 1;
        }
        tx.commit()?;
        Ok(changed)
    }
    pub fn finish_fake(&self, cmd: RunCommand) -> Result<Case> {
        validate_meta(&cmd.meta)?;
        let fp = fingerprint("finish-fake", &cmd.proof.run_id, &cmd)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &cmd.meta.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = read::claim(&tx, &cmd.proof)?;
        if cmd.meta.actor != c.executor {
            return Err(Error::Invalid(
                "executor attribution does not match run proof".into(),
            ));
        }
        let accepted = complete_result(&tx, &c, FakeAdapter.execute(&c.task), &self.now())?;
        if !accepted {
            tx.commit()?;
            return Err(Error::Conflict(
                "late result retained as evidence; worker cannot advance workflow".into(),
            ));
        }
        let r = db::case(&tx, &c.case_id)?;
        save_receipt(&tx, &cmd.meta.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
    pub(crate) fn complete_team_result(
        &self,
        claim: &Claim,
        result: ExecutionResult,
    ) -> Result<()> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let c = read::claim(
            &tx,
            &RunProof {
                run_id: claim.run_id.clone(),
                fence: claim.fence.clone(),
            },
        )?;
        if c.case_id != claim.case_id
            || c.task.assignment_id != claim.task.assignment_id
            || c.job_id != claim.job_id
            || c.executor != claim.executor
        {
            return Err(Error::Conflict("misattributed claim".into()));
        }
        let accepted = complete_result(&tx, &c, result, &self.now())?;
        tx.commit()?;
        if accepted {
            Ok(())
        } else {
            Err(Error::Conflict(
                "late result retained as evidence; worker cannot advance workflow".into(),
            ))
        }
    }
}
fn complete_result(
    tx: &Transaction<'_>,
    c: &Claim,
    result: ExecutionResult,
    now: &str,
) -> Result<bool> {
    if result.assignment_id != c.task.assignment_id
        || result.conclusion != "insufficient_evidence"
        || result.actual_model.is_some()
        || result.usage.is_some()
    {
        return Err(Error::Invalid("result violates fake-v1 contract".into()));
    }
    text(&result.summary, "summary", 16000)?;
    let content = serde_json::to_string(&result)?;
    let accepted: Option<String> = tx
        .query_row(
            "SELECT result FROM runs WHERE id=?1 AND state='completed'",
            [&c.run_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = accepted {
        return if old == content {
            Ok(true)
        } else {
            Err(Error::Conflict(
                "completed run has a different result".into(),
            ))
        };
    }
    if !read::live(tx, c, now)? {
        let rejected = serde_json::to_string(
            &serde_json::json!({"schema_version":1,"run_id":c.run_id,"executor":c.executor,"authority":"rejected-late-result","result":result}),
        )?;
        let digest = evidence(tx, &c.case_id, &rejected, "fake-v1:late-unaccepted", now)?;
        let count = tx.execute(
            "INSERT OR IGNORE INTO late_results VALUES (?1,?2,?3)",
            params![c.run_id, digest, now],
        )?;
        if count > 0 {
            db::emit(tx,&c.case_id,executor(&c.executor),&c.run_id,now,EventData::LateResultRecorded{run_id:c.run_id.clone(),evidence_refs:vec![digest],reason:"Old proof attributed, result retained only as unaccepted evidence; no authoritative workflow advance".into()})?;
        }
        return Ok(false);
    }
    crate::challenges::ensure_clear(tx, &c.case_id)?;
    let digest = evidence(tx, &c.case_id, &content, "fake-v1:synthetic-result", now)?;
    tx.execute(
        "UPDATE runs SET state='completed',finished_at=?1,result=?2,evidence_digest=?3 WHERE id=?4",
        params![now, content, digest, c.run_id],
    )?;
    tx.execute(
        "UPDATE assignments SET state='completed' WHERE id=?1",
        [&c.task.assignment_id],
    )?;
    tx.execute(
        "UPDATE assignment_control SET state='completed' WHERE assignment_id=?1",
        [&c.task.assignment_id],
    )?;
    tx.execute(
        "UPDATE jobs SET state='completed',lease_until=NULL WHERE id=?1",
        [&c.job_id],
    )?;
    tx.execute(
        "UPDATE cases SET status='awaiting_review' WHERE id=?1",
        [&c.case_id],
    )?;
    db::emit(tx,&c.case_id,executor(&c.executor),&c.run_id,now,EventData::AssignmentCompleted{assignment_id:c.task.assignment_id.clone(),run_id:c.run_id.clone(),evidence_refs:vec![digest],reason:"Synthetic result persisted; operator owns review, no recovery or disposition asserted".into()})?;
    Ok(true)
}
