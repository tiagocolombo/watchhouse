use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tower::ServiceExt;
use watchhouse::{
    access::{Policy, Secrets},
    application::Application,
    domain::ManualClock,
    store::Store,
};

const PROXY: &str = "fixture-proxy-credential-32-bytes-only";
const ALICE: &str = "fixture-alice-credential-32-bytes-only";
const BOB: &str = "fixture-observer-credential-32-bytes-only";
const ADMIN: &str = "fixture-admin-credential-32-bytes-only";
#[derive(Default)]
struct Keys(RwLock<HashMap<String, String>>);
impl Secrets for Keys {
    fn get(&self, r: &str) -> Option<String> {
        self.0.read().unwrap().get(r).cloned()
    }
}
fn policy() -> Policy {
    serde_json::from_value(json!({"schema_version":1,"revision":1,"public_origin":"https://watchhouse.example","proxy_token_env":"WATCHHOUSE_REMOTE_PROXY","max_audit_rows":100,"paused":false,"principals":[{"id":"alice","role":"operator","token_env":"WATCHHOUSE_REMOTE_ALICE","disabled":false},{"id":"bob","role":"observer","token_env":"WATCHHOUSE_REMOTE_BOB","disabled":false},{"id":"admin","role":"admin","token_env":"WATCHHOUSE_REMOTE_ADMIN","disabled":false}]})).unwrap()
}
fn setup() -> (tempfile::TempDir, Application, Arc<Keys>, axum::Router) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap(),
        )),
    );
    a.install_access_policy(policy(), "operator", "synthetic OS fixture")
        .unwrap();
    let keys = Arc::new(Keys::default());
    for (r, v) in [
        ("PROXY", PROXY),
        ("ALICE", ALICE),
        ("BOB", BOB),
        ("ADMIN", ADMIN),
    ] {
        keys.0
            .write()
            .unwrap()
            .insert(format!("WATCHHOUSE_REMOTE_{r}"), v.into());
    }
    let router =
        watchhouse::http::remote_router(a.clone(), "127.0.0.1:8787".into(), keys.clone()).unwrap();
    (d, a, keys, router)
}
fn request(method: &str, path: &str, key: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header("host", "127.0.0.1:8787")
        .header("x-forwarded-proto", "https")
        .header("x-forwarded-host", "watchhouse.example")
        .header("x-watchhouse-proxy-token", PROXY)
        .header("origin", "https://watchhouse.example")
        .header("authorization", format!("Bearer {key}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
fn create(actor: &str, key: &str) -> Value {
    json!({"schema_version":1,"actor":actor,"idempotency_key":key,"reason":"Synthetic identity boundary fixture","title":"Fixture incident","description":"No live service or account involved"})
}
async fn value(r: axum::response::Response) -> Value {
    serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn proxy_origin_and_distinct_credential_boundaries_fail_closed() {
    let (_d, a, keys, r) = setup();
    for header in [
        "host",
        "x-forwarded-proto",
        "x-forwarded-host",
        "x-watchhouse-proxy-token",
        "origin",
    ] {
        let mut req = request("GET", "/cases", ALICE, Value::Null);
        req.headers_mut()
            .insert(header, "attacker.example".parse().unwrap());
        assert_eq!(
            r.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN,
            "{header}"
        );
    }
    for header in [
        "authorization",
        "host",
        "x-forwarded-proto",
        "x-forwarded-host",
        "x-watchhouse-proxy-token",
        "origin",
    ] {
        let mut req = request("GET", "/cases", ALICE, Value::Null);
        let v = req.headers()[header].clone();
        req.headers_mut().append(header, v);
        let status = r.clone().oneshot(req).await.unwrap().status();
        assert!(
            status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
            "{header}"
        );
    }
    assert_eq!(
        r.clone()
            .oneshot(request("GET", "/cases", PROXY, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.clone()
            .oneshot(request(
                "GET",
                "/cases",
                "local-operator-token-32-bytes-only",
                Value::Null
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = r
        .clone()
        .oneshot(request("GET", "/session", ALICE, Value::Null))
        .await
        .unwrap();
    assert!(response.headers().contains_key("strict-transport-security"));
    assert_eq!(value(response).await["principal"], "alice");
    keys.0
        .write()
        .unwrap()
        .insert("WATCHHOUSE_REMOTE_ALICE".into(), PROXY.into());
    assert_eq!(
        r.oneshot(request("GET", "/cases", ALICE, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(watchhouse::http::remote_router(a, "127.0.0.1:8787".into(), keys).is_err());
}

#[tokio::test]
async fn roles_bind_actual_identity_keep_receipts_and_deny_worker_impersonation() {
    let (_d, a, _keys, r) = setup();
    assert_eq!(
        r.clone()
            .oneshot(request("POST", "/cases", BOB, create("bob", "observer")))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        r.clone()
            .oneshot(request(
                "POST",
                "/cases",
                ALICE,
                create("operator", "spoof")
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = r
        .clone()
        .oneshot(request(
            "POST",
            "/cases",
            ALICE,
            create("alice", "own-case"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let case = value(response).await;
    assert_eq!(case["owner"], "alice");
    let retry = value(
        r.clone()
            .oneshot(request(
                "POST",
                "/cases",
                ALICE,
                create("alice", "own-case"),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(retry, case);
    let id = case["id"].as_str().unwrap();
    assert_eq!(
        r.clone()
            .oneshot(request("GET", &format!("/cases/{id}"), BOB, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let control = json!({"meta":{"schema_version":1,"idempotency_key":"factory-pause","actor":"admin","reason":"Fixture global pause"},"expected_version":0,"action":{"type":"pause","scope":"global","target":null,"effect":"all","cancel_running":false}});
    // The role boundary precedes domain parsing/version checks.
    assert_eq!(
        r.clone()
            .oneshot(request("POST", "/factory", ALICE, control.clone()))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        r.clone()
            .oneshot(request("POST", "/factory", ADMIN, control))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    for key in [ALICE, ADMIN] {
        for endpoint in [
            "/team/claim",
            "/team/heartbeat",
            "/team/progress",
            "/team/block",
            "/team/finish",
        ] {
            assert_eq!(
                r.clone()
                    .oneshot(request("POST", endpoint, key, json!({"actor":"admin"})))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
    }
    let mut conflicting = create("alice", "contradictory");
    conflicting["meta"] = json!({"actor":"admin"});
    assert_eq!(
        r.oneshot(request("POST", "/cases", ALICE, conflicting))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let audit = serde_json::to_string(&a.access_audit().unwrap()).unwrap();
    for forbidden in [
        ALICE,
        BOB,
        ADMIN,
        PROXY,
        "Fixture incident",
        "No live service",
    ] {
        assert!(!audit.contains(forbidden));
    }
    assert!(
        audit.contains("identity_denied")
            && audit.contains("role_denied")
            && audit.contains("alice")
    );
}

#[tokio::test]
async fn key_rotation_policy_revocation_and_live_stream_revalidation() {
    let (_d, a, keys, r) = setup();
    let c = a
        .create_case(serde_json::from_value(create("alice", "stream-case")).unwrap())
        .unwrap();
    let response = r
        .clone()
        .oneshot(request(
            "GET",
            &format!("/cases/{}/events", c.id),
            ALICE,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let first = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(first.data_ref().unwrap()).contains("timeline"));
    let mut p = policy();
    p.revision = 2;
    p.principals[0].disabled = true;
    a.install_access_policy(p, "operator", "Disable identity from OS")
        .unwrap();
    let bytes = tokio::time::timeout(std::time::Duration::from_secs(2), body.collect())
        .await
        .unwrap()
        .unwrap()
        .to_bytes();
    assert!(String::from_utf8_lossy(&bytes).contains("access_revoked"));
    assert_eq!(
        r.clone()
            .oneshot(request("GET", "/cases", ALICE, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    keys.0.write().unwrap().insert(
        "WATCHHOUSE_REMOTE_BOB".into(),
        "fixture-rotated-observer-32-bytes-only".into(),
    );
    assert_eq!(
        r.clone()
            .oneshot(request("GET", "/cases", BOB, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        r.oneshot(request(
            "GET",
            "/session",
            "fixture-rotated-observer-32-bytes-only",
            Value::Null
        ))
        .await
        .unwrap()
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn audit_capacity_denies_mutation_without_stopping_local_duties() {
    let (_d, a, keys, r) = setup();
    // Populate through authenticated reads, with fresh router rate buckets only;
    // every request shares the same durable audit quota.
    for _ in 0..2 {
        let router =
            watchhouse::http::remote_router(a.clone(), "127.0.0.1:8787".into(), keys.clone())
                .unwrap();
        for _ in 0..50 {
            assert_eq!(
                router
                    .clone()
                    .oneshot(request("GET", "/cases", BOB, Value::Null))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
        }
    }
    assert_eq!(
        r.clone()
            .oneshot(request(
                "POST",
                "/cases",
                ALICE,
                create("alice", "overflow")
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(a.list_cases().unwrap().len(), 0);
    a.create_case(serde_json::from_value(create("alice", "OS-available")).unwrap())
        .unwrap();
    a.supervise_team().unwrap();
    assert_eq!(a.factory_status().unwrap().state, "limited");
    assert_eq!(a.access_status().unwrap().audit_remaining, 0);
    assert_eq!(
        r.oneshot(request("GET", "/session", ALICE, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn bounded_bodies_and_incomplete_audit_preserve_authoritative_command_receipts() {
    let (d, a, _keys, r) = setup();
    let mut req = request("POST", "/cases", ALICE, Value::Null);
    *req.body_mut() = Body::from("x".repeat(32 * 1024 + 1));
    assert_eq!(
        r.clone().oneshot(req).await.unwrap().status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut req = request("POST", "/cases", ALICE, Value::Null);
    *req.body_mut() = Body::from("{invalid-json");
    assert_eq!(
        r.clone().oneshot(req).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    assert!(a.list_cases().unwrap().is_empty());
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fixture_lost_audit_completion BEFORE INSERT ON api_access_results BEGIN SELECT RAISE(ABORT,'synthetic audit completion interruption'); END;").unwrap();
    let response = r
        .clone()
        .oneshot(request(
            "POST",
            "/cases",
            ALICE,
            create("alice", "retained-receipt"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let original = value(response).await;
    let later = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 1, 0).unwrap(),
        )),
    );
    assert_eq!(later.access_status().unwrap().audit_stale_incomplete, 1);
    assert_eq!(later.factory_status().unwrap().state, "degraded");
    db.execute_batch("DROP TRIGGER fixture_lost_audit_completion;")
        .unwrap();
    let retry = value(
        r.oneshot(request(
            "POST",
            "/cases",
            ALICE,
            create("alice", "retained-receipt"),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(retry, original);
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert!(
        a.access_audit()
            .unwrap()
            .iter()
            .any(|v| v["status"].is_null())
    );
}

#[tokio::test]
async fn stream_capacity_is_held_until_the_reader_disconnects() {
    let (_d, a, _keys, r) = setup();
    let c = a
        .create_case(serde_json::from_value(create("alice", "stream-capacity")).unwrap())
        .unwrap();
    let path = format!("/cases/{}/events", c.id);
    let mut streams = vec![];
    for _ in 0..16 {
        let response = r
            .clone()
            .oneshot(request("GET", &path, BOB, Value::Null))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        streams.push(response);
    }
    assert_eq!(
        r.clone()
            .oneshot(request("GET", &path, BOB, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    drop(streams.pop());
    assert_eq!(
        r.oneshot(request("GET", &path, BOB, Value::Null))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[test]
fn human_case_owner_receives_supervised_escalation() {
    let (d, a, _keys, _r) = setup();
    let case = a
        .create_case(serde_json::from_value(create("alice", "owned-escalation")).unwrap())
        .unwrap();
    a.run_fake_worker_once("fake-investigator", true).unwrap();
    let later = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 10, 0).unwrap(),
        )),
    );
    later.supervise_team().unwrap();
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    let owners: Vec<String> = db
        .prepare("SELECT json_extract(record,'$.owner') FROM escalations WHERE case_id=?1")
        .unwrap()
        .query_map([case.id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!owners.is_empty());
    assert!(owners.iter().all(|owner| owner == "alice"));
}

#[test]
fn policy_revisions_and_restored_credentials_require_fresh_os_authority() {
    let (d, a, keys, _r) = setup();
    let original = a
        .install_access_policy(policy(), "operator", "Retry identical policy")
        .unwrap();
    assert!(!original.is_empty());
    let mut changed = policy();
    changed.paused = true;
    assert!(
        a.install_access_policy(changed.clone(), "operator", "Mutate old revision")
            .is_err()
    );
    changed.revision = 3;
    assert!(
        a.install_access_policy(changed, "operator", "Skip revision")
            .is_err()
    );
    let mut invalid = policy();
    invalid.public_origin = "http://watchhouse.example".into();
    assert!(invalid.validate().is_err());
    invalid = policy();
    invalid.principals[0].token_env = invalid.proxy_token_env.clone();
    assert!(invalid.validate().is_err());
    let backup = d.path().join("backup");
    let report = a
        .create_backup(&backup, "operator", "Fixture copied-identity isolation")
        .unwrap();
    let output = d.path().join("restored");
    let _record = watchhouse::restore::restore(
        &backup,
        &output,
        &report.manifest_digest,
        "operator",
        "Fixture restore",
    )
    .unwrap();
    let restored = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(watchhouse::domain::SystemClock),
    );
    assert!(restored.access_status().unwrap().restored_authority_held);
    assert!(
        watchhouse::http::remote_router(restored.clone(), "127.0.0.1:8787".into(), keys.clone())
            .is_err()
    );
    let mut p = policy();
    p.revision = 2;
    restored
        .install_access_policy(p, "operator", "Fresh deployment proxy and identities")
        .unwrap();
    assert!(watchhouse::http::remote_router(restored, "127.0.0.1:8787".into(), keys).is_ok());
}
