use chrono::{Duration, TimeZone, Utc};
use std::sync::Arc;
use watchhouse::{
    application::Application, capacity::Limits, contracts::*, domain::ManualClock, error::Error,
    intake::FakeSource, store::Store,
};
fn fixture() -> (tempfile::TempDir, Application, ManualClock) {
    let d = tempfile::tempdir().unwrap();
    let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    (d, a, t)
}
fn limits() -> Limits {
    Limits {
        revision: 2,
        ..Limits::default()
    }
}
#[test]
fn queued_intake_rejects_before_ack_but_exact_receipts_work_while_full() {
    let (_d, a, _t) = fixture();
    let mut p = limits();
    p.max_queued_cases = 1;
    a.install_factory_limits(p, "operator", "fixture").unwrap();
    let cmd = FakeSource.manual_command("one-admitted");
    let first = a.create_case(cmd.clone()).unwrap();
    assert!(matches!(
        a.create_case(FakeSource.manual_command("overfull")),
        Err(Error::Backpressure(_))
    ));
    assert_eq!(a.create_case(cmd).unwrap().id, first.id);
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert_eq!(a.capacity_status().unwrap().queued_cases, 1);
    let c = a.claim().unwrap().unwrap();
    assert_eq!(c.case_id, first.id);
    assert!(
        a.create_case(FakeSource.manual_command("new-capacity-after-claim"))
            .is_ok()
    );
}
#[test]
fn utf8_evidence_quota_counter_dedup_shrink_delete_and_rollback_are_exact() {
    let (d, a, _t) = fixture();
    let mut p = limits();
    p.max_evidence_bytes = 1024;
    p.supervisor_evidence_bytes = 65536;
    let hash = a
        .install_factory_limits(p.clone(), "operator", "fixture")
        .unwrap();
    assert_eq!(
        a.install_factory_limits(p, "operator", "retry fixture")
            .unwrap(),
        hash
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    let data = "é".repeat(512);
    let digest = watchhouse::store::digest(&data);
    db.execute(
        "INSERT INTO evidence VALUES (?1,'text/plain',?2,'fixture','now')",
        rusqlite::params![digest, data],
    )
    .unwrap();
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, 1024);
    db.execute(
        "INSERT OR IGNORE INTO evidence VALUES (?1,'text/plain',?2,'fixture','now')",
        rusqlite::params![digest, data],
    )
    .unwrap();
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, 1024);
    let error = Error::from(
        db.execute(
            "INSERT INTO evidence VALUES ('over','text/plain','x','fixture','now')",
            [],
        )
        .unwrap_err(),
    );
    assert!(matches!(error, Error::Backpressure(_)));
    db.execute(
        "UPDATE evidence SET content='tiny' WHERE digest=?1",
        [&digest],
    )
    .unwrap();
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, 4);
    db.execute("DELETE FROM evidence WHERE digest=?1", [&digest])
        .unwrap();
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, 0);
    db.execute_batch("BEGIN IMMEDIATE; INSERT INTO evidence VALUES ('rolled','text/plain','1234','fixture','now'); ROLLBACK;").unwrap();
    assert_eq!(a.capacity_status().unwrap().ordinary_evidence_bytes, 0);
    assert!(db.execute("DELETE FROM factory_limits", []).is_err());
    assert!(a.capacity_status().unwrap().database_and_journal_bytes > 0);
}
#[test]
fn ordinary_storage_flood_cannot_spend_supervisor_checkpoint_headroom() {
    let (d, a, t) = fixture();
    let id = a
        .create_case(FakeSource.manual_command("flooded-case"))
        .unwrap()
        .id;
    let claim = a.claim().unwrap().unwrap();
    let mut p = limits();
    p.max_evidence_bytes = 1024;
    p.supervisor_evidence_bytes = 65536;
    a.install_factory_limits(p, "operator", "small fixture quota")
        .unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute(
        "INSERT INTO evidence VALUES ('flood','text/plain',?1,'fixture','now')",
        ["x".repeat(1024)],
    )
    .unwrap();
    assert!(matches!(
        a.record_evidence(
            &id,
            RecordEvidence {
                schema_version: V1,
                idempotency_key: "flood-report".into(),
                actor: "operator".into(),
                reason: "Observe capacity fixture".into(),
                expected_version: a.snapshot(&id).unwrap().case.version,
                scope: "fixture".into(),
                statement: "New bytes cannot fit".into(),
                collection_method: "fixture".into(),
                observed_at: None,
                uncertainty: "No live evidence".into()
            }
        ),
        Err(Error::Backpressure(_))
    ));
    t.advance(Duration::seconds(31));
    assert!(a.supervise_team().unwrap() > 0);
    assert!(
        a.snapshot(&id)
            .unwrap()
            .team
            .blocks
            .iter()
            .any(|b| b.run_id == claim.run_id)
    );
    let usage = a.capacity_status().unwrap();
    assert_eq!(usage.ordinary_evidence_bytes, 1024);
    assert!(usage.supervisor_evidence_bytes > 0);
    assert!(usage.supervisor_evidence_bytes <= 65536);
    assert!(!a.factory_status().unwrap().clock_behind);
}
#[test]
fn signed_delivery_capacity_preserves_duplicate_ack_and_rejects_new_receipt_atomically() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let (d, a, _t) = fixture();
    let cfg: watchhouse::sentry::SentryConfig =
        serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();
    a.configure_sentry(cfg.clone(), "operator").unwrap();
    let mut p = limits();
    p.max_pending_deliveries = 1;
    a.install_factory_limits(p, "operator", "delivery fixture")
        .unwrap();
    let secret = b"sanitized-fixture-secret-32-bytes-only";
    let body = |id: &str| {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/sentry/issue.json")).unwrap();
        v["id"] = id.into();
        let raw = serde_json::to_vec(&serde_json::json!({"action":"created","data":{"issue":v}}))
            .unwrap();
        let mut h = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        h.update(&raw);
        let sig = format!("{:x}", h.finalize().into_bytes());
        (raw, sig)
    };
    let (raw, sig) = body("1001");
    let receipt = a.receive_sentry_verified(&raw, &sig, secret, &cfg).unwrap();
    assert_eq!(
        a.receive_sentry_verified(&raw, &sig, secret, &cfg).unwrap(),
        receipt
    );
    let (other, sig) = body("1002");
    assert!(matches!(
        a.receive_sentry_verified(&other, &sig, secret, &cfg),
        Err(Error::Backpressure(_))
    ));
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM source_receipts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(a.capacity_status().unwrap().pending_deliveries, 1);
}
#[tokio::test]
async fn api_capacity_is_429_with_retry_header_and_exact_retry_is_still_201() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (_d, a, _t) = fixture();
    let mut p = limits();
    p.max_queued_cases = 1;
    a.install_factory_limits(p, "operator", "http fixture")
        .unwrap();
    let r = watchhouse::http::router(
        a,
        "capacity-fixture-token-32-bytes-minimum".into(),
        "127.0.0.1:8787".into(),
    );
    let request = |cmd: &CreateCase| {
        Request::post("/api/v1/cases")
            .header("host", "127.0.0.1:8787")
            .header(
                "authorization",
                "Bearer capacity-fixture-token-32-bytes-minimum",
            )
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(cmd).unwrap()))
            .unwrap()
    };
    let cmd = FakeSource.manual_command("http-one");
    assert_eq!(
        r.clone().oneshot(request(&cmd)).await.unwrap().status(),
        201
    );
    let full = r
        .clone()
        .oneshot(request(&FakeSource.manual_command("http-two")))
        .await
        .unwrap();
    assert_eq!(full.status(), 429);
    assert_eq!(full.headers()["retry-after"], "1");
    assert_eq!(r.oneshot(request(&cmd)).await.unwrap().status(), 201);
}

#[test]
fn coordinator_remains_available_after_fake_completion_hits_storage_backpressure() {
    use std::{
        process::{Command, Stdio},
        time::{Duration as StdDuration, Instant},
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let (d, a, _t) = fixture();
    a.create_case(FakeSource.manual_command("full-storage-worker"))
        .unwrap();
    let mut p = limits();
    p.max_evidence_bytes = 1024;
    a.install_factory_limits(p, "operator", "server liveness fixture")
        .unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute(
        "INSERT INTO evidence VALUES ('full','text/plain',?1,'fixture','now')",
        ["x".repeat(1024)],
    )
    .unwrap();
    let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let diagnostics = d.path().join("coordinator-stderr.log");
    let stderr = std::fs::File::create(&diagnostics).unwrap();
    let mut child = Child(
        Command::new(env!("CARGO_BIN_EXE_watchhouse"))
            .args([
                "--db",
                d.path().join("db").to_str().unwrap(),
                "serve",
                "--port",
                &port.to_string(),
            ])
            .env_clear()
            .env(
                "WATCHHOUSE_TOKEN",
                "storage-capacity-fixture-token-32-bytes",
            )
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap(),
    );
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(StdDuration::from_millis(500))
        .build()
        .unwrap();
    let start = Instant::now();
    let url = format!("http://127.0.0.1:{port}/api/v1/factory");
    loop {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "coordinator exited on expected backpressure: {}",
            std::fs::read_to_string(&diagnostics).unwrap_or_default()
        );
        if let Ok(r) = client
            .get(&url)
            .bearer_auth("storage-capacity-fixture-token-32-bytes")
            .send()
            && r.status().is_success()
        {
            let s: serde_json::Value = r.json().unwrap();
            if s["capacity"]["queued_cases"] == 0 {
                assert_eq!(s["capacity"]["ordinary_evidence_bytes"], 1024);
                break;
            }
        }
        assert!(
            start.elapsed() < StdDuration::from_secs(5),
            "coordinator health unavailable"
        );
        std::thread::sleep(StdDuration::from_millis(50));
    }
    std::thread::sleep(StdDuration::from_millis(300));
    assert!(child.0.try_wait().unwrap().is_none());
    assert!(
        client
            .get(&url)
            .bearer_auth("storage-capacity-fixture-token-32-bytes")
            .send()
            .unwrap()
            .status()
            .is_success()
    );
}
