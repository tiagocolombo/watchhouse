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
    let mut f = fixture();
    f.clock
        .advance(Utc::now() - Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap());
    f.p.revision = 2;
    f.p.cgroup_root = std::fs::canonicalize(
        std::env::var_os("WATCHHOUSE_MODEL_CGROUP").expect("explicit delegated cgroup required"),
    )
    .unwrap();
    f.p.runtime.path = std::fs::canonicalize(
        std::env::var_os("WATCHHOUSE_MODEL_CODEX").expect("pinned native Codex required"),
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
        f.app.run_model_once(&Native).await.unwrap();
        let current = f.app.model_job(&f.case, &j.id).unwrap();
        eprintln!(
            "native fixture stage={} elapsed={:?} started={} result={} cancelled={}",
            current.state,
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            current.root.join("started.json").exists(),
            current.root.join("result.json").exists(),
            current.root.join("cancel.json").exists()
        );
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
