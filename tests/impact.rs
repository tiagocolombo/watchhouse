mod common;
use chrono::Duration;
use common::*;
use watchhouse::{application::Application, contracts::*, impact::*, store::Store};
fn report(a: &Application, id: &str, key: &str) -> String {
    a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: "operator".into(),
            reason: "Sanitized reported impact fixture".into(),
            expected_version: a.snapshot(id).unwrap().case.version,
            scope: "checkout / test".into(),
            statement: "<script>untrusted impact</script> Reported counts only".into(),
            collection_method: "Synthetic fixture report".into(),
            observed_at: None,
            uncertainty: "No production measurement".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
fn assessment(a: &Application, id: &str, key: &str, refs: Vec<String>) -> Assess {
    Assess {
        command: command(a, id, key, "operator"),
        expected_revision: 0,
        service: "checkout".into(),
        environment: "test".into(),
        scope: "Checkout requests".into(),
        window: None,
        critical_workflow: "Submit order".into(),
        impact_statement: "Affected scope and denominator remain unknown".into(),
        uncertainty: "Only invented fixture reports".into(),
        collection_owner: "impact-owner".into(),
        collection_task: "Collect success/error denominator and ingestion coverage".into(),
        review_seconds: 60,
        evidence_refs: refs,
    }
}
fn window() -> Window {
    Window {
        start: "2026-11-15T11:58:00Z".into(),
        end: "2026-11-15T11:59:00Z".into(),
        affected_requests: Some(0),
        total_requests: Some(0),
        affected_users: None,
        total_users: None,
        instrumentation: "Fixture report; ingestion completeness unknown".into(),
    }
}
#[test]
fn unknown_and_zero_traffic_stay_inconclusive_with_restart_safe_collection_duties() {
    let (d, a, t, id) = fixture();
    assert!(
        a.assess_impact(&id, assessment(&a, &id, "no-report", evidence(&a, &id)))
            .is_err()
    );
    let refs = vec![report(&a, &id, "impact-report")];
    let cmd = assessment(&a, &id, "impact", refs.clone());
    let first = a.assess_impact(&id, cmd.clone()).unwrap();
    assert_eq!(first.observation, "unknown_window");
    assert_eq!(first.provenance, "operator-reported-impact-v1");
    assert_eq!(a.assess_impact(&id, cmd).unwrap().id, first.id);
    t.advance(Duration::seconds(60));
    a.supervise_team().unwrap();
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t.clone()),
    );
    let s = a.snapshot(&id).unwrap();
    assert!(s.impact[0].due);
    assert!(
        s.team
            .escalations
            .iter()
            .any(|e| format!("{e:?}").contains("impact-owner"))
    );
    assert!(s.incident.unwrap().recovered_at.is_none());
    let mut next = assessment(&a, &id, "zero-traffic", refs);
    next.expected_revision = 1;
    next.window = Some(window());
    let second = a.assess_impact(&id, next).unwrap();
    assert_eq!(second.observation, "no_request_traffic");
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.impact.len(), 2);
    assert!(s.impact[1].due);
    assert!(!s.impact[0].due);
    assert!(s.model_jobs.is_empty());
    assert!(s.publications.is_empty());
    assert!(s.incident.unwrap().recovered_at.is_none());
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    assert!(
        db.execute("UPDATE impact_assessments SET record='{}'", [])
            .is_err()
    );
    assert!(db.execute("DELETE FROM impact_assessments", []).is_err());
}
#[test]
fn invalid_windows_denominators_scope_and_versions_never_leave_an_assessment() {
    let (_d, a, _t, id) = fixture();
    let refs = vec![report(&a, &id, "impact-report")];
    for invalid in [
        "order",
        "future",
        "denominator",
        "precision",
        "scope",
        "version",
        "revision",
    ] {
        let mut cmd = assessment(&a, &id, invalid, refs.clone());
        cmd.window = Some(window());
        match invalid {
            "order" => {
                cmd.window.as_mut().unwrap().start = cmd.window.as_ref().unwrap().end.clone()
            }
            "future" => cmd.window.as_mut().unwrap().end = "2099-01-01T00:00:00Z".into(),
            "denominator" => cmd.window.as_mut().unwrap().affected_requests = Some(1),
            "precision" => {
                cmd.window.as_mut().unwrap().total_requests = Some(9_007_199_254_740_992)
            }
            "scope" => cmd.service = "other-service".into(),
            "version" => cmd.command.expected_version -= 1,
            "revision" => cmd.expected_revision = 1,
            _ => unreachable!(),
        }
        assert!(a.assess_impact(&id, cmd).is_err(), "{invalid}");
        assert!(a.snapshot(&id).unwrap().impact.is_empty());
    }
}
#[test]
fn assessment_record_event_and_receipt_roll_back_together() {
    let (d, a, _t, id) = fixture();
    let refs = vec![report(&a, &id, "impact-report")];
    let cmd = assessment(&a, &id, "rollback", refs);
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_impact BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='impact.changed' BEGIN SELECT RAISE(ABORT,'injected impact failure'); END;").unwrap();
    let before = a.snapshot(&id).unwrap().case.version;
    assert!(a.assess_impact(&id, cmd.clone()).is_err());
    assert_eq!(a.snapshot(&id).unwrap().case.version, before);
    assert!(a.snapshot(&id).unwrap().impact.is_empty());
    db.execute_batch("DROP TRIGGER fail_impact").unwrap();
    assert_eq!(a.assess_impact(&id, cmd).unwrap().revision, 1);
}
#[tokio::test]
async fn authenticated_http_and_cli_share_impact_revisions_and_receipts() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (d, a, _t, id) = fixture();
    let refs = vec![report(&a, &id, "impact-report")];
    let cmd = assessment(&a, &id, "http-impact", refs.clone());
    let token = "impact-fixture-operator-token-32-bytes";
    let router = watchhouse::http::router(a.clone(), token.into(), "127.0.0.1:8787".into());
    let path = format!("/api/v1/cases/{id}/impact");
    let req = |auth: bool| {
        let mut r = Request::builder()
            .method("POST")
            .uri(&path)
            .header("Host", "127.0.0.1:8787")
            .header("Content-Type", "application/json");
        if auth {
            r = r.header("Authorization", format!("Bearer {token}"));
        }
        r.body(Body::from(serde_json::to_vec(&cmd).unwrap()))
            .unwrap()
    };
    assert_eq!(
        router.clone().oneshot(req(false)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let response = router.clone().oneshot(req(true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first: Assessment =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let response = router.oneshot(req(true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let replay: Assessment =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(first.id, replay.id);
    let mut next = assessment(&a, &id, "cli-impact", refs);
    next.expected_revision = 1;
    let file = d.path().join("impact-command.json");
    std::fs::write(&file, serde_json::to_vec(&next).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .arg("--db")
        .arg(d.path().join("db"))
        .args(["case", "assess-impact", &id, "--file"])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let second: Assessment = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(a.snapshot(&id).unwrap().impact.len(), 2);
}
