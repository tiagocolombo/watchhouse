//! Explicit HTTPS-proxy access. Domain ownership and independent review still apply.
use crate::{
    application::{Application, text},
    contracts::V1,
    error::{Error, Result},
    store,
};
use chrono::{Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Observer,
    Operator,
    Admin,
}
impl Role {
    pub(crate) fn operates(self) -> bool {
        self != Self::Observer
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub id: String,
    pub role: Role,
    pub token_env: String,
    pub disabled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: V1,
    pub revision: i64,
    pub public_origin: String,
    pub proxy_token_env: String,
    pub principals: Vec<Principal>,
    pub max_audit_rows: u32,
    pub paused: bool,
}
fn reference(s: &str) -> bool {
    s.starts_with("WATCHHOUSE_REMOTE_")
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.public_origin).map_err(|_| {
            Error::Invalid("remote access requires a canonical HTTPS origin".into())
        })?;
        if self.revision < 1
            || url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.public_origin != url.origin().ascii_serialization()
            || !reference(&self.proxy_token_env)
            || self.principals.is_empty()
            || self.principals.len() > 64
            || !(100..=1_000_000).contains(&self.max_audit_rows)
        {
            return Err(Error::Invalid(
                "invalid remote access origin, references or bounds".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut refs = HashSet::from([self.proxy_token_env.as_str()]);
        for p in &self.principals {
            if !crate::operations::ident(&p.id)
                || p.id.starts_with("fake-")
                || p.id.starts_with("model-")
                || !ids.insert(&p.id)
                || !reference(&p.token_env)
                || !refs.insert(&p.token_env)
            {
                return Err(Error::Invalid("remote principals need unique human identities and dedicated credential references".into()));
            }
        }
        if !self.paused
            && !self
                .principals
                .iter()
                .any(|p| !p.disabled && p.role == Role::Admin)
        {
            return Err(Error::Invalid(
                "enabled remote access needs an explicitly configured administrator".into(),
            ));
        }
        Ok(())
    }
}
pub trait Secrets: Send + Sync {
    fn get(&self, reference: &str) -> Option<String>;
}
pub struct Environment;
impl Secrets for Environment {
    fn get(&self, reference: &str) -> Option<String> {
        std::env::var(reference).ok()
    }
}
pub(crate) fn digest(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}
pub(crate) fn equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |n, (a, b)| n | (a ^ b)) == 0
}
fn secret(s: Option<String>) -> Result<[u8; 32]> {
    let s = s.ok_or_else(|| Error::Invalid("remote credential reference unavailable".into()))?;
    if !(32..=512).contains(&s.len())
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
    {
        return Err(Error::Invalid(
            "remote credentials need distinct 32..512-byte bearer values".into(),
        ));
    }
    Ok(digest(&s))
}
pub(crate) struct Keys {
    pub proxy: [u8; 32],
    pub principals: Vec<(Principal, [u8; 32])>,
}
pub(crate) fn keys(p: &Policy, secrets: &dyn Secrets) -> Result<Keys> {
    let proxy = secret(secrets.get(&p.proxy_token_env))?;
    let mut unique = HashSet::from([proxy]);
    let mut principals = vec![];
    for principal in p.principals.iter().filter(|p| !p.disabled) {
        let key = secret(secrets.get(&principal.token_env))?;
        if !unique.insert(key) {
            return Err(Error::Invalid(
                "proxy and principal credentials must be distinct".into(),
            ));
        }
        principals.push((principal.clone(), key));
    }
    Ok(Keys { proxy, principals })
}
pub(crate) fn policy(db: &Connection) -> Result<Option<Policy>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM api_access_policies ORDER BY revision DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((raw, sum)) = row else {
        return Ok(None);
    };
    if store::digest(&raw) != sum {
        return Err(Error::Conflict("access policy integrity failure".into()));
    }
    let p: Policy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(Some(p))
}
pub(crate) fn active(app: &Application) -> Result<Policy> {
    let db = app.store.connect()?;
    let p = policy(&db)?
        .ok_or_else(|| Error::Conflict("remote access has no OS configuration".into()))?;
    crate::restore::require_fresh(&db, "api_access", p.revision)?;
    if p.paused {
        return Err(Error::Conflict(
            "remote access is paused by OS policy".into(),
        ));
    }
    Ok(p)
}
#[derive(Clone)]
pub(crate) struct Session {
    pub id: String,
    pub role: Role,
    pub revision: Option<i64>,
    pub key: [u8; 32],
}
impl Session {
    pub(crate) fn local() -> Self {
        Self {
            id: "operator".into(),
            role: Role::Admin,
            revision: None,
            key: [0; 32],
        }
    }
    pub(crate) fn information(&self) -> Value {
        json!({"mode":if self.revision.is_some(){"remote"}else{"local"},"principal":self.id,"role":self.role,"policy_revision":self.revision,"can_operate":self.role.operates(),"can_manage_factory":self.role==Role::Admin,"identity_source":if self.revision.is_some(){"OS-configured credential"}else{"shared local operator attribution"},"scope":"global operational records; service-owner and independent remote-worker capabilities remain separate development","limits":{"requests_per_second":64,"concurrent_requests":64,"event_streams":16}})
    }
    pub(crate) fn current(&self, app: &Application, secrets: &dyn Secrets) -> Result<bool> {
        let p = active(app)?;
        if self.revision != Some(p.revision) {
            return Ok(false);
        }
        Ok(keys(&p, secrets)?
            .principals
            .iter()
            .any(|(p, k)| p.id == self.id && p.role == self.role && equal(k, &self.key)))
    }
}
pub(crate) fn identify(p: &Policy, keys: &Keys, supplied: &str) -> Option<Session> {
    if !(32..=512).contains(&supplied.len()) {
        return None;
    }
    let key = digest(supplied);
    keys.principals
        .iter()
        .find(|(_, k)| equal(k, &key))
        .map(|(principal, _)| Session {
            id: principal.id.clone(),
            role: principal.role,
            revision: Some(p.revision),
            key,
        })
}
/// New mutation endpoints must opt into a capability. Remote UI credentials never
/// authenticate the legacy fake-worker simulator or independent remote workers.
pub(crate) fn permitted(role: Role, method: &str, path: &str) -> bool {
    if method == "GET" {
        return !path.starts_with("/admin/") || role == Role::Admin;
    }
    if method != "POST" {
        return false;
    }
    if path == "/factory" {
        return role == Role::Admin;
    }
    if path == "/cases" {
        return role.operates();
    }
    let Some(tail) = path
        .strip_prefix("/cases/")
        .and_then(|s| s.split_once('/').map(|(_, tail)| tail))
    else {
        return false;
    };
    if tail == "hallmark-export" {
        return true;
    }
    role.operates()
        && [
            "rename",
            "challenges",
            "gate",
            "correlations",
            "repairs",
            "container-qualification",
            "model-proposals/adopt",
            "model-reviews/adopt",
            "publications",
            "model-jobs",
            "model-manage",
            "container-manage",
            "candidates",
            "candidate-checks",
            "candidate-reviews",
            "recovery",
            "operations",
            "merges",
            "merges/retry",
            "learning",
            "knowledge-imports",
            "dispositions",
            "impact",
            "reconsider",
            "incident",
            "incident/operations",
            "incident/artifacts",
            "team/messages",
            "team/message-ack",
            "team/handoffs",
            "team/escalation-ack",
            "team/escalation-resolve",
            "team/delivery-reconcile",
            "worker-qualification/reconcile",
            "adapter-trials",
            "restore-reminder",
            "evidence",
        ]
        .contains(&tail)
}
pub(crate) fn binds_actor(body: &Value, principal: &str) -> bool {
    let mut found = false;
    for pointer in ["/actor", "/meta/actor", "/command/meta/actor"] {
        if let Some(v) = body.pointer(pointer) {
            if v.as_str() != Some(principal) {
                return false;
            }
            found = true;
        }
    }
    found
}
pub(crate) struct AuditInput<'a> {
    pub method: &'a str,
    pub route: &'a str,
    pub path: &'a str,
    pub body_digest: Option<String>,
    pub decision: &'a str,
}
#[derive(Debug, Serialize)]
pub struct Health {
    pub configured_revision: Option<i64>,
    pub restored_authority_held: bool,
    pub paused: bool,
    pub audit_remaining: i64,
    pub audit_stale_incomplete: i64,
}
impl Application {
    pub(crate) fn access_health(&self) -> Result<Health> {
        let s = self.access_status()?;
        Ok(Health {
            configured_revision: s.policy.as_ref().map(|p| p.revision),
            restored_authority_held: s.restored_authority_held,
            paused: s.policy.as_ref().is_some_and(|p| p.paused),
            audit_remaining: s.audit_remaining,
            audit_stale_incomplete: s.audit_stale_incomplete,
        })
    }
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub policy: Option<Policy>,
    pub restored_authority_held: bool,
    pub audit_requests: i64,
    pub audit_remaining: i64,
    pub audit_stale_incomplete: i64,
    pub principals: Vec<Value>,
    pub limitation: String,
}
impl Application {
    pub fn install_access_policy(&self, p: Policy, who: &str, source: &str) -> Result<String> {
        crate::incident_ops::human(who)?;
        if who.starts_with("model-") {
            return Err(Error::Invalid("OS operator required".into()));
        }
        text(source, "configuration source", 4096)?;
        p.validate()?;
        let raw = serde_json::to_string(&p)?;
        let sum = store::digest(&raw);
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT digest FROM api_access_policies WHERE revision=?1",
                [p.revision],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            return if old == sum {
                Ok(sum)
            } else {
                Err(Error::Conflict("access policy revisions immutable".into()))
            };
        }
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM api_access_policies",
            [],
            |r| r.get(0),
        )?;
        if p.revision != latest + 1 {
            return Err(Error::Conflict(
                "access policy revision must advance consecutively".into(),
            ));
        }
        tx.execute(
            "INSERT INTO api_access_policies VALUES (?1,?2,?3,?4,?5,?6)",
            params![p.revision, sum, raw, who, source, self.now()],
        )?;
        tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[json!({"type":"api_access_policy_installed","revision":p.revision,"digest":sum,"operator":who,"source":source,"at":self.now()}).to_string()])?;
        tx.commit()?;
        Ok(sum)
    }
    pub fn access_status(&self) -> Result<Status> {
        let db = self.store.connect()?;
        let p = policy(&db)?;
        let held = p
            .as_ref()
            .map(|p| crate::restore::fenced(&db, "api_access", p.revision))
            .transpose()?
            .unwrap_or(false);
        let count = db.query_row("SELECT COUNT(*) FROM api_access_requests", [], |r| {
            r.get::<_, i64>(0)
        })?;
        let cutoff =
            (self.clock.now() - Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Millis, true);
        let incomplete=db.query_row("SELECT COUNT(*) FROM api_access_requests r WHERE started_at<?1 AND NOT EXISTS(SELECT 1 FROM api_access_results s WHERE s.id=r.id)",[cutoff],|r|r.get(0))?;
        let principals=p.as_ref().map(|p|p.principals.iter().map(|u|json!({"id":u.id,"role":u.role,"disabled":u.disabled,"credential_checked":!held&&!p.paused&&!u.disabled,"credential_present":!held&&!p.paused&&!u.disabled&&std::env::var(&u.token_env).is_ok_and(|v|(32..=512).contains(&v.len()))})).collect()).unwrap_or_default();
        Ok(Status{audit_remaining:p.as_ref().map_or(0,|p|(i64::from(p.max_audit_rows)-count).max(0)),policy:p,restored_authority_held:held,audit_requests:count,audit_stale_incomplete:incomplete,principals,limitation:"Explicit proxy HTTPS assertion plus distinct bearer identities; global observer/operator/admin roles only. Domain ownership, independent review and OS-only policy installation remain required. Service-owner data scopes and remote worker trust are separate development. Audit counts server intent/status, not client delivery; bounded audit capacity denies remote access while local CLI/timers remain available.".into()})
    }
    pub fn access_audit(&self) -> Result<Vec<Value>> {
        let db = self.store.connect()?;
        let mut q=db.prepare("SELECT r.record,s.status,s.completed_at FROM api_access_requests r LEFT JOIN api_access_results s ON s.id=r.id ORDER BY r.rowid DESC LIMIT 200")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<u16>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?
        .map(|r| {
            let (raw, status, completed_at) = r?;
            let mut v: Value = serde_json::from_str(&raw)?;
            v["status"] = json!(status);
            v["completed_at"] = json!(completed_at);
            Ok(v)
        })
        .collect()
    }
    pub(crate) fn reserve_access(
        &self,
        p: &Policy,
        session: &Session,
        input: AuditInput<'_>,
    ) -> Result<String> {
        let AuditInput {
            method,
            route,
            path,
            body_digest,
            decision,
        } = input;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM api_access_requests", [], |r| r.get(0))?;
        if count >= i64::from(p.max_audit_rows) {
            return Err(Error::Backpressure("Remote access audit capacity reached; OS operator must retain/export audit and configure capacity. Local CLI and deterministic duties remain available".into()));
        }
        let id = store::id("access");
        let at = self.now();
        let v = json!({"id":id,"at":at,"principal":session.id,"role":session.role,"policy_revision":p.revision,"method":method,"route":route,"resource_digest":store::digest(path),"body_digest":body_digest,"decision":decision});
        tx.execute(
            "INSERT INTO api_access_requests VALUES (?1,?2,?3)",
            params![id, v.to_string(), at],
        )?;
        tx.commit()?;
        Ok(id)
    }
    pub(crate) fn finish_access(&self, id: &str, status: u16) -> Result<()> {
        self.store.connect()?.execute(
            "INSERT INTO api_access_results VALUES (?1,?2,?3)",
            params![id, status, self.now()],
        )?;
        Ok(())
    }
}
