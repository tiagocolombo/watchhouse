use chrono::{TimeZone, Utc};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use watchhouse::{
    adapter_trials::{Action, Scenario, State, TrialCommand},
    application::Application,
    contracts::*,
    domain::ManualClock,
    repair::RepairRequest,
    store::{Store, digest},
};
fn setup() -> (TempDir, Application, Arc<ManualClock>, String, String) {
    let d = TempDir::new().unwrap();
    let store = Store::open(d.path().join("test.db")).unwrap();
    let clock = Arc::new(ManualClock::new(
        Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap(),
    ));
    let app = Application::new(store.clone(), clock.clone());
    let case=app.create_case(serde_json::from_value(json!({"schema_version":1,"idempotency_key":"case","actor":"operator","reason":"fixture","title":"Synthetic protocol","description":"Synthetic fixture"})).unwrap()).unwrap();
    // Seed an immutable synthetic request, without preparing or executing any source.
    let repair = RepairRequest {
        schema_version: V1,
        id: "repair-fixture".into(),
        case_id: case.id.clone(),
        owner: "operator".into(),
        state: "blocked".into(),
        source_revision: 1,
        config_digest: digest("config"),
        manifest_digest: digest("manifest"),
        repository: "fixture/checkout".into(),
        base_commit: "a".repeat(40),
        base_tree: "b".repeat(40),
        policy_provenance: "fixture".into(),
        objective: "<script>untrusted</script>".into(),
        expected_behavior: "synthetic".into(),
        regression_plan: "unexecuted".into(),
        evidence_refs: vec![],
        recorded_at: "2026-11-15T12:00:00.000Z".into(),
        denials: vec!["No admitted worker".into()],
        next_action: "Qualify worker".into(),
    };
    store
        .connect()
        .unwrap()
        .execute(
            "INSERT INTO repair_requests VALUES (?1,?2,?3)",
            rusqlite::params![repair.id, case.id, serde_json::to_string(&repair).unwrap()],
        )
        .unwrap();
    (d, app, clock, case.id, repair.id)
}
fn cmd(app: &Application, case: &str, key: &str, action: Action) -> TrialCommand {
    TrialCommand {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: key.into(),
                actor: "operator".into(),
                reason: "Synthetic protocol fixture".into(),
            },
            expected_version: app.snapshot(case).unwrap().case.version,
        },
        action,
    }
}
fn start(
    app: &Application,
    case: &str,
    repair: &str,
    scenario: Scenario,
) -> watchhouse::adapter_trials::Trial {
    app.adapter_trial(
        case,
        cmd(
            app,
            case,
            "start",
            Action::Start {
                repair_id: repair.into(),
                scenario,
                wall_seconds: 30,
            },
        ),
    )
    .unwrap()
}
#[test]
fn restart_reconciles_lost_start_without_repeating_and_replays_ordered_events() {
    let (d, app, clock, case, repair) = setup();
    let t = start(&app, &case, &repair, Scenario::StartUnknown);
    assert_eq!(t.state, State::Unknown);
    assert_eq!(t.pending_operation.as_deref(), Some("start"));
    let restarted = Application::new(Store::open(d.path().join("test.db")).unwrap(), clock);
    assert!(
        restarted
            .adapter_trial(
                &case,
                cmd(
                    &restarted,
                    &case,
                    "blind-repeat",
                    Action::Start {
                        repair_id: repair,
                        scenario: Scenario::Complete,
                        wall_seconds: 30
                    }
                )
            )
            .is_err()
    );
    let running = restarted
        .adapter_trial(
            &case,
            cmd(
                &restarted,
                &case,
                "reconcile",
                Action::Reconcile {
                    trial_id: t.id.clone(),
                },
            ),
        )
        .unwrap();
    assert_eq!(running.state, State::Running);
    assert_eq!(running.envelope.fence, t.envelope.fence);
    let complete = restarted
        .adapter_trial(
            &case,
            cmd(
                &restarted,
                &case,
                "complete",
                Action::Advance {
                    trial_id: t.id.clone(),
                },
            ),
        )
        .unwrap();
    assert_eq!(complete.state, State::Completed);
    assert!(!complete.real_execution_admitted);
    assert!(complete.result.as_ref().unwrap().actual_model.is_none());
    assert!(complete.result.as_ref().unwrap().synthetic);
    let events = restarted.adapter_trial_events(&case, &t.id, 0).unwrap();
    assert_eq!(
        events.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        restarted
            .adapter_trial_events(&case, &t.id, 2)
            .unwrap()
            .len(),
        2
    );
    assert!(restarted.adapter_trial_events(&case, &t.id, -1).is_err());
    let bytes = restarted
        .evidence(complete.evidence_digest.as_ref().unwrap())
        .unwrap();
    assert!(bytes.contains("fake-adapter-protocol-v1"));
    assert_eq!(
        restarted.snapshot(&case).unwrap().repairs[0].state,
        "blocked"
    );
    assert!(
        restarted
            .escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_id == t.id && e.state == "open")
    );
}
#[test]
fn lost_cancel_ack_is_unknown_and_same_key_inspects_current_status() {
    let (_d, app, _clock, case, repair) = setup();
    let start_cmd = cmd(
        &app,
        &case,
        "start",
        Action::Start {
            repair_id: repair,
            scenario: Scenario::CancelUnknown,
            wall_seconds: 30,
        },
    );
    let t = app.adapter_trial(&case, start_cmd.clone()).unwrap();
    let cancel = cmd(
        &app,
        &case,
        "cancel",
        Action::Cancel {
            trial_id: t.id.clone(),
        },
    );
    let unknown = app.adapter_trial(&case, cancel.clone()).unwrap();
    assert_eq!(unknown.state, State::Unknown);
    assert!(
        app.adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "late",
                Action::Advance {
                    trial_id: t.id.clone()
                }
            )
        )
        .is_err()
    );
    let terminal = app
        .adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "reconcile",
                Action::Reconcile { trial_id: t.id },
            ),
        )
        .unwrap();
    assert_eq!(terminal.state, State::Cancelled);
    assert!(terminal.result.is_none());
    assert_eq!(
        app.adapter_trial(&case, cancel).unwrap().sequence,
        terminal.sequence
    );
    assert_eq!(
        app.adapter_trial(&case, start_cmd).unwrap().state,
        State::Cancelled
    );
    assert_eq!(app.snapshot(&case).unwrap().adapter_trials.len(), 1);
}
#[test]
fn model_offline_supervision_does_not_refresh_deadlines_or_repeat_cancel() {
    for scenario in [Scenario::Stall, Scenario::CancelUnknown] {
        let (_d, app, clock, case, repair) = setup();
        let t = start(&app, &case, &repair, scenario);
        app.adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "heartbeat",
                Action::Advance {
                    trial_id: t.id.clone(),
                },
            ),
        )
        .unwrap();
        clock.advance(chrono::Duration::seconds(31));
        app.supervise_team().unwrap();
        let after = app.snapshot(&case).unwrap().adapter_trials.remove(0);
        assert_eq!(
            after.state,
            if scenario == Scenario::Stall {
                State::TimedOut
            } else {
                State::Unknown
            }
        );
        assert_eq!(after.envelope.deadline, t.envelope.deadline);
        app.supervise_team().unwrap();
        assert_eq!(
            app.snapshot(&case).unwrap().adapter_trials[0].sequence,
            after.sequence
        );
        if after.state == State::Unknown {
            let r = app
                .adapter_trial(
                    &case,
                    cmd(
                        &app,
                        &case,
                        "reconcile",
                        Action::Reconcile { trial_id: t.id },
                    ),
                )
                .unwrap();
            assert_eq!(r.state, State::Cancelled);
        }
    }
}
#[test]
fn invalid_success_output_and_structured_quota_cannot_pass() {
    for scenario in [Scenario::InvalidOutput, Scenario::Quota] {
        let (_d, app, _clock, case, repair) = setup();
        let t = start(&app, &case, &repair, scenario);
        let r = app
            .adapter_trial(
                &case,
                cmd(&app, &case, "advance", Action::Advance { trial_id: t.id }),
            )
            .unwrap();
        assert_eq!(r.state, State::Failed);
        assert!(r.result.is_none());
        assert!(r.evidence_digest.is_some());
        assert!(!r.real_execution_admitted);
        assert!(
            app.escalation_inbox()
                .unwrap()
                .iter()
                .any(|e| e.source_id == r.id)
        );
    }
}
#[test]
fn owner_version_scope_schema_and_idempotency_conflicts_fail() {
    let (_d, app, _clock, case, repair) = setup();
    let mut c = cmd(
        &app,
        &case,
        "start",
        Action::Start {
            repair_id: repair.clone(),
            scenario: Scenario::Complete,
            wall_seconds: 30,
        },
    );
    c.command.meta.actor = "other".into();
    assert!(app.adapter_trial(&case, c.clone()).is_err());
    c.command.meta.actor = "fake-engineer".into();
    assert!(app.adapter_trial(&case, c.clone()).is_err());
    c.command.meta.actor = "operator".into();
    c.command.expected_version = -1;
    assert!(app.adapter_trial(&case, c).is_err());
    let t = start(&app, &case, &repair, Scenario::Complete);
    assert!(
        app.adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "start",
                Action::Cancel {
                    trial_id: t.id.clone()
                }
            )
        )
        .is_err()
    );
    let other=app.create_case(serde_json::from_value(json!({"schema_version":1,"idempotency_key":"other","actor":"operator","reason":"fixture","title":"Other case","description":"Synthetic fixture"})).unwrap()).unwrap();
    assert!(app.adapter_trial_events(&other.id, &t.id, 0).is_err());
    assert!(
        app.adapter_trial(
            &other.id,
            cmd(
                &app,
                &other.id,
                "foreign",
                Action::Cancel { trial_id: t.id }
            )
        )
        .is_err()
    );
    let raw = json!({"command":{"meta":{"schema_version":1,"idempotency_key":"schema","actor":"operator","reason":"fixture"},"expected_version":0},"action":{"type":"start","repair_id":repair,"scenario":"complete","wall_seconds":30}});
    assert!(serde_json::from_value::<TrialCommand>(raw.clone()).is_ok());
    let mut invalid = raw.clone();
    invalid["command"]["meta"]["schema_version"] = json!(2);
    assert!(serde_json::from_value::<TrialCommand>(invalid).is_err());
    let mut invalid = raw;
    invalid["runtime_path"] = json!("/bin/sh");
    assert!(serde_json::from_value::<TrialCommand>(invalid).is_err());
}
#[test]
fn rollback_leaves_no_backend_session_or_receipt_and_retry_is_safe() {
    let (d, app, _clock, case, repair) = setup();
    let db = Store::open(d.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fail_trial_event BEFORE INSERT ON adapter_trial_events BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let c = cmd(
        &app,
        &case,
        "start",
        Action::Start {
            repair_id: repair,
            scenario: Scenario::Complete,
            wall_seconds: 30,
        },
    );
    assert!(app.adapter_trial(&case, c.clone()).is_err());
    for table in [
        "adapter_trials",
        "fake_adapter_sessions",
        "adapter_trial_events",
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    db.execute_batch("DROP TRIGGER fail_trial_event").unwrap();
    let t = app.adapter_trial(&case, c).unwrap();
    let advance = cmd(
        &app,
        &case,
        "advance",
        Action::Advance {
            trial_id: t.id.clone(),
        },
    );
    db.execute_batch("CREATE TRIGGER fail_trial_save BEFORE UPDATE ON adapter_trials BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(app.adapter_trial(&case, advance.clone()).is_err());
    assert_eq!(
        app.snapshot(&case).unwrap().adapter_trials[0].state,
        State::Running
    );
    let backend: String = db
        .query_row(
            "SELECT state FROM fake_adapter_sessions WHERE id=?1",
            [&t.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(backend, "running");
    db.execute_batch("DROP TRIGGER fail_trial_save").unwrap();
    assert_eq!(
        app.adapter_trial(&case, advance).unwrap().state,
        State::Completed
    );
}
#[test]
fn missing_backend_is_not_absence_proof_and_expired_start_reconciles_to_timeout() {
    let (d, app, clock, case, repair) = setup();
    let t = start(&app, &case, &repair, Scenario::StartUnknown);
    let db = Store::open(d.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute("DELETE FROM fake_adapter_sessions WHERE id=?1", [&t.id])
        .unwrap();
    let r = app
        .adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "missing",
                Action::Reconcile {
                    trial_id: t.id.clone(),
                },
            ),
        )
        .unwrap();
    assert_eq!(r.state, State::Unknown);
    clock.advance(chrono::Duration::seconds(31));
    app.supervise_team().unwrap();
    assert_eq!(
        app.snapshot(&case).unwrap().adapter_trials[0].state,
        State::Unknown
    );
    db.execute(
        "INSERT INTO fake_adapter_sessions VALUES (?1,'running',NULL)",
        [&t.id],
    )
    .unwrap();
    let r = app
        .adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "observed",
                Action::Reconcile { trial_id: t.id },
            ),
        )
        .unwrap();
    assert_eq!(r.state, State::TimedOut);
}

#[test]
fn concurrent_starts_and_attempt_budget_cannot_create_extra_sessions() {
    let (_d, app, _clock, case, repair) = setup();
    let c = cmd(
        &app,
        &case,
        "start-a",
        Action::Start {
            repair_id: repair.clone(),
            scenario: Scenario::Complete,
            wall_seconds: 30,
        },
    );
    let mut c2 = c.clone();
    c2.command.meta.idempotency_key = "start-b".into();
    let a = app.clone();
    let case2 = case.clone();
    let thread = std::thread::spawn(move || a.adapter_trial(&case2, c2));
    let first = app.adapter_trial(&case, c);
    let second = thread.join().unwrap();
    assert_ne!(first.is_ok(), second.is_ok());
    let t = first.or(second).unwrap();
    app.adapter_trial(
        &case,
        cmd(
            &app,
            &case,
            "cancel-first",
            Action::Cancel { trial_id: t.id },
        ),
    )
    .unwrap();
    for n in 2..=3 {
        let t = app
            .adapter_trial(
                &case,
                cmd(
                    &app,
                    &case,
                    &format!("start-{n}"),
                    Action::Start {
                        repair_id: repair.clone(),
                        scenario: Scenario::Complete,
                        wall_seconds: 30,
                    },
                ),
            )
            .unwrap();
        assert_eq!(t.envelope.attempt, n);
        app.adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                &format!("cancel-{n}"),
                Action::Cancel { trial_id: t.id },
            ),
        )
        .unwrap();
    }
    assert!(
        app.adapter_trial(
            &case,
            cmd(
                &app,
                &case,
                "fourth",
                Action::Start {
                    repair_id: repair,
                    scenario: Scenario::Complete,
                    wall_seconds: 30
                }
            )
        )
        .is_err()
    );
    assert_eq!(app.snapshot(&case).unwrap().adapter_trials.len(), 3);
}

#[test]
fn lost_running_backend_never_fabricates_a_result_or_cancel_confirmation() {
    for cancel in [false, true] {
        let (d, app, _clock, case, repair) = setup();
        let t = start(&app, &case, &repair, Scenario::Complete);
        let db = Store::open(d.path().join("test.db"))
            .unwrap()
            .connect()
            .unwrap();
        db.execute("DELETE FROM fake_adapter_sessions WHERE id=?1", [&t.id])
            .unwrap();
        let action = if cancel {
            Action::Cancel { trial_id: t.id }
        } else {
            Action::Advance { trial_id: t.id }
        };
        let r = app
            .adapter_trial(&case, cmd(&app, &case, "observe", action))
            .unwrap();
        assert_eq!(r.state, State::Unknown);
        assert!(r.result.is_none());
        assert!(r.evidence_digest.is_none());
    }
}

#[test]
fn expired_missing_backend_stays_unknown_instead_of_claiming_cancellation() {
    let (d, app, clock, case, repair) = setup();
    let t = start(&app, &case, &repair, Scenario::Stall);
    let db = Store::open(d.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute("DELETE FROM fake_adapter_sessions WHERE id=?1", [&t.id])
        .unwrap();
    clock.advance(chrono::Duration::seconds(31));
    app.supervise_team().unwrap();
    let r = app.adapter_trial_status(&case, &t.id).unwrap();
    assert_eq!(r.state, State::Unknown);
    assert!(r.result.is_none());
    app.supervise_team().unwrap();
    assert_eq!(
        app.adapter_trial_status(&case, &t.id).unwrap().sequence,
        r.sequence
    );
}
