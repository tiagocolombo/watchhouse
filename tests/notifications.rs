use chrono::{Duration, TimeZone, Utc};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use watchhouse::{
    application::Application, contracts::*, domain::ManualClock, intake::FakeSource,
    notifications::*, store::Store,
};
struct Transport {
    outcomes: Mutex<Vec<DeliveryOutcome>>,
    sent: Mutex<Vec<String>>,
}
impl Transport {
    fn new(outcomes: Vec<DeliveryOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().rev().collect()),
            sent: Mutex::new(vec![]),
        }
    }
}
impl NotificationAdapter for Transport {
    fn send(&self, _: &Recipient, id: &str, _: &str, _: &str) -> DeliveryOutcome {
        self.sent.lock().unwrap().push(id.into());
        self.outcomes
            .lock()
            .unwrap()
            .pop()
            .expect("unexpected duplicate send")
    }
}
fn setup() -> (TempDir, Application, ManualClock, String) {
    let d = TempDir::new().unwrap();
    let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    let c = a
        .create_case(FakeSource.manual_command("offline-models"))
        .unwrap();
    a.claim().unwrap();
    t.advance(Duration::seconds(61));
    a.supervise_team().unwrap();
    t.advance(Duration::seconds(121));
    a.supervise_team().unwrap();
    assert_eq!(a.escalation_inbox().unwrap().len(), 1);
    (d, a, t, c.id)
}
fn config() -> NotificationConfig {
    NotificationConfig {
        schema_version: V1,
        revision: 1,
        recipients: vec![
            Recipient {
                owner: "primary-human".into(),
                webhook_env: "WATCHHOUSE_NOTIFY_FIXTURE_PRIMARY".into(),
            },
            Recipient {
                owner: "backup-human".into(),
                webhook_env: "WATCHHOUSE_NOTIFY_FIXTURE_BACKUP".into(),
            },
        ],
        acknowledge_seconds: 10,
        response_seconds: 30,
        retry_seconds: 1,
        max_attempts: 2,
    }
}
fn cmd(a: &Application, id: &str, key: &str, actor: &str) -> OperatorCommand {
    OperatorCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: actor.into(),
            reason: "Fixture human response".into(),
        },
        expected_version: a.snapshot(id).unwrap().case.version,
    }
}
fn evidence(a: &Application, id: &str) -> String {
    a.record_evidence(id,serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":"resolution-evidence","actor":"operator","reason":"Fixture check","expected_version":a.snapshot(id).unwrap().case.version,"scope":"fixture only","statement":"Human confirmed owned next action","collection_method":"Sanitized fixture","observed_at":null,"uncertainty":"No production recovery asserted"})).unwrap()).unwrap().evidence.digest
}
#[test]
fn models_offline_retry_restart_fallback_acknowledgment_and_explicit_resolution() {
    let (d, a, t, id) = setup();
    assert!(
        a.configure_notifications(config(), "fake-investigator")
            .is_err()
    );
    a.configure_notifications(config(), "operator").unwrap();
    let transport = Transport::new(vec![
        DeliveryOutcome::RetryAfter(2),
        DeliveryOutcome::Accepted,
        DeliveryOutcome::Accepted,
    ]);
    a.process_notifications(&transport).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].state, "retry");
    t.advance(Duration::seconds(2));
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    a.process_notifications(&transport).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].state, "accepted");
    assert_eq!(a.process_notifications(&transport).unwrap(), 0);
    t.advance(Duration::seconds(9));
    a.process_notifications(&transport).unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.notifications.len(), 2);
    assert_eq!(s.notifications[1].recipient, "backup-human");
    let e = &s.team.escalations[0];
    a.acknowledge_escalation(
        &id,
        AcknowledgeEscalation {
            command: cmd(&a, &id, "backup-ack", "backup-human"),
            escalation_id: e.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(a.escalation_inbox().unwrap()[0].state, "acknowledged");
    assert_eq!(a.process_notifications(&transport).unwrap(), 0);
    let proof = evidence(&a, &id);
    let resolve = ResolveEscalation {
        command: cmd(&a, &id, "resolve", "backup-human"),
        escalation_id: e.id.clone(),
        outcome: "Assigned explicit follow-up to the operator".into(),
        uncertainty: "Service outcome unknown".into(),
        evidence_refs: vec![proof],
    };
    a.resolve_escalation(&id, resolve.clone()).unwrap();
    a.resolve_escalation(&id, resolve).unwrap();
    t.advance(Duration::seconds(100));
    assert_eq!(a.process_notifications(&transport).unwrap(), 0);
    assert_eq!(
        a.snapshot(&id).unwrap().team.escalations[0].state,
        "resolved"
    );
    assert_eq!(transport.sent.lock().unwrap().len(), 3);
    // Resolving notification duty never clears the underlying assignment blocker.
    assert_eq!(
        a.snapshot(&id).unwrap().team.control.unwrap().state,
        "blocked"
    );
}
#[test]
fn uncertain_delivery_is_never_blindly_retried_and_process_loss_is_reconciled() {
    let (d, a, t, id) = setup();
    a.configure_notifications(config(), "operator").unwrap();
    let transport = Transport::new(vec![DeliveryOutcome::Uncertain]);
    a.process_notifications(&transport).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].state, "uncertain");
    assert_eq!(a.process_notifications(&transport).unwrap(), 0);
    let delivery = a.snapshot(&id).unwrap().notifications[0].id.clone();
    let proof = evidence(&a, &id);
    a.reconcile_delivery(
        &id,
        ReconcileDelivery {
            command: cmd(&a, &id, "reconcile", "operator"),
            delivery_id: delivery.clone(),
            delivered: true,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].state, "accepted");
    // Simulate process loss after durable claim, before effect outcome storage.
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute(
        "UPDATE notification_deliveries SET state='inflight',lease_until=?1 WHERE id=?2",
        rusqlite::params!["2026-11-14T12:03:00.000Z", delivery],
    )
    .unwrap();
    let a = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(t));
    a.process_notifications(&transport).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].state, "uncertain");
    assert_eq!(transport.sent.lock().unwrap().len(), 1);
}
#[test]
fn exhausted_chain_and_failed_finalization_remain_visible_without_duplicate_effects() {
    let (d, a, _t, id) = setup();
    a.configure_notifications(config(), "operator").unwrap();
    let transport = Transport::new(vec![DeliveryOutcome::Rejected, DeliveryOutcome::Rejected]);
    a.process_notifications(&transport).unwrap();
    a.process_notifications(&transport).unwrap();
    a.process_notifications(&transport).unwrap();
    assert_eq!(a.escalation_inbox().unwrap()[0].state, "open");
    assert_eq!(a.snapshot(&id).unwrap().notifications.len(), 2);
    assert_eq!(a.process_notifications(&transport).unwrap(), 0);
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    let exhausted: i64 = db
        .query_row("SELECT exhausted FROM escalation_routes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(exhausted, 1);
    // Start an independent database to inject failure after the transport accepted.
    let (_d2, b, _t2, case) = setup();
    b.configure_notifications(config(), "operator").unwrap();
    let store = Store::open(_d2.path().join("db")).unwrap();
    let db = store.connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_delivery BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='escalation.delivery_changed' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let once = Transport::new(vec![DeliveryOutcome::Accepted]);
    assert!(b.process_notifications(&once).is_err());
    assert_eq!(
        b.snapshot(&case).unwrap().notifications[0].state,
        "inflight"
    );
    assert_eq!(once.sent.lock().unwrap().len(), 1);
}

#[test]
fn concurrent_notification_ticks_claim_one_external_effect() {
    let (_d, a, _t, id) = setup();
    a.configure_notifications(config(), "operator").unwrap();
    let transport = Arc::new(Transport::new(vec![DeliveryOutcome::Accepted]));
    let b = a.clone();
    let c = a.clone();
    let t = transport.clone();
    let u = transport.clone();
    let first = std::thread::spawn(move || b.process_notifications(t.as_ref()));
    let second = std::thread::spawn(move || c.process_notifications(u.as_ref()));
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    assert_eq!(transport.sent.lock().unwrap().len(), 1);
    assert_eq!(a.snapshot(&id).unwrap().notifications[0].attempts, 1);
}
