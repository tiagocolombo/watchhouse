use chrono::{Duration, TimeZone, Utc};
use rusqlite::params;
use std::{process::Command, sync::Arc};
use tempfile::TempDir;
use watchhouse::{
    application::Application,
    contracts::*,
    domain::{Case, CaseStatus, ManualClock},
    error::Error,
    execution::FakeAdapter,
    intake::FakeSource,
    store::{Store, digest},
};
fn clock() -> ManualClock {
    ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap())
}
fn app(path: &std::path::Path, clock: ManualClock) -> Application {
    Application::new(Store::open(path).unwrap(), Arc::new(clock))
}
fn setup() -> (TempDir, Application, ManualClock, Case) {
    let dir = TempDir::new().unwrap();
    let time = clock();
    let app = app(&dir.path().join("test.db"), time.clone());
    let case = app
        .create_case(FakeSource.manual_command("qualification-case"))
        .unwrap();
    app.run_once(&FakeAdapter).unwrap();
    let case = app.snapshot(&case.id).unwrap().case;
    (dir, app, time, case)
}
fn reported(app: &Application, id: &str, key: &str) -> String {
    let version = app.snapshot(id).unwrap().case.version;
    app.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: "operator".into(),
            reason: "Record a sanitized observation".into(),
            expected_version: version,
            scope: "fixture checkout / staging / build 1".into(),
            statement: format!("Operator fixture observation {key}; not independently verified"),
            collection_method: "Manual sanitized fixture report".into(),
            observed_at: None,
            uncertainty: "Impact and cause are not established".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
fn decision(app: &Application, id: &str, plan: QualificationPlan) -> RecordDisposition {
    let snapshot = app.snapshot(id).unwrap();
    RecordDisposition {schema_version:V1,idempotency_key:"decision-1".into(),actor:"operator".into(),reason:"Bounded qualification based on cited fixture evidence".into(),expected_version:snapshot.case.version,owner:"service-owner".into(),scope:"fixture checkout / staging / build 1".into(),uncertainty:"No conclusion about other scopes or incident recovery".into(),evidence_refs:snapshot.qualification.evidence.iter().map(|e|e.digest.clone()).collect(),reconsideration_conditions:"Reopen on new in-scope failures, contradictory evidence, scope change, or review deadline".into(),reopen_after_occurrences:2,plan}
}
fn reconsider(
    app: &Application,
    id: &str,
    key: &str,
    evidence: String,
    trigger: ReconsiderTrigger,
) -> ReconsiderCase {
    let s = app.snapshot(id).unwrap();
    ReconsiderCase {
        schema_version: V1,
        idempotency_key: key.into(),
        actor: "operator".into(),
        reason: "New observation requires reassessment".into(),
        expected_version: s.case.version,
        disposition_id: s.qualification.dispositions.last().unwrap().id.clone(),
        trigger,
        evidence_refs: vec![evidence],
    }
}
#[test]
fn expected_behavior_requires_reported_evidence_and_expiry_reopens_owned_review() {
    let (_dir, app, time, case) = setup();
    let unsupported = decision(
        &app,
        &case.id,
        QualificationPlan::ExpectedBehavior { after_seconds: 60 },
    );
    assert!(matches!(
        app.record_disposition(&case.id, unsupported),
        Err(Error::Invalid(_))
    ));
    reported(&app, &case.id, "expected-401-fixture");
    let cmd = decision(
        &app,
        &case.id,
        QualificationPlan::ExpectedBehavior { after_seconds: 60 },
    );
    let receipt = app.record_disposition(&case.id, cmd.clone()).unwrap();
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Active
    );
    assert_eq!(s.case.owner, "service-owner");
    time.advance(Duration::seconds(59));
    assert_eq!(app.process_due_obligations().unwrap(), 0);
    time.advance(Duration::seconds(1));
    assert_eq!(app.process_due_obligations().unwrap(), 1);
    assert_eq!(app.process_due_obligations().unwrap(), 0);
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Reopened
    );
    assert_eq!(s.qualification.obligations[0].state, ObligationState::Due);
    assert_eq!(s.qualification.obligations[0].owner, "service-owner");
    assert_eq!(s.case.status, CaseStatus::AwaitingReview);
    assert!(!app.run_once(&FakeAdapter).unwrap());
    assert_eq!(
        app.record_disposition(&case.id, cmd).unwrap().version,
        receipt.version
    );
    assert_eq!(s.results.len(), 1);
    assert_eq!(
        s.events
            .iter()
            .filter(|e| matches!(e.data, EventData::CaseReopened { .. }))
            .count(),
        1
    );
}
#[test]
fn transient_recurrence_is_deduplicated_and_reopens_at_its_explicit_threshold() {
    let (_dir, app, _time, case) = setup();
    let older = reported(&app, &case.id, "older-unused-report");
    let baseline = reported(&app, &case.id, "dependency-timeout");
    let mut d = decision(
        &app,
        &case.id,
        QualificationPlan::ApparentlyTransient {
            after_seconds: 600,
            recovery_indicators:
                "Observe successful fixture checkouts with sufficient request volume".into(),
        },
    );
    d.evidence_refs.retain(|digest| digest != &older);
    app.record_disposition(&case.id, d).unwrap();
    let prior = reconsider(
        &app,
        &case.id,
        "older-report-is-not-new",
        older,
        ReconsiderTrigger::Recurrence,
    );
    assert!(matches!(
        app.reconsider_case(&case.id, prior),
        Err(Error::Conflict(_))
    ));
    let reused = reconsider(
        &app,
        &case.id,
        "baseline-is-not-new",
        baseline,
        ReconsiderTrigger::Recurrence,
    );
    assert!(matches!(
        app.reconsider_case(&case.id, reused),
        Err(Error::Conflict(_))
    ));
    let first = reported(&app, &case.id, "recurrence-1");
    let cmd = reconsider(
        &app,
        &case.id,
        "observe-1",
        first.clone(),
        ReconsiderTrigger::Recurrence,
    );
    let receipt = app.reconsider_case(&case.id, cmd.clone()).unwrap();
    assert_eq!(
        app.reconsider_case(&case.id, cmd).unwrap().version,
        receipt.version
    );
    let reused = reconsider(
        &app,
        &case.id,
        "same-evidence-new-key",
        first,
        ReconsiderTrigger::Recurrence,
    );
    assert!(matches!(
        app.reconsider_case(&case.id, reused),
        Err(Error::Conflict(_))
    ));
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(s.qualification.dispositions[0].occurrence_count, 1);
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Active
    );
    let second = reported(&app, &case.id, "recurrence-2");
    let cmd = reconsider(
        &app,
        &case.id,
        "observe-2",
        second,
        ReconsiderTrigger::Recurrence,
    );
    app.reconsider_case(&case.id, cmd).unwrap();
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(s.qualification.dispositions[0].occurrence_count, 2);
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Reopened
    );
    assert_eq!(s.qualification.obligations[0].state, ObligationState::Due);
    assert_eq!(s.results.len(), 1);
}
#[test]
fn contradictory_evidence_reopens_expected_auth_behavior_without_suppressing_regressions() {
    let (_dir, app, _time, case) = setup();
    reported(&app, &case.id, "expected-unauthenticated-401");
    app.record_disposition(
        &case.id,
        decision(
            &app,
            &case.id,
            QualificationPlan::ExpectedBehavior { after_seconds: 600 },
        ),
    )
    .unwrap();
    let digest = reported(&app, &case.id, "authenticated-refresh-now-returns-401");
    let cmd = reconsider(
        &app,
        &case.id,
        "auth-regression",
        digest,
        ReconsiderTrigger::ContradictoryEvidence,
    );
    app.reconsider_case(&case.id, cmd).unwrap();
    let mut next = decision(
        &app,
        &case.id,
        QualificationPlan::InsufficientEvidence {
            after_seconds: 60,
            collection_task:
                "Reproduce refresh failure and compare authenticated and unauthenticated paths"
                    .into(),
        },
    );
    next.idempotency_key = "decision-2".into();
    app.record_disposition(&case.id, next).unwrap();
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(s.qualification.dispositions.len(), 2);
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Superseded
    );
    assert_eq!(
        s.qualification.obligations[0].state,
        ObligationState::Superseded
    );
    assert!(matches!(
        s.qualification.dispositions[1].record.plan,
        QualificationPlan::InsufficientEvidence { .. }
    ));
}
#[test]
fn invalid_decisions_and_cross_case_evidence_leave_no_partial_obligations() {
    let (dir, app, _time, case) = setup();
    let plan = QualificationPlan::InsufficientEvidence {
        after_seconds: 60,
        collection_task: "Collect a sanitized trace".into(),
    };
    let good = decision(&app, &case.id, plan);
    for changed in [
        "empty_evidence",
        "blank_scope",
        "no_task",
        "unbounded_time",
        "zero_threshold",
    ] {
        let mut invalid = good.clone();
        match changed {
            "empty_evidence" => invalid.evidence_refs.clear(),
            "blank_scope" => invalid.scope = " ".into(),
            "no_task" => {
                invalid.plan = QualificationPlan::InsufficientEvidence {
                    after_seconds: 1,
                    collection_task: "".into(),
                }
            }
            "unbounded_time" => {
                invalid.plan = QualificationPlan::ExpectedBehavior {
                    after_seconds: MAX_REVIEW_SECONDS + 1,
                }
            }
            _ => invalid.reopen_after_occurrences = 0,
        }
        assert!(matches!(
            app.record_disposition(&case.id, invalid),
            Err(Error::Invalid(_))
        ));
    }
    let other = app
        .create_case(FakeSource.manual_command("other-case"))
        .unwrap();
    app.run_once(&FakeAdapter).unwrap();
    let foreign = reported(&app, &other.id, "foreign-evidence");
    let mut invalid = good.clone();
    invalid.evidence_refs = vec![foreign];
    assert!(matches!(
        app.record_disposition(&case.id, invalid),
        Err(Error::Invalid(_))
    ));
    let store = Store::open(dir.path().join("test.db")).unwrap();
    let db = store.connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_obligation BEFORE INSERT ON obligations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(app.record_disposition(&case.id, good.clone()).is_err());
    let s = app.snapshot(&case.id).unwrap();
    assert!(s.qualification.dispositions.is_empty());
    assert_eq!(s.case.version, case.version);
    assert_eq!(s.case.owner, case.owner);
    db.execute_batch("DROP TRIGGER fail_obligation").unwrap();
    app.record_disposition(&case.id, good).unwrap();
    let mut next = decision(
        &app,
        &case.id,
        QualificationPlan::InsufficientEvidence {
            after_seconds: 1,
            collection_task: "Collect more".into(),
        },
    );
    next.idempotency_key = "next".into();
    app.record_disposition(&case.id, next).unwrap();
    let mut retry = decision(
        &app,
        &case.id,
        QualificationPlan::InsufficientEvidence {
            after_seconds: 1,
            collection_task: "Different decision".into(),
        },
    );
    retry.idempotency_key = "next".into();
    assert!(matches!(
        app.record_disposition(&case.id, retry),
        Err(Error::Conflict(_))
    ));
}
#[test]
fn concurrent_deadline_ticks_and_operator_decisions_have_no_duplicate_transitions() {
    let (_dir, app, time, case) = setup();
    let cmd = decision(
        &app,
        &case.id,
        QualificationPlan::InsufficientEvidence {
            after_seconds: 1,
            collection_task: "Collect traces".into(),
        },
    );
    let threads: Vec<_> = (0..2)
        .map(|i| {
            let app = app.clone();
            let id = case.id.clone();
            let mut cmd = cmd.clone();
            cmd.idempotency_key = format!("parallel-{i}");
            std::thread::spawn(move || app.record_disposition(&id, cmd))
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
    time.advance(Duration::seconds(1));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let app = app.clone();
            std::thread::spawn(move || app.process_due_obligations().unwrap())
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .sum::<usize>(),
        1
    );
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(s.qualification.dispositions.len(), 1);
    assert_eq!(
        s.events
            .iter()
            .filter(|e| matches!(e.data, EventData::ObligationDue { .. }))
            .count(),
        1
    );
}
#[test]
fn upgrade_from_v1_preserves_events_receipts_and_backfills_case_evidence() {
    let (dir, application, _time, case) = setup();
    let source = Store::open(dir.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    let legacy_path = dir.path().join("legacy.db");
    let legacy = rusqlite::Connection::open(&legacy_path).unwrap();
    legacy
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    legacy
        .execute_batch(
            "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,digest TEXT NOT NULL);",
        )
        .unwrap();
    legacy
        .execute(
            "INSERT INTO schema_migrations VALUES (1,?1)",
            [digest(include_str!("../migrations/001_initial.sql"))],
        )
        .unwrap();
    legacy
        .execute(
            "ATTACH DATABASE ?1 AS source",
            [dir.path().join("test.db").to_str().unwrap()],
        )
        .unwrap();
    for table in [
        "cases",
        "assignments",
        "jobs",
        "evidence",
        "runs",
        "events",
        "commands",
    ] {
        legacy
            .execute_batch(&format!(
                "INSERT INTO {table} SELECT * FROM source.{table};"
            ))
            .unwrap();
    }
    drop(legacy);
    let before: String = source
        .query_row(
            "SELECT envelope FROM events ORDER BY cursor LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let upgraded = app(&legacy_path, clock());
    let snapshot = upgraded.snapshot(&case.id).unwrap();
    assert_eq!(snapshot.qualification.evidence.len(), 1);
    assert_eq!(snapshot.events.len(), 4);
    assert_eq!(
        upgraded
            .create_case(FakeSource.manual_command("qualification-case"))
            .unwrap()
            .version,
        2
    );
    let db = Store::open(legacy_path).unwrap().connect().unwrap();
    assert_eq!(
        before,
        db.query_row(
            "SELECT envelope FROM events ORDER BY cursor LIMIT 1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
    );
    assert_eq!(
        application.snapshot(&case.id).unwrap().case.version,
        snapshot.case.version
    );
    db.execute(
        "UPDATE schema_migrations SET digest='corrupt' WHERE version=2",
        [],
    )
    .unwrap();
    assert!(matches!(
        Store::open(dir.path().join("legacy.db")),
        Err(Error::Conflict(_))
    ));
}
#[test]
fn qualification_crash_child() {
    let Ok(path) = std::env::var("WATCHHOUSE_QUALIFICATION_CRASH_DB") else {
        return;
    };
    let app = app(std::path::Path::new(&path), clock());
    let case = app
        .create_case(FakeSource.manual_command("qualification-case"))
        .unwrap();
    app.run_once(&FakeAdapter).unwrap();
    app.record_disposition(
        &case.id,
        decision(
            &app,
            &case.id,
            QualificationPlan::InsufficientEvidence {
                after_seconds: 60,
                collection_task: "Collect trace on next startup".into(),
            },
        ),
    )
    .unwrap();
    std::process::exit(17)
}
#[test]
fn deadline_survives_process_loss_and_failed_tick_rolls_back_then_replays_once() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("restart.db");
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "qualification_crash_child"])
        .env("WATCHHOUSE_QUALIFICATION_CRASH_DB", &path)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(17));
    let time = clock();
    time.advance(Duration::seconds(60));
    let app = app(&path, time);
    let id = app.list_cases().unwrap()[0].id.clone();
    let db = Store::open(&path).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_timer BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='obligation.due' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(app.process_due_obligations().is_err());
    let s = app.snapshot(&id).unwrap();
    assert_eq!(
        s.qualification.dispositions[0].state,
        DispositionState::Active
    );
    assert_eq!(
        s.qualification.obligations[0].state,
        ObligationState::Pending
    );
    db.execute_batch("DROP TRIGGER fail_timer").unwrap();
    assert_eq!(app.process_due_obligations().unwrap(), 1);
    assert_eq!(app.process_due_obligations().unwrap(), 0);
    assert_eq!(
        app.snapshot(&id).unwrap().qualification.obligations[0].state,
        ObligationState::Due
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state='completed'",
            params![],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn qualification_fixtures_are_versioned_and_old_snapshot_payloads_remain_readable() {
    let evidence: RecordEvidence =
        serde_json::from_str(include_str!("../contracts/v1/record-evidence.json")).unwrap();
    assert!(evidence.observed_at.is_none());
    let decision: RecordDisposition =
        serde_json::from_str(include_str!("../contracts/v1/record-disposition.json")).unwrap();
    assert!(matches!(
        decision.plan,
        QualificationPlan::ApparentlyTransient { .. }
    ));
    let _: ReconsiderCase =
        serde_json::from_str(include_str!("../contracts/v1/reconsider-case.json")).unwrap();
    let value = serde_json::to_value(&decision).unwrap();
    for invalid in [
        serde_json::json!({"kind":"recovered","after_seconds":60}),
        serde_json::json!({"kind":"expected_behavior","after_seconds":60,"grant":"production"}),
    ] {
        let mut wrong = value.clone();
        wrong["plan"] = invalid;
        assert!(serde_json::from_value::<RecordDisposition>(wrong).is_err());
    }
    let mut wrong = value;
    wrong["schema_version"] = 2.into();
    assert!(serde_json::from_value::<RecordDisposition>(wrong).is_err());
    let (_dir, app, _time, case) = setup();
    let mut old = serde_json::to_value(app.snapshot(&case.id).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("qualification");
    let old: Snapshot = serde_json::from_value(old).unwrap();
    assert!(old.qualification.dispositions.is_empty());
}
#[test]
fn evidence_failure_and_corrupt_digests_do_not_allow_a_decision() {
    let (dir, app, _time, case) = setup();
    let db = Store::open(dir.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    let cmd: RecordEvidence =
        serde_json::from_str(include_str!("../contracts/v1/record-evidence.json")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_evidence BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='evidence.recorded' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(app.record_evidence(&case.id, cmd.clone()).is_err());
    let s = app.snapshot(&case.id).unwrap();
    assert_eq!(s.qualification.evidence.len(), 1);
    assert_eq!(s.case.version, case.version);
    db.execute_batch("DROP TRIGGER fail_evidence").unwrap();
    let receipt = app.record_evidence(&case.id, cmd).unwrap();
    db.execute(
        "UPDATE evidence SET content='{}' WHERE digest=?1",
        [receipt.evidence.digest],
    )
    .unwrap();
    let d = decision(
        &app,
        &case.id,
        QualificationPlan::ExpectedBehavior { after_seconds: 60 },
    );
    assert!(matches!(
        app.record_disposition(&case.id, d),
        Err(Error::Conflict(_))
    ));
    assert!(
        app.snapshot(&case.id)
            .unwrap()
            .qualification
            .dispositions
            .is_empty()
    );
}
#[test]
fn migration_checksums_accept_legacy_crlf_and_reject_future_history_without_writes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("crlf.db");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    db.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,digest TEXT NOT NULL);",
    )
    .unwrap();
    let legacy = digest(
        &include_str!("../migrations/001_initial.sql")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n"),
    );
    db.execute("INSERT INTO schema_migrations VALUES (1,?1)", [&legacy])
        .unwrap();
    Store::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT digest FROM schema_migrations WHERE version=1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        legacy
    );
    let previous: i64 = db
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    db.execute("INSERT INTO schema_migrations VALUES (999,'future')", [])
        .unwrap();
    assert!(matches!(Store::open(&path), Err(Error::Conflict(_))));
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        previous + 1
    );
}
