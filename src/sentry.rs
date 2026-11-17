//! Authenticated Sentry v1 intake and bounded, resumable, read-only source processing.
use crate::{
    application::{Application, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentryConfig {
    pub schema_version: V1,
    pub revision: i64,
    pub organization: String,
    pub project_id: String,
    pub service: String,
    pub environment: String,
    pub repository: String,
    pub releases: BTreeMap<String, String>,
    pub incident_update_seconds: BTreeMap<String, u32>,
    pub webhook_secret_env: String,
    pub api_token_env: String,
    #[serde(
        default = "default_refresh",
        skip_serializing_if = "is_default_refresh"
    )]
    pub refresh_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_policy: Option<ReadPolicy>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadPolicy {
    pub schema_version: V1,
    pub request_seconds: u32,
    pub lookback_seconds: u32,
    pub overlap_seconds: u32,
    pub max_pages: u32,
}
impl SentryConfig {
    pub(crate) fn read_limits(&self) -> ReadPolicy {
        // Frozen v1 compatibility; explicit project installation requires its own read_policy.
        self.read_policy.unwrap_or(ReadPolicy {
            schema_version: V1,
            request_seconds: 10,
            lookback_seconds: 86400,
            overlap_seconds: 300,
            max_pages: 100,
        })
    }
}

fn default_refresh() -> u32 {
    60
}
fn is_default_refresh(value: &u32) -> bool {
    *value == 60
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn digits(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_digit())
}
impl SentryConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        let reads = self.read_limits();
        if !(1..=30).contains(&reads.request_seconds)
            || !(1..=2592000).contains(&reads.lookback_seconds)
            || reads.overlap_seconds > reads.lookback_seconds
            || reads.overlap_seconds > 86400
            || !(1..=100).contains(&reads.max_pages)
        {
            return Err(Error::Invalid("bounded versioned source read policy required".into()));
        }
        if !(1..=86400).contains(&self.refresh_seconds)
            || self.revision < 1
            || !slug(&self.organization)
            || !digits(&self.project_id)
            || !self
                .repository
                .split_once('/')
                .is_some_and(|(o, r)| slug(o) && slug(r))
            || self.releases.len() > 100
        {
            return Err(Error::Invalid(
                "invalid source identity or explicit repository mapping".into(),
            ));
        }
        text(&self.service, "service", 128)?;
        text(&self.environment, "environment", 128)?;
        if self.incident_update_seconds.len() != 3
            || ["critical", "high", "moderate"].iter().any(|k| {
                !self
                    .incident_update_seconds
                    .get(*k)
                    .is_some_and(|v| (1..=86400).contains(v))
            })
        {
            return Err(Error::Invalid(
                "bounded trusted severity update profiles required".into(),
            ));
        }
        for (release, revision) in &self.releases {
            text(release, "release", 128)?;
            if !(7..=64).contains(&revision.len())
                || !revision.bytes().all(|c| c.is_ascii_hexdigit())
            {
                return Err(Error::Invalid(
                    "release mapping needs an explicit commit digest".into(),
                ));
            }
        }
        for e in [&self.webhook_secret_env, &self.api_token_env] {
            if !e.starts_with("WATCHHOUSE_SENTRY_")
                || e.len() > 128
                || !e
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            {
                return Err(Error::Invalid(
                    "credentials must be trusted environment references".into(),
                ));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub issue_id: String,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub last_seen: String,
    pub count: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceObservation {
    pub source_key: String,
    pub signal: Signal,
    pub authoritative: bool,
    pub observed_at: String,
    pub configuration_revision: i64,
    pub service: String,
    pub environment: String,
    pub repository: String,
    pub release: Option<String>,
    pub deployed_revision: Option<String>,
    pub exceptions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<crate::source_context::DiagnosticContext>,
    pub uncertainty: String,
}
pub struct Enrichment {
    pub issue: Value,
    pub event: Value,
}
pub struct Page {
    pub issues: Vec<Value>,
    pub next_cursor: Option<String>,
}
pub trait SentryApi {
    fn enrich(&self, cfg: &SentryConfig, issue: &str) -> Result<Enrichment>;
    fn page(
        &self,
        cfg: &SentryConfig,
        start: &str,
        end: &str,
        cursor: Option<&str>,
    ) -> Result<Page>;
}
pub struct LiveSentry;
fn get(cfg: &SentryConfig, path: &str, query: &[(&str, &str)]) -> Result<(Value, Option<String>)> {
    let token = std::env::var(&cfg.api_token_env)
        .map_err(|_| Error::Invalid("Sentry API credential missing; activation blocked".into()))?;
    if token.trim().is_empty() {
        return Err(Error::Invalid(
            "Sentry API credential missing; activation blocked".into(),
        ));
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(cfg.read_limits().request_seconds.into()))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| Error::Invalid("Sentry client unavailable".into()))?;
    let response = client
        .get(format!("https://sentry.io/api/0/{path}"))
        .bearer_auth(token)
        .query(query)
        .send()
        .map_err(|_| Error::Invalid("Sentry API unavailable; coverage incomplete".into()))?;
    if !response.status().is_success() {
        return Err(Error::Invalid(format!(
            "Sentry API HTTP {}; coverage incomplete",
            response.status().as_u16()
        )));
    }
    let link = response
        .headers()
        .get("link")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let mut bytes = vec![];
    response
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Invalid("Sentry response interrupted".into()))?;
    if bytes.len() > 262144 {
        return Err(Error::Invalid("Sentry response exceeds 256KiB".into()));
    }
    Ok((serde_json::from_slice(&bytes)?, link))
}
fn cursor(link: Option<&str>) -> Result<Option<String>> {
    for part in link.unwrap_or("").split(',') {
        if part.contains("rel=\"next\"") && part.contains("results=\"true\"") {
            let value = part
                .split("cursor=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .ok_or_else(|| Error::Invalid("missing Sentry cursor".into()))?;
            if value.is_empty()
                || value.len() > 256
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'-' || b == b'_')
            {
                return Err(Error::Invalid("invalid Sentry cursor".into()));
            }
            return Ok(Some(value.into()));
        }
    }
    Ok(None)
}
impl SentryApi for LiveSentry {
    fn enrich(&self, cfg: &SentryConfig, issue: &str) -> Result<Enrichment> {
        if !digits(issue) {
            return Err(Error::Invalid("invalid issue ID".into()));
        }
        let base = format!("organizations/{}/issues/{issue}/", cfg.organization);
        let (issue, _) = get(cfg, &base, &[("environment", &cfg.environment)])?;
        let event = get(
            cfg,
            &format!("{base}events/latest/"),
            &[("environment", &cfg.environment)],
        )
        .map(|(v, _)| v)
        .unwrap_or(Value::Null);
        Ok(Enrichment { issue, event })
    }
    fn page(&self, cfg: &SentryConfig, start: &str, end: &str, c: Option<&str>) -> Result<Page> {
        let mut q = vec![
            ("project", cfg.project_id.as_str()),
            ("environment", cfg.environment.as_str()),
            ("query", ""),
            ("start", start),
            ("end", end),
            ("per_page", "50"),
        ];
        if let Some(c) = c {
            q.push(("cursor", c))
        }
        let (v, link) = get(
            cfg,
            &format!("organizations/{}/issues/", cfg.organization),
            &q,
        )?;
        let issues = v
            .as_array()
            .ok_or_else(|| Error::Invalid("issue page must be an array".into()))?
            .clone();
        if issues.len() > 100 {
            return Err(Error::Invalid("issue page exceeds limit".into()));
        }
        Ok(Page {
            issues,
            next_cursor: cursor(link.as_deref())?,
        })
    }
}
fn field<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid(format!("missing Sentry field {k}")))
}
pub(crate) fn normalize(v: &Value, cfg: &SentryConfig) -> Result<Signal> {
    let id = field(v, "id")?;
    let project = v
        .pointer("/project/id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("source project ID required".into()))?;
    if !digits(id) || project != cfg.project_id {
        return Err(Error::Invalid("source outside configured project".into()));
    }
    let title = field(v, "title")?;
    text(title, "source title", 200)?;
    let status = field(v, "status")?;
    if !matches!(status, "unresolved" | "resolved" | "ignored") {
        return Err(Error::Invalid("unknown source status".into()));
    }
    let last_seen = DateTime::parse_from_rfc3339(field(v, "lastSeen")?)
        .map_err(|_| Error::Invalid("source occurrence time must be RFC3339".into()))?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let count = v
        .get("count")
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()));
    Ok(Signal {
        issue_id: id.into(),
        project_id: project.into(),
        title: title.into(),
        status: status.into(),
        last_seen,
        count,
    })
}
pub(crate) fn config(db: &Connection) -> Result<Option<SentryConfig>> {
    crate::sentry_projects::default_config(db)
}
fn queue(tx: &Transaction<'_>, cfg: &SentryConfig, signal: &Signal, now: &str) -> Result<String> {
    let raw = serde_json::to_string(signal)?;
    let key = store::digest(&format!("{}:{}:{raw}", cfg.organization, cfg.revision));
    tx.execute("INSERT OR IGNORE INTO source_deliveries(id,revision,record,next_at,received_at) VALUES (?1,?2,?3,?4,?4)",params![key,cfg.revision,raw,now])?;
    Ok(key)
}
pub fn verify_signature(raw: &[u8], sig: &str, secret: &[u8]) -> bool {
    if sig.len() != 64 || !sig.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let expected: Vec<u8> = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&sig[i..i + 2], 16).unwrap())
        .collect();
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(raw);
    mac.verify_slice(&expected).is_ok()
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<Option<SourceObservation>> {
    let raw: Option<String> = db
        .query_row(
            "SELECT record FROM source_cases WHERE case_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
type SyncRow = (String, String, Option<String>, Option<String>, String);
impl Application {
    pub fn configure_sentry(&self, cfg: SentryConfig, who: &str) -> Result<SentryConfig> {
        self.install_sentry_project(cfg, who, "legacy default-project operator control", true)
    }
    pub fn receive_sentry(&self, raw: &[u8], signature: &str) -> Result<String> {
        let cfg = config(&self.store.connect()?)?
            .ok_or_else(|| Error::Invalid("Sentry intake not configured".into()))?;
        let secret = std::env::var(&cfg.webhook_secret_env)
            .map_err(|_| Error::Invalid("webhook credential missing; activation blocked".into()))?;
        self.receive_sentry_verified(raw, signature, secret.as_bytes(), &cfg)
    }
    /// Key injection is for contract tests; HTTP resolves only the installed secret reference.
    pub fn receive_sentry_verified(
        &self,
        raw: &[u8],
        signature: &str,
        secret: &[u8],
        cfg: &SentryConfig,
    ) -> Result<String> {
        if raw.len() > 32768 || secret.len() < 32 || !verify_signature(raw, signature, secret) {
            return Err(Error::Invalid(
                "invalid source signature or body limit".into(),
            ));
        }
        let v: Value = serde_json::from_slice(raw)?;
        if !matches!(
            v.get("action").and_then(Value::as_str),
            Some("created" | "resolved" | "unresolved" | "assigned" | "archived" | "ignored")
        ) {
            return Err(Error::Invalid("unsupported issue action".into()));
        }
        let signal = normalize(
            v.pointer("/data/issue")
                .ok_or_else(|| Error::Invalid("issue webhook required".into()))?,
            cfg,
        )?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored: String = tx.query_row(
            "SELECT record FROM source_configs WHERE revision=?1",
            [cfg.revision],
            |r| r.get(0),
        )?;
        if stored != serde_json::to_string(cfg)? {
            return Err(Error::Conflict("trusted source revision mismatch".into()));
        }
        let id = queue(&tx, cfg, &signal, &self.now())?;
        let raw_digest = format!("sha256:{:x}", Sha256::digest(raw));
        tx.execute(
            "INSERT OR IGNORE INTO source_receipts VALUES (?1,?2,?3,?4)",
            params![raw_digest, cfg.revision, id, self.now()],
        )?;
        tx.commit()?;
        Ok(raw_digest)
    }
    pub fn sentry_health(&self) -> Result<Value> {
        let db = self.store.connect()?;
        let cfg = config(&db)?;
        let mut s=db.prepare("SELECT revision,window_start,window_end,cursor,covered_until,next_at,error FROM source_sync ORDER BY revision")?;
        let rows=s.query_map([],|r|Ok(json!({"revision":r.get::<_,i64>(0)?,"window_start":r.get::<_,String>(1)?,"window_end":r.get::<_,String>(2)?,"cursor":r.get::<_,Option<String>>(3)?,"covered_until":r.get::<_,Option<String>>(4)?,"next_at":r.get::<_,String>(5)?,"error":r.get::<_,Option<String>>(6)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let pending: (i64, Option<String>) = db.query_row(
            "SELECT COUNT(*),MIN(received_at) FROM source_deliveries WHERE state!='processed'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(
            json!({"known_issue_freshness":crate::sentry_refresh::all_health(&db,self.clock.now())?,"api_credential_present":cfg.as_ref().is_some_and(|c|std::env::var(&c.api_token_env).is_ok_and(|s|!s.trim().is_empty())),"webhook_credential_present":cfg.as_ref().is_some_and(|c|std::env::var(&c.webhook_secret_env).is_ok()),"configuration":cfg,"projects":self.configured_sentry_projects()?,"reconciliation":rows,"pending":pending.0,"oldest_pending":pending.1,"coverage_limit":"Issue snapshots in requested window; not complete historical event coverage or recovery evidence"}),
        )
    }
    pub fn sentry_tick(&self, api: &dyn SentryApi) -> Result<usize> {
        let n = match self.reconcile_sentry(api) {
            Ok(n) => n,
            Err(Error::Invalid(_) | Error::Conflict(_)) => 0,
            Err(e) => return Err(e),
        };
        Ok(n + self.process_sentry_intake(api)? + self.refresh_known_sentry(api)?)
    }
    pub fn reconcile_sentry(&self, api: &dyn SentryApi) -> Result<usize> {
        let Some(cfg) = crate::sentry_projects::select_poll(self)? else { return Ok(0) };
        let result = self.reconcile_sentry_project(cfg.clone(), api);
        if matches!(result, Err(Error::Invalid(_) | Error::Conflict(_))) {
            self.store.connect()?.execute("UPDATE source_sync SET error='Invalid or conflicting project page; coverage incomplete',next_at=?1 WHERE revision=?2",params![(self.clock.now()+Duration::seconds(cfg.refresh_seconds.into())).to_rfc3339_opts(SecondsFormat::Millis,true),cfg.revision])?;
        }
        result
    }
    fn reconcile_sentry_project(&self, cfg: SentryConfig, api: &dyn SentryApi) -> Result<usize> {
        let now = self.now();
        let (start, end, c) = {
            let mut db = self.store.connect()?;
            let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row:Option<SyncRow>=tx.query_row("SELECT window_start,window_end,cursor,covered_until,next_at FROM source_sync WHERE revision=?1",[cfg.revision],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
            let window = match row {
                Some((_, _, _, _, next)) if next > now => return Ok(0),
                Some((start, end, c, covered, _))
                    if c.is_some() || covered.as_deref() != Some(end.as_str()) =>
                {
                    (start, end, c)
                }
                Some((_, _, _, Some(covered), _)) => (
                    (DateTime::parse_from_rfc3339(&covered)
                        .map_err(|_| Error::Invalid("invalid coverage watermark".into()))?
                        - Duration::seconds(cfg.read_limits().overlap_seconds.into()))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                    now.clone(),
                    None,
                ),
                _ => (
                    (self.clock.now() - Duration::seconds(cfg.read_limits().lookback_seconds.into()))
                        .to_rfc3339_opts(SecondsFormat::Millis, true),
                    now.clone(),
                    None,
                ),
            };
            tx.execute("INSERT INTO source_sync VALUES (?1,?2,?3,?4,NULL,?5,NULL) ON CONFLICT(revision) DO UPDATE SET window_start=excluded.window_start,window_end=excluded.window_end,cursor=excluded.cursor",params![cfg.revision,window.0,window.1,window.2,now])?;
            tx.commit()?;
            window
        };
        let pages: u32 = self.store.connect()?.query_row("SELECT COUNT(*) FROM source_cursors WHERE revision=?1 AND window_end=?2",params![cfg.revision,end],|r|r.get(0))?;
        if pages >= cfg.read_limits().max_pages {
            return Err(Error::Invalid("configured source page budget exhausted; coverage remains incomplete".into()));
        }
        let page = match api.page(&cfg, &start, &end, c.as_deref()) {
            Ok(p) => p,
            Err(_) => {
                self.store.connect()?.execute("UPDATE source_sync SET error='Source API unavailable; coverage incomplete',next_at=?1 WHERE revision=?2",params![(self.clock.now()+Duration::seconds(cfg.refresh_seconds.into())).to_rfc3339_opts(SecondsFormat::Millis,true),cfg.revision])?;
                return Ok(0);
            }
        };
        if page.issues.len() > 100
            || page.next_cursor.as_ref().is_some_and(|n| n.len() > 256)
            || page.next_cursor == c && c.is_some()
        {
            return Err(Error::Invalid("unbounded or repeating source page".into()));
        }
        let signals = page
            .issues
            .iter()
            .map(|v| normalize(v, &cfg))
            .collect::<Result<Vec<_>>>()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: (String, String, Option<String>) = tx.query_row(
            "SELECT window_start,window_end,cursor FROM source_sync WHERE revision=?1",
            [cfg.revision],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if current != (start.clone(), end.clone(), c) {
            return Err(Error::Conflict(
                "another reconciler advanced the page".into(),
            ));
        }
        if let Some(n) = &page.next_cursor {
            let seen:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM source_cursors WHERE revision=?1 AND window_end=?2 AND cursor=?3)",params![cfg.revision,end,n],|r|r.get(0))?;
            if seen {
                return Err(Error::Invalid("cyclic source pagination".into()));
            }
            tx.execute(
                "INSERT INTO source_cursors VALUES (?1,?2,?3)",
                params![cfg.revision, end, n],
            )?;
        }
        if !crate::sentry_projects::revision_active(&tx, cfg.revision)? {
            return Err(Error::Conflict("project configuration changed during discovery; old page cannot queue observations".into()));
        }
        for signal in &signals {
            queue(&tx, &cfg, signal, &now)?;
        }
        tx.execute("UPDATE source_sync SET cursor=?1,covered_until=CASE WHEN ?1 IS NULL THEN window_end ELSE covered_until END,next_at=?2,error=NULL WHERE revision=?3",params![page.next_cursor,if page.next_cursor.is_some(){now}else{(self.clock.now()+Duration::seconds(cfg.refresh_seconds.into())).to_rfc3339_opts(SecondsFormat::Millis,true)},cfg.revision])?;
        tx.commit()?;
        Ok(signals.len())
    }
    pub fn process_sentry_intake(&self, api: &dyn SentryApi) -> Result<usize> {
        crate::sentry_refresh::intake(self, api)
    }
    pub fn refresh_known_sentry(&self, api: &dyn SentryApi) -> Result<usize> {
        crate::sentry_refresh::refresh_known(self, api)
    }
}
