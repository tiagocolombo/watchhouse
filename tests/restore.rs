mod common;
use chrono::{Duration, Utc};
use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};
use watchhouse::{
    application::Application, contracts::*, domain::ManualClock, error::Result, intake::FakeSource,
    notifications::*, restore, store::Store,
};
struct Sources {
    allowed: bool,
    calls: Cell<usize>,
}
impl Sources {
    fn called(&self) {
        assert!(
            self.allowed,
            "restored credential authority reached external I/O"
        );
        self.calls.set(self.calls.get() + 1);
    }
}
impl watchhouse::sentry::SentryApi for Sources {
    fn page(
        &self,
        _: &watchhouse::sentry::SentryConfig,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> Result<watchhouse::sentry::Page> {
        self.called();
        Ok(watchhouse::sentry::Page {
            issues: vec![],
            next_cursor: None,
        })
    }
    fn enrich(
        &self,
        _: &watchhouse::sentry::SentryConfig,
        _: &str,
    ) -> Result<watchhouse::sentry::Enrichment> {
        self.called();
        Ok(watchhouse::sentry::Enrichment {
            issue: serde_json::from_str(include_str!("fixtures/sentry/issue.json"))?,
            event: serde_json::from_str(include_str!("fixtures/sentry/event.json"))?,
        })
    }
}
impl watchhouse::datadog::Api for Sources {
    fn monitor(&self, _: &watchhouse::datadog::Config, id: u64) -> Result<serde_json::Value> {
        self.called();
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/datadog/monitor.json"))?;
        v["id"] = id.into();
        Ok(v)
    }
}
fn reopen(path: &std::path::Path, r: &restore::Record) -> (Application, ManualClock) {
    let t = ManualClock::new(
        chrono::DateTime::parse_from_rfc3339(&r.restored_at)
            .unwrap()
            .with_timezone(&Utc)
            + Duration::seconds(1),
    );
    (
        Application::new(
            Store::open(path.join("watchhouse.db")).unwrap(),
            Arc::new(t.clone()),
        ),
        t,
    )
}
#[test]
fn restored_source_credentials_stay_held_after_resume_until_fresh_os_installation() {
    let (d, a, _t, case) = common::fixture();
    let mut dd: watchhouse::datadog::Config =
        serde_json::from_str(include_str!("fixtures/datadog/config.json")).unwrap();
    a.configure_datadog(dd.clone(), "operator", "Synthetic source configuration")
        .unwrap();
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "Source restore drill")
        .unwrap();
    let output = d.path().join("restored");
    let r = restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "Hold all copied source authority",
    )
    .unwrap();
    let (app, _clock) = reopen(&output, &r);
    let current = app.factory_status().unwrap();
    app.control_factory(watchhouse::factory::Command {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: "resume-source-fixture".into(),
            actor: "operator".into(),
            reason: "Resume does not authorize old credentials".into(),
        },
        expected_version: current.version,
        action: watchhouse::factory::Action::Resume {
            pause_id: r.pause_id,
        },
    })
    .unwrap();
    let denied = Sources {
        allowed: false,
        calls: Cell::new(0),
    };
    assert_eq!(app.sentry_tick(&denied).unwrap(), 0);
    assert!(!app.sync_datadog(&denied).unwrap());
    assert_eq!(denied.calls.get(), 0);
    assert!(app.configured_sentry_projects().unwrap()[0].restore_held);
    assert!(app.datadog_status().unwrap()[0].restore_held);
    let db = Store::open(output.join("watchhouse.db"))
        .unwrap()
        .connect()
        .unwrap();
    assert!(
        !db.query_row(
            "SELECT current FROM source_context_validity WHERE case_id=?1",
            [&case],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
    let mut sentry: watchhouse::sentry::SentryConfig =
        serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();
    assert!(
        app.receive_sentry_verified(
            b"{}",
            "invalid",
            b"sanitized-fixture-secret-32-bytes-only",
            &sentry
        )
        .unwrap_err()
        .to_string()
        .contains("restored authority")
    );
    sentry.revision = 2;
    app.configure_sentry(sentry, "operator").unwrap();
    dd.revision = 2;
    app.configure_datadog(dd, "operator", "Fresh explicit fixture authority")
        .unwrap();
    let allowed = Sources {
        allowed: true,
        calls: Cell::new(0),
    };
    app.sentry_tick(&allowed).unwrap();
    assert!(app.sync_datadog(&allowed).unwrap());
    assert!(allowed.calls.get() > 0);
    assert!(!app.configured_sentry_projects().unwrap()[0].restore_held);
    assert!(!app.datadog_status().unwrap()[0].restore_held);
    assert!(
        db.query_row(
            "SELECT current FROM source_context_validity WHERE case_id=?1",
            [case],
            |r| r.get::<_, bool>(0)
        )
        .unwrap()
    );
}
struct Sender {
    calls: Mutex<Vec<String>>,
    uncertain: bool,
}
impl NotificationAdapter for Sender {
    fn send(&self, _: &Recipient, _: &str, escalation: &str, _: &str) -> DeliveryOutcome {
        self.calls.lock().unwrap().push(escalation.into());
        if self.uncertain {
            DeliveryOutcome::Uncertain
        } else {
            DeliveryOutcome::Accepted
        }
    }
}
fn config(revision: i64) -> NotificationConfig {
    NotificationConfig {
        schema_version: V1,
        revision,
        recipients: vec![Recipient {
            owner: "fixture-human".into(),
            webhook_env: "WATCHHOUSE_NOTIFY_FIXTURE_RESTORE".into(),
        }],
        acknowledge_seconds: 60,
        response_seconds: 120,
        retry_seconds: 1,
        max_attempts: 2,
    }
}
fn evidence(a: &Application, case: &str, key: &str) -> String {
    a.record_evidence(case,serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":key,"actor":"operator","reason":"Review restored delivery uncertainty","expected_version":a.snapshot(case).unwrap().case.version,"scope":"synthetic notification only","statement":"Operator explicitly authorizes a new reminder despite possible prior delivery","collection_method":"Fixture human review","observed_at":null,"uncertainty":"No actual person or live destination contacted"})).unwrap()).unwrap().evidence.digest
}
#[test]
fn restored_notifications_require_explicit_reminders_and_persist_global_pacing_across_restart() {
    let d = tempfile::tempdir().unwrap();
    let t = ManualClock::new(Utc::now() - Duration::hours(1));
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    for n in 0..2 {
        a.create_case(FakeSource.manual_command(&format!("restore-notify-{n}")))
            .unwrap();
        a.claim().unwrap();
    }
    t.advance(Duration::seconds(61));
    a.supervise_team().unwrap();
    t.advance(Duration::seconds(121));
    a.supervise_team().unwrap();
    let escalations = a.escalation_inbox().unwrap();
    assert_eq!(escalations.len(), 2);
    let old_evidence = evidence(&a, &escalations[0].case_id, "old-reminder-proof");
    a.configure_notifications(config(1), "operator").unwrap();
    let old_sender = Sender {
        calls: Mutex::new(vec![]),
        uncertain: true,
    };
    a.process_notifications(&old_sender).unwrap();
    assert_eq!(old_sender.calls.lock().unwrap().len(), 2);
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "Notification restore drill")
        .unwrap();
    let output = d.path().join("restored");
    let r = restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "No old delivery replay",
    )
    .unwrap();
    let (app, t) = reopen(&output, &r);
    let sender = Sender {
        calls: Mutex::new(vec![]),
        uncertain: false,
    };
    app.process_notifications(&sender).unwrap();
    assert!(sender.calls.lock().unwrap().is_empty());
    app.configure_notifications(config(2), "operator").unwrap();
    app.process_notifications(&sender).unwrap();
    assert!(sender.calls.lock().unwrap().is_empty());
    let cmd = |a: &Application, e: &watchhouse::domain::team::Escalation, key: &str, refs| {
        restore::Reminder {
            command: common::command(a, &e.case_id, key, "operator"),
            escalation_id: e.id.clone(),
            evidence_refs: refs,
        }
    };
    assert!(
        app.reissue_restored_escalation(
            &escalations[0].case_id,
            cmd(
                &app,
                &escalations[0],
                "stale-reminder-proof",
                vec![old_evidence]
            )
        )
        .is_err()
    );
    let mut ids = vec![];
    for (n, e) in escalations.iter().enumerate() {
        let fresh = evidence(&app, &e.case_id, &format!("new-reminder-evidence-{n}"));
        let c = cmd(&app, e, &format!("explicit-reminder-{n}"), vec![fresh]);
        let id = app
            .reissue_restored_escalation(&e.case_id, c.clone())
            .unwrap();
        assert_eq!(id, app.reissue_restored_escalation(&e.case_id, c).unwrap());
        ids.push(id);
    }
    app.process_notifications(&sender).unwrap();
    assert_eq!(sender.calls.lock().unwrap().len(), 1);
    let restarted = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(t.clone()),
    );
    restarted.process_notifications(&sender).unwrap();
    assert_eq!(sender.calls.lock().unwrap().len(), 1);
    t.advance(Duration::seconds(30));
    restarted.process_notifications(&sender).unwrap();
    assert_eq!(sender.calls.lock().unwrap().len(), 2);
    for id in sender.calls.lock().unwrap().iter() {
        assert!(ids.contains(id));
        assert!(!escalations.iter().any(|e| &e.id == id));
    }
    assert_eq!(a.escalation_inbox().unwrap().len(), 2);
    assert_eq!(
        restarted
            .restore_status()
            .unwrap()
            .unwrap()
            .held_notification_routes,
        2
    );
}
