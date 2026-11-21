//! Read-only current-monitor intake, independently mapped and reconciled without models.
use crate::{
    application::{Application, operator, text},
    contracts::*,
    error::{Error, Result},
    sentry::{Signal, SourceObservation},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub service: String,
    pub environment: String,
    pub repository: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: V1,
    pub revision: i64,
    pub profile: String,
    pub site: String,
    pub api_key_env: String,
    pub application_key_env: String,
    pub refresh_seconds: u32,
    pub request_seconds: u32,
    pub incident_update_seconds: BTreeMap<String, u32>,
    pub monitors: BTreeMap<u64, Mapping>,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.revision < 1
            || !crate::operations::ident(&self.profile)
            || ![
                "datadoghq.com",
                "datadoghq.eu",
                "us3.datadoghq.com",
                "us5.datadoghq.com",
                "ap1.datadoghq.com",
                "ap2.datadoghq.com",
            ]
            .contains(&self.site.as_str())
            || !(1..=86400).contains(&self.refresh_seconds)
            || !(1..=30).contains(&self.request_seconds)
            || self.monitors.is_empty()
            || self.monitors.len() > 100
            || self.incident_update_seconds.len() != 3
            || ["critical", "high", "moderate"].iter().any(|s| {
                !self
                    .incident_update_seconds
                    .get(*s)
                    .is_some_and(|n| (1..=86400).contains(n))
            })
        {
            return Err(Error::Invalid(
                "Datadog profile exceeds bounded explicit scope".into(),
            ));
        }
        for s in [&self.api_key_env, &self.application_key_env] {
            if !s.starts_with("WATCHHOUSE_DATADOG_")
                || s.len() > 128
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(Error::Invalid(
                    "dedicated Datadog credential references required".into(),
                ));
            }
        }
        for (id, m) in &self.monitors {
            if *id == 0 || *id > i64::MAX as u64 {
                return Err(Error::Invalid("monitor id out of range".into()));
            }
            for (s, n) in [
                (&m.service, "service"),
                (&m.environment, "environment"),
                (&m.repository, "repository"),
            ] {
                text(s, n, 128)?;
            }
            if !m.repository.split_once('/').is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && [a, b].iter().all(|s| {
                        ![".", ".."].contains(s)
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                    })
            }) {
                return Err(Error::Invalid(
                    "explicit repository mapping required".into(),
                ));
            }
        }
        Ok(())
    }
}
pub(crate) fn config(db: &Connection, profile: &str, revision: Option<i64>) -> Result<Config> {
    let row:Option<(String,String)>=db.query_row("SELECT record,digest FROM datadog_configs WHERE profile=?1 AND revision=COALESCE(?2,(SELECT MAX(revision) FROM datadog_configs WHERE profile=?1))",params![profile,revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict("Datadog config integrity failure".into()));
    }
    let c: Config = serde_json::from_str(&raw)?;
    c.validate()?;
    Ok(c)
}
pub(crate) fn active_revision(db: &Connection, key: &str) -> Result<Option<i64>> {
    let Some((profile, id)) = key.strip_prefix("datadog:").and_then(|s| s.split_once('/')) else {
        return Ok(None);
    };
    Ok(db
        .query_row(
            "SELECT revision FROM datadog_monitors WHERE profile=?1 AND monitor_id=?2",
            params![profile, id],
            |r| r.get(0),
        )
        .optional()?)
}
/// Adapter-owned translation preserves provider-independent incident timers.
pub(crate) fn update_seconds(
    db: &Connection,
    source: &SourceObservation,
    kind: &str,
) -> Result<u32> {
    let intervals = if let Some((profile, _)) = source
        .source_key
        .strip_prefix("datadog:")
        .and_then(|s| s.split_once('/'))
    {
        config(db, profile, Some(source.configuration_revision))?.incident_update_seconds
    } else {
        let raw: String = db.query_row(
            "SELECT record FROM source_configs WHERE revision=?1",
            [source.configuration_revision],
            |r| r.get(0),
        )?;
        serde_json::from_str::<crate::sentry::SentryConfig>(&raw)?.incident_update_seconds
    };
    intervals
        .get(kind)
        .copied()
        .ok_or_else(|| Error::Invalid("missing trusted severity profile".into()))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub restore_held: bool,
    pub reads: Vec<crate::sentry_refresh::SourceFreshness>,
    pub config: Config,
    pub capabilities: Vec<String>,
    pub api_credentials_present: bool,
    pub installed_by: String,
    pub installed_at: String,
    pub configuration_source: String,
}
pub trait Api {
    fn monitor(&self, c: &Config, id: u64) -> Result<Value>;
}
pub struct Native;
impl Api for Native {
    fn monitor(&self, c: &Config, id: u64) -> Result<Value> {
        c.validate()?;
        if !c.monitors.contains_key(&id) {
            return Err(Error::Invalid("unmapped monitor".into()));
        }
        let key = std::env::var(&c.api_key_env)
            .map_err(|_| Error::Invalid("Datadog API credential missing".into()))?;
        let app = std::env::var(&c.application_key_env)
            .map_err(|_| Error::Invalid("Datadog application credential missing".into()))?;
        if [&key, &app]
            .iter()
            .any(|s| s.is_empty() || s.len() > 4096 || s.bytes().any(|b| !b.is_ascii_graphic()))
        {
            return Err(Error::Invalid("Datadog credentials unavailable".into()));
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(std::time::Duration::from_secs(c.request_seconds.into()))
            .build()
            .map_err(|_| Error::Invalid("Datadog client unavailable".into()))?;
        let response = client
            .get(format!("https://api.{}/api/v1/monitor/{id}", c.site))
            .header("DD-API-KEY", key)
            .header("DD-APPLICATION-KEY", app)
            .header("User-Agent", "Watchhouse-datadog-v1")
            .query(&[("with_downtimes", "false")])
            .send()
            .map_err(|_| Error::Invalid("Datadog read unavailable".into()))?;
        if !response.status().is_success() {
            return Err(Error::Invalid("Datadog read denied or unavailable".into()));
        }
        let mut bytes = vec![];
        response
            .take(262145)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Invalid("Datadog response lost".into()))?;
        if bytes.len() > 262144 {
            return Err(Error::Invalid("Datadog response exceeds bound".into()));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}
fn normalize(c: &Config, id: u64, raw: &Value, now: &str) -> Result<SourceObservation> {
    let m = c.monitors.get(&id).ok_or(Error::NotFound)?;
    if raw["id"].as_u64() != Some(id)
        || !raw["overall_state"].as_str().is_some_and(|s| {
            [
                "Alert", "Warn", "No Data", "OK", "Ignored", "Skipped", "Unknown",
            ]
            .contains(&s)
        })
    {
        return Err(Error::Invalid(
            "wrong monitor or unsupported Datadog state".into(),
        ));
    }
    let title = raw["name"]
        .as_str()
        .ok_or_else(|| Error::Invalid("monitor name unavailable".into()))?;
    text(title, "monitor name", 500)?;
    Ok(SourceObservation{source_key:format!("datadog:{}/{id}",c.profile),signal:Signal{issue_id:id.to_string(),project_id:c.profile.clone(),title:title.into(),status:raw["overall_state"].as_str().unwrap().into(),last_seen:now.into(),count:None},authoritative:true,observed_at:now.into(),configuration_revision:c.revision,service:m.service.clone(),environment:m.environment.clone(),repository:m.repository.clone(),release:None,deployed_revision:None,exceptions:vec![],diagnostic:None,uncertainty:"Datadog current monitor snapshot, not an error issue. Snapshot time is not failure onset; transition history, release/deployment, request denominator, cause and recovery unknown. Query/message/tags/user data excluded.".into()})
}
fn facts(o: &SourceObservation) -> Result<Value> {
    let mut v = serde_json::to_value(o)?;
    v.as_object_mut().unwrap().remove("observed_at");
    v["signal"].as_object_mut().unwrap().remove("last_seen");
    Ok(v)
}
impl Application {
    pub fn configure_datadog(&self, c: Config, who: &str, source: &str) -> Result<Config> {
        c.validate()?;
        crate::incident_ops::human(who)?;
        text(source, "configuration source", 4096)?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let last: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM datadog_configs",
            [],
            |r| r.get(0),
        )?;
        let raw = serde_json::to_string(&c)?;
        if c.revision <= last {
            if serde_json::to_string(&config(&tx, &c.profile, Some(c.revision))?)? == raw {
                return Ok(c);
            }
            return Err(Error::Conflict("Datadog config immutable".into()));
        }
        if c.revision != last + 1 {
            return Err(Error::Conflict(
                "consecutive global Datadog config revision required".into(),
            ));
        }
        let profiles: i64 = tx.query_row(
            "SELECT COUNT(DISTINCT profile) FROM datadog_configs",
            [],
            |r| r.get(0),
        )?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM datadog_configs WHERE profile=?1)",
            [&c.profile],
            |r| r.get(0),
        )?;
        if !exists && profiles >= 20 {
            return Err(Error::Invalid("Datadog profile capacity exceeded".into()));
        }
        tx.execute(
            "INSERT INTO datadog_configs VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                c.revision,
                c.profile,
                store::digest(&raw),
                raw,
                who,
                source,
                self.now()
            ],
        )?;
        tx.execute(
            "DELETE FROM datadog_monitors WHERE profile=?1",
            [&c.profile],
        )?;
        for id in c.monitors.keys() {
            tx.execute(
                "INSERT INTO datadog_monitors VALUES (?1,?2,?3,0,?4)",
                params![c.profile, *id as i64, c.revision, self.now()],
            )?;
        }
        tx.commit()?;
        Ok(c)
    }
    pub fn datadog_status(&self) -> Result<Vec<Status>> {
        let db = self.store.connect()?;
        let mut q=db.prepare("SELECT profile,operator,recorded_at,source FROM datadog_configs c WHERE revision=(SELECT MAX(revision) FROM datadog_configs d WHERE d.profile=c.profile) ORDER BY profile LIMIT 20")?;
        q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.map(|r|{let (profile,installed_by,installed_at,configuration_source)=r?;let config=config(&db,&profile,None)?;let restore_held=crate::restore::fenced(&db,"datadog",config.revision)?;let present=|s:&str|std::env::var(s).is_ok_and(|s|!s.is_empty());Ok(Status{restore_held,reads:config.monitors.keys().map(|id|crate::sentry_refresh::health(&db,&format!("datadog:{}/{id}",config.profile),self.clock.now())).collect::<Result<_>>()?,api_credentials_present:!restore_held&&present(&config.api_key_env)&&present(&config.application_key_env),config,installed_by,installed_at,configuration_source,capabilities:vec!["read-only scoped current monitor snapshots; fair periodic reconciliation".into(),"polling intake; no webhook or historical-transition retention".into(),"no error-event equivalence, metric denominator, release or deployment authority".into()]})}).collect()
    }
    /// Reserve one due read and rotate its next time before I/O, including during an outage.
    pub fn sync_datadog(&self, api: &dyn Api) -> Result<bool> {
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let row:Option<(String,u64,i64,i64)>=tx.query_row("SELECT profile,monitor_id,revision,generation FROM datadog_monitors d WHERE next_at<=?1 AND NOT EXISTS(SELECT 1 FROM restore_fences f WHERE f.kind='datadog' AND f.identity=CAST(d.revision AS TEXT)) ORDER BY next_at,profile,monitor_id LIMIT 1",[self.now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((profile, id, revision, generation)) = row else {
            return Ok(false);
        };
        let c = config(&tx, &profile, Some(revision))?;
        let key = format!("datadog:{profile}/{id}");
        let generation = generation + 1;
        let next = (self.clock.now() + Duration::seconds(c.refresh_seconds.into()))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        tx.execute("UPDATE datadog_monitors SET generation=?1,next_at=?2 WHERE profile=?3 AND monitor_id=?4",params![generation,next,profile,id as i64])?;
        tx.execute("INSERT INTO datadog_reads VALUES (?1,?2,?3,?4,?5,NULL,NULL,NULL) ON CONFLICT(source_key) DO UPDATE SET generation=excluded.generation,configuration_revision=excluded.configuration_revision,started_at=excluded.started_at,next_at=excluded.next_at",params![key,generation,revision,self.now(),next])?;
        tx.commit()?;
        let observed_at = self.now();
        let result = api
            .monitor(&c, id)
            .and_then(|v| normalize(&c, id, &v, &observed_at));
        // API/auth/outage errors remain factory health, not fabricated source observations.
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM datadog_monitors WHERE profile=?1 AND monitor_id=?2 AND revision=?3 AND generation=?4)",params![profile,id as i64,revision,generation],|r|r.get(0))?;
        if !active {
            tx.commit()?;
            return Ok(true);
        }
        if result.is_err() {
            tx.execute("UPDATE datadog_reads SET error='Datadog current-state read unavailable; silence is not health' WHERE source_key=?1 AND generation=?2",params![key,generation])?;
            tx.commit()?;
            return Ok(true);
        }
        // Case intake uses its own idempotent transaction; provider reservation is checked again below.
        tx.commit()?;
        let o = result?;
        let case = self.create_case(CreateCase {
            schema_version: V1,
            idempotency_key: store::digest(&format!("source-case:{key}")),
            actor: "operator".into(),
            reason:
                "Datadog observation requires qualification, not automatic incident declaration"
                    .into(),
            title: format!("Datadog monitor {id}"),
            description:
                "Current monitor snapshot; no error grouping, traffic count or recovery conclusion"
                    .into(),
        })?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM datadog_monitors WHERE profile=?1 AND monitor_id=?2 AND revision=?3 AND generation=?4)",params![profile,id as i64,revision,generation],|r|r.get(0))?;
        if !active {
            tx.commit()?;
            return Ok(true);
        }
        let old = crate::sentry::snapshot(&tx, &case.id)?;
        if old.as_ref().map(facts).transpose()?.as_ref() != Some(&facts(&o)?) {
            let raw = serde_json::to_string(&o)?;
            let digest = store::digest(&raw);
            tx.execute("INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,'datadog-monitor-api-v1',?3)",params![digest,raw,self.now()])?;
            tx.execute(
                "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
                params![case.id, digest],
            )?;
            tx.execute("INSERT INTO source_cases VALUES (?1,?2,?3,?4) ON CONFLICT(source_key) DO UPDATE SET record=excluded.record,digest=excluded.digest",params![key,case.id,raw,digest])?;
            store::emit(
                &tx,
                &case.id,
                operator("datadog-adapter"),
                &format!("datadog-read:{key}:{generation}"),
                &self.now(),
                EventData::SourceObserved {
                    evidence_refs: vec![digest],
                    source_key: key.clone(),
                    authoritative: true,
                    reason: o.uncertainty,
                },
            )?;
        }
        tx.execute("UPDATE datadog_reads SET last_success_at=?1,last_success_generation=?2,error=NULL WHERE source_key=?3 AND generation=?2",params![self.now(),generation,key])?;
        tx.commit()?;
        Ok(true)
    }
}
