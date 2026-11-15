//! Trusted control-plane configuration. No worker or HTTP mutation route exists.
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
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub name: String,
    pub lease_seconds: i64,
    pub progress_seconds: i64,
    pub response_seconds: i64,
    pub assignment_seconds: i64,
    pub max_attempts: i64,
    pub max_ack_seconds: u32,
    pub max_challenge_seconds: u32,
    pub max_review_seconds: u32,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        text(&self.name, "policy name", 128)?;
        if self.revision < 1
            || !(1..=86400).contains(&self.lease_seconds)
            || self.progress_seconds < self.lease_seconds
            || self.response_seconds < 1
            || self.progress_seconds > self.assignment_seconds
            || self.response_seconds > self.assignment_seconds
            || self.assignment_seconds > 2592000
            || !(1..=100).contains(&self.max_attempts)
            || !(1..=86400).contains(&self.max_ack_seconds)
            || self.max_challenge_seconds < self.max_ack_seconds
            || self.max_challenge_seconds > 2592000
            || !(1..=2592000).contains(&self.max_review_seconds)
        {
            return Err(Error::Invalid(
                "invalid policy timer ordering or execution budget".into(),
            ));
        }
        Ok(())
    }
    pub fn provenance(&self) -> String {
        format!(
            "{}@{}:{}",
            self.name,
            self.revision,
            store::digest(&serde_json::to_string(self).expect("serializable policy"))
        )
    }
}
pub(crate) fn bootstrap(tx: &Transaction<'_>) -> Result<()> {
    let p: Policy = serde_json::from_str(include_str!("../policy/local-fake-v1.json"))?;
    p.validate()?;
    let raw = serde_json::to_string(&p)?;
    tx.execute("INSERT OR IGNORE INTO policy_versions VALUES (1,?1,?2,?3,'compiled-bootstrap','compiled local-fake-v1','2026-11-14T00:00:00.000Z')",params![p.name,store::digest(&raw),raw])?;
    tx.execute("INSERT OR IGNORE INTO policy_active VALUES(1,1)", [])?;
    // Legacy cases keep their exact development profile and original persisted deadlines.
    tx.execute(
        "INSERT OR IGNORE INTO case_policy SELECT id,1 FROM cases",
        [],
    )?;
    Ok(())
}
pub(crate) fn bind(tx: &Transaction<'_>, id: &str) -> Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO case_policy SELECT ?1,revision FROM policy_active WHERE singleton=1",
        [id],
    )?;
    Ok(())
}
pub(crate) fn for_case(db: &Connection, id: &str) -> Result<Policy> {
    let (raw,digest):(String,String)=db.query_row("SELECT p.record,p.digest FROM policy_versions p JOIN case_policy c ON c.revision=p.revision WHERE c.case_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("governing policy integrity failure".into()));
    }
    let p: Policy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(p)
}
impl Application {
    /// OS database access is operator authority in this fake-only deployment. Never expose to agents.
    pub fn install_policy(&self, policy: Policy, operator: &str, source: &str) -> Result<Policy> {
        text(operator, "trusted operator", 128)?;
        text(source, "configuration source", 4096)?;
        policy.validate()?;
        if operator.starts_with("fake-") {
            return Err(Error::Invalid(
                "fake agents cannot install their governing policy".into(),
            ));
        }
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw = serde_json::to_string(&policy)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT record FROM policy_versions WHERE revision=?1",
                [policy.revision],
                |r| r.get(0),
            )
            .optional()?;
        let active: i64 = tx.query_row(
            "SELECT revision FROM policy_active WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        if let Some(old) = existing {
            if old != raw || policy.revision != active {
                return Err(Error::Conflict(
                    "policy revisions are immutable and cannot roll back".into(),
                ));
            }
            return Ok(policy);
        }
        if policy.revision != active + 1 {
            return Err(Error::Conflict(
                "policy revision must advance exactly once".into(),
            ));
        }
        tx.execute(
            "INSERT INTO policy_versions VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                policy.revision,
                policy.name,
                store::digest(&raw),
                raw,
                operator,
                source,
                self.now()
            ],
        )?;
        tx.execute(
            "UPDATE policy_active SET revision=?1 WHERE singleton=1",
            [policy.revision],
        )?;
        tx.commit()?;
        Ok(policy)
    }
}
