use super::*;
use crate::contracts::V1;
use chrono::{Duration, SecondsFormat};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    NormalizedSource,
    ExecutionEvidence,
    ReportedEvidence,
    SupervisorCheckpoint,
    Simulation,
    Other,
}
const CLASSES: [Class; 6] = [
    Class::NormalizedSource,
    Class::ExecutionEvidence,
    Class::ReportedEvidence,
    Class::SupervisorCheckpoint,
    Class::Simulation,
    Class::Other,
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub paused: bool,
    pub interval_seconds: u32,
    /// An explicit null keeps the class indefinitely; no omitted class default.
    pub retain_seconds: BTreeMap<Class, Option<u32>>,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        if self.revision < 1
            || !(1..=86400).contains(&self.interval_seconds)
            || self.retain_seconds.len() != 6
            || CLASSES.iter().any(|c| !self.retain_seconds.contains_key(c))
            || self
                .retain_seconds
                .values()
                .flatten()
                .any(|v| !(1..=315_360_000).contains(v))
        {
            return Err(Error::Invalid("retention policy requires six explicit class ages (null keeps indefinitely), 1..315360000-second ages and 1..86400-second interval".into()));
        }
        Ok(())
    }
}
fn policy(db: &Connection) -> Result<Option<Policy>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM retention_policies ORDER BY revision DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(raw, sum)| {
        if store::digest(&raw) != sum {
            return Err(Error::Conflict("retention policy integrity failure".into()));
        }
        let p: Policy = serde_json::from_str(&raw)?;
        p.validate()?;
        Ok(p)
    })
    .transpose()
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub policy: Option<Policy>,
    pub owner: Option<String>,
    pub unaged_expiring_rows: i64,
    pub restored_authority_held: bool,
    pub next_at: String,
    pub last_at: Option<String>,
    pub last_digest: Option<String>,
    pub pending_by_class: Vec<Value>,
    pub removed_bodies: i64,
    pub deferred_failures: i64,
    pub unknown_time_rows: i64,
    pub limitation: String,
}
const TIME_PATTERN: &str =
    "[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9].[0-9][0-9][0-9]Z";
impl Application {
    pub fn install_retention_policy(&self, p: Policy, who: &str, source: &str) -> Result<String> {
        validate_operator(who, "Install explicit OS retention authority")?;
        text(source, "retention configuration source", 4096)?;
        p.validate()?;
        let raw = serde_json::to_string(&p)?;
        let sum = store::digest(&raw);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT digest FROM retention_policies WHERE revision=?1",
                [p.revision],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            return if old == sum {
                Ok(sum)
            } else {
                Err(Error::Conflict("retention revisions are immutable".into()))
            };
        }
        let last: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM retention_policies",
            [],
            |r| r.get(0),
        )?;
        if p.revision != last + 1 {
            return Err(Error::Conflict(
                "retention revision must advance consecutively".into(),
            ));
        }
        tx.execute(
            "INSERT INTO retention_policies VALUES (?1,?2,?3,?4,?5,?6)",
            params![p.revision, raw, sum, who, source, self.now()],
        )?;
        let last_at: Option<String> = tx.query_row(
            "SELECT last_at FROM retention_schedule WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let next = last_at
            .as_ref()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map(|at| {
                (at + Duration::seconds(i64::from(p.interval_seconds)))
                    .to_rfc3339_opts(SecondsFormat::Millis, true)
            })
            .unwrap_or_default();
        tx.execute(
            "UPDATE retention_schedule SET next_at=?1 WHERE id=1",
            [next],
        )?;
        tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[json!({"type":"retention_policy_installed","revision":p.revision,"digest":sum,"operator":who,"source":source,"at":self.now()}).to_string()])?;
        tx.commit()?;
        Ok(sum)
    }
    pub fn retention_status(&self) -> Result<Status> {
        let db = self.store.connect()?;
        let p = policy(&db)?;
        let held = p
            .as_ref()
            .map(|p| crate::restore::fenced(&db, "retention", p.revision))
            .transpose()?
            .unwrap_or(false);
        let (next_at, last_at, last_digest): (String, Option<String>, Option<String>) = db
            .query_row(
                "SELECT next_at,last_at,last_digest FROM retention_schedule WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        for at in std::iter::once(Some(&next_at))
            .chain(std::iter::once(last_at.as_ref()))
            .flatten()
        {
            if !at.is_empty() && chrono::DateTime::parse_from_rfc3339(at).is_err() {
                return Err(Error::Conflict(
                    "retention schedule timestamp integrity failure".into(),
                ));
            }
        }
        let mut q=db.prepare("SELECT p.class,COUNT(*),COALESCE(SUM(length(CAST(e.content AS BLOB))),0),MIN(p.recorded_at) FROM retention_pending p JOIN evidence e ON e.digest=p.digest GROUP BY p.class ORDER BY p.class")?;
        let pending_by_class=q.query_map([],|r|Ok(json!({"class":r.get::<_,String>(0)?,"bodies":r.get::<_,i64>(1)?,"bytes":r.get::<_,i64>(2)?,"oldest_recorded_at":r.get::<_,String>(3)?})))?.collect::<std::result::Result<_,_>>()?;
        let owner = p
            .as_ref()
            .map(|p| {
                db.query_row(
                    "SELECT operator FROM retention_policies WHERE revision=?1",
                    [p.revision],
                    |r| r.get::<_, String>(0),
                )
            })
            .transpose()?;
        let classes: Vec<_> = p
            .as_ref()
            .map(|p| {
                p.retain_seconds
                    .iter()
                    .filter_map(|(c, age)| age.map(|_| *c))
                    .collect()
            })
            .unwrap_or_default();
        let unaged_expiring_rows=db.query_row("SELECT COUNT(*) FROM retention_pending p JOIN json_each(?1) c ON c.value=p.class WHERE p.recorded_at NOT GLOB ?2",params![serde_json::to_string(&classes)?,TIME_PATTERN],|r|r.get(0))?;
        Ok(Status{owner,unaged_expiring_rows,policy:p,restored_authority_held:held,next_at,last_at,last_digest,pending_by_class,removed_bodies:db.query_row("SELECT COUNT(*) FROM evidence_redactions",[],|r|r.get(0))?,deferred_failures:db.query_row("SELECT COUNT(*) FROM retention_failures",[],|r|r.get(0))?,unknown_time_rows:db.query_row("SELECT COUNT(*) FROM retention_pending WHERE recorded_at NOT GLOB ?1",[TIME_PATTERN],|r|r.get(0))?,limitation:"OS-configured evidence-store body expiry only; one body per persisted interval. Null retains a class indefinitely. Canonical recorded time is required; unknown legacy ages remain for OS review. Invalidation and original stop/cleanup duties are preserved. Other derived/formal/job/journal/export/backup copies are separate unfinished scopes. Failure metadata contains no raw evidence; no physical erasure or live cleanup is implied.".into()})
    }
    /// One bounded evidence-store expiry. A failed body is deferred so it cannot
    /// starve another class or stop the model-independent coordinator duties.
    pub fn retention_tick(&self) -> Result<Option<Tombstone>> {
        let mut db = self.store.connect()?;
        db.pragma_update(None, "secure_delete", true)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(p) = policy(&tx)? else {
            return Ok(None);
        };
        if p.paused || crate::restore::fenced(&tx, "retention", p.revision)? {
            return Ok(None);
        }
        let observed = self.clock.now();
        let now = observed.to_rfc3339_opts(SecondsFormat::Millis, true);
        let next: String = tx.query_row(
            "SELECT next_at FROM retention_schedule WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if !next.is_empty() && chrono::DateTime::parse_from_rfc3339(&next).is_err() {
            return Err(Error::Conflict(
                "retention schedule timestamp integrity failure".into(),
            ));
        }
        if next > now {
            return Ok(None);
        }
        let behind: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM factory_state WHERE high_at>?1)",
            [&now],
            |r| r.get(0),
        )?;
        if behind {
            return Ok(None);
        }
        let mut cutoffs = BTreeMap::new();
        for (class, age) in &p.retain_seconds {
            if let Some(age) = age {
                let cutoff = observed
                    .checked_sub_signed(Duration::seconds(i64::from(*age)))
                    .ok_or_else(|| {
                        Error::Invalid("retention cutoff outside supported clock".into())
                    })?;
                cutoffs.insert(*class, cutoff.to_rfc3339_opts(SecondsFormat::Millis, true));
            }
        }
        let row:Option<(String,String)>=tx.query_row("SELECT i.digest,i.recorded_at FROM json_each(?1) rule CROSS JOIN retention_pending i ON i.class=rule.key AND i.recorded_at<=rule.value WHERE i.recorded_at GLOB ?2 AND NOT EXISTS(SELECT 1 FROM retention_failures f WHERE f.digest=i.digest AND f.next_at>?3) ORDER BY i.recorded_at,i.digest LIMIT 1",params![serde_json::to_string(&cutoffs)?,TIME_PATTERN,now],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let scheduled = (observed + Duration::seconds(i64::from(p.interval_seconds)))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        tx.execute(
            "UPDATE retention_schedule SET next_at=?1,last_at=?2 WHERE id=1",
            params![scheduled, now],
        )?;
        let Some((sum, recorded_at)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        let who: String = tx.query_row(
            "SELECT operator FROM retention_policies WHERE revision=?1",
            [p.revision],
            |r| r.get(0),
        )?;
        let result = if chrono::DateTime::parse_from_rfc3339(&recorded_at).is_err() {
            Err(Error::Invalid("invalid retained evidence time".into()))
        } else {
            remove(
                &tx,
                &sum,
                None,
                &who,
                &format!(
                    "Scheduled evidence-store expiry under OS retention policy {}",
                    p.revision
                ),
                &now,
            )
        };
        match result {
            Ok(t) => {
                tx.execute(
                    "UPDATE retention_schedule SET last_digest=?1 WHERE id=1",
                    [&sum],
                )?;
                tx.execute("DELETE FROM retention_failures WHERE digest=?1", [&sum])?;
                tx.commit()?;
                Ok(Some(t))
            }
            Err(error) => {
                drop(tx); // body, effects, proof epochs and audit all roll back
                let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let later =
                    (observed + Duration::seconds(60)).to_rfc3339_opts(SecondsFormat::Millis, true);
                tx.execute("INSERT INTO retention_failures VALUES (?1,?2,?3,?4,1,?5) ON CONFLICT(digest) DO UPDATE SET policy_revision=excluded.policy_revision,failed_at=excluded.failed_at,next_at=excluded.next_at,attempts=attempts+1,code=excluded.code",params![sum,p.revision,now,later,match error{Error::Backpressure(_)=>"backpressure",Error::Storage(_)=>"storage",_=>"integrity_or_context"}])?;
                tx.execute(
                    "UPDATE retention_schedule SET next_at=?1,last_at=?2 WHERE id=1",
                    params![scheduled, now],
                )?;
                tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[json!({"type":"retention_deferred","digest":sum,"policy_revision":p.revision,"at":now,"next_at":later,"action":"OS operator must inspect retained evidence/audit; other bodies and deterministic duties continue"}).to_string()])?;
                tx.commit()?;
                Ok(None)
            }
        }
    }
}
