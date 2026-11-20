mod common;
use chrono::Duration;
use common::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use watchhouse::{
    application::Application,
    datadog::*,
    domain::Clock,
    error::{Error, Result},
    store::Store,
};
fn config() -> Config {
    serde_json::from_str(include_str!("fixtures/datadog/config.json")).unwrap()
}
struct FixtureApi {
    ids: RefCell<Vec<u64>>,
    fail: bool,
    state: &'static str,
}
impl FixtureApi {
    fn new() -> Self {
        Self {
            ids: RefCell::new(vec![]),
            fail: false,
            state: "Alert",
        }
    }
}
impl Api for FixtureApi {
    fn monitor(&self, _: &Config, id: u64) -> Result<Value> {
        self.ids.borrow_mut().push(id);
        if self.fail {
            return Err(Error::Invalid("unavailable fixture".into()));
        }
        let mut v: Value = serde_json::from_str(include_str!("fixtures/datadog/monitor.json"))?;
        v["id"] = json!(id);
        v["overall_state"] = json!(self.state);
        Ok(v)
    }
}
fn ddcase(a: &Application, id: u64) -> String {
    a.list_cases()
        .unwrap()
        .into_iter()
        .find(|c| c.title == format!("Datadog monitor {id}"))
        .unwrap()
        .id
}
#[test]
fn periodic_intake_is_fair_deduplicated_and_never_infers_recovery() {
    let (_d, a, t, sentry) = fixture();
    a.configure_datadog(config(), "operator", "fixture source")
        .unwrap();
    let api = FixtureApi::new();
    assert!(a.sync_datadog(&api).unwrap());
    assert!(a.sync_datadog(&api).unwrap());
    assert!(!a.sync_datadog(&api).unwrap());
    assert_eq!(*api.ids.borrow(), vec![7, 8]);
    let id = ddcase(&a, 7);
    let s = a.snapshot(&id).unwrap();
    let source = s.source.unwrap();
    assert_eq!(source.source_key, "datadog:fixture/7");
    assert!(source.signal.count.is_none());
    assert!(source.release.is_none());
    assert!(source.deployed_revision.is_none());
    assert!(s.incident.is_none());
    assert_eq!(s.source_freshness.unwrap().state, "current_snapshot");
    assert_eq!(
        a.snapshot(&sentry).unwrap().source.unwrap().source_key,
        "sentry:fixture-org/42/1001"
    );
    let version = s.case.version;
    t.advance(Duration::seconds(11));
    a.sync_datadog(&api).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().case.version, version);
    let raw = a
        .snapshot(&id)
        .unwrap()
        .qualification
        .evidence
        .into_iter()
        .find(|e| e.provenance == "datadog-monitor-api-v1")
        .unwrap();
    let db = Store::open(_d.path().join("db"))
        .unwrap()
        .connect()
        .unwrap();
    let content: String = db
        .query_row(
            "SELECT content FROM evidence WHERE digest=?1",
            [raw.digest],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!content.contains("sensitive query"));
    assert!(!content.contains("untrusted instructions"));
    // A missed delivery/current-state transition is discovered by polling after restart.
    t.advance(Duration::seconds(11));
    let reopened = Application::new(
        Store::open(_d.path().join("db")).unwrap(),
        std::sync::Arc::new(t),
    );
    let mut recovered = FixtureApi::new();
    recovered.state = "OK";
    reopened.sync_datadog(&recovered).unwrap();
    reopened.sync_datadog(&recovered).unwrap();
    let s = reopened.snapshot(&id).unwrap();
    assert_eq!(s.source.unwrap().signal.status, "OK");
    assert!(!s.recovery.recovery.response_resolved);
}
#[test]
fn outage_is_visible_without_fabricating_a_healthy_case_and_does_not_starve_others() {
    let (_d, a, _t, _) = fixture();
    a.configure_datadog(config(), "operator", "fixture")
        .unwrap();
    let mut api = FixtureApi::new();
    api.fail = true;
    a.sync_datadog(&api).unwrap();
    let status = a.datadog_status().unwrap();
    assert_eq!(status[0].reads[0].state, "unavailable");
    assert_eq!(status[0].installed_by, "operator");
    assert!(
        a.list_cases()
            .unwrap()
            .iter()
            .all(|c| !c.title.starts_with("Datadog"))
    );
    api.fail = false;
    a.sync_datadog(&api).unwrap();
    assert_eq!(*api.ids.borrow(), vec![7, 8]);
    assert_eq!(
        a.snapshot(&ddcase(&a, 8))
            .unwrap()
            .source
            .unwrap()
            .signal
            .issue_id,
        "8"
    );
}
#[test]
fn changed_configuration_fences_late_reads_and_preserves_original_scope() {
    let (d, a, t, _) = fixture();
    let mut c = config();
    c.monitors.remove(&8);
    a.configure_datadog(c.clone(), "operator", "fixture")
        .unwrap();
    a.sync_datadog(&FixtureApi::new()).unwrap();
    let id = ddcase(&a, 7);
    let digest = a.snapshot(&id).unwrap().source.unwrap().repository;
    t.advance(Duration::seconds(11));
    struct Reconfigure<'a> {
        a: &'a Application,
        c: Config,
    }
    impl Api for Reconfigure<'_> {
        fn monitor(&self, _: &Config, id: u64) -> Result<Value> {
            self.a
                .configure_datadog(self.c.clone(), "operator", "during read")?;
            Ok(json!({"id":id,"name":"obsolete result","overall_state":"OK"}))
        }
    }
    c.revision = 2;
    c.monitors.get_mut(&7).unwrap().repository = "example/new-scope".into();
    a.sync_datadog(&Reconfigure {
        a: &a,
        c: c.clone(),
    })
    .unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.source.unwrap().repository, digest);
    assert_eq!(s.source_freshness.unwrap().state, "configuration_changed");
    a.sync_datadog(&FixtureApi::new()).unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source.unwrap().repository,
        "example/new-scope"
    );
    assert_eq!(
        a.list_cases()
            .unwrap()
            .iter()
            .filter(|c| c.title == "Datadog monitor 7")
            .count(),
        1
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert!(
        db.execute("UPDATE datadog_configs SET source='replaced'", [])
            .is_err()
    );
    assert!(db.execute("DELETE FROM datadog_configs", []).is_err());
    assert!(
        a.configure_datadog(c, "fake-engineer", "self modification")
            .is_err()
    );
}
#[test]
fn duplicate_overlapping_reads_cannot_overwrite_a_newer_generation() {
    let (_d, a, t, _) = fixture();
    let mut c = config();
    c.monitors.remove(&8);
    a.configure_datadog(c, "operator", "fixture").unwrap();
    struct Overlap<'a> {
        a: &'a Application,
        t: &'a watchhouse::domain::ManualClock,
    }
    impl Api for Overlap<'_> {
        fn monitor(&self, _: &Config, id: u64) -> Result<Value> {
            self.t.advance(Duration::seconds(11));
            let mut newer = FixtureApi::new();
            newer.state = "OK";
            self.a.sync_datadog(&newer)?;
            Ok(json!({"id":id,"name":"old response","overall_state":"Alert"}))
        }
    }
    a.sync_datadog(&Overlap { a: &a, t: &t }).unwrap();
    let id = ddcase(&a, 7);
    assert_eq!(a.snapshot(&id).unwrap().source.unwrap().signal.status, "OK");
    assert_eq!(
        a.snapshot(&id)
            .unwrap()
            .source_freshness
            .unwrap()
            .generation,
        2
    );
}
#[test]
fn same_incident_contract_works_without_sentry_configuration_and_uses_its_own_timers() {
    let d = tempfile::tempdir().unwrap();
    let t = watchhouse::domain::ManualClock::new(chrono::Utc::now());
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t.clone()),
    );
    let mut c = config();
    c.monitors.remove(&8);
    a.configure_datadog(c, "operator", "fixture").unwrap();
    a.sync_datadog(&FixtureApi::new()).unwrap();
    let id = ddcase(&a, 7);
    let refs = a
        .snapshot(&id)
        .unwrap()
        .qualification
        .evidence
        .iter()
        .filter(|e| e.provenance == "datadog-monitor-api-v1")
        .map(|e| e.digest.clone())
        .collect();
    let incident = a
        .declare_incident(
            &id,
            watchhouse::incidents::DeclareIncident {
                command: command(&a, &id, "declare-dd", "operator"),
                severity: watchhouse::incidents::Severity::High,
                rationale: "Fixture scoped workflow".into(),
                impact: "Unknown request denominator".into(),
                uncertainty: "Fixture; current monitor state only".into(),
                objective: "Investigate".into(),
                roles: watchhouse::incidents::IncidentRoles {
                    commander: "operator".into(),
                    operations: "operator".into(),
                    communications: "operator".into(),
                    scribe: "operator".into(),
                },
                evidence_refs: refs,
                known_start: None,
            },
        )
        .unwrap();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&incident.next_update_at)
            .unwrap()
            .timestamp()
            - t.now().timestamp(),
        1800
    );
    let ops = a
        .operate_incident(
            &id,
            watchhouse::incident_ops::IncidentOperation {
                command: command(&a, &id, "update-dd", "operator"),
                action: watchhouse::incident_ops::IncidentAction::Update {
                    severity: watchhouse::incidents::Severity::Moderate,
                    impact: "Unknown".into(),
                    uncertainty: "No live data".into(),
                    objective: "Collect evidence".into(),
                    evidence_refs: incident.declaration.evidence_refs,
                },
            },
        )
        .unwrap();
    assert_eq!(ops.revision, 2);
    a.recovery_command(
        &id,
        watchhouse::recovery::RecoveryCommand {
            command: command(&a, &id, "dd-recovery-plan", "operator"),
            action: watchhouse::recovery::RecoveryAction::Plan {
                criteria: watchhouse::recovery::Criteria {
                    workflow: "checkout".into(),
                    min_requests: 100,
                    min_window_seconds: 60,
                    max_error_ppm: 1000,
                    max_p95_ms: 1000,
                    max_source_lag_seconds: 30,
                },
                review_seconds: 300,
            },
        },
    )
    .unwrap();
    assert!(a.snapshot(&id).unwrap().recovery.recovery.context_current);
    let mut changed = config();
    changed.revision = 2;
    changed.monitors.remove(&8);
    changed.monitors.get_mut(&7).unwrap().repository = "example/remapped".into();
    a.configure_datadog(changed, "operator", "fixture remap")
        .unwrap();
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.context_current);
}
#[test]
fn malformed_foreign_and_unsupported_responses_are_not_facts() {
    let (_d, a, _t, _) = fixture();
    a.configure_datadog(config(), "operator", "fixture")
        .unwrap();
    struct Foreign;
    impl Api for Foreign {
        fn monitor(&self, _: &Config, _: u64) -> Result<Value> {
            Ok(json!({"id":999,"name":"wrong","overall_state":"Alert"}))
        }
    }
    a.sync_datadog(&Foreign).unwrap();
    assert!(
        a.list_cases()
            .unwrap()
            .iter()
            .all(|c| !c.title.starts_with("Datadog"))
    );
    assert_eq!(a.datadog_status().unwrap()[0].reads[0].state, "unavailable");
    let mut c = config();
    c.site = "evil.invalid".into();
    assert!(c.validate().is_err());
    let mut c = config();
    c.monitors.clear();
    assert!(c.validate().is_err());
    let mut c = config();
    c.api_key_env = "HOME".into();
    assert!(c.validate().is_err());
}
#[test]
fn projection_failure_rolls_back_source_evidence_and_retry_reuses_case() {
    let (d, a, t, _) = fixture();
    let mut c = config();
    c.monitors.remove(&8);
    a.configure_datadog(c, "operator", "fixture").unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER reject_dd BEFORE INSERT ON source_cases WHEN NEW.source_key LIKE 'datadog:%' BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(a.sync_datadog(&FixtureApi::new()).is_err());
    let id = ddcase(&a, 7);
    assert!(a.snapshot(&id).unwrap().source.is_none());
    assert!(a.snapshot(&id).unwrap().qualification.evidence.is_empty());
    db.execute_batch("DROP TRIGGER reject_dd").unwrap();
    t.advance(Duration::seconds(11));
    a.sync_datadog(&FixtureApi::new()).unwrap();
    assert!(a.snapshot(&id).unwrap().source.is_some());
    assert_eq!(
        a.list_cases()
            .unwrap()
            .iter()
            .filter(|c| c.title == "Datadog monitor 7")
            .count(),
        1
    );
}
