use chrono::{TimeZone, Utc};
use rusqlite::params;
use serde_json::{Value, json};
use std::sync::Arc;
use watchhouse::{
    application::Application,
    bundles::{self, Options},
    domain::ManualClock,
    store::Store,
};
fn setup() -> (tempfile::TempDir, Application, String) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap(),
        )),
    );
    let c = a
        .create_case(
            serde_json::from_str(include_str!("../contracts/v1/manual-case.json")).unwrap(),
        )
        .unwrap();
    (d, a, c.id)
}
fn evidence(a: &Application, id: &str, key: &str) -> String {
    a.record_evidence(id,serde_json::from_value(json!({"schema_version":1,"idempotency_key":key,"actor":"operator","reason":"Invented export fixture","expected_version":a.snapshot(id).unwrap().case.version,"scope":"Fixture only","statement":format!("invented evidence {key}"),"collection_method":"Synthetic fixture","observed_at":null,"uncertainty":"No live verification"})).unwrap()).unwrap().evidence.digest
}
#[test]
fn retained_versions_receipts_and_safe_views_are_complete_beyond_snapshot_caps() {
    let (d, a, id) = setup();
    a.run_once(&watchhouse::execution::FakeAdapter).unwrap();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    for revision in 1..=251 {
        db.execute("INSERT INTO incident_artifacts VALUES ('versioned-fixture',?1,?2,?3)",params![revision,id,json!({"revision":revision,"body":"invented draft","fence":"never-export-control","changes":[{"path":"fixture","content":"invented patch"}]}).to_string()]).unwrap();
    }
    let frame = a.case_bundle(&id, Options::default(), "operator").unwrap();
    assert_eq!(frame.counts.sections["incident_artifacts"], 251);
    assert!(!frame.sections["commands"].is_empty());
    let encoded = serde_json::to_string(&frame).unwrap();
    assert!(!encoded.contains("never-export-control"));
    assert!(!encoded.contains("invented patch"));
    assert_eq!(bundles::inspect_frame(&frame).unwrap(), frame.counts);
    assert_eq!(a.case_exports(&id).unwrap()[0].state, "prepared");
    let with = a
        .case_bundle(
            &id,
            Options {
                include_patches: true,
                ..Options::default()
            },
            "operator",
        )
        .unwrap();
    assert!(
        serde_json::to_string(&with)
            .unwrap()
            .contains("invented patch")
    );
    let mut tampered = frame.clone();
    tampered.sections.get_mut("cases").unwrap()[0]["record"]["title"] = json!("changed");
    assert!(bundles::inspect_frame(&tampered).is_err());
}
#[test]
fn evidence_remains_explicitly_present_removed_missing_or_corrupt() {
    let (d, a, id) = setup();
    let removed = evidence(&a, &id, "removed");
    let corrupt = evidence(&a, &id, "corrupt");
    let present = evidence(&a, &id, "present");
    a.redact_evidence(&removed, "operator", "Remove only this invented body")
        .unwrap();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute(
        "UPDATE evidence SET content='invented corrupt bytes' WHERE digest=?1",
        [&corrupt],
    )
    .unwrap();
    // A deliberately damaged orphan is a diagnostic fixture, never a valid live write.
    db.pragma_update(None, "foreign_keys", "OFF").unwrap();
    let missing = watchhouse::store::digest("invented missing evidence");
    db.execute(
        "INSERT INTO case_evidence(case_id,digest) VALUES (?1,?2)",
        params![id, missing],
    )
    .unwrap();
    let frame = a
        .case_bundle(
            &id,
            Options {
                include_evidence: true,
                ..Options::default()
            },
            "operator",
        )
        .unwrap();
    bundles::inspect_frame(&frame).unwrap();
    let rows = &frame.sections["evidence_manifest"];
    let find = |sum: &str| rows.iter().find(|r| r["digest"] == sum).unwrap();
    assert_eq!(find(&removed)["state"], "redacted");
    assert!(find(&removed)["body"].is_null());
    assert_eq!(find(&corrupt)["state"], "corrupt");
    assert_eq!(find(&corrupt)["body"], "invented corrupt bytes");
    assert_eq!(find(&missing)["state"], "missing");
    assert_eq!(find(&present)["state"], "present");
    assert_eq!(frame.counts.evidence_redacted, 1);
    assert_eq!(frame.counts.evidence_missing, 1);
    assert_eq!(frame.counts.evidence_corrupt, 1);
    let omitted = a.case_bundle(&id, Options::default(), "operator").unwrap();
    assert!(
        omitted.sections["evidence_manifest"]
            .iter()
            .all(|r| r["body"].is_null())
    );
}
#[test]
fn private_directory_inspection_no_overwrite_tamper_and_pinned_reconciliation() {
    let (d, a, id) = setup();
    evidence(&a, &id, "exported");
    let output = d.path().join("bundle");
    let e = a
        .export_case(
            &id,
            &output,
            Options {
                include_evidence: true,
                ..Options::default()
            },
            "operator",
            "Synthetic internal export",
        )
        .unwrap();
    let pin = e.manifest_digest.as_deref().unwrap();
    assert_eq!(
        bundles::inspect(&output, Some(pin)).unwrap().header.case_id,
        id
    );
    assert!(
        a.export_case(&id, &output, Options::default(), "operator", "No overwrite")
            .is_err()
    );
    assert!(bundles::inspect(&output, Some("sha256:wrong")).is_err());
    std::fs::write(
        output.join("export-incomplete.json"),
        json!({"id":"wrong","case_id":id}).to_string(),
    )
    .unwrap();
    assert!(bundles::inspect(&output, Some(pin)).is_err());
    assert!(
        a.reconcile_case_export(&e.id, &output, pin, "operator", "Wrong marker must reject")
            .is_err()
    );
    assert!(output.join("export-incomplete.json").exists());
    std::fs::write(
        output.join("export-incomplete.json"),
        json!({"id":e.id,"case_id":id}).to_string(),
    )
    .unwrap();
    a.reconcile_case_export(
        &e.id,
        &output,
        pin,
        "operator",
        "Inspect interrupted completion",
    )
    .unwrap();
    assert!(!output.join("export-incomplete.json").exists());
    std::fs::write(output.join("unlisted"), "invented").unwrap();
    assert!(bundles::inspect(&output, Some(pin)).is_err());
    std::fs::remove_file(output.join("unlisted")).unwrap();
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output.join("manifest.json")).unwrap()).unwrap();
    let part = output.join(manifest["parts"][0]["path"].as_str().unwrap());
    std::fs::write(part, "changed\n").unwrap();
    assert!(bundles::inspect(&output, Some(pin)).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
#[test]
fn failed_capture_has_owned_metadata_and_retained_incomplete_directory() {
    let (d, a, id) = setup();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute("DROP TABLE case_correlations", []).unwrap();
    let output = d.path().join("interrupted");
    assert!(
        a.export_case(
            &id,
            &output,
            Options::default(),
            "operator",
            "Failure ownership fixture"
        )
        .is_err()
    );
    let e = a.case_exports(&id).unwrap().remove(0);
    assert_eq!(e.state, "failed");
    assert_eq!(e.owner, "operator");
    assert!(output.join("export-incomplete.json").exists());
    assert!(bundles::inspect(&output, None).is_err());
}
#[tokio::test]
async fn authenticated_downloads_hold_a_bounded_permit_until_body_release() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (_d, a, id) = setup();
    let token = "synthetic-bundle-token-32-bytes-only";
    let router = watchhouse::http::router(a, token.into(), "127.0.0.1:8787".into());
    let req = || {
        Request::builder()
            .uri(format!("/api/v1/cases/{id}/bundle"))
            .header("host", "127.0.0.1:8787")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    let mut unauth = req();
    unauth.headers_mut().remove("authorization");
    assert_eq!(
        router.clone().oneshot(unauth).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let one = router.clone().oneshot(req()).await.unwrap();
    let two = router.clone().oneshot(req()).await.unwrap();
    assert_eq!(one.status(), StatusCode::OK);
    assert_eq!(two.status(), StatusCode::OK);
    assert_eq!(one.headers()["cache-control"], "no-store");
    assert!(
        one.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("attachment")
    );
    assert_eq!(
        router.clone().oneshot(req()).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let bytes = one.into_body().collect().await.unwrap().to_bytes();
    let frame: bundles::Frame = serde_json::from_slice(&bytes).unwrap();
    bundles::inspect_frame(&frame).unwrap();
    drop(two);
    assert_eq!(
        router.oneshot(req()).await.unwrap().status(),
        StatusCode::OK
    );
}

#[test]
fn browser_limit_rejects_instead_of_truncating_while_directory_streams_all_rows() {
    let (d, a, id) = setup();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    let body = "invented retained content ".repeat(18000);
    for revision in 1..=24 {
        db.execute(
            "INSERT INTO incident_artifacts VALUES ('large-fixture',?1,?2,?3)",
            params![
                revision,
                id,
                json!({"revision":revision,"body":body}).to_string()
            ],
        )
        .unwrap();
    }
    assert!(matches!(
        a.case_bundle(&id, Options::default(), "operator"),
        Err(watchhouse::error::Error::Backpressure(_))
    ));
    assert!(a.case_exports(&id).unwrap().is_empty());
    let output = d.path().join("streamed");
    let exported = a
        .export_case(
            &id,
            &output,
            Options::default(),
            "operator",
            "Stream the full invented large case",
        )
        .unwrap();
    let checked = bundles::inspect(&output, exported.manifest_digest.as_deref()).unwrap();
    assert_eq!(checked.counts.sections["incident_artifacts"], 24);
    assert!(checked.bytes > 8 * 1024 * 1024);
}
#[test]
fn restored_location_and_audit_failure_cannot_authorize_or_claim_completion() {
    let (d, a, id) = setup();
    let output = d.path().join("bundle");
    let exported = a
        .export_case(
            &id,
            &output,
            Options::default(),
            "operator",
            "Export registry authority fixture",
        )
        .unwrap();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute(
        "INSERT INTO restore_runs VALUES ('fixture-restore','{}')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO restore_fences VALUES ('fixture-restore','case_export',?1,'{}')",
        [&exported.id],
    )
    .unwrap();
    assert!(
        a.reconcile_case_export(
            &exported.id,
            &output,
            exported.manifest_digest.as_deref().unwrap(),
            "operator",
            "A copied path grants no authority",
        )
        .is_err()
    );
    db.execute_batch("CREATE TRIGGER deny_export_audit BEFORE INSERT ON factory_events BEGIN SELECT RAISE(ABORT,'invented audit failure'); END;")
        .unwrap();
    assert!(a.case_bundle(&id, Options::default(), "operator").is_err());
    assert_eq!(a.case_exports(&id).unwrap().len(), 1);
    let next = d.path().join("audit-failed");
    assert!(
        a.export_case(
            &id,
            &next,
            Options::default(),
            "operator",
            "Audit reservation failure must prevent filesystem creation",
        )
        .is_err()
    );
    assert!(!next.exists());
}
