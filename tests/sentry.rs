use chrono::{Duration, TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;
use watchhouse::{
    application::Application,
    contracts::*,
    domain::ManualClock,
    error::{Error, Result},
    incidents::*,
    sentry::*,
    store::Store,
};
const SECRET: &[u8] = b"sanitized-fixture-secret-32-bytes-only";
fn sign(raw: &[u8]) -> String {
    let mut h = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
    h.update(raw);
    format!("{:x}", h.finalize().into_bytes())
}
fn issue() -> Value {
    serde_json::from_str(include_str!("fixtures/sentry/issue.json")).unwrap()
}
fn event() -> Value {
    serde_json::from_str(include_str!("fixtures/sentry/event.json")).unwrap()
}
fn cfg() -> SentryConfig {
    serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap()
}
fn setup() -> (TempDir, Application, ManualClock) {
    let d = TempDir::new().unwrap();
    let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    a.configure_sentry(cfg(), "operator").unwrap();
    (d, a, t)
}
struct Api {
    current: Value,
    event: Value,
    pages: Mutex<VecDeque<Result<Page>>>,
}
impl Api {
    fn new(current: Value) -> Self {
        Self {
            current,
            event: event(),
            pages: Mutex::new(VecDeque::new()),
        }
    }
}
impl SentryApi for Api {
    fn enrich(&self, _: &SentryConfig, id: &str) -> Result<Enrichment> {
        let mut v = self.current.clone();
        v["id"] = id.into();
        Ok(Enrichment {
            issue: v,
            event: self.event.clone(),
        })
    }
    fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
        self.pages.lock().unwrap().pop_front().unwrap_or(Ok(Page {
            issues: vec![],
            next_cursor: None,
        }))
    }
}
struct Offline;
impl SentryApi for Offline {
    fn enrich(&self, _: &SentryConfig, _: &str) -> Result<Enrichment> {
        Err(Error::Invalid("offline".into()))
    }
    fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
        Err(Error::Invalid("offline".into()))
    }
}
fn receive(a: &Application, v: Value) -> String {
    let raw = serde_json::to_vec(&json!({"action":"created","data":{"issue":v}})).unwrap();
    a.receive_sentry_verified(&raw, &sign(&raw), SECRET, &cfg())
        .unwrap()
}
fn declaration(a: &Application, id: &str, key: &str) -> DeclareIncident {
    let s = a.snapshot(id).unwrap();
    DeclareIncident {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: key.into(),
                actor: "operator".into(),
                reason: "Measured workflow impact requires coordinated response".into(),
            },
            expected_version: s.case.version,
        },
        severity: Severity::High,
        rationale: "Fixture workflow unavailable, impact requires confirmation".into(),
        impact: "Three source occurrences; denominator and users affected unknown".into(),
        uncertainty: "Fixture only, no production service inspected".into(),
        objective: "Independently reproduce and assess impact".into(),
        roles: IncidentRoles {
            commander: "operator".into(),
            operations: "operator".into(),
            communications: "operator".into(),
            scribe: "operator".into(),
        },
        evidence_refs: s
            .events
            .iter()
            .rev()
            .find_map(|e| {
                if let EventData::SourceObserved { evidence_refs, .. } = &e.data {
                    Some(evidence_refs.clone())
                } else {
                    None
                }
            })
            .unwrap(),
        known_start: None,
    }
}
#[test]
fn signatures_are_over_exact_bytes_and_scope_is_checked_before_durable_acceptance() {
    let (d, a, _t) = setup();
    let raw = serde_json::to_vec(&json!({"action":"created","data":{"issue":issue()}})).unwrap();
    assert!(verify_signature(&raw, &sign(&raw), SECRET));
    let mut changed = raw.clone();
    changed.push(b' ');
    assert!(!verify_signature(&changed, &sign(&raw), SECRET));
    assert!(!verify_signature(&raw, "bad", SECRET));
    assert!(
        a.receive_sentry_verified(&raw, &"0".repeat(64), SECRET, &cfg())
            .is_err()
    );
    let first = a
        .receive_sentry_verified(&raw, &sign(&raw), SECRET, &cfg())
        .unwrap();
    assert_eq!(
        a.receive_sentry_verified(&raw, &sign(&raw), SECRET, &cfg())
            .unwrap(),
        first
    );
    assert_ne!(
        a.receive_sentry_verified(&changed, &sign(&changed), SECRET, &cfg())
            .unwrap(),
        first
    );
    let mut wrong = issue();
    wrong["project"]["id"] = "99".into();
    let raw = serde_json::to_vec(&json!({"action":"created","data":{"issue":wrong}})).unwrap();
    assert!(
        a.receive_sentry_verified(&raw, &sign(&raw), SECRET, &cfg())
            .is_err()
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM source_deliveries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM source_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(a.list_cases().unwrap().is_empty());
    assert!(verify_signature(
        b"Hi There",
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        &[0x0b; 20]
    ));
}
#[test]
fn duplicates_out_of_order_enrichment_and_incident_declaration_have_one_case_and_incident() {
    let (d, a, t) = setup();
    receive(&a, issue());
    let api = Api::new(issue());
    a.process_sentry_intake(&api).unwrap();
    let id = a.list_cases().unwrap()[0].id.clone();
    let mut older = issue();
    older["lastSeen"] = "2026-11-14T13:00:00+02:00".into();
    older["status"] = "resolved".into();
    older["title"] = "Old misleading source title".into();
    receive(&a, older.clone());
    a.process_sentry_intake(&Api::new(older)).unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source.unwrap().signal.status,
        "unresolved"
    );
    receive(&a, issue());
    assert_eq!(a.process_sentry_intake(&api).unwrap(), 0);
    assert_eq!(a.list_cases().unwrap().len(), 1);
    let s = a.snapshot(&id).unwrap();
    let source = s.source.unwrap();
    assert!(source.authoritative);
    assert_eq!(
        source.deployed_revision.as_deref(),
        Some("abcdef1234567890")
    );
    assert_eq!(source.exceptions, vec!["TimeoutError"]);
    let raw = serde_json::to_string(&source).unwrap();
    for excluded in ["OMIT", "email", "Authorization", "/private/"] {
        assert!(!raw.contains(excluded))
    }
    let mut cmd = declaration(&a, &id, "declare");
    cmd.evidence_refs = vec![
        s.events
            .iter()
            .find_map(|e| {
                if let EventData::SourceObserved { evidence_refs, .. } = &e.data {
                    Some(evidence_refs[0].clone())
                } else {
                    None
                }
            })
            .unwrap(),
    ];
    let incident = a.declare_incident(&id, cmd.clone()).unwrap();
    assert_eq!(a.declare_incident(&id, cmd).unwrap().id, incident.id);
    assert!(
        a.declare_incident(&id, declaration(&a, &id, "second-declaration"))
            .is_err()
    );
    assert!(incident.recovered_at.is_none());
    assert!(
        a.incident_markdown(&id)
            .unwrap()
            .contains("Recovery at declaration: not assessed")
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .args([
            "--db",
            d.path().join("db").to_str().unwrap(),
            "case",
            "incident-markdown",
            &id,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("# Incident ")
    );
    let (_other, b, _clock) = setup();
    let mut hostile = Api::new(issue());
    hostile.event["release"] = json!({"version":"![x](https://example.invalid/pixel)"});
    receive(&b, issue());
    b.process_sentry_intake(&hostile).unwrap();
    let bid = b.list_cases().unwrap()[0].id.clone();
    b.declare_incident(&bid, declaration(&b, &bid, "escaped-report"))
        .unwrap();
    let report = b.incident_markdown(&bid).unwrap();
    let brief = report
        .split("## Authoritative structured record")
        .next()
        .unwrap();
    assert!(!brief.contains("![x](https://"));
    assert!(brief.contains("\\!\\[x\\]"));
    t.advance(Duration::seconds(1801));
    a.supervise_team().unwrap();
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_type == "incident_update")
    );
    assert_eq!(a.supervise_team().unwrap(), 0);
}
#[test]
fn missed_events_reconcile_across_pages_restart_and_outage_without_false_coverage() {
    let (d, a, t) = setup();
    let api = Api::new(issue());
    let mut missed = issue();
    missed["id"] = "1002".into();
    api.pages.lock().unwrap().extend([
        Ok(Page {
            issues: vec![issue()],
            next_cursor: Some("page:2".into()),
        }),
        Err(Error::Invalid("fixture outage".into())),
        Ok(Page {
            issues: vec![missed],
            next_cursor: None,
        }),
    ]);
    assert_eq!(a.reconcile_sentry(&api).unwrap(), 1);
    assert!(a.sentry_health().unwrap()["reconciliation"][0]["covered_until"].is_null());
    a.process_sentry_intake(&api).unwrap();
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    assert_eq!(a.reconcile_sentry(&api).unwrap(), 0);
    assert_eq!(
        a.sentry_health().unwrap()["reconciliation"][0]["cursor"],
        "page:2"
    );
    assert!(!a.sentry_health().unwrap()["reconciliation"][0]["error"].is_null());
    t.advance(Duration::seconds(61));
    assert_eq!(a.reconcile_sentry(&api).unwrap(), 1);
    a.process_sentry_intake(&api).unwrap();
    assert_eq!(a.list_cases().unwrap().len(), 2);
    assert!(!a.sentry_health().unwrap()["reconciliation"][0]["covered_until"].is_null());
    assert_eq!(a.reconcile_sentry(&api).unwrap(), 0);
}
#[test]
fn enrichment_outage_stays_visible_and_retry_does_not_duplicate_qualification() {
    let (d, a, t) = setup();
    receive(&a, issue());
    a.sentry_tick(&Offline).unwrap();
    let id = a.list_cases().unwrap()[0].id.clone();
    let s = a.snapshot(&id).unwrap();
    assert!(!s.source.unwrap().authoritative);
    assert!(a.sentry_health().unwrap()["pending"].as_i64().unwrap() > 0);
    t.advance(Duration::seconds(61));
    a.process_sentry_intake(&Offline).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().events.len(), s.events.len());
    t.advance(Duration::seconds(61));
    let a = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(t));
    a.process_sentry_intake(&Api::new(issue())).unwrap();
    assert!(a.snapshot(&id).unwrap().source.unwrap().authoritative);
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert_eq!(a.sentry_health().unwrap()["pending"], 0);
}
#[test]
fn source_faults_roll_back_and_replay_recovers_case_creation_boundary() {
    let (d, a, _t) = setup();
    receive(&a, issue());
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_source BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='source.observed' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;").unwrap();
    assert!(a.process_sentry_intake(&Api::new(issue())).is_err());
    assert_eq!(a.list_cases().unwrap().len(), 1);
    let id = a.list_cases().unwrap()[0].id.clone();
    assert!(a.snapshot(&id).unwrap().source.is_none());
    db.execute_batch("DROP TRIGGER fail_source").unwrap();
    a.process_sentry_intake(&Api::new(issue())).unwrap();
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert!(a.snapshot(&id).unwrap().source.is_some());
    let mut cmd = declaration(&a, &id, "stale");
    cmd.command.expected_version -= 1;
    assert!(a.declare_incident(&id, cmd).is_err());
    assert!(a.snapshot(&id).unwrap().incident.is_none());
}
#[test]
fn cycling_pagination_never_claims_complete_coverage() {
    let (_d, a, _t) = setup();
    let api = Api::new(issue());
    api.pages.lock().unwrap().extend([
        Ok(Page {
            issues: vec![issue()],
            next_cursor: Some("cycle".into()),
        }),
        Ok(Page {
            issues: vec![],
            next_cursor: Some("cycle".into()),
        }),
    ]);
    a.sentry_tick(&api).unwrap();
    a.sentry_tick(&api).unwrap();
    let h = a.sentry_health().unwrap();
    assert!(h["reconciliation"][0]["covered_until"].is_null());
    assert!(!h["reconciliation"][0]["error"].is_null());
    assert_eq!(a.list_cases().unwrap().len(), 1);
}
