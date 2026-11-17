//! Trusted dependency mapping only permits scoped correlation review, never causal authority.
use crate::{
    application::{Application, text},
    contracts::V1,
    error::{Error, Result},
    sentry::SourceObservation,
    store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub service: String,
    pub environment: String,
    pub repository: String,
}
impl Scope {
    fn validate(&self) -> Result<()> {
        text(&self.service, "topology service", 128)?;
        text(&self.environment, "topology environment", 128)?;
        if !self.repository.split_once('/').is_some_and(|(a, b)| {
            [a, b].iter().all(|p| {
                !p.is_empty()
                    && p.len() <= 128
                    && ![".", ".."].contains(p)
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            })
        }) {
            return Err(Error::Invalid(
                "explicit topology repository mapping required".into(),
            ));
        }
        Ok(())
    }
    pub fn matches(&self, o: &SourceObservation) -> bool {
        self.service == o.service
            && self.environment == o.environment
            && self.repository == o.repository
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    pub caller: Scope,
    pub dependency: Scope,
    pub rationale: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub paused: bool,
    pub max_gap_seconds: u32,
    pub dependencies: Vec<Dependency>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub revision: i64,
    pub digest: String,
    pub paused: bool,
    pub dependencies: Vec<Dependency>,
    pub max_gap_seconds: u32,
}
impl Policy {
    fn validate(&self) -> Result<()> {
        if self.revision < 1
            || !(1..=3600).contains(&self.max_gap_seconds)
            || self.dependencies.len() > 64
        {
            return Err(Error::Invalid("bounded versioned topology required".into()));
        }
        let mut pairs = BTreeSet::new();
        for d in &self.dependencies {
            d.caller.validate()?;
            d.dependency.validate()?;
            text(&d.rationale, "dependency rationale", 2000)?;
            if d.caller.service == d.dependency.service {
                return Err(Error::Invalid(
                    "cross-service dependency must name distinct services".into(),
                ));
            }
            let pair = if d.caller < d.dependency {
                (&d.caller, &d.dependency)
            } else {
                (&d.dependency, &d.caller)
            };
            if !pairs.insert(pair) {
                return Err(Error::Invalid(
                    "duplicate or reversed dependency mapping".into(),
                ));
            }
        }
        Ok(())
    }
}
pub(crate) fn active(db: &Connection) -> Result<Option<View>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM service_topologies ORDER BY revision DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((raw, digest)) = row else {
        return Ok(None);
    };
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("service topology integrity failure".into()));
    }
    let p: Policy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(Some(View {
        revision: p.revision,
        digest,
        paused: p.paused,
        dependencies: p.dependencies,
        max_gap_seconds: p.max_gap_seconds,
    }))
}
pub(crate) fn edge<'a>(
    p: &'a View,
    a: &SourceObservation,
    b: &SourceObservation,
) -> Option<&'a Dependency> {
    if p.paused {
        return None;
    }
    p.dependencies.iter().find(|d| {
        (d.caller.matches(a) && d.dependency.matches(b))
            || (d.caller.matches(b) && d.dependency.matches(a))
    })
}
impl Application {
    pub fn service_topology(&self) -> Result<Option<View>> {
        active(&self.store.connect()?)
    }
    pub fn install_service_topology(&self, p: Policy, who: &str, source: &str) -> Result<View> {
        crate::incident_ops::human(who)?;
        text(source, "topology source", 4096)?;
        p.validate()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM service_topologies",
            [],
            |r| r.get(0),
        )?;
        let raw = serde_json::to_string(&p)?;
        if p.revision <= latest {
            let old: Option<String> = tx
                .query_row(
                    "SELECT record FROM service_topologies WHERE revision=?1",
                    [p.revision],
                    |r| r.get(0),
                )
                .optional()?;
            if old.as_deref() != Some(&raw) {
                return Err(Error::Conflict(
                    "service topology versions are immutable".into(),
                ));
            }
            return active(&tx)?.ok_or(Error::NotFound);
        }
        if p.revision != latest + 1 {
            return Err(Error::Conflict("topology must advance one revision".into()));
        }
        tx.execute(
            "INSERT INTO service_topologies VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                p.revision,
                store::digest(&raw),
                raw,
                who,
                source,
                self.now()
            ],
        )?;
        let view = active(&tx)?.ok_or(Error::NotFound)?;
        tx.commit()?;
        Ok(view)
    }
}
