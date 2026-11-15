use chrono::{TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::sync::Arc;
use tempfile::TempDir;
use watchhouse::{
    application::Application, contracts::*, domain::ManualClock, error::Result, incidents::*,
    sentry::*, store::Store,
};
pub fn command(a: &Application, id: &str, key: &str, actor: &str) -> OperatorCommand {
    OperatorCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: actor.into(),
            reason: "Sanitized M3 coordination fixture".into(),
        },
        expected_version: a.snapshot(id).unwrap().case.version,
    }
}
pub fn evidence(a: &Application, id: &str) -> Vec<String> {
    a.snapshot(id)
        .unwrap()
        .qualification
        .evidence
        .iter()
        .filter(|e| e.provenance == "sentry-api-v1")
        .map(|e| e.digest.clone())
        .collect()
}
pub fn fixture() -> (TempDir, Application, ManualClock, String) {
    let d = TempDir::new().unwrap();
    let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    let cfg: SentryConfig =
        serde_json::from_str(include_str!("../fixtures/sentry/config.json")).unwrap();
    a.configure_sentry(cfg.clone(), "operator").unwrap();
    let issue: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/sentry/issue.json")).unwrap();
    let raw = serde_json::to_vec(&json!({"action":"created","data":{"issue":issue}})).unwrap();
    let secret = b"sanitized-fixture-secret-32-bytes-only";
    let mut h = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    h.update(&raw);
    let sig = format!("{:x}", h.finalize().into_bytes());
    a.receive_sentry_verified(&raw, &sig, secret, &cfg).unwrap();
    struct Api;
    impl SentryApi for Api {
        fn enrich(&self, _: &SentryConfig, _: &str) -> Result<Enrichment> {
            Ok(Enrichment {
                issue: serde_json::from_str(include_str!("../fixtures/sentry/issue.json"))?,
                event: serde_json::from_str(include_str!("../fixtures/sentry/event.json"))?,
            })
        }
        fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
            Ok(Page {
                issues: vec![],
                next_cursor: None,
            })
        }
    }
    a.process_sentry_intake(&Api).unwrap();
    let id = a.list_cases().unwrap()[0].id.clone();
    a.declare_incident(
        &id,
        DeclareIncident {
            command: command(&a, &id, "declare", "operator"),
            severity: Severity::High,
            rationale: "Fixture workflow impact".into(),
            impact: "Traffic denominator unknown".into(),
            uncertainty: "Fixture only".into(),
            objective: "Investigate fixture".into(),
            roles: IncidentRoles {
                commander: "operator".into(),
                operations: "operator".into(),
                communications: "operator".into(),
                scribe: "operator".into(),
            },
            evidence_refs: evidence(&a, &id),
            known_start: None,
        },
    )
    .unwrap();
    (d, a, t, id)
}
