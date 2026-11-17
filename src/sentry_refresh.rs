//! Fenced read snapshots and bounded refresh of known issues outside discovery windows.
use crate::{
    application::{Application, operator},
    contracts::*,
    error::{Error, Result},
    sentry::{self, Enrichment, SentryApi, SentryConfig, Signal, SourceObservation},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFreshness {
    pub source_key: String,
    pub state: String,
    pub generation: i64,
    pub configuration_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_configuration_revision: Option<i64>,
    pub started_at: Option<String>,
    pub next_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_success_generation: Option<i64>,
    pub age_seconds: Option<i64>,
    pub error: Option<String>,
}
fn key(cfg: &SentryConfig, id: &str) -> String {
    format!("sentry:{}/{}/{id}", cfg.organization, cfg.project_id)
}
fn next(a: &Application, seconds: u32) -> String {
    (a.clock.now() + Duration::seconds(seconds.into())).to_rfc3339_opts(SecondsFormat::Millis, true)
}
fn claim(a: &Application, cfg: &SentryConfig, key: &str) -> Result<i64> {
    let mut db = a.store.connect()?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("INSERT INTO source_read_state VALUES (?1,1,?2,?3,?4,NULL,NULL,NULL) ON CONFLICT(source_key) DO UPDATE SET generation=generation+1,configuration_revision=excluded.configuration_revision,started_at=excluded.started_at,next_at=excluded.next_at",params![key,cfg.revision,a.now(),next(a,cfg.refresh_seconds)])?;
    let gen_id = tx.query_row(
        "SELECT generation FROM source_read_state WHERE source_key=?1",
        [key],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok(gen_id)
}
fn status(a: &Application, key: &str, generation: i64, error: Option<&str>) -> Result<()> {
    a.store.connect()?.execute(
        "UPDATE source_read_state SET error=?1 WHERE source_key=?2 AND generation=?3",
        params![error, key, generation],
    )?;
    Ok(())
}
fn observation(
    cfg: &SentryConfig,
    signal: Signal,
    event: Value,
    authoritative: bool,
    observed_at: String,
) -> SourceObservation {
    let release = event
        .get("release")
        .and_then(|r| r.as_str().or_else(|| r.get("version")?.as_str()))
        .filter(|s| s.len() <= 128)
        .map(str::to_owned);
    let exceptions = event
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|e| e.get("type").and_then(Value::as_str) == Some("exception"))
        .flat_map(|e| {
            e.pointer("/data/values")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|v| v.get("type").and_then(Value::as_str))
        .filter(|s| s.len() <= 128)
        .take(10)
        .map(str::to_owned)
        .collect();
    SourceObservation{source_key:key(cfg,&signal.issue_id),signal,authoritative,observed_at,configuration_revision:cfg.revision,service:cfg.service.clone(),environment:cfg.environment.clone(),repository:cfg.repository.clone(),deployed_revision:release.as_ref().and_then(|r|cfg.releases.get(r)).cloned(),release,exceptions,diagnostic:crate::source_context::normalize(&event),uncertainty:if authoritative{"API snapshot; traffic denominator and recovery are unknown. Representative event fields may be unavailable."}else{"Enrichment unavailable; webhook status is an unverified hint, not current state. No cause or recovery inferred."}.into()}
}
fn validated(cfg: &SentryConfig, expected: &str, e: Enrichment) -> Result<(Signal, Value)> {
    let signal = sentry::normalize(&e.issue, cfg)?;
    if signal.issue_id != expected {
        return Err(Error::Invalid("enrichment returned another issue".into()));
    }
    let event =
        if e.event.get("environment").and_then(Value::as_str) == Some(cfg.environment.as_str()) {
            e.event
        } else {
            Value::Null
        };
    Ok((signal, event))
}
fn facts(o: &SourceObservation) -> Result<Value> {
    let mut v = serde_json::to_value(o)?;
    v.as_object_mut().unwrap().remove("observed_at");
    Ok(v)
}
fn persist(
    a: &Application,
    cfg: &SentryConfig,
    o: SourceObservation,
    generation: i64,
    delivery: Option<&str>,
) -> Result<usize> {
    let source = &o.source_key;
    let case = a.create_case(CreateCase {
        schema_version: V1,
        idempotency_key: store::digest(&format!("source-case:{source}")),
        actor: "operator".into(),
        reason:
            "Authenticated observation requires qualification, not automatic incident declaration"
                .into(),
        title: format!("Sentry issue {}", o.signal.issue_id),
        description: format!(
            "{source}. Imported source data is untrusted; source status is not recovery evidence."
        ),
    })?;
    let mut db = a.store.connect()?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: i64 = tx.query_row(
        "SELECT generation FROM source_read_state WHERE source_key=?1",
        [source],
        |r| r.get(0),
    )?;
    let active = current == generation && crate::sentry_projects::revision_active(&tx, cfg.revision)?;
    let old = sentry::snapshot(&tx, &case.id)?;
    let projectable = active
        && old.as_ref().is_none_or(|v| {
            (!v.authoritative || o.authoritative) && v.signal.last_seen <= o.signal.last_seen
        });
    let same = projectable && old.as_ref().map(facts).transpose()?.as_ref() == Some(&facts(&o)?);
    let mut changed = 0;
    if !same {
        let content = if projectable {
            serde_json::to_string(&o)?
        } else {
            serde_json::to_string(
                &json!({"observation":facts(&o)?,"read_generation":if active {None}else{Some(generation)},"acceptance":if active {"unaccepted-old-or-unverified"}else{"stale-source-read"}}),
            )?
        };
        let digest = store::digest(&content);
        let provenance = match (o.authoritative, projectable) {
            (true, true) => "sentry-api-v1",
            (false, true) => "sentry-webhook-v1",
            (_, false) => "sentry-v1:unaccepted-source-read",
        };
        tx.execute(
            "INSERT OR IGNORE INTO evidence VALUES (?1,'application/json',?2,?3,?4)",
            params![digest, content, provenance, a.now()],
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO case_evidence VALUES (?1,?2)",
            params![case.id, digest],
        )?;
        if projectable {
            tx.execute("INSERT INTO source_cases VALUES (?1,?2,?3,?4) ON CONFLICT(source_key) DO UPDATE SET record=excluded.record,digest=excluded.digest",params![source,case.id,serde_json::to_string(&o)?,digest])?;
        }
        if inserted > 0 {
            store::emit(
                &tx,
                &case.id,
                operator("source-adapter"),
                &delivery
                    .map(str::to_owned)
                    .unwrap_or_else(|| store::digest(&format!("refresh:{source}:{generation}"))),
                &a.now(),
                EventData::SourceObserved {
                    evidence_refs: vec![digest],
                    source_key: source.clone(),
                    authoritative: o.authoritative && projectable,
                    reason: if !active {
                        "Late read retained as unaccepted evidence; a newer read or project configuration owns source projection".into()
                    } else if !projectable {
                        "Older or unverified read cannot replace the retained API snapshot".into()
                    } else {
                        o.uncertainty.clone()
                    },
                },
            )?;
            changed += 1;
        }
    }
    if active {
        let usable = o.authoritative && projectable;
        tx.execute("UPDATE source_read_state SET last_success_at=CASE WHEN ?1 THEN ?2 ELSE last_success_at END,last_success_generation=CASE WHEN ?1 THEN ?3 ELSE last_success_generation END,error=CASE WHEN ?1 THEN NULL ELSE 'Enrichment unavailable or older than retained evidence; freshness incomplete' END WHERE source_key=?4 AND generation=?3",params![usable,a.now(),generation,source])?;
        if let Some(id) = delivery {
            tx.execute("UPDATE source_deliveries SET state=?1,next_at=?2 WHERE id=?3 AND state!='processed'",params![if usable{"processed"}else{"needs_enrichment"},next(a,cfg.refresh_seconds),id])?;
        }
    }
    tx.commit()?;
    Ok(changed)
}
pub(crate) fn intake(a: &Application, api: &dyn SentryApi) -> Result<usize> {
    let db = a.store.connect()?;
    let row:Option<(String,String,String,String)>=db.query_row("SELECT d.id,d.record,c.record,d.received_at FROM source_deliveries d JOIN source_configs c ON c.revision=d.revision WHERE d.state IN ('pending','needs_enrichment') AND d.next_at<=?1 ORDER BY d.next_at,d.rowid LIMIT 1",[a.now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((delivery, raw, cfg, received_at)) = row else {
        return Ok(0);
    };
    let hint: Signal = serde_json::from_str(&raw)?;
    let original: SentryConfig = serde_json::from_str(&cfg)?;
    let cfg = crate::sentry_projects::config_for(&db, &original.organization, &original.project_id)?
        .ok_or_else(|| Error::Conflict("delivery project configuration is unavailable".into()))?;
    let generation = claim(a, &cfg, &key(&cfg, &hint.issue_id))?;
    let observed_at = a.now();
    let (signal, event, authoritative) = match api
        .enrich(&cfg, &hint.issue_id)
        .and_then(|e| validated(&cfg, &hint.issue_id, e))
    {
        Ok((s, event)) if s.last_seen >= hint.last_seen => (s, event, true),
        _ => (hint, Value::Null, false),
    };
    let o = observation(
        &cfg,
        signal,
        event,
        authoritative,
        if authoritative {
            observed_at
        } else {
            received_at
        },
    );
    persist(a, &cfg, o, generation, Some(&delivery))
}
pub(crate) fn refresh_known(a: &Application, api: &dyn SentryApi) -> Result<usize> {
    let db = a.store.connect()?;
    let row:Option<(String,String)>=db.query_row("SELECT c.record,active.record FROM source_cases c JOIN source_configs old ON old.revision=json_extract(c.record,'$.configuration_revision') JOIN source_projects p ON p.organization=json_extract(old.record,'$.organization') AND p.project_id=json_extract(old.record,'$.project_id') JOIN source_configs active ON active.revision=p.revision LEFT JOIN source_read_state s ON s.source_key=c.source_key WHERE c.source_key LIKE 'sentry:%' AND (s.next_at IS NULL OR s.next_at<=?1 OR s.configuration_revision!=active.revision) ORDER BY CASE WHEN s.configuration_revision!=active.revision THEN 0 ELSE 1 END,COALESCE(s.next_at,''),c.rowid LIMIT 1",[a.now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((raw, profile)) = row else { return Ok(0) };
    let old: SourceObservation = serde_json::from_str(&raw)?;
    let cfg: SentryConfig = serde_json::from_str(&profile)?;
    cfg.validate()?;
    let source = key(&cfg, &old.signal.issue_id);
    if source != old.source_key {
        return Err(Error::Conflict("known issue project identity changed; no cross-project fetch is allowed".into()));
    }
    let generation = claim(a, &cfg, &source)?;
    let observed_at = a.now();
    let (signal, event) = match api
        .enrich(&cfg, &old.signal.issue_id)
        .and_then(|e| validated(&cfg, &old.signal.issue_id, e))
    {
        Ok(v) => v,
        Err(_) => {
            status(
                a,
                &source,
                generation,
                Some("Known issue refresh unavailable; last snapshot may be stale"),
            )?;
            return Ok(0);
        }
    };
    persist(
        a,
        &cfg,
        observation(&cfg, signal, event, true, observed_at),
        generation,
        None,
    )
}
type ReadRow = (
    i64,
    i64,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<String>,
);
fn health(db: &Connection, key: &str, now: DateTime<Utc>) -> Result<SourceFreshness> {
    let row:Option<ReadRow>=db.query_row("SELECT generation,configuration_revision,started_at,next_at,last_success_at,last_success_generation,error FROM source_read_state WHERE source_key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
    let Some((generation, revision, started, next, last_success, last_gen, error)) = row else {
        return Ok(SourceFreshness {
            source_key: key.into(),
            state: "untracked".into(),
            generation: 0,
            configuration_revision: None,
            active_configuration_revision: None,
            started_at: None,
            next_at: None,
            last_success_at: None,
            last_success_generation: None,
            age_seconds: None,
            error: Some(
                "No refresh ledger yet; existing observation freshness is not inferred".into(),
            ),
        });
    };
    let age = last_success
        .as_ref()
        .map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|t| (now - t.with_timezone(&Utc)).num_seconds().max(0))
                .map_err(|_| Error::Invalid("invalid source freshness timestamp".into()))
        })
        .transpose()?;
    let active_revision: Option<i64> = db.query_row("SELECT p.revision FROM source_configs c JOIN source_projects p ON p.organization=json_extract(c.record,'$.organization') AND p.project_id=json_extract(c.record,'$.project_id') WHERE c.revision=?1",[revision],|r|r.get(0)).optional()?;
    Ok(SourceFreshness {
        source_key: key.into(),
        state: if active_revision.is_none() {
            "configuration_unavailable"
        } else if active_revision != Some(revision) {
            "configuration_changed"
        } else if error.is_some() {
            "unavailable"
        } else if last_gen != Some(generation) {
            "refreshing_or_interrupted"
        } else if next.as_str() <= now.to_rfc3339_opts(SecondsFormat::Millis, true).as_str() {
            "refresh_due"
        } else {
            "current_snapshot"
        }
        .into(),
        generation,
        configuration_revision: Some(revision),
        active_configuration_revision: active_revision,
        started_at: Some(started),
        next_at: Some(next),
        last_success_at: last_success,
        last_success_generation: last_gen,
        age_seconds: age,
        error,
    })
}
pub(crate) fn snapshot(
    db: &Connection,
    case: &str,
    now: DateTime<Utc>,
) -> Result<Option<SourceFreshness>> {
    let key: Option<String> = db
        .query_row(
            "SELECT source_key FROM source_cases WHERE case_id=?1",
            [case],
            |r| r.get(0),
        )
        .optional()?;
    key.map(|k| health(db, &k, now)).transpose()
}
pub(crate) fn all_health(db: &Connection, now: DateTime<Utc>) -> Result<Vec<SourceFreshness>> {
    let mut stmt =
        db.prepare("SELECT source_key FROM source_cases ORDER BY rowid DESC LIMIT 200")?;
    let keys = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    keys.iter().map(|k| health(db, k, now)).collect()
}
