//! Controller fixtures are not native process/resource proof. No account or paid model is used.
use super::{cgroup::*, client::ClientResult, codex::*, ops::*, policy::*, protocol::*};
use crate::{
    application::Application, contracts::*, domain::ManualClock, intake::FakeSource,
    policy::Policy, store::Store,
};
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn stream(output: Value) -> String {
    [
        json!({"type":"thread.started","thread_id":"fixture-session"}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"type":"agent_message","text":output.to_string()}}),
        json!({"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":4}}),
    ]
    .iter()
    .map(|v| format!("{v}\n"))
    .collect()
}
#[test]
fn protocol_rejects_tool_interaction_and_terminal_or_schema_confusion() {
    let output = json!({"schema_version":1,"summary":"bounded fix","changes":[{"path":"answer.txt","before_digest":null,"after":"6\n"}]});
    let text = stream(output.clone());
    let p = parse(&text, Role::Engineer).unwrap();
    assert!(p.actual_model.is_none());
    assert_eq!(p.usage.input_tokens, Some(3));
    for item in [
        "command_execution",
        "mcp_tool_call",
        "request_user_input",
        "web_search",
        "plan",
        "file_change",
    ] {
        let malicious = text.replace("agent_message", item);
        assert!(parse(&malicious, Role::Engineer).is_err(), "{item}");
    }
    assert!(
        parse(
            &(text.clone() + "{\"type\":\"turn.started\"}\n"),
            Role::Engineer
        )
        .is_err()
    );
    assert!(
        parse(
            &text.replace("thread.started", "unknown-event"),
            Role::Engineer
        )
        .is_err()
    );
    assert!(
        parse(
            &text.replace("turn.started", "turn.completed"),
            Role::Engineer
        )
        .is_err()
    );
    assert!(
        parse(
            &text.replace("turn.completed", "turn.failed"),
            Role::Engineer
        )
        .is_err()
    );
    let mut bad = output.clone();
    bad["changes"][0]["path"] = json!("../policy");
    assert!(parse(&stream(bad), Role::Engineer).is_err());
    let mut bad = output;
    bad["approve"] = json!(true);
    assert!(parse(&stream(bad), Role::Engineer).is_err());
    assert!(parse(&text, Role::Reviewer).is_err());
    let no_usage = text.replace("\"input_tokens\":3,\"output_tokens\":4", "");
    assert_eq!(
        parse(&no_usage, Role::Engineer).unwrap().usage.source,
        "unknown"
    );
}
#[test]
fn reasoning_is_discarded_and_reordered_or_duplicate_outputs_rejected() {
    let text =
        stream(json!({"schema_version":1,"approve":false,"findings":"regression insufficient"}));
    let private = "PRIVATE_REASONING_NEVER_RETAIN";
    let reasoning = format!(
        "{{\"type\":\"item.completed\",\"item\":{{\"type\":\"reasoning\",\"text\":\"{private}\"}}}}\n"
    );
    let mut lines = text.lines().collect::<Vec<_>>();
    let final_line = lines.pop().unwrap();
    let with_private = format!("{}\n{}{final_line}\n", lines.join("\n"), reasoning);
    let p = parse(&with_private, Role::Reviewer).unwrap();
    assert!(!serde_json::to_string(&p).unwrap().contains(private));
    let duplicate = format!("{}\n{}\n{}\n", lines.join("\n"), lines[2], final_line);
    assert!(parse(&duplicate, Role::Reviewer).is_err());
    assert!(
        parse(
            &format!("{}\n{}\n{}\n{}", lines[2], lines[0], lines[1], final_line),
            Role::Reviewer
        )
        .is_err()
    );
}
struct Fixture {
    root: tempfile::TempDir,
    app: Application,
    clock: ManualClock,
    case: String,
    p: ModelPolicy,
}
fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let clock = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap());
    let app = Application::new(
        Store::open(root.path().join("db")).unwrap(),
        Arc::new(clock.clone()),
    );
    let mut governing: Policy =
        serde_json::from_str(include_str!("../../policy/local-fake-v1.json")).unwrap();
    governing.revision = 2;
    governing.name = "explicit-model-controller-v2".into();
    app.install_policy(governing, "operator", "model-controller-fixture")
        .unwrap();
    let case = app
        .create_case(FakeSource.manual_command("model-fixture"))
        .unwrap()
        .id;
    // Fake resources never execute this tiny pinned fixture. Avoid repeatedly hashing the
    // entire debug test executable in ordinary deterministic controller tests.
    let binary = root.path().join("never-executed-client");
    std::fs::write(&binary, b"synthetic-client-identity").unwrap();
    let pin = Pin {
        path: binary.canonicalize().unwrap(),
        digest: crate::store::digest("synthetic-client-identity"),
    };
    let jobs = root.path().join("jobs");
    std::fs::create_dir(&jobs).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&jobs, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let p = ModelPolicy {
        profile: "default".into(),
        adapter: crate::model_runtime::policy::Adapter::Codex,
        schema_version: V1,
        revision: 1,
        container_revision: 1,
        dispatch_grace_seconds: 10,
        runtime: pin.clone(),
        initializer: pin,
        cgroup_root: root.path().join("synthetic-cgroup-root"),
        jobs_root: jobs.canonicalize().unwrap(),
        requested_model: "synthetic-only".into(),
        authority: Authority::Fixture,
        limits: Limits {
            memory_bytes: 256 * 1024 * 1024,
            pids: 32,
            cpu_micros: 50000,
            wall_seconds: 10,
            output_bytes: 65536,
        },
        proof_seconds: 60,
        cleanup_attempts: 2,
        cleanup_retry_seconds: 1,
    };
    app.install_model_policy(
        p.clone(),
        "operator",
        "controller fixture, no native qualification",
    )
    .unwrap();
    Fixture {
        root,
        app,
        clock,
        case,
        p,
    }
}
fn command(f: &Fixture, key: &str) -> OperatorCommand {
    OperatorCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: "operator".into(),
            reason: "synthetic controller verification".into(),
        },
        expected_version: f.app.snapshot(&f.case).unwrap().case.version,
    }
}
fn queue(f: &Fixture, key: &str) -> Job {
    f.app
        .request_model(
            &f.case,
            Request {
                command: command(f, key),
                policy_revision: 1,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap()
}

#[tokio::test]
async fn restore_never_touches_original_model_roots_and_requires_new_inspection_and_policy() {
    let f = fixture();
    let original = Fake::default();
    let j = queue(&f, "restore-model");
    f.app.run_model_once(&original).await.unwrap();
    let canary = j.root.join("private-auth-fixture");
    std::fs::write(&canary, "synthetic private neighbor").unwrap();
    let names = |p: &std::path::Path| {
        let mut v = std::fs::read_dir(p)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>();
        v.sort();
        v
    };
    let before = names(&j.root);
    let evidence = |app: &Application, key: &str| {
        app.record_evidence(&f.case,serde_json::from_value(json!({"schema_version":1,"idempotency_key":key,"actor":"operator","reason":"Controlled reported cleanup fixture","expected_version":app.snapshot(&f.case).unwrap().case.version,"scope":"synthetic resource only","statement":"Operator reports retired original deployment and inspected its resource","collection_method":"Fixture inspection, not physical proof","observed_at":null,"uncertainty":"No native/live capability granted"})).unwrap()).unwrap().evidence.digest
    };
    let old_evidence = evidence(&f.app, "before-restore");
    let backup = f.root.path().join("backup");
    let b = f
        .app
        .create_backup(&backup, "operator", "Model restore fixture")
        .unwrap();
    let output = f.root.path().join("restored");
    let r = crate::restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "Isolate old resource",
    )
    .unwrap();
    let now = chrono::DateTime::parse_from_rfc3339(&r.restored_at)
        .unwrap()
        .with_timezone(&Utc)
        + chrono::Duration::seconds(1);
    let app = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(ManualClock::new(now)),
    );
    let status = app.factory_status().unwrap();
    app.control_factory(crate::factory::Command {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: "resume-restored-model".into(),
            actor: "operator".into(),
            reason: "Review fixture hold".into(),
        },
        expected_version: status.version,
        action: crate::factory::Action::Resume {
            pause_id: r.pause_id,
        },
    })
    .unwrap();
    let backend = Fake::default();
    assert!(!app.run_model_once(&backend).await.unwrap());
    let current = app.model_job(&f.case, &j.id).unwrap();
    assert_eq!(current.state, "unknown");
    assert!(current.quarantined);
    assert!(
        app.request_model(
            &f.case,
            Request {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "old-profile-after-resume".into(),
                        actor: "operator".into(),
                        reason: "Must not inherit old policy".into()
                    },
                    expected_version: app.snapshot(&f.case).unwrap().case.version
                },
                policy_revision: 1,
                action: Action::Qualification,
                fixture_response: None
            }
        )
        .is_err()
    );
    let manage = |key: &str, action, evidence_refs| Manage {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: key.into(),
                actor: "operator".into(),
                reason: "Inspect old deployment before closure".into(),
            },
            expected_version: app.snapshot(&f.case).unwrap().case.version,
        },
        job_id: j.id.clone(),
        action,
        evidence_refs,
    };
    assert!(
        app.manage_model(
            &f.case,
            manage("deny-old-cancel", ManageAction::Cancel, vec![]),
            &backend
        )
        .await
        .is_err()
    );
    assert!(
        app.manage_model(
            &f.case,
            manage(
                "deny-old-proof",
                ManageAction::ReportCleanup,
                vec![old_evidence]
            ),
            &backend
        )
        .await
        .is_err()
    );
    let fresh = evidence(&app, "new-after-restore");
    let closed = app
        .manage_model(
            &f.case,
            manage(
                "reported-old-cleanup",
                ManageAction::ReportCleanup,
                vec![fresh],
            ),
            &backend,
        )
        .await
        .unwrap();
    assert_eq!(closed.state, "reported_cleaned");
    assert!(!closed.native);
    assert!(!closed.quarantined);
    assert_eq!(names(&j.root), before);
    assert_eq!(
        std::fs::read_to_string(canary).unwrap(),
        "synthetic private neighbor"
    );
    assert_eq!(original.state.lock().unwrap().cancels, 0);
    assert_eq!(backend.state.lock().unwrap().creates, 0);
    assert_eq!(backend.state.lock().unwrap().cancels, 0);
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "created");
    let mut p = f.p.clone();
    p.revision = 2;
    app.install_model_policy(p, "operator", "Fresh explicit fixture authority")
        .unwrap();
    let case = app
        .create_case(FakeSource.manual_command("fresh-after-restore"))
        .unwrap();
    let queued = app
        .request_model(
            &case.id,
            Request {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "fresh-model-after-restore".into(),
                        actor: "operator".into(),
                        reason: "New fixture qualification, no inherited proof".into(),
                    },
                    expected_version: case.version,
                },
                policy_revision: 2,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    assert_eq!(queued.state, "queued");
    assert!(app.run_model_once(&backend).await.unwrap());
    assert_eq!(backend.state.lock().unwrap().creates, 1);
    assert!(admission(&app.store.connect().unwrap(), &f.p, &app.now()).is_err());
}
#[derive(Default)]
struct State {
    creates: u32,
    starts: u32,
    cancels: u32,
    lost_start: bool,
    lost_create: bool,
    lost_cancel: bool,
    preflight_fail: bool,
    empty_before: bool,
}
#[derive(Default)]
struct Fake {
    state: Arc<Mutex<State>>,
}
impl Backend for Fake {
    fn preflight(&self, _: ModelPolicy) -> Effect<()> {
        let fail = self.state.lock().unwrap().preflight_fail;
        Box::pin(async move {
            if fail {
                Err(crate::error::Error::Invalid(
                    "fixture preflight failure".into(),
                ))
            } else {
                Ok(())
            }
        })
    }
    fn create(&self, j: Job, p: ModelPolicy) -> Effect<Handle> {
        let mut s = self.state.lock().unwrap();
        s.creates += 1;
        let lost = s.lost_create;
        drop(s);
        Box::pin(async move {
            if lost {
                return Err(crate::error::Error::Conflict("lost create ack".into()));
            }
            Ok(Handle {
                operation: j.id.clone(),
                directory: p.cgroup_root.join(format!("watchhouse-{}", j.id)),
                inode: 123,
                boot_id: uuid::Uuid::nil().to_string(),
                limits: p.limits,
                profile_digest: j.profile_digest,
            })
        })
    }
    fn recover(&self, j: Job, p: ModelPolicy) -> Effect<Handle> {
        Box::pin(async move {
            Ok(Handle {
                operation: j.id.clone(),
                directory: p.cgroup_root.join(format!("watchhouse-{}", j.id)),
                inode: 123,
                boot_id: uuid::Uuid::nil().to_string(),
                limits: p.limits,
                profile_digest: j.profile_digest,
            })
        })
    }
    fn start(&self, _: Job, _: ModelPolicy) -> Effect<()> {
        let mut s = self.state.lock().unwrap();
        s.starts += 1;
        let lost = s.lost_start;
        Box::pin(async move {
            if lost {
                Err(crate::error::Error::Conflict("lost start ack".into()))
            } else {
                Ok(())
            }
        })
    }
    fn output(&self, j: Job, _: ModelPolicy) -> Effect<Option<Output>> {
        Box::pin(async move {
            Ok(Some(Output {
                schema_version: V1,
                operation: j.id,
                nonce: j.nonce.clone(),
                profile_digest: j.profile_digest,
                client: ClientResult {
                    parsed: Some(parse(
                        &stream(json!({"schema_version":1,"nonce":j.nonce})),
                        Role::Qualification,
                    )?),
                    failure_code: None,
                    stderr_digest: crate::store::digest(""),
                },
                synthetic: true,
                execution_tools_absent: Some(true),
                advertised_tools_digest: Some(crate::store::digest("[]")),
                descendant_started: true,
            }))
        })
    }
    fn cancel(&self, _: Job, _: ModelPolicy) -> Effect<bool> {
        let mut s = self.state.lock().unwrap();
        s.cancels += 1;
        let lost = s.lost_cancel;
        let before = !s.empty_before;
        Box::pin(async move {
            if lost {
                Err(crate::error::Error::Conflict("lost cancel ack".into()))
            } else {
                Ok(before)
            }
        })
    }
}
#[tokio::test]
async fn simulated_protocol_never_admits_native_and_acceptance_waits_for_cleanup() {
    let f = fixture();
    let b = Fake::default();
    let j = queue(&f, "q");
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "created");
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "running");
    f.app.run_model_once(&b).await.unwrap();
    let staged = f.app.model_job(&f.case, &j.id).unwrap();
    assert_eq!(staged.state, "finishing");
    assert!(staged.quarantined);
    f.app.run_model_once(&b).await.unwrap();
    let done = f.app.model_job(&f.case, &j.id).unwrap();
    assert_eq!(done.state, "simulated");
    assert!(!done.quarantined);
    assert_eq!(b.state.lock().unwrap().cancels, 1);
    assert!(admission(&f.app.store.connect().unwrap(), &f.p, &f.app.now()).is_err());
    let reopened = Application::new(
        Store::open(f.root.path().join("db")).unwrap(),
        Arc::new(f.clock.clone()),
    );
    assert_eq!(
        reopened.model_job(&f.case, &j.id).unwrap().state,
        "simulated"
    );
    let mut changed = f.p.clone();
    changed.requested_model = "different".into();
    assert!(
        f.app
            .install_model_policy(changed, "operator", "changed fixture")
            .is_err()
    );
}
#[tokio::test]
async fn unknown_start_is_not_replayed_and_cleanup_budget_cannot_be_reset() {
    let f = fixture();
    let b = Fake::default();
    b.state.lock().unwrap().lost_start = true;
    b.state.lock().unwrap().lost_cancel = true;
    let j = queue(&f, "lost-start");
    f.app.run_model_once(&b).await.unwrap();
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "unknown");
    for i in 0..4 {
        f.app.run_model_once(&b).await.unwrap();
        f.clock.advance(chrono::Duration::seconds(2));
        let c = command(&f, &format!("cancel-{i}"));
        f.app
            .manage_model(
                &f.case,
                Manage {
                    command: c,
                    job_id: j.id.clone(),
                    action: ManageAction::Cancel,
                    evidence_refs: vec![],
                },
                &b,
            )
            .await
            .unwrap();
    }
    assert_eq!(b.state.lock().unwrap().starts, 1);
    assert_eq!(b.state.lock().unwrap().creates, 1);
    assert_eq!(b.state.lock().unwrap().cancels, 2);
    assert!(f.app.model_job(&f.case, &j.id).unwrap().quarantined);
    assert!(
        f.app
            .request_model(
                &f.case,
                Request {
                    command: command(&f, "again"),
                    policy_revision: 1,
                    action: Action::Qualification,
                    fixture_response: None
                }
            )
            .is_err()
    );
}
#[tokio::test]
async fn offline_deadline_cancels_queued_work_without_client_and_preflight_has_no_phantom_resource()
{
    let f = fixture();
    let b = Fake::default();
    let j = queue(&f, "expires");
    f.clock.advance(chrono::Duration::seconds(30));
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "cancelled");
    assert_eq!(b.state.lock().unwrap().creates, 0);
    let j = queue(&f, "preflight");
    b.state.lock().unwrap().preflight_fail = true;
    f.app.run_model_once(&b).await.unwrap();
    let j = f.app.model_job(&f.case, &j.id).unwrap();
    assert_eq!(j.state, "blocked");
    assert!(!j.create_dispatched);
    assert!(!j.quarantined);
}
#[test]
fn cancellation_fence_prevents_delayed_start_and_is_identity_bound() {
    let f = fixture();
    let j = queue(&f, "fence");
    let e = Envelope {
        schema_version: V1,
        operation: j.id.clone(),
        nonce: j.nonce.clone(),
        deadline: "2099-01-01T00:00:00Z".into(),
        profile: f.p.clone(),
        profile_digest: j.profile_digest.clone(),
        resource: Handle {
            operation: j.id.clone(),
            directory: f.p.cgroup_root.join(format!("watchhouse-{}", j.id)),
            inode: 1,
            boot_id: uuid::Uuid::nil().to_string(),
            limits: f.p.limits.clone(),
            profile_digest: j.profile_digest.clone(),
        },
        role: Role::Qualification,
        prompt: j.plan.prompt,
        output_schema: j.plan.output_schema,
        fixture_response: None,
        qualification: true,
    };
    check_start(&j.root, &e).unwrap();
    revoke(&j.root, &j.id, &j.nonce, &j.profile_digest).unwrap();
    revoke(&j.root, &j.id, &j.nonce, &j.profile_digest).unwrap();
    assert!(check_start(&j.root, &e).is_err());
    assert!(revoke(&j.root, &j.id, "foreign", &j.profile_digest).is_err());
    let mut wrong = e.clone();
    wrong.nonce = uuid::Uuid::new_v4().to_string();
    assert!(cancelled(&j.root, &wrong).is_err());
    let mut expired = e;
    expired.deadline = "2000-01-01T00:00:00Z".into();
    assert!(check_start(&j.root, &expired).is_err());
}
#[test]
fn fixed_adapter_flags_and_native_root_fail_closed() {
    let f = fixture();
    let args = super::client::args(&f.p, &f.root.path().join("schema.json"), None);
    assert!(args.windows(2).any(|a| a == ["--sandbox", "read-only"]));
    assert!(args.contains(&"--ignore-user-config".into()));
    assert!(args.contains(&"--ignore-rules".into()));
    assert!(
        !args
            .iter()
            .any(|a| a.contains("dangerously") || a.contains("yolo"))
    );
    assert!(super::cgroup::validate_root(f.root.path()).is_err());
    let mut invalid = f.p;
    invalid.limits.pids = 10000;
    assert!(invalid.validate().is_err());
}

#[tokio::test]
#[ignore = "requires an explicitly delegated native Linux cgroup and pinned Codex; no account/models"]
async fn native_codex_cgroup_qualification_and_coordinator_offline_watchdog() {
    native_text_client_drill(super::policy::Adapter::Codex, "WATCHHOUSE_MODEL_CODEX").await;
}
#[tokio::test]
#[ignore = "requires an explicitly delegated Linux cgroup and pinned Claude Code; no account/models"]
async fn native_claude_cgroup_qualification_and_coordinator_offline_watchdog() {
    native_text_client_drill(
        super::policy::Adapter::ClaudeCode,
        "WATCHHOUSE_MODEL_CLAUDE",
    )
    .await;
}
async fn native_text_client_drill(adapter: super::policy::Adapter, runtime_env: &str) {
    let mut f = fixture();
    f.p.adapter = adapter;
    f.clock
        .advance(Utc::now() - Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap());
    f.p.revision = 2;
    f.p.cgroup_root = std::fs::canonicalize(
        std::env::var_os("WATCHHOUSE_MODEL_CGROUP").expect("explicit delegated cgroup required"),
    )
    .unwrap();
    f.p.runtime.path = std::fs::canonicalize(
        std::env::var_os(runtime_env).expect("pinned native text client required"),
    )
    .unwrap();
    f.p.runtime.digest = crate::repair::snapshot::file_digest(&f.p.runtime.path).unwrap();
    f.p.initializer.path = std::fs::canonicalize(
        std::env::var_os("WATCHHOUSE_MODEL_INIT").expect("trusted initializer required"),
    )
    .unwrap();
    f.p.initializer.digest = crate::repair::snapshot::file_digest(&f.p.initializer.path).unwrap();
    f.p.limits.wall_seconds = 45;
    f.p.limits.pids = 128;
    f.p.limits.memory_bytes = 1024 * 1024 * 1024;
    f.p.limits.cpu_micros = 100000;
    f.p.limits.output_bytes = 262144;
    f.app
        .install_model_policy(f.p.clone(), "operator", "native no-account protocol drill")
        .unwrap();
    f.app
        .install_worker_routes(
            crate::routing::Policy {
                schema_version: V1,
                revision: 1,
                workers: vec![crate::routing::Worker {
                    id: "native-model".into(),
                    platform: "linux".into(),
                    architecture: std::env::consts::ARCH.into(),
                    paused: false,
                    targets: vec![crate::routing::Target {
                        kind: crate::routing::Kind::Model,
                        policy_revision: 2,
                    }],
                }],
            },
            "operator",
            "native routed qualification",
        )
        .unwrap();
    f.p.native_preflight().unwrap();
    f.clock
        .advance(Utc::now() - crate::domain::Clock::now(&f.clock));
    let j = f
        .app
        .request_model(
            &f.case,
            Request {
                command: command(&f, "native-transport"),
                policy_revision: 2,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(40);
    loop {
        f.app
            .run_model_on(&Native, Some("native-model"))
            .await
            .unwrap();
        let current = f.app.model_job(&f.case, &j.id).unwrap();
        eprintln!(
            "native fixture stage={} elapsed={:?} started={} result={} cancelled={}",
            current.state,
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            current.root.join("started.json").exists(),
            current.root.join("result.json").exists(),
            current.root.join("cancel.json").exists()
        );
        if let Ok(file) = std::fs::File::open(current.root.join("initializer-fixture.stderr")) {
            use std::io::Read;
            let mut diagnostic = String::new();
            file.take(4096).read_to_string(&mut diagnostic).unwrap();
            assert!(
                diagnostic.is_empty(),
                "trusted initializer fixture failed before result: {diagnostic}"
            );
        }
        if matches!(
            current.state.as_str(),
            "qualified" | "failed" | "cancelled" | "unknown" | "blocked"
        ) {
            if current.state != "qualified" {
                panic!(
                    "native text-client qualification failed closed: state={} reason={} output={:?}",
                    current.state, current.reason, current.output
                );
            }
            assert!(current.native);
            assert_eq!(current.populated_before_cleanup, Some(true));
            let output = current.output.unwrap();
            assert!(output.synthetic);
            assert_eq!(output.execution_tools_absent, Some(true));
            assert!(output.descendant_started);
            assert!(!output.client.parsed.unwrap().session.is_empty());
            let a = admission(&f.app.store.connect().unwrap(), &f.p, &f.app.now()).unwrap();
            assert!(!a.live_model_verified);
            assert!(
                !super::cgroup::populated(&f.p.cgroup_root, &current.resource.unwrap()).unwrap()
            );
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "native protocol result not observed before drill budget"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    // Exercise the factory stop against an actually populated native resource, without
    // waiting for a role result or treating cancellation as a successful qualification.
    f.clock
        .advance(Utc::now() - crate::domain::Clock::now(&f.clock));
    let stopping = f
        .app
        .request_model(
            &f.case,
            Request {
                command: command(&f, "native-factory-stop"),
                policy_revision: 2,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    f.app
        .run_model_on(&Native, Some("native-model"))
        .await
        .unwrap();
    f.app
        .run_model_on(&Native, Some("native-model"))
        .await
        .unwrap();
    let resource = f
        .app
        .model_job(&f.case, &stopping.id)
        .unwrap()
        .resource
        .unwrap();
    let wait = tokio::time::Instant::now();
    while !super::cgroup::populated(&f.p.cgroup_root, &resource).unwrap() {
        assert!(
            wait.elapsed() < std::time::Duration::from_secs(3),
            "factory-stop fixture never entered its cgroup"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // A restored coordinator on the same host must neither adopt this handle nor
    // revoke/kill the original deployment's live group, even after explicit resume.
    let backup = f.root.path().join("native-restore-backup");
    let b = f
        .app
        .create_backup(
            &backup,
            "operator",
            "Native synthetic-resource restore drill",
        )
        .unwrap();
    let destination = f.root.path().join("native-restored");
    let restored_record = crate::restore::restore(
        &backup,
        &destination,
        &b.manifest_digest,
        "operator",
        "Keep original native resource isolated",
    )
    .unwrap();
    assert_eq!(restored_record.uncertain_native_resources, 1);
    let restored = Application::new(
        Store::open(destination.join("watchhouse.db")).unwrap(),
        Arc::new(crate::domain::SystemClock),
    );
    assert_eq!(
        restored.model_job(&f.case, &j.id).unwrap().state,
        "qualified"
    );
    let controls = restored.factory_status().unwrap();
    restored
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "native-restored-resume".into(),
                actor: "operator".into(),
                reason: "Resume does not inherit original native authority".into(),
            },
            expected_version: controls.version,
            action: crate::factory::Action::Resume {
                pause_id: restored_record.pause_id,
            },
        })
        .unwrap();
    assert!(
        !restored
            .run_model_on(&Native, Some("native-model"))
            .await
            .unwrap()
    );
    assert!(
        super::cgroup::populated(&f.p.cgroup_root, &resource).unwrap(),
        "restored coordinator disturbed original native resource"
    );
    assert!(admission(&restored.store.connect().unwrap(), &f.p, &restored.now()).is_err());
    let stop = f
        .app
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "native-stop-control".into(),
                actor: "operator".into(),
                reason: "Native synthetic-resource factory stop drill".into(),
            },
            expected_version: 0,
            action: crate::factory::Action::Pause {
                scope: crate::factory::Scope::Case,
                target: Some(f.case.clone()),
                effect: crate::factory::Effect::Model,
                cancel_running: true,
            },
        })
        .unwrap();
    assert_eq!(stop.affected_jobs, 1);
    assert!(
        f.app
            .run_model_on(&Native, Some("native-model"))
            .await
            .unwrap()
    );
    assert!(!super::cgroup::populated(&f.p.cgroup_root, &resource).unwrap());
    assert_eq!(
        f.app.model_job(&f.case, &stopping.id).unwrap().state,
        "cancelled"
    );
    f.app
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "native-resume-control".into(),
                actor: "operator".into(),
                reason: "Resume admission without restoring the cancelled job".into(),
            },
            expected_version: 1,
            action: crate::factory::Action::Resume {
                pause_id: stop.pause_id,
            },
        })
        .unwrap();
    // A separate profile keeps the watchdog drill independent of successful client completion.
    f.clock
        .advance(Utc::now() - crate::domain::Clock::now(&f.clock));
    f.p.revision = 3;
    f.p.limits.wall_seconds = 2;
    f.app
        .install_model_policy(
            f.p.clone(),
            "operator",
            "coordinator-offline watchdog fixture",
        )
        .unwrap();
    let mut j = f
        .app
        .request_model(
            &f.case,
            Request {
                command: command(&f, "watchdog"),
                policy_revision: 3,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    let handle = super::cgroup::create(
        &f.p.cgroup_root,
        &j.id,
        &j.profile_digest,
        f.p.limits.clone(),
    )
    .unwrap();
    j.resource = Some(handle.clone());
    Native.start(j.clone(), f.p.clone()).await.unwrap();
    let start = tokio::time::Instant::now();
    while !super::cgroup::populated(&f.p.cgroup_root, &handle).unwrap() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(3),
            "initializer never joined reserved resource"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    assert!(
        !super::cgroup::populated(&f.p.cgroup_root, &handle).unwrap(),
        "watchdog must kill the whole resource without coordinator help"
    );
    let fence: CancellationFence = read_json(&j.root.join("cancel.json"), 2048).unwrap();
    assert_eq!(fence.operation, j.id);
    // A delayed launch cannot call the client after cancellation, even if it starts outside the group.
    let started_digest = std::fs::read(j.root.join("started.json")).unwrap();
    assert!(Native.start(j.clone(), f.p.clone()).await.is_err());
    assert_eq!(
        std::fs::read(j.root.join("started.json")).unwrap(),
        started_digest
    );
    assert!(!Native.cancel(j, f.p).await.unwrap());
}

#[tokio::test]
async fn staged_output_cannot_survive_deadline_or_owner_change() {
    for change_owner in [false, true] {
        let f = fixture();
        let b = Fake::default();
        let j = queue(&f, if change_owner { "owner" } else { "deadline" });
        for _ in 0..3 {
            f.app.run_model_once(&b).await.unwrap();
        }
        assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "finishing");
        if change_owner {
            f.app
                .store
                .connect()
                .unwrap()
                .execute("UPDATE cases SET owner='successor' WHERE id=?1", [&f.case])
                .unwrap();
        } else {
            f.clock.advance(chrono::Duration::seconds(30));
        }
        f.app.run_model_once(&b).await.unwrap();
        let done = f.app.model_job(&f.case, &j.id).unwrap();
        assert_eq!(done.state, "cancelled");
        assert!(!done.quarantined);
        assert!(done.evidence_digest.is_some());
        assert_eq!(b.state.lock().unwrap().starts, 1);
    }
}

#[tokio::test]
async fn lost_create_and_interrupted_reservations_never_repeat_creation() {
    let f = fixture();
    let b = Fake::default();
    b.state.lock().unwrap().lost_create = true;
    let j = queue(&f, "lost-create");
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "unknown");
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(b.state.lock().unwrap().creates, 1);
    assert_eq!(b.state.lock().unwrap().starts, 0);
    assert!(f.app.model_job("foreign-case", &j.id).is_err());
    let n = f.app.snapshot(&f.case).unwrap().events.len();
    let mut cmd = command(&f, "stale");
    cmd.expected_version -= 1;
    assert!(
        f.app
            .manage_model(
                &f.case,
                Manage {
                    command: cmd,
                    job_id: j.id,
                    action: ManageAction::ReportCleanup,
                    evidence_refs: vec![]
                },
                &b
            )
            .await
            .is_err()
    );
    assert_eq!(f.app.snapshot(&f.case).unwrap().events.len(), n);
}

#[cfg(unix)]
#[tokio::test]
async fn client_prompt_pipe_closes_before_waiting_for_output() {
    use std::process::Stdio;
    use tokio::{
        io::AsyncReadExt,
        process::Command,
        time::{Duration, timeout},
    };
    // A fixed OS test helper waits for EOF just like the CLI's stdin prompt reader.
    let mut child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let result = timeout(Duration::from_secs(2), async {
        super::client::send_prompt(input, "bounded synthetic stdin")
            .await
            .unwrap();
        let mut bytes = vec![];
        output.read_to_end(&mut bytes).await.unwrap();
        assert!(child.wait().await.unwrap().success());
        assert_eq!(bytes, b"bounded synthetic stdin");
    })
    .await;
    assert!(
        result.is_ok(),
        "model client must receive EOF before its response is awaited"
    );
}

#[test]
fn queued_model_control_directory_is_private_before_dispatch() {
    let f = fixture();
    let j = queue(&f, "private-control-directory");
    super::policy::private_dir(&j.root).unwrap();
}

#[test]
fn independent_profiles_preserve_legacy_digest_shape_and_do_not_inherit_admission() {
    let f = fixture();
    let raw = serde_json::to_value(&f.p).unwrap();
    assert!(raw.get("adapter").is_none());
    assert!(raw.get("profile").is_none());
    let mut second = f.p.clone();
    second.revision = 2;
    second.adapter = super::policy::Adapter::ClaudeCode;
    second.profile = "reviewer".into();
    f.app
        .install_model_policy(second.clone(), "operator", "fixture second profile")
        .unwrap();
    let status = f.app.model_profiles().unwrap();
    assert_eq!(status.iter().filter(|s| s.current).count(), 2);
    assert!(status.iter().all(|s| !s.native_admission_current));
    let mut changed = f.p.clone();
    changed.revision = 3;
    changed.requested_model = "new-explicit-model".into();
    f.app
        .install_model_policy(
            changed,
            "operator",
            "supersede only the default Codex profile",
        )
        .unwrap();
    let status = f.app.model_profiles().unwrap();
    assert!(
        !status
            .iter()
            .find(|s| s.policy.revision == 1)
            .unwrap()
            .current
    );
    assert!(
        status
            .iter()
            .find(|s| s.policy.revision == 2)
            .unwrap()
            .current
    );
    second.revision = 4;
    second.authority = super::policy::Authority::OpenaiAuthHome {
        path: f.p.jobs_root.clone(),
    };
    assert!(second.validate().is_err());
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn routed_model_work_retains_cleanup_owner_across_pause_and_restart() {
    use crate::routing::{Kind, Policy as Routes, Target, Worker};
    let f = fixture();
    let job = queue(&f, "route-job");
    let b = Fake::default();
    let mut routes = Routes {
        schema_version: V1,
        revision: 1,
        workers: vec![Worker {
            id: "model-a".into(),
            platform: "linux".into(),
            architecture: std::env::consts::ARCH.into(),
            paused: false,
            targets: vec![Target {
                kind: Kind::Model,
                policy_revision: 1,
            }],
        }],
    };
    let digest = f
        .app
        .install_worker_routes(routes.clone(), "operator", "controller fixture")
        .unwrap();
    assert_eq!(
        f.app
            .install_worker_routes(routes.clone(), "operator", "controller fixture retry")
            .unwrap(),
        digest
    );
    assert!(
        f.app
            .install_worker_routes(routes.clone(), "fake-agent", "controller fixture")
            .is_err()
    );
    assert!(!f.app.run_model_once(&b).await.unwrap());
    assert!(!f.app.run_model_on(&b, Some("wrong-worker")).await.unwrap());
    assert!(f.app.run_model_on(&b, Some("model-a")).await.unwrap());
    assert_eq!(b.state.lock().unwrap().creates, 1);
    routes.revision = 2;
    routes.workers[0].paused = true;
    f.app
        .install_worker_routes(routes, "operator", "paused fixture")
        .unwrap();
    let restarted = Application::new(
        Store::open(f.root.path().join("db")).unwrap(),
        Arc::new(f.clock.clone()),
    );
    assert!(!restarted.run_model_on(&b, Some("model-a")).await.unwrap());
    assert_eq!(b.state.lock().unwrap().starts, 0);
    f.clock.advance(chrono::Duration::seconds(21));
    restarted.supervise_models().unwrap();
    assert!(
        !restarted
            .run_model_on(&b, Some("wrong-worker"))
            .await
            .unwrap()
    );
    assert!(restarted.run_model_on(&b, Some("model-a")).await.unwrap());
    assert_eq!(b.state.lock().unwrap().cancels, 1);
    assert_eq!(b.state.lock().unwrap().creates, 1);
    assert_eq!(
        restarted.model_job(&f.case, &job.id).unwrap().state,
        "cancelled"
    );
    assert_eq!(
        restarted.worker_routes().unwrap().bindings[0]["worker_id"],
        "model-a"
    );
}

#[tokio::test]
async fn factory_stop_survives_resume_and_never_restarts_a_created_client() {
    let f = fixture();
    let j = queue(&f, "factory-stop-model");
    let b = Fake::default();
    f.app.run_model_once(&b).await.unwrap();
    let stopped = f
        .app
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "stop-model".into(),
                actor: "operator".into(),
                reason: "Synthetic stop before client start".into(),
            },
            expected_version: 0,
            action: crate::factory::Action::Pause {
                scope: crate::factory::Scope::Case,
                target: Some(f.case.clone()),
                effect: crate::factory::Effect::Model,
                cancel_running: true,
            },
        })
        .unwrap();
    assert_eq!(stopped.affected_jobs, 1);
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "unknown");
    f.app
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "resume-model".into(),
                actor: "operator".into(),
                reason: "Resume admission does not restore old client authority".into(),
            },
            expected_version: 1,
            action: crate::factory::Action::Resume {
                pause_id: stopped.pause_id,
            },
        })
        .unwrap();
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "cancelled");
    let s = b.state.lock().unwrap();
    assert_eq!(s.creates, 1);
    assert_eq!(s.starts, 0);
    assert_eq!(s.cancels, 1);
}

#[tokio::test]
async fn native_capacity_wait_does_not_starve_running_result_cleanup_or_extend_queue_deadline() {
    let f = fixture();
    let first = queue(&f, "capacity-first");
    let b = Fake::default();
    let mut limits = crate::capacity::Limits {
        revision: 2,
        ..Default::default()
    };
    limits.max_native_resources = 1;
    limits.max_native_per_repository = 1;
    limits.max_native_per_service = 1;
    f.app
        .install_factory_limits(limits, "operator", "capacity fixture")
        .unwrap();
    f.app.run_model_once(&b).await.unwrap();
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.capacity_status().unwrap().native_resources, 1);
    let other = f
        .app
        .create_case(FakeSource.manual_command("capacity-other-case"))
        .unwrap();
    let second = f
        .app
        .request_model(
            &other.id,
            Request {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "capacity-second".into(),
                        actor: "operator".into(),
                        reason: "Second bounded resource fixture".into(),
                    },
                    expected_version: other.version,
                },
                policy_revision: 1,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    let original_deadline = second.deadline.clone();
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(b.state.lock().unwrap().creates, 1);
    assert_eq!(
        f.app.model_job(&other.id, &second.id).unwrap().state,
        "queued"
    );
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert!(matches!(
        f.app.model_job(&f.case, &first.id).unwrap().state.as_str(),
        "simulated" | "failed"
    ));
    assert_eq!(f.app.capacity_status().unwrap().native_resources, 0);
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(b.state.lock().unwrap().creates, 2);
    assert_eq!(
        f.app.model_job(&other.id, &second.id).unwrap().deadline,
        original_deadline
    );
}

#[tokio::test]
async fn queued_effect_limit_preserves_exact_model_receipt_and_existing_transition() {
    let f = fixture();
    let mut limits = crate::capacity::Limits {
        revision: 2,
        ..Default::default()
    };
    limits.max_queued_effects = 1;
    f.app
        .install_factory_limits(limits, "operator", "effect quota fixture")
        .unwrap();
    let cmd = Request {
        command: command(&f, "one-effect"),
        policy_revision: 1,
        action: Action::Qualification,
        fixture_response: None,
    };
    let j = f.app.request_model(&f.case, cmd.clone()).unwrap();
    let other = f
        .app
        .create_case(FakeSource.manual_command("effect-capacity-other"))
        .unwrap();
    let second = Request {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "two-effects".into(),
                actor: "operator".into(),
                reason: "Queued effect bound fixture".into(),
            },
            expected_version: other.version,
        },
        policy_revision: 1,
        action: Action::Qualification,
        fixture_response: None,
    };
    assert!(matches!(
        f.app.request_model(&other.id, second.clone()),
        Err(crate::error::Error::Backpressure(_))
    ));
    assert_eq!(f.app.request_model(&f.case, cmd).unwrap().id, j.id);
    assert_eq!(std::fs::read_dir(&f.p.jobs_root).unwrap().count(), 1);
    let b = Fake::default();
    f.app.run_model_once(&b).await.unwrap();
    f.app.request_model(&other.id, second).unwrap();
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(b.state.lock().unwrap().starts, 1);
}

#[tokio::test]
async fn native_reservations_limit_same_repository_or_service_without_blocking_other_scopes() {
    let f = fixture();
    let mut cases = vec![f.case.clone()];
    for n in 1..4 {
        cases.push(
            f.app
                .create_case(FakeSource.manual_command(&format!("scope-{n}")))
                .unwrap()
                .id,
        );
    }
    let db = f.app.store.connect().unwrap();
    for (n, (repo, service)) in [
        ("example/a", "service-a"),
        ("example/a", "service-b"),
        ("example/b", "service-a"),
        ("example/c", "service-c"),
    ]
    .into_iter()
    .enumerate()
    {
        // Explicit synthetic source projection, not a provider or native proof.
        let source = crate::sentry::SourceObservation {
            source_key: format!("fixture:capacity/{n}"),
            signal: crate::sentry::Signal {
                issue_id: n.to_string(),
                project_id: "fixture".into(),
                title: "Capacity mapping fixture".into(),
                status: "unresolved".into(),
                last_seen: f.app.now(),
                count: None,
            },
            authoritative: false,
            observed_at: f.app.now(),
            configuration_revision: 1,
            service: service.into(),
            environment: "fixture".into(),
            repository: repo.into(),
            release: None,
            deployed_revision: None,
            exceptions: vec![],
            diagnostic: None,
            uncertainty: "Projection fixture only".into(),
        };
        let raw = serde_json::to_string(&source).unwrap();
        let digest = crate::store::digest(&raw);
        db.execute(
            "INSERT INTO evidence VALUES (?1,'application/json',?2,'capacity-fixture','now')",
            rusqlite::params![digest, raw],
        )
        .unwrap();
        db.execute(
            "INSERT INTO source_cases VALUES (?1,?2,?3,?4)",
            rusqlite::params![source.source_key, cases[n], raw, digest],
        )
        .unwrap();
    }
    let mut limits = crate::capacity::Limits {
        revision: 2,
        ..Default::default()
    };
    limits.max_native_resources = 4;
    limits.max_native_per_repository = 1;
    limits.max_native_per_service = 1;
    f.app
        .install_factory_limits(limits, "operator", "scoped native fixture")
        .unwrap();
    let mut jobs = vec![];
    for (n, case) in cases.iter().enumerate() {
        jobs.push(
            f.app
                .request_model(
                    case,
                    Request {
                        command: OperatorCommand {
                            meta: CommandMeta {
                                schema_version: V1,
                                idempotency_key: format!("scope-job-{n}"),
                                actor: "operator".into(),
                                reason: "Scoped capacity fixture".into(),
                            },
                            expected_version: f.app.snapshot(case).unwrap().case.version,
                        },
                        policy_revision: 1,
                        action: Action::Qualification,
                        fixture_response: None,
                    },
                )
                .unwrap(),
        );
    }
    let b = Fake::default();
    for _ in 0..3 {
        assert!(f.app.run_model_once(&b).await.unwrap());
    }
    assert_eq!(b.state.lock().unwrap().creates, 2);
    assert_eq!(
        f.app.model_job(&cases[1], &jobs[1].id).unwrap().state,
        "queued"
    );
    assert_eq!(
        f.app.model_job(&cases[2], &jobs[2].id).unwrap().state,
        "queued"
    );
    assert_eq!(
        f.app.model_job(&cases[3], &jobs[3].id).unwrap().state,
        "created"
    );
    db.execute("UPDATE source_cases SET record=json_set(record,'$.repository','example/remapped','$.service','remapped') WHERE case_id=?1",[&cases[0]]).unwrap();
    let scope: (String, String) = db
        .query_row(
            "SELECT repository,service FROM factory_scoped_resources WHERE id=?1",
            [&jobs[0].id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(scope, ("example/a".into(), "service-a".into()));
    assert!(
        f.app
            .capacity_status()
            .unwrap()
            .scoped_usage
            .iter()
            .any(|v| v["scope"] == "example/a")
    );
}

#[tokio::test]
async fn evidence_capacity_cancels_model_resource_without_accepting_or_replaying_output() {
    let f = fixture();
    let j = queue(&f, "quota-client");
    let b = Fake::default();
    let p = crate::capacity::Limits {
        revision: 2,
        max_evidence_bytes: 1024,
        ..Default::default()
    };
    f.app
        .install_factory_limits(p, "operator", "model quota fixture")
        .unwrap();
    f.app
        .store
        .connect()
        .unwrap()
        .execute(
            "INSERT INTO evidence VALUES ('full-model-capacity','text/plain',?1,'fixture','now')",
            ["x".repeat(1024)],
        )
        .unwrap();
    for _ in 0..3 {
        f.app.run_model_once(&b).await.unwrap();
    }
    let current = f.app.model_job(&f.case, &j.id).unwrap();
    assert_eq!(current.state, "unknown");
    assert!(current.output.is_none());
    assert!(current.cancel_requested);
    assert_eq!(f.app.capacity_status().unwrap().native_resources, 1);
    f.app.run_model_once(&b).await.unwrap();
    assert_eq!(f.app.model_job(&f.case, &j.id).unwrap().state, "cancelled");
    assert_eq!(f.app.capacity_status().unwrap().native_resources, 0);
    let s = b.state.lock().unwrap();
    assert_eq!(s.creates, 1);
    assert_eq!(s.starts, 1);
    assert_eq!(s.cancels, 1);
}

#[tokio::test]
async fn corrupt_capacity_policy_denies_new_resource_binding_but_keeps_old_cleanup() {
    let f = fixture();
    let first = queue(&f, "capacity-corruption-existing");
    let b = Fake::default();
    f.app.run_model_once(&b).await.unwrap();
    let other = f
        .app
        .create_case(FakeSource.manual_command("capacity-corruption-other"))
        .unwrap();
    let second = f
        .app
        .request_model(
            &other.id,
            Request {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "capacity-corruption-queued".into(),
                        actor: "operator".into(),
                        reason: "Corrupt policy must not grant a resource".into(),
                    },
                    expected_version: other.version,
                },
                policy_revision: 1,
                action: Action::Qualification,
                fixture_response: None,
            },
        )
        .unwrap();
    f.app
        .store
        .connect()
        .unwrap()
        .execute_batch(
            "DROP TRIGGER factory_limits_no_update; UPDATE factory_limits SET digest='corrupt';",
        )
        .unwrap();
    f.app
        .control_factory(crate::factory::Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "capacity-corruption-stop".into(),
                actor: "operator".into(),
                reason: "Cleanup remains available with corrupt capacity policy".into(),
            },
            expected_version: 0,
            action: crate::factory::Action::Pause {
                scope: crate::factory::Scope::Case,
                target: Some(f.case.clone()),
                effect: crate::factory::Effect::Model,
                cancel_running: true,
            },
        })
        .unwrap();
    assert!(f.app.run_model_once(&b).await.unwrap());
    assert_eq!(
        f.app.model_job(&f.case, &first.id).unwrap().state,
        "cancelled"
    );
    assert!(f.app.run_model_once(&b).await.is_err());
    assert_eq!(b.state.lock().unwrap().creates, 1);
    assert_eq!(
        f.app.model_job(&other.id, &second.id).unwrap().state,
        "queued"
    );
    let bound: i64 = f
        .app
        .store
        .connect()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM worker_bindings WHERE job_id=?1",
            [&second.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bound, 0);
}
