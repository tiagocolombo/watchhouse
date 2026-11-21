mod common;
use chrono::Duration;
use common::*;
use watchhouse::{
    application::Application,
    contracts::{CommandMeta, MessageKind, PostMessage, RunCommand, RunProof, V1},
    execution::{ExecutionAdapter, FakeAdapter},
    factory::{Action, Command, Effect, Scope},
    intake::FakeSource,
    store::Store,
};
fn meta(key: &str) -> CommandMeta {
    CommandMeta {
        schema_version: V1,
        idempotency_key: key.into(),
        actor: "operator".into(),
        reason: "Synthetic factory control fixture".into(),
    }
}
fn control(a: &Application, key: &str, action: Action) -> Command {
    Command {
        meta: meta(key),
        expected_version: a.factory_status().unwrap().version,
        action,
    }
}
fn pause(scope: Scope, target: Option<String>, effect: Effect, stop: bool) -> Action {
    Action::Pause {
        scope,
        target,
        effect,
        cancel_running: stop,
    }
}
#[test]
fn scoped_pause_preserves_unrelated_intake_and_idempotent_control_history() {
    let (_d, a, _t, id) = fixture();
    let other = a
        .create_case(FakeSource.manual_command("other-factory-case"))
        .unwrap();
    let cmd = control(
        &a,
        "pause-one",
        pause(Scope::Case, Some(id.clone()), Effect::All, false),
    );
    let r = a.control_factory(cmd.clone()).unwrap();
    assert_eq!(r.version, 1);
    assert_eq!(r.affected_jobs, 0);
    assert_eq!(a.control_factory(cmd).unwrap().pause_id, r.pause_id);
    let claimed = a.claim().unwrap().unwrap();
    assert_eq!(claimed.case_id, other.id);
    assert!(a.claim().unwrap().is_none());
    let fresh = a
        .create_case(FakeSource.manual_command("intake-while-paused"))
        .unwrap();
    assert_eq!(a.claim().unwrap().unwrap().case_id, fresh.id);
    let resumed = a
        .control_factory(control(
            &a,
            "resume-one",
            Action::Resume {
                pause_id: r.pause_id,
            },
        ))
        .unwrap();
    assert_eq!(resumed.version, 2);
    assert_eq!(a.claim().unwrap().unwrap().case_id, id);
    assert_eq!(a.factory_status().unwrap().audit.len(), 2);
}
#[test]
fn service_and_repository_scopes_match_source_and_manual_repair_mapping() {
    let (d, a, _t, id) = fixture();
    let other = a
        .create_case(FakeSource.manual_command("manual-repository-case"))
        .unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    // Minimal SQL projection fixture: not candidate/native execution evidence.
    db.execute("INSERT INTO repair_requests VALUES ('projection-only',?1,'{\"repository\":\"example/fixture-service\"}')",[&other.id]).unwrap();
    let service = a
        .control_factory(control(
            &a,
            "service-hold",
            pause(
                Scope::Service,
                Some("checkout".into()),
                Effect::Model,
                false,
            ),
        ))
        .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM factory_holds WHERE pause_id=?1",
            [service.pause_id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let repo = a
        .control_factory(control(
            &a,
            "repo-hold",
            pause(
                Scope::Repository,
                Some("example/fixture-service".into()),
                Effect::All,
                false,
            ),
        ))
        .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM factory_holds WHERE pause_id=?1",
            [repo.pause_id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert!(a.claim().unwrap().is_none());
    assert!(
        a.factory_status()
            .unwrap()
            .pauses
            .iter()
            .any(|p| p["target"] == "checkout")
    );
    assert!(!id.is_empty());
}
#[test]
fn stopped_fake_fence_cannot_resume_and_timers_escalate_while_paused() {
    let (_d, a, t, id) = fixture();
    let claim = a.claim().unwrap().unwrap();
    a.post_team_message(
        &id,
        PostMessage {
            command: command(&a, &id, "offline-question", "operator"),
            kind: MessageKind::Question,
            body: "Need a bounded human decision".into(),
            recipients: vec!["operator".into()],
            evidence_refs: vec![],
            reply_to: None,
            ack_within_seconds: Some(1),
        },
    )
    .unwrap();
    let stopped = a
        .control_factory(control(
            &a,
            "global-stop",
            pause(Scope::Global, None, Effect::All, true),
        ))
        .unwrap();
    assert_eq!(stopped.affected_jobs, 1);
    assert!(
        a.complete(&claim, FakeAdapter.execute(&claim.task))
            .is_err()
    );
    t.advance(Duration::seconds(2));
    a.supervise_team().unwrap();
    assert!(a.factory_status().unwrap().open_escalations > 0);
    a.control_factory(control(
        &a,
        "resume-global",
        Action::Resume {
            pause_id: stopped.pause_id,
        },
    ))
    .unwrap();
    assert!(
        a.heartbeat(RunCommand {
            meta: meta("stale-heartbeat"),
            proof: RunProof {
                run_id: claim.run_id,
                fence: claim.fence
            }
        })
        .is_err()
    );
    assert!(a.snapshot(&id).unwrap().team.late_results.len() == 1);
}
#[test]
fn control_conflicts_and_audit_failure_rollback_every_projection() {
    let (d, a, _t, id) = fixture();
    let cmd = control(
        &a,
        "atomic-control",
        pause(Scope::Case, Some(id), Effect::All, true),
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER reject_control BEFORE INSERT ON factory_events BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END;").unwrap();
    assert!(a.control_factory(cmd.clone()).is_err());
    assert_eq!(a.factory_status().unwrap().version, 0);
    assert!(a.factory_status().unwrap().pauses.is_empty());
    db.execute_batch("DROP TRIGGER reject_control").unwrap();
    a.control_factory(cmd.clone()).unwrap();
    assert_eq!(a.control_factory(cmd.clone()).unwrap().version, 1);
    let mut conflict = cmd;
    conflict.meta.idempotency_key = "stale-control".into();
    assert!(a.control_factory(conflict).is_err());
    let mut agent = control(
        &a,
        "agent-control",
        pause(Scope::Global, None, Effect::All, false),
    );
    agent.meta.actor = "fake-agent".into();
    assert!(a.control_factory(agent).is_err());
    assert!(db.execute("DELETE FROM factory_events", []).is_err());
}
#[test]
fn backward_clock_holds_new_effects_without_resetting_retained_time() {
    let (_d, a, t, _id) = fixture();
    a.supervise_team().unwrap();
    let high = a.factory_status().unwrap().clock_high_at;
    t.advance(Duration::seconds(-10));
    assert!(a.claim().unwrap().is_none());
    let s = a.factory_status().unwrap();
    assert!(s.clock_behind);
    assert_eq!(s.clock_high_at, high);
    assert_eq!(s.state, "degraded");
    t.advance(Duration::seconds(11));
    assert!(a.claim().unwrap().is_some());
    assert!(!a.factory_status().unwrap().clock_behind);
}
#[tokio::test]
async fn factory_api_requires_auth_and_preserves_versioned_commands() {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (_d, a, _t, _id) = fixture();
    let router = watchhouse::http::router(
        a.clone(),
        "factory-fixture-token-32-bytes-minimum".into(),
        "127.0.0.1:8787".into(),
    );
    let denied = router
        .clone()
        .oneshot(
            Request::get("/api/v1/factory")
                .header("host", "127.0.0.1:8787")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), 401);
    let cmd = control(
        &a,
        "http-pause",
        pause(Scope::Global, None, Effect::All, false),
    );
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/v1/factory")
                .header("host", "127.0.0.1:8787")
                .header(
                    "authorization",
                    "Bearer factory-fixture-token-32-bytes-minimum",
                )
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&cmd).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let data: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(data["version"], 1);
    assert!(a.claim().unwrap().is_none());
}
