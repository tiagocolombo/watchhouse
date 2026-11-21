use chrono::{Duration, TimeZone, Utc};
use std::{collections::BTreeMap, sync::Arc};
use watchhouse::{
    application::Application,
    contracts::V1,
    domain::ManualClock,
    retention::{Class, Policy},
    store::{Store, digest},
};
fn setup() -> (tempfile::TempDir, Application, ManualClock, String) {
    let d = tempfile::tempdir().unwrap();
    let clock = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(clock.clone()),
    );
    let case = a
        .create_case(watchhouse::intake::FakeSource.manual_command("retention-schedule-case"))
        .unwrap();
    (d, a, clock, case.id)
}
fn policy() -> Policy {
    Policy {
        schema_version: V1,
        revision: 1,
        paused: false,
        interval_seconds: 1,
        retain_seconds: BTreeMap::from([
            (Class::NormalizedSource, Some(60)),
            (Class::ExecutionEvidence, None),
            (Class::ReportedEvidence, None),
            (Class::SupervisorCheckpoint, None),
            (Class::Simulation, None),
            (Class::Other, None),
        ]),
    }
}
fn evidence(d: &tempfile::TempDir, case: &str, body: &str, provenance: &str, at: &str) -> String {
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    let sum = digest(body);
    db.execute(
        "INSERT INTO evidence VALUES (?1,'text/plain',?2,?3,?4)",
        rusqlite::params![sum, body, provenance, at],
    )
    .unwrap();
    db.execute(
        "INSERT INTO case_evidence VALUES (?1,?2)",
        rusqlite::params![case, sum],
    )
    .unwrap();
    sum
}
#[test]
fn expiry_uses_explicit_classes_filters_before_limits_and_persists_interval_across_restart() {
    let (d, a, clock, case) = setup();
    let first = evidence(
        &d,
        &case,
        "old normalized source A",
        "sentry-api-v1",
        "2026-11-21T11:57:00.000Z",
    );
    let second = evidence(
        &d,
        &case,
        "old normalized source B",
        "datadog-monitor-api-v1",
        "2026-11-21T11:58:00.000Z",
    );
    let kept = evidence(
        &d,
        &case,
        "reported retained indefinitely",
        "operator-reported-v1",
        "2026-01-01T00:00:00.000Z",
    );
    for i in 0..201 {
        evidence(
            &d,
            &case,
            &format!("unselected other class {i}"),
            "fixture",
            "2020-01-01T00:00:00.000Z",
        );
    }
    assert!(a.retention_tick().unwrap().is_none());
    a.install_retention_policy(policy(), "operator", "Synthetic explicit class schedule")
        .unwrap();
    assert_eq!(a.retention_tick().unwrap().unwrap().digest, first);
    assert!(a.retention_tick().unwrap().is_none());
    let restarted = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(clock.clone()),
    );
    assert!(restarted.retention_tick().unwrap().is_none());
    clock.advance(Duration::seconds(1));
    assert_eq!(restarted.retention_tick().unwrap().unwrap().digest, second);
    assert!(
        restarted
            .evidence(&kept)
            .unwrap()
            .contains("retained indefinitely")
    );
    assert_eq!(restarted.retention_status().unwrap().removed_bodies, 2);
    let mut p = policy();
    p.revision = 2;
    p.paused = true;
    restarted
        .install_retention_policy(p, "operator", "Pause configured expiry")
        .unwrap();
    clock.advance(Duration::days(365));
    assert!(restarted.retention_tick().unwrap().is_none());
    assert!(
        restarted
            .evidence(&kept)
            .unwrap()
            .contains("retained indefinitely")
    );
}
#[test]
fn corrupt_body_is_deferred_without_starving_valid_evidence_or_revealing_its_contents() {
    let (d, a, clock, case) = setup();
    let bad = evidence(
        &d,
        &case,
        "original source bytes",
        "sentry-api-v1",
        "2026-11-21T11:55:00.000Z",
    );
    let good = evidence(
        &d,
        &case,
        "valid next source bytes",
        "sentry-api-v1",
        "2026-11-21T11:56:00.000Z",
    );
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute(
        "UPDATE evidence SET content='private corrupted fixture body' WHERE digest=?1",
        [&bad],
    )
    .unwrap();
    a.install_retention_policy(policy(), "operator", "Fixture failure isolation")
        .unwrap();
    assert!(a.retention_tick().unwrap().is_none());
    assert_eq!(a.retention_status().unwrap().deferred_failures, 1);
    assert!(a.evidence_redactions(&case).unwrap().is_empty());
    assert_eq!(a.factory_status().unwrap().state, "degraded");
    clock.advance(Duration::seconds(1));
    assert_eq!(a.retention_tick().unwrap().unwrap().digest, good);
    let failure: String = db
        .query_row(
            "SELECT code FROM retention_failures WHERE digest=?1",
            [&bad],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(failure, "integrity_or_context");
    assert!(
        !serde_json::to_string(&a.factory_status().unwrap().audit)
            .unwrap()
            .contains("private corrupted")
    );
    let inspected = a.inspect_evidence(&bad).unwrap();
    assert_eq!(inspected["integrity_current"], false);
    assert!(!inspected.to_string().contains("private corrupted"));
    assert!(
        a.redact_evidence_observed(
            &bad,
            Some(&digest("wrong inspected value")),
            "operator",
            "Reject stale body inspection"
        )
        .is_err()
    );
    let removed = a
        .redact_evidence_observed(
            &bad,
            inspected["observed_digest"].as_str(),
            "operator",
            "Explicit targeted removal of the currently inspected corrupt body",
        )
        .unwrap();
    assert_eq!(
        removed.observed_digest.as_deref(),
        inspected["observed_digest"].as_str()
    );
    assert_eq!(a.retention_status().unwrap().deferred_failures, 0);
}
#[test]
fn unknown_legacy_time_is_visible_and_never_guessed_as_expired() {
    let (d, a, _clock, case) = setup();
    let unknown = evidence(
        &d,
        &case,
        "legacy source with unknown age",
        "sentry-api-v1",
        "now",
    );
    a.install_retention_policy(policy(), "operator", "No guessed historical ages")
        .unwrap();
    assert!(a.retention_tick().unwrap().is_none());
    let status = a.retention_status().unwrap();
    assert_eq!(status.unknown_time_rows, 1);
    assert_eq!(status.unaged_expiring_rows, 1);
    assert_eq!(status.owner.as_deref(), Some("operator"));
    assert_eq!(a.factory_status().unwrap().state, "degraded");
    assert!(a.evidence(&unknown).unwrap().contains("unknown age"));
}
#[test]
fn copied_policy_cannot_drain_a_restored_backlog_and_configuration_is_immutable() {
    let (d, a, _clock, case) = setup();
    let sum = evidence(
        &d,
        &case,
        "copied old evidence body",
        "sentry-api-v1",
        "2020-01-01T00:00:00.000Z",
    );
    let installed = a
        .install_retention_policy(policy(), "operator", "Fixture source installation")
        .unwrap();
    assert_eq!(
        installed,
        a.install_retention_policy(policy(), "operator", "Exact retry")
            .unwrap()
    );
    let mut changed = policy();
    changed.paused = true;
    assert!(
        a.install_retention_policy(changed, "operator", "Cannot mutate original")
            .is_err()
    );
    let backup = d.path().join("backup");
    let inspected = a
        .create_backup(&backup, "operator", "Before any expiry")
        .unwrap();
    let output = d.path().join("restore");
    watchhouse::restore::restore(
        &backup,
        &output,
        &inspected.manifest_digest,
        "operator",
        "Copied schedule is not deletion authority",
    )
    .unwrap();
    let restored = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(watchhouse::domain::SystemClock),
    );
    assert!(restored.retention_status().unwrap().restored_authority_held);
    assert!(restored.retention_tick().unwrap().is_none());
    assert!(restored.evidence(&sum).unwrap().contains("copied old"));
    let mut p = policy();
    p.revision = 2;
    restored
        .install_retention_policy(p, "operator", "Fresh explicit restored expiry authority")
        .unwrap();
    assert_eq!(restored.retention_tick().unwrap().unwrap().digest, sum);
    let mut missing = policy();
    missing.retain_seconds.remove(&Class::Other);
    assert!(missing.validate().is_err());
    let mut zero = policy();
    zero.retain_seconds.insert(Class::NormalizedSource, Some(0));
    assert!(zero.validate().is_err());
    assert!(
        a.install_retention_policy(
            policy(),
            "fake-investigator",
            "Worker cannot configure expiry"
        )
        .is_err()
    );
}

#[test]
fn corrupt_retention_authority_or_schedule_cannot_stop_the_independent_core_duties() {
    for corrupt_policy in [true, false] {
        let (d, a, _clock, case) = setup();
        let sum = evidence(
            &d,
            &case,
            "body remains under unavailable authority",
            "sentry-api-v1",
            "2020-01-01T00:00:00.000Z",
        );
        a.install_retention_policy(policy(), "operator", "Synthetic unavailable retention")
            .unwrap();
        let db = Store::open(d.path().join("state.db"))
            .unwrap()
            .connect()
            .unwrap();
        if corrupt_policy {
            db.execute_batch("DROP TRIGGER retention_policy_no_update; UPDATE retention_policies SET digest='fixture-corruption';").unwrap();
        } else {
            db.execute(
                "UPDATE retention_schedule SET next_at='fixture-bad-time' WHERE id=1",
                [],
            )
            .unwrap();
        }
        assert!(a.retention_tick().is_err());
        let status = a.factory_status().unwrap();
        assert!(status.retention_unavailable);
        assert_eq!(status.state, "degraded");
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
            .args([
                "--db",
                d.path().join("state.db").to_str().unwrap(),
                "worker",
                "--once",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Retention unavailable; deterministic duties continue")
        );
        assert_eq!(
            a.snapshot(&case).unwrap().case.status,
            watchhouse::domain::CaseStatus::AwaitingReview
        );
        assert!(a.evidence(&sum).unwrap().contains("body remains"));
    }
}

#[tokio::test]
async fn retention_metadata_is_authenticated_and_does_not_expose_evidence_bodies() {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (d, a, _clock, case) = setup();
    evidence(
        &d,
        &case,
        "synthetic body absent from metadata",
        "sentry-api-v1",
        "2026-11-21T11:59:00.000Z",
    );
    a.install_retention_policy(policy(), "operator", "Synthetic metadata contract")
        .unwrap();
    let token = "fixture-retention-status-32-byte-credential";
    let router = watchhouse::http::router(a, token.into(), "127.0.0.1:8787".into());
    let request = |authenticated: bool| {
        let mut b = Request::builder()
            .uri("/api/v1/retention")
            .header("host", "127.0.0.1:8787");
        if authenticated {
            b = b.header("authorization", format!("Bearer {token}"));
        }
        b.body(Body::empty()).unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request(false))
            .await
            .unwrap()
            .status(),
        401
    );
    let response = router.oneshot(request(true)).await.unwrap();
    assert_eq!(response.status(), 200);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["policy"]["revision"], 1);
    assert_eq!(value["owner"], "operator");
    assert!(!String::from_utf8_lossy(&bytes).contains("synthetic body absent"));
}

#[test]
fn every_supported_body_class_has_an_independent_expiry_choice() {
    let (d, a, clock, case) = setup();
    let mut p = policy();
    for age in p.retain_seconds.values_mut() {
        *age = Some(1);
    }
    for (class, provenance) in [
        (Class::NormalizedSource, "sentry-webhook-v1"),
        (Class::ExecutionEvidence, "native-container-v1"),
        (Class::ReportedEvidence, "operator-reported-v1"),
        (Class::SupervisorCheckpoint, "supervisor-checkpoint-v1"),
        (Class::Simulation, "fake-worker-progress-v1"),
        (Class::Other, "fixture-custom"),
    ] {
        evidence(
            &d,
            &case,
            &format!("Class fixture {class:?}"),
            provenance,
            "2026-11-21T11:00:00.000Z",
        );
    }
    let status = a.retention_status().unwrap();
    assert_eq!(status.pending_by_class.len(), 6);
    assert!(status.pending_by_class.iter().all(|c| c["bodies"] == 1));
    a.install_retention_policy(p, "operator", "Six independently explicit body ages")
        .unwrap();
    for _ in 0..6 {
        assert!(a.retention_tick().unwrap().is_some());
        clock.advance(Duration::seconds(1));
    }
    assert!(a.retention_tick().unwrap().is_none());
    assert!(a.retention_status().unwrap().pending_by_class.is_empty());
    assert_eq!(a.retention_status().unwrap().removed_bodies, 6);
}
