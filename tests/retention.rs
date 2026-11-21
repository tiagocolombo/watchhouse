use chrono::{TimeZone, Utc};
use std::sync::Arc;
use watchhouse::{application::Application, contracts::*, domain::ManualClock, store::Store};
fn setup() -> (tempfile::TempDir, Application, String) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap(),
        )),
    );
    let case = a
        .create_case(
            serde_json::from_str(include_str!("../contracts/v1/manual-case.json")).unwrap(),
        )
        .unwrap();
    (d, a, case.id)
}
fn report(a: &Application, case: &str, key: &str) -> RecordEvidence {
    serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":key,"actor":"operator","reason":"Synthetic evidence retention fixture","expected_version":a.snapshot(case).unwrap().case.version,"scope":"Synthetic evidence-store blob only","statement":"fixture-private-✨-body","collection_method":"Invented fixture","observed_at":null,"uncertainty":"No actual private incident or integration"})).unwrap()
}
#[test]
fn redaction_retains_identity_receipts_and_deadlines_but_not_a_usable_body() {
    let (d, a, case) = setup();
    a.run_once(&watchhouse::execution::FakeAdapter).unwrap();
    let command = report(&a, &case, "redacted-report");
    let proof = a
        .record_evidence(&case, command.clone())
        .unwrap()
        .evidence
        .digest;
    let snapshot = a.snapshot(&case).unwrap();
    let decision:RecordDisposition=serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":"retained-decision","actor":"operator","reason":"Monitor synthetic uncertainty","expected_version":snapshot.case.version,"owner":"operator","scope":"Fixture only","uncertainty":"Unmeasured","evidence_refs":[proof],"reconsideration_conditions":"New proof","reopen_after_occurrences":2,"plan":{"kind":"insufficient_evidence","after_seconds":100,"collection_task":"Collect an independent replacement observation"}})).unwrap();
    a.record_disposition(&case, decision).unwrap();
    let before = a.snapshot(&case).unwrap();
    let original = a.evidence(&proof).unwrap();
    let usage = a.capacity_status().unwrap().ordinary_evidence_bytes;
    let tombstone = a
        .redact_evidence(
            &proof,
            "operator",
            "Remove the targeted fixture evidence body",
        )
        .unwrap();
    assert_eq!(tombstone.original_bytes, original.len() as u64);
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT content FROM evidence WHERE digest=?1",
            [&proof],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        ""
    );
    assert!(a.capacity_status().unwrap().ordinary_evidence_bytes < usage);
    let inspected = a.evidence(&proof).unwrap();
    assert!(inspected.contains("\"redacted\":true"));
    assert!(!inspected.contains("fixture-private"));
    let after = a.snapshot(&case).unwrap();
    assert_eq!(after.evidence_redactions.len(), 1);
    assert_eq!(
        after.qualification.dispositions[0].state,
        watchhouse::domain::qualification::DispositionState::Reopened
    );
    assert_eq!(
        after.qualification.obligations[0].due_at,
        before.qualification.obligations[0].due_at
    );
    assert_eq!(
        after.qualification.obligations[0].state,
        watchhouse::domain::qualification::ObligationState::Due
    );
    assert!(
        after
            .team
            .escalations
            .iter()
            .any(|e| e.source_type == "evidence_redaction" && e.owner == "operator")
    );
    assert!(
        a.factory_status()
            .unwrap()
            .pauses
            .iter()
            .any(|p| p["target"] == case)
    );
    assert_eq!(
        a.record_evidence(&case, command).unwrap().evidence.digest,
        proof
    ); // old receipt is historical, not authority
    assert_eq!(
        a.redact_evidence(&proof, "operator", "Retry keeps the original audit")
            .unwrap()
            .redacted_at,
        tombstone.redacted_at
    );
    assert_eq!(a.snapshot(&case).unwrap().evidence_redactions.len(), 1);
    assert!(
        db.execute(
            "UPDATE evidence SET content=?1 WHERE digest=?2",
            rusqlite::params![original, proof]
        )
        .is_err()
    );
    assert!(db.execute("DELETE FROM evidence_redactions", []).is_err());
    let bad:RecordDisposition=serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":"cannot-reuse-erased","actor":"operator","reason":"Fixture proof rejection","expected_version":after.case.version,"owner":"operator","scope":"Fixture only","uncertainty":"Unmeasured","evidence_refs":[proof],"reconsideration_conditions":"New proof","reopen_after_occurrences":2,"plan":{"kind":"insufficient_evidence","after_seconds":100,"collection_task":"Collect new data"}})).unwrap();
    assert!(a.record_disposition(&case, bad).is_err());
}
#[test]
fn audit_failure_rolls_back_body_usage_epochs_pauses_and_events() {
    let (d, a, case) = setup();
    let proof = a
        .record_evidence(&case, report(&a, &case, "rollback-proof"))
        .unwrap()
        .evidence
        .digest;
    let original = a.evidence(&proof).unwrap();
    let version = a.snapshot(&case).unwrap().case.version;
    let usage = a.capacity_status().unwrap().ordinary_evidence_bytes;
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fixture_redaction_failure BEFORE INSERT ON factory_events BEGIN SELECT RAISE(ABORT,'synthetic audit failure'); END;").unwrap();
    assert!(
        a.redact_evidence(
            &proof,
            "operator",
            "Rollback the complete redaction transaction"
        )
        .is_err()
    );
    assert_eq!(a.evidence(&proof).unwrap(), original);
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, usage);
    assert_eq!(a.snapshot(&case).unwrap().case.version, version);
    assert!(a.evidence_redactions(&case).unwrap().is_empty());
    assert!(a.factory_status().unwrap().pauses.is_empty());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM retention_invalidations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fixture_redaction_failure")
        .unwrap();
    assert!(
        a.redact_evidence(&proof, "operator", "Retry after rollback")
            .is_ok()
    );
}
#[test]
fn backup_and_restore_keep_tombstones_and_never_rehydrate_removed_bodies() {
    let (d, a, case) = setup();
    let proof = a
        .record_evidence(&case, report(&a, &case, "backup-proof"))
        .unwrap()
        .evidence
        .digest;
    a.redact_evidence(&proof, "operator", "Retain absence across restore")
        .unwrap();
    let backup = d.path().join("backup");
    let report = a
        .create_backup(&backup, "operator", "Redacted fixture backup")
        .unwrap();
    watchhouse::backups::inspect(&backup, Some(&report.manifest_digest)).unwrap();
    let output = d.path().join("restored");
    watchhouse::restore::restore(
        &backup,
        &output,
        &report.manifest_digest,
        "operator",
        "Restore redacted fixture",
    )
    .unwrap();
    let restored = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(watchhouse::domain::SystemClock),
    );
    assert_eq!(restored.evidence_redactions(&case).unwrap().len(), 1);
    assert_eq!(
        restored.evidence(&proof).unwrap(),
        a.evidence(&proof).unwrap()
    );
    assert!(
        a.redact_evidence(&proof, "fake-investigator", "A worker cannot erase proof")
            .is_err()
    );
    assert!(
        a.redact_evidence(&proof, "model-engineer", "A model cannot erase proof")
            .is_err()
    );
}
