//! Durable human escalation delivery. Network effects never hold a SQLite transaction.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::io::Read;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipient {
    pub owner: String,
    pub webhook_env: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationConfig {
    pub schema_version: V1,
    pub revision: i64,
    pub recipients: Vec<Recipient>,
    pub acknowledge_seconds: u32,
    pub response_seconds: u32,
    pub retry_seconds: u32,
    pub max_attempts: u32,
}
impl NotificationConfig {
    fn validate(&self) -> Result<()> {
        if self.revision < 1
            || self.recipients.is_empty()
            || self.recipients.len() > 8
            || self.acknowledge_seconds == 0
            || self.response_seconds < self.acknowledge_seconds
            || self.response_seconds > 2592000
            || !(1..=3600).contains(&self.retry_seconds)
            || !(1..=10).contains(&self.max_attempts)
        {
            return Err(Error::Invalid("invalid notification limits".into()));
        }
        let mut owners = std::collections::HashSet::new();
        for r in &self.recipients {
            text(&r.owner, "human recipient", 128)?;
            if r.owner.starts_with("fake-")
                || !owners.insert(&r.owner)
                || !r.webhook_env.starts_with("WATCHHOUSE_NOTIFY_")
                || r.webhook_env.len() > 128
                || !r
                    .webhook_env
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b == b'_' || b.is_ascii_digit())
            {
                return Err(Error::Invalid(
                    "invalid or duplicate human recipient/secret reference".into(),
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    pub escalation_id: String,
    pub recipient: String,
    pub state: String,
    pub attempts: u32,
    pub next_at: String,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveEscalation {
    pub command: OperatorCommand,
    pub escalation_id: String,
    pub outcome: String,
    pub uncertainty: String,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileDelivery {
    pub command: OperatorCommand,
    pub delivery_id: String,
    pub delivered: bool,
    pub evidence_refs: Vec<String>,
}
/// A transport response is not human acknowledgment or resolution.
pub enum DeliveryOutcome {
    Accepted,
    Rejected,
    RetryAfter(u32),
    Uncertain,
    Unconfigured,
}
pub trait NotificationAdapter {
    fn send(
        &self,
        recipient: &Recipient,
        operation_id: &str,
        escalation_id: &str,
        case_id: &str,
    ) -> DeliveryOutcome;
}
pub struct SlackWebhook;
impl NotificationAdapter for SlackWebhook {
    fn send(
        &self,
        r: &Recipient,
        operation: &str,
        escalation: &str,
        case: &str,
    ) -> DeliveryOutcome {
        let Ok(secret) = std::env::var(&r.webhook_env) else {
            return DeliveryOutcome::Unconfigured;
        };
        let Ok(url) = reqwest::Url::parse(&secret) else {
            return DeliveryOutcome::Unconfigured;
        };
        if url.scheme() != "https"
            || url.host_str() != Some("hooks.slack.com")
            || !url.path().starts_with("/services/")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return DeliveryOutcome::Unconfigured;
        }
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return DeliveryOutcome::Unconfigured;
        };
        let result=client.post(url).json(&serde_json::json!({"text":format!("Watchhouse escalation {escalation}. Case {case}. Delivery {operation}. Human acknowledgment and explicit resolution required in Watchhouse."),"mrkdwn":false,"unfurl_links":false,"unfurl_media":false})).send();
        let Ok(response) = result else {
            return DeliveryOutcome::Uncertain;
        };
        let status = response.status().as_u16();
        let after = response
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(60)
            .clamp(1, 86400);
        let mut body = String::new();
        if response.take(1024).read_to_string(&mut body).is_err() {
            return DeliveryOutcome::Uncertain;
        }
        classify_slack(status, &body, after)
    }
}
fn classify_slack(status: u16, body: &str, after: u32) -> DeliveryOutcome {
    match status {
        200 if body.trim() == "ok" => DeliveryOutcome::Accepted,
        429 => DeliveryOutcome::RetryAfter(after),
        400..=499 => DeliveryOutcome::Rejected,
        _ => DeliveryOutcome::Uncertain,
    }
}

fn after(now: chrono::DateTime<chrono::Utc>, seconds: u32) -> String {
    (now + Duration::seconds(seconds.into())).to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub(crate) fn resolution_state(db: &Connection, e: &mut Escalation) -> Result<()> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM escalation_resolutions WHERE escalation_id=?1",
            [&e.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = raw {
        e.state = "resolved".into();
        e.resolution = Some(serde_json::from_str(&raw)?);
    }
    Ok(())
}
pub(crate) fn can_ack(db: &Connection, e: &Escalation, actor: &str) -> Result<bool> {
    Ok(e.owner==actor || db.query_row("SELECT EXISTS(SELECT 1 FROM escalation_routes r JOIN notification_deliveries d ON d.escalation_id=r.escalation_id AND d.level=r.level WHERE r.escalation_id=?1 AND d.recipient=?2)",params![e.id,actor],|r|r.get(0))?)
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<Vec<Delivery>> {
    let mut s=db.prepare("SELECT d.id,d.escalation_id,d.recipient,d.state,d.attempts,d.next_at,d.detail FROM notification_deliveries d JOIN escalations e ON e.id=d.escalation_id WHERE e.case_id=?1 ORDER BY d.rowid")?;
    s.query_map([id], |r| {
        Ok(Delivery {
            id: r.get(0)?,
            escalation_id: r.get(1)?,
            recipient: r.get(2)?,
            state: r.get(3)?,
            attempts: r.get(4)?,
            next_at: r.get(5)?,
            detail: r.get(6)?,
        })
    })?
    .collect::<std::result::Result<_, _>>()
    .map_err(Into::into)
}
fn delivery_event(
    tx: &rusqlite::Transaction<'_>,
    id: &str,
    state: &str,
    reason: &str,
    now: &str,
) -> Result<()> {
    let (case,recipient):(String,String)=tx.query_row("SELECT e.case_id,d.recipient FROM notification_deliveries d JOIN escalations e ON e.id=d.escalation_id WHERE d.id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    store::emit(
        tx,
        &case,
        operator("notification-supervisor"),
        id,
        now,
        EventData::DeliveryChanged {
            delivery_id: id.into(),
            state: state.into(),
            recipient,
            reason: reason.into(),
        },
    )
}
impl Application {
    pub fn configure_notifications(
        &self,
        config: NotificationConfig,
        who: &str,
    ) -> Result<NotificationConfig> {
        text(who, "operator", 128)?;
        config.validate()?;
        if who.starts_with("fake-") {
            return Err(Error::Invalid(
                "agents cannot grant external communication authority".into(),
            ));
        }
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw = serde_json::to_string(&config)?;
        let active: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM notification_configs",
            [],
            |r| r.get(0),
        )?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT record FROM notification_configs WHERE revision=?1",
                [config.revision],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = existing {
            if old == raw && active == config.revision {
                return Ok(config);
            }
            return Err(Error::Conflict("immutable notification revision".into()));
        }
        if config.revision != active + 1 {
            return Err(Error::Conflict(
                "notification revision must advance by one".into(),
            ));
        }
        tx.execute(
            "INSERT INTO notification_configs VALUES (?1,?2,?3,?4)",
            params![config.revision, raw, who, self.now()],
        )?;
        tx.commit()?;
        Ok(config)
    }
    /// Bounded deterministic supervisor, safe to run concurrently without a model.
    pub fn process_notifications(&self, adapter: &dyn NotificationAdapter) -> Result<usize> {
        let mut changed = 0;
        let now = self.now();
        {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let config: Option<String> = tx
                .query_row(
                    "SELECT record FROM notification_configs ORDER BY revision DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(raw) = config {
                let cfg: NotificationConfig = serde_json::from_str(&raw)?;
                cfg.validate()?;
                let mut s=tx.prepare("SELECT id FROM escalations WHERE id NOT IN (SELECT escalation_id FROM escalation_routes) AND id NOT IN (SELECT escalation_id FROM escalation_resolutions) ORDER BY rowid LIMIT 100")?;
                let ids = s
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                for id in ids {
                    tx.execute(
                        "INSERT INTO escalation_routes VALUES (?1,?2,0,?3,?4,0)",
                        params![
                            id,
                            raw,
                            after(self.clock.now(), cfg.acknowledge_seconds),
                            after(self.clock.now(), cfg.response_seconds)
                        ],
                    )?;
                    changed += 1;
                }
            }
            let mut s=tx.prepare("SELECT r.escalation_id,r.config,r.level,r.acknowledge_by,r.response_by,e.state FROM escalation_routes r JOIN escalations e ON e.id=r.escalation_id WHERE r.exhausted=0 AND r.escalation_id NOT IN (SELECT escalation_id FROM escalation_resolutions) ORDER BY r.rowid LIMIT 100")?;
            let routes = s
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, usize>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(s);
            for (id, raw, mut level, ack, response, state) in routes {
                let cfg: NotificationConfig = serde_json::from_str(&raw)?;
                let failed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM notification_deliveries WHERE escalation_id=?1 AND level=?2 AND state='failed')",params![id,level],|r|r.get(0))?;
                if failed
                    || (state == "open" && ack <= now)
                    || (state == "acknowledged" && response <= now)
                {
                    level += 1;
                    changed += 1;
                    if level >= cfg.recipients.len() {
                        tx.execute(
                            "UPDATE escalation_routes SET exhausted=1 WHERE escalation_id=?1",
                            [&id],
                        )?;
                        let last:Option<String>=tx.query_row("SELECT id FROM notification_deliveries WHERE escalation_id=?1 ORDER BY level DESC LIMIT 1",[&id],|r|r.get(0)).optional()?;
                        if let Some(last) = last {
                            delivery_event(
                                &tx,
                                &last,
                                "chain_exhausted",
                                "All configured recipients exhausted; escalation remains visibly unresolved",
                                &now,
                            )?;
                        }
                        continue;
                    }
                    tx.execute("UPDATE escalation_routes SET level=?1,acknowledge_by=?2,response_by=?3 WHERE escalation_id=?4",params![level,after(self.clock.now(),cfg.acknowledge_seconds),after(self.clock.now(),cfg.response_seconds),id])?;
                    tx.execute("UPDATE escalations SET state='open' WHERE id=?1", [&id])?;
                }
                let inserted=tx.execute("INSERT OR IGNORE INTO notification_deliveries(id,escalation_id,level,recipient,state,next_at,detail) VALUES (?1,?2,?3,?4,'pending',?5,'Configured destination; no delivery attempted')",params![store::id("delivery"),id,level,cfg.recipients[level].owner,now])?;
                changed += inserted;
            }
            let mut s = tx.prepare(
                "SELECT id FROM notification_deliveries WHERE state='inflight' AND lease_until<=?1",
            )?;
            let expired = s
                .query_map([&now], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(s);
            for id in expired {
                tx.execute("UPDATE notification_deliveries SET state='uncertain',detail='Sender lost after claim; operator must reconcile before resend' WHERE id=?1",[&id])?;
                delivery_event(
                    &tx,
                    &id,
                    "uncertain",
                    "Sending process lost; delivery may have occurred",
                    &now,
                )?;
                changed += 1;
            }
            tx.commit()?;
        }
        // Claim at most 10 effects per tick, each with an independent lease/fence.
        for _ in 0..10 {
            let job = {
                let mut db = self.store.connect()?;
                let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let row:Option<(String,String,String,String,usize)>=tx.query_row("SELECT d.id,d.escalation_id,e.case_id,r.config,d.level FROM notification_deliveries d JOIN escalation_routes r ON r.escalation_id=d.escalation_id AND r.level=d.level JOIN escalations e ON e.id=d.escalation_id WHERE d.state IN ('pending','retry') AND d.next_at<=?1 AND r.exhausted=0 AND e.state='open' AND e.id NOT IN (SELECT escalation_id FROM escalation_resolutions) ORDER BY d.rowid LIMIT 1",[&now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
                match row {
                    None => None,
                    Some((id, esc, case, raw, level)) => {
                        let cfg: NotificationConfig = serde_json::from_str(&raw)?;
                        let fence = store::id("send");
                        tx.execute("UPDATE notification_deliveries SET state='inflight',attempts=attempts+1,lease_until=?1,fence=?2 WHERE id=?3",params![after(self.clock.now(),30),fence,id])?;
                        tx.commit()?;
                        Some((id, esc, case, cfg, level, fence))
                    }
                }
            };
            let Some((id, esc, case, cfg, level, fence)) = job else {
                break;
            };
            let outcome = adapter.send(&cfg.recipients[level], &id, &esc, &case);
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let attempts: u32 = tx.query_row(
                "SELECT attempts FROM notification_deliveries WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            let (state, detail, delay) = match outcome {
                DeliveryOutcome::Accepted => (
                    "accepted",
                    "Slack accepted the request; human acknowledgment still required",
                    0,
                ),
                DeliveryOutcome::Rejected => (
                    "failed",
                    "Destination rejected notification; fallback required",
                    0,
                ),
                DeliveryOutcome::RetryAfter(s) if attempts < cfg.max_attempts => (
                    "retry",
                    "Known rate-limit rejection; bounded retry scheduled",
                    s.max(cfg.retry_seconds),
                ),
                DeliveryOutcome::RetryAfter(_) => (
                    "failed",
                    "Known-rejected delivery exhausted retry budget",
                    0,
                ),
                DeliveryOutcome::Uncertain => (
                    "uncertain",
                    "Delivery unknown; reconcile using operation ID before resending",
                    0,
                ),
                DeliveryOutcome::Unconfigured => (
                    "failed",
                    "Destination credential missing or invalid; activation blocked",
                    0,
                ),
            };
            let count=tx.execute("UPDATE notification_deliveries SET state=?1,next_at=?2,detail=?3,lease_until=NULL WHERE id=?4 AND state='inflight' AND fence=?5",params![state,after(self.clock.now(),delay),detail,id,fence])?;
            if count > 0 {
                delivery_event(&tx, &id, state, detail, &self.now())?;
                changed += 1;
            }
            tx.commit()?;
        }
        Ok(changed)
    }
    pub fn resolve_escalation(&self, case_id: &str, cmd: ResolveEscalation) -> Result<Escalation> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        text(&cmd.outcome, "outcome", 2000)?;
        text(&cmd.uncertainty, "uncertainty", 2000)?;
        let fp = store::digest(&format!(
            "resolve-escalation:{case_id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let case = store::case(&tx, case_id)?;
        if case.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale escalation resolution".into()));
        }
        let raw: String = tx
            .query_row(
                "SELECT record FROM escalations WHERE id=?1 AND case_id=?2",
                params![cmd.escalation_id, case_id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::NotFound)?;
        let mut e: Escalation = serde_json::from_str(&raw)?;
        if m.actor.starts_with("fake-") || !can_ack(&tx, &e, &m.actor)? {
            return Err(Error::Invalid(
                "configured human or local accountable owner required".into(),
            ));
        }
        crate::qualification::evidence_refs(&tx, case_id, &cmd.evidence_refs, true)?;
        tx.execute(
            "INSERT INTO escalation_resolutions VALUES (?1,?2)",
            params![e.id, serde_json::to_string(&cmd)?],
        )?;
        store::emit(
            &tx,
            case_id,
            operator(&m.actor),
            &m.idempotency_key,
            &self.now(),
            EventData::EscalationResolved {
                escalation_id: e.id.clone(),
                evidence_refs: cmd.evidence_refs.clone(),
                reason: m.reason.clone(),
            },
        )?;
        e.state = "resolved".into();
        e.resolution = Some(serde_json::to_value(&cmd)?);
        save_receipt(&tx, &m.idempotency_key, &fp, &e)?;
        tx.commit()?;
        Ok(e)
    }
    pub fn reconcile_delivery(
        &self,
        case_id: &str,
        cmd: ReconcileDelivery,
    ) -> Result<Vec<Delivery>> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        let fp = store::digest(&format!(
            "reconcile-delivery:{case_id}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = store::case(&tx, case_id)?;
        if c.version != cmd.command.expected_version
            || c.owner != m.actor
            || m.actor.starts_with("fake-")
        {
            return Err(Error::Conflict(
                "current accountable operator required".into(),
            ));
        }
        crate::qualification::evidence_refs(&tx, case_id, &cmd.evidence_refs, true)?;
        let count=tx.execute("UPDATE notification_deliveries SET state=?1,detail='Operator reconciled uncertain outcome with evidence' WHERE id=?2 AND state='uncertain' AND escalation_id IN (SELECT id FROM escalations WHERE case_id=?3)",params![if cmd.delivered{"accepted"}else{"failed"},cmd.delivery_id,case_id])?;
        if count != 1 {
            return Err(Error::Conflict(
                "delivery is not uncertain for this case".into(),
            ));
        }
        delivery_event(
            &tx,
            &cmd.delivery_id,
            if cmd.delivered { "accepted" } else { "failed" },
            &m.reason,
            &self.now(),
        )?;
        let r = snapshot(&tx, case_id)?;
        save_receipt(&tx, &m.idempotency_key, &fp, &r)?;
        tx.commit()?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slack_contract_distinguishes_acceptance_rejection_rate_limit_and_uncertainty() {
        assert!(matches!(
            classify_slack(200, "ok", 1),
            DeliveryOutcome::Accepted
        ));
        assert!(matches!(
            classify_slack(200, "unexpected", 1),
            DeliveryOutcome::Uncertain
        ));
        assert!(matches!(
            classify_slack(429, "rate_limited", 7),
            DeliveryOutcome::RetryAfter(7)
        ));
        assert!(matches!(
            classify_slack(403, "action_prohibited", 1),
            DeliveryOutcome::Rejected
        ));
        assert!(matches!(
            classify_slack(500, "unknown", 1),
            DeliveryOutcome::Uncertain
        ));
        assert!(matches!(
            classify_slack(302, "redirect", 1),
            DeliveryOutcome::Uncertain
        ));
    }
}
