use chrono::{Duration, TimeZone, Utc};
use std::{
    process::Command,
    sync::{Arc, Barrier},
};
use tempfile::TempDir;
use watchhouse::{
    application::Application,
    contracts::*,
    domain::{CaseStatus, ManualClock},
    error::Error,
    execution::{ExecutionAdapter, FakeAdapter},
    store::{Store, digest},
};
fn clock() -> ManualClock {
    ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap())
}
fn setup() -> (TempDir, Store, Application, ManualClock) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("watchhouse.db")).unwrap();
    let clock = clock();
    let app = Application::new(store.clone(), Arc::new(clock.clone()));
    (dir, store, app, clock)
}
fn command() -> CreateCase {
    serde_json::from_str(include_str!("../contracts/v1/manual-case.json")).unwrap()
}
#[test]
fn vertical_slice_keeps_review_and_evidence_explicit() {
    let (_dir, store, app, clock) = setup();
    let case = app.create_case(command()).unwrap();
    assert_eq!(case.version, 2);
    assert_eq!(case.status, CaseStatus::Queued);
    let queued = app.snapshot(&case.id).unwrap();
    assert_eq!(queued.assignment.owner, case.owner);
    assert_eq!(queued.events.len(), 2);
    assert!(app.run_once(&FakeAdapter).unwrap());
    assert!(!app.run_once(&FakeAdapter).unwrap());
    let result = app.snapshot(&case.id).unwrap();
    assert_eq!(result.case.status, CaseStatus::AwaitingReview);
    assert_eq!(result.case.version, 4);
    assert_eq!(result.results[0].conclusion, "insufficient_evidence");
    assert!(result.results[0].actual_model.is_none());
    let EventData::AssignmentCompleted { evidence_refs, .. } = &result.events.last().unwrap().data
    else {
        panic!("missing result event")
    };
    let content = app.evidence(&evidence_refs[0]).unwrap();
    assert_eq!(digest(&content), evidence_refs[0]);
    let reopened = Application::new(Store::open(store_path(&_dir)).unwrap(), Arc::new(clock));
    assert_eq!(reopened.snapshot(&case.id).unwrap().events.len(), 4);
    assert_eq!(
        app.create_case(command()).unwrap().version,
        2,
        "duplicate receipt is the original response"
    );
    assert_eq!(
        store
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE state='completed'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
fn store_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("watchhouse.db")
}
#[test]
fn duplicate_commands_and_concurrent_updates_have_one_winner() {
    let (_dir, _store, app, _clock) = setup();
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let app = app.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                app.create_case(command()).unwrap()
            })
        })
        .collect();
    let cases: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert!(cases.iter().all(|c| c.id == cases[0].id));
    let mut changed = command();
    changed.title = "Different payload".into();
    assert!(matches!(app.create_case(changed), Err(Error::Conflict(_))));
    let case = cases[0].clone();
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2)
        .map(|i| {
            let app = app.clone();
            let id = case.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                app.rename_case(
                    &id,
                    RenameCase {
                        schema_version: V1,
                        idempotency_key: format!("rename-{i}"),
                        actor: "operator".into(),
                        reason: "Correct title".into(),
                        title: format!("Updated {i}"),
                        expected_version: 2,
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::Conflict(_))))
            .count(),
        1
    );
    assert_eq!(app.snapshot(&case.id).unwrap().case.version, 3);
}
#[test]
fn concurrent_claims_are_exclusive_and_expired_results_are_fenced() {
    let (_dir, _store, app, clock) = setup();
    let case = app.create_case(command()).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let app = app.clone();
            std::thread::spawn(move || app.claim().unwrap())
        })
        .collect();
    let claims: Vec<_> = threads
        .into_iter()
        .filter_map(|t| t.join().unwrap())
        .collect();
    assert_eq!(claims.len(), 1);
    let old = &claims[0];
    let result = FakeAdapter.execute(&old.task);
    clock.advance(Duration::seconds(30));
    assert!(matches!(
        app.complete(old, result.clone()),
        Err(Error::Conflict(_))
    ));
    assert!(
        app.claim().unwrap().is_none(),
        "lost worker becomes an owned block, never a silent retry"
    );
    let new = recover_blocked(&app, &case.id);
    assert_ne!(old.fence, new.fence);
    assert!(matches!(app.complete(old, result), Err(Error::Conflict(_))));
    let result = FakeAdapter.execute(&new.task);
    app.complete(&new, result.clone()).unwrap();
    app.complete(&new, result).unwrap();
    let state = app.snapshot(&case.id).unwrap();
    assert_eq!(state.results.len(), 1);
    assert_eq!(state.team.late_results.len(), 1);
    assert_eq!(state.team.blocks.len(), 1);
    assert_eq!(state.team.handoffs[0].state, "accepted");
    assert_eq!(state.team.control.unwrap().state, "completed");
}
#[test]
fn faults_roll_back_state_events_work_receipts_and_evidence() {
    let (_dir, store, app, _clock) = setup();
    let db = store.connect().unwrap();
    db.execute_batch(
        "CREATE TRIGGER fault BEFORE INSERT ON jobs BEGIN SELECT RAISE(ABORT,'injected'); END;",
    )
    .unwrap();
    assert!(app.create_case(command()).is_err());
    for table in ["cases", "assignments", "events", "jobs", "commands"] {
        assert_eq!(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    db.execute_batch("DROP TRIGGER fault").unwrap();
    let case = app.create_case(command()).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fault BEFORE INSERT ON runs BEGIN SELECT RAISE(ABORT,'injected'); END;",
    )
    .unwrap();
    assert!(app.claim().is_err());
    assert_eq!(
        app.snapshot(&case.id).unwrap().case.status,
        CaseStatus::Queued
    );
    db.execute_batch("DROP TRIGGER fault").unwrap();
    let claim = app.claim().unwrap().unwrap();
    db.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='assignment.completed' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(
        app.complete(&claim, FakeAdapter.execute(&claim.task))
            .is_err()
    );
    assert_eq!(
        app.snapshot(&case.id).unwrap().case.status,
        CaseStatus::Running
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM evidence", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM runs WHERE result IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fault").unwrap();
    app.complete(&claim, FakeAdapter.execute(&claim.task))
        .unwrap();
    assert!(db.execute("DELETE FROM events", []).is_err());
    assert!(
        db.execute("UPDATE events SET aggregate_version=99", [])
            .is_err()
    );
}
#[test]
fn migration_integrity_and_contract_versions_are_enforced() {
    let (dir, store, app, _clock) = setup();
    Store::open(store_path(&dir)).unwrap();
    let mut fixture: serde_json::Value =
        serde_json::from_str(include_str!("../contracts/v1/manual-case.json")).unwrap();
    fixture["schema_version"] = 2.into();
    assert!(serde_json::from_value::<CreateCase>(fixture).is_err());
    let case = app.create_case(command()).unwrap();
    let snapshot = app.snapshot(&case.id).unwrap();
    for event in snapshot.events {
        let value = serde_json::to_value(&event).unwrap();
        assert!(serde_json::from_value::<Event>(value).is_ok());
    }
    store
        .connect()
        .unwrap()
        .execute("UPDATE schema_migrations SET digest='changed'", [])
        .unwrap();
    assert!(matches!(
        Store::open(store_path(&dir)),
        Err(Error::Conflict(_))
    ));
}
// This helper exits without destructors, leaving real SQLite WAL state behind.
#[test]
fn crash_child() {
    let Ok(path) = std::env::var("WATCHHOUSE_CRASH_DB") else {
        return;
    };
    let stage = std::env::var("WATCHHOUSE_CRASH_STAGE").unwrap();
    let app = Application::new(Store::open(path).unwrap(), Arc::new(clock()));
    app.create_case(command()).unwrap();
    if stage == "intake" {
        std::process::exit(17)
    }
    let claim = app.claim().unwrap().unwrap();
    if stage == "claim" {
        std::process::exit(17)
    }
    let result = FakeAdapter.execute(&claim.task);
    if stage == "execution" {
        std::process::exit(17)
    }
    app.complete(&claim, result).unwrap();
    std::process::exit(17)
}
#[test]
fn process_loss_at_every_committed_boundary_recovers_without_lost_work() {
    for stage in ["intake", "claim", "execution", "completion"] {
        let dir = TempDir::new().unwrap();
        let path = store_path(&dir);
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "crash_child"])
            .env("WATCHHOUSE_CRASH_DB", &path)
            .env("WATCHHOUSE_CRASH_STAGE", stage)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        let clock = clock();
        clock.advance(Duration::seconds(31));
        let app = Application::new(Store::open(path).unwrap(), Arc::new(clock));
        let case = app.create_case(command()).unwrap();
        app.run_once(&FakeAdapter).unwrap();
        if app
            .snapshot(&case.id)
            .unwrap()
            .team
            .control
            .as_ref()
            .unwrap()
            .state
            == "blocked"
        {
            let successor = recover_blocked(&app, &case.id);
            app.complete(&successor, FakeAdapter.execute(&successor.task))
                .unwrap();
        }
        let snapshot = app.snapshot(&case.id).unwrap();
        assert_eq!(app.list_cases().unwrap().len(), 1);
        assert_eq!(snapshot.case.status, CaseStatus::AwaitingReview);
        assert_eq!(snapshot.results.len(), 1);
        assert!(!app.run_once(&FakeAdapter).unwrap());
    }
}
#[test]
fn invalid_or_misattributed_results_cannot_advance_work() {
    let (_dir, _store, app, _clock) = setup();
    let case = app
        .create_case(watchhouse::intake::FakeSource.manual_command("fake-source-1"))
        .unwrap();
    let claim = app.claim().unwrap().unwrap();
    let mut result = FakeAdapter.execute(&claim.task);
    result.assignment_id = "another-assignment".into();
    assert!(matches!(
        app.complete(&claim, result),
        Err(Error::Invalid(_))
    ));
    let mut forged = claim.clone();
    forged.run_id = "unrelated-run".into();
    assert!(matches!(
        app.complete(&forged, FakeAdapter.execute(&claim.task)),
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        app.snapshot(&case.id).unwrap().case.status,
        CaseStatus::Running
    );
    let result = FakeAdapter.execute(&claim.task);
    assert_eq!(
        serde_json::to_string(&result).unwrap(),
        serde_json::to_string(&FakeAdapter.execute(&claim.task)).unwrap()
    );
    app.complete(&claim, result).unwrap();
}
#[test]
fn versioned_event_fixture_rejects_unknown_versions_types_and_fields() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../contracts/v1/event.json")).unwrap();
    assert!(serde_json::from_value::<Event>(fixture.clone()).is_ok());
    for (field, value) in [
        ("schema_version", serde_json::json!(2)),
        ("type", serde_json::json!("policy.bypass")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut invalid = fixture.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<Event>(invalid).is_err());
    }
    let mut invalid = fixture;
    invalid["payload"]["authority"] = serde_json::json!("production");
    assert!(serde_json::from_value::<Event>(invalid).is_err());
}

fn recover_blocked(app: &Application, id: &str) -> watchhouse::application::Claim {
    let s = app.snapshot(id).unwrap();
    assert_eq!(s.case.owner, "operator");
    let h = app
        .offer_handoff(
            id,
            OfferHandoff {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "recovery-offer".into(),
                        actor: "operator".into(),
                        reason: "Recover committed work through explicit ownership".into(),
                    },
                    expected_version: s.case.version,
                },
                block_id: s.team.blocks[0].id.clone(),
                successor: "fake-recovery".into(),
                next_owner: "operator".into(),
                acknowledge_within_seconds: 60,
            },
        )
        .unwrap();
    let s = app.snapshot(id).unwrap();
    assert_eq!(s.case.owner, "operator");
    app.accept_handoff(
        id,
        AcceptHandoff {
            command: OperatorCommand {
                meta: CommandMeta {
                    schema_version: V1,
                    idempotency_key: "recovery-accept".into(),
                    actor: "fake-recovery".into(),
                    reason: "Accept bounded durable briefing".into(),
                },
                expected_version: s.case.version,
            },
            handoff_id: h.id,
        },
    )
    .unwrap()
}
