use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use watchhouse::{
    application::Application, contracts::*, domain::SystemClock, execution::FakeAdapter,
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
