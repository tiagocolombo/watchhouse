use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use watchhouse::{
    application::Application,
    contracts::*,
    domain::{Case, SystemClock},
    execution::FakeAdapter,
    store::Store,
};
const TOKEN: &str = "test-operator-token-32-bytes-minimum";
fn setup() -> (TempDir, Application, axum::Router) {
    let dir = TempDir::new().unwrap();
    let app = Application::new(
        Store::open(dir.path().join("test.db")).unwrap(),
        Arc::new(SystemClock),
    );
    let router = watchhouse::http::router(app.clone(), TOKEN.into(), "127.0.0.1:8787".into());
    (dir, app, router)
}
fn request(method: &str, path: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("Host", "127.0.0.1:8787")
        .header("Authorization", format!("Bearer {TOKEN}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
async fn auth_origin_schema_and_conflicts_are_enforced() {
    let (_dir, app, router) = setup();
    let mut req = request("GET", "/api/v1/cases", "");
    req.headers_mut().remove("Authorization");
    assert_eq!(
        router.clone().oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let mut req = request("GET", "/api/v1/cases", "");
    req.headers_mut()
        .insert("Origin", "https://attacker.example".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let mut req = request("GET", "/", "");
    req.headers_mut()
        .insert("Host", "attacker.example".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let fixture = include_str!("../contracts/v1/manual-case.json");
    let response = router
        .clone()
        .oneshot(request("POST", "/api/v1/cases", fixture))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let case: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let id = case["id"].as_str().unwrap();
    let retry = router
        .clone()
        .oneshot(request("POST", "/api/v1/cases", fixture))
        .await
        .unwrap();
    let retry: serde_json::Value =
        serde_json::from_slice(&retry.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(retry, case);
    let changed = fixture.replace("Intermittent checkout timeout", "Different title");
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", "/api/v1/cases", &changed))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let newer = fixture.replace("\"schema_version\": 1", "\"schema_version\": 2");
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", "/api/v1/cases", &newer))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    app.run_once(&FakeAdapter).unwrap();
    let stale = serde_json::json!({"schema_version":1,"idempotency_key":"stale","actor":"operator","reason":"Rename","title":"Updated","expected_version":2});
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/cases/{id}/rename"),
                &stale.to_string()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let response = router
        .clone()
        .oneshot(request("GET", "/", ""))
        .await
        .unwrap();
    assert!(response.headers().contains_key("content-security-policy"));
    assert_eq!(
        router
            .oneshot(request(
                "GET",
                &format!("/api/v1/cases/{id}/events?after=-1"),
                ""
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn timeline_snapshot_and_last_event_id_replay_across_reconnection() {
    let (_dir, app, router) = setup();
    let case = app
        .create_case(
            serde_json::from_str::<CreateCase>(include_str!("../contracts/v1/manual-case.json"))
                .unwrap(),
        )
        .unwrap();
    let snapshot = app.snapshot(&case.id).unwrap();
    let cursor = snapshot.cursor;
    // Connection disappears after the snapshot, before work completes.
    app.run_once(&FakeAdapter).unwrap();
    let mut req = request(
        "GET",
        &format!("/api/v1/cases/{}/events?after=0", case.id),
        "",
    );
    req.headers_mut()
        .insert("Last-Event-ID", cursor.to_string().parse().unwrap());
    let response = router.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut body = response.into_body();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
    assert!(text.contains("run.started"));
    assert!(!text.contains("case.created"));
    let expected = app.timeline(&case.id, cursor).unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0].aggregate_version, 3);
    let mut received = text;
    if !received.contains("assignment.completed") {
        let frame = body.frame().await.unwrap().unwrap();
        received.push_str(std::str::from_utf8(&frame.into_data().unwrap()).unwrap());
    }
    assert!(received.contains("assignment.completed"));
}
#[tokio::test]
async fn qualification_commands_share_strict_version_conflict_and_replay_contracts() {
    let (_dir, app, router) = setup();
    let case = app
        .create_case(
            serde_json::from_str(include_str!("../contracts/v1/manual-case.json")).unwrap(),
        )
        .unwrap();
    app.run_once(&FakeAdapter).unwrap();
    let snapshot = app.snapshot(&case.id).unwrap();
    let evidence = serde_json::json!({"schema_version":1,"idempotency_key":"http-evidence","actor":"operator","reason":"Sanitized HTTP fixture","expected_version":snapshot.case.version,"scope":"staging checkout","statement":"A dependency timeout was manually reported","collection_method":"Fixture report","observed_at":null,"uncertainty":"Cause and recovery unknown"});
    let path = format!("/api/v1/cases/{}/evidence", case.id);
    let response = router
        .clone()
        .oneshot(request("POST", &path, &evidence.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: RecordedEvidence =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let command = serde_json::json!({"schema_version":1,"idempotency_key":"http-disposition","actor":"operator","reason":"Monitor dependency behavior without asserting recovery","expected_version":receipt.case.version,"owner":"operator","scope":"staging checkout","uncertainty":"Traffic and recovery unknown","evidence_refs":[receipt.evidence.digest],"reconsideration_conditions":"New failures, scope changes, or deadline","reopen_after_occurrences":1,"plan":{"kind":"apparently_transient","after_seconds":60,"recovery_indicators":"Successful checkouts at sufficient volume"}});
    let path = format!("/api/v1/cases/{}/dispositions", case.id);
    let response = router
        .clone()
        .oneshot(request("POST", &path, &command.to_string()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: Case =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let mut changed = command.clone();
    changed["idempotency_key"] = "stale-decision".into();
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", &path, &changed.to_string()))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut unknown = command.clone();
    unknown["schema_version"] = 2.into();
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", &path, &unknown.to_string()))
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    let snapshot = app.snapshot(&case.id).unwrap();
    let cursor = snapshot.cursor;
    let reconsider = serde_json::json!({"schema_version":1,"idempotency_key":"http-review","actor":"operator","reason":"Operator requests early review","expected_version":receipt.version,"disposition_id":snapshot.qualification.dispositions[0].id,"trigger":"operator_review","evidence_refs":[snapshot.qualification.evidence[0].digest]});
    let path = format!("/api/v1/cases/{}/reconsider", case.id);
    assert_eq!(
        router
            .oneshot(request("POST", &path, &reconsider.to_string()))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let events = app.timeline(&case.id, cursor).unwrap();
    assert_eq!(events.len(), 3);
    assert!(matches!(events[1].data, EventData::CaseReopened { .. }));
    for event in events {
        assert!(serde_json::from_value::<Event>(serde_json::to_value(event).unwrap()).is_ok());
    }
}
