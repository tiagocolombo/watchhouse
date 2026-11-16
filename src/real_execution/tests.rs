//! Controller fixtures prove state handling, not physical containment or live model access.
use super::{docker::*, protocol::*};
use crate::{contracts::*, store};
use std::path::PathBuf;
fn sample_policy(root: &std::path::Path) -> ContainerPolicy {
    ContainerPolicy {
        schema_version: V1,
        revision: 1,
        docker_path: PathBuf::from("/usr/bin/true"),
        docker_digest: store::digest("client"),
        socket: PathBuf::from("/var/run/docker.sock"),
        image: store::digest("image"),
        runner_digest: store::digest("runner"),
        staging_root: root.to_owned(),
        program: "/watchhouse-worker-init".into(),
        args: vec!["fixture-regression".into()],
        memory_bytes: 256 * 1024 * 1024,
        pids: 32,
        cpu_micros: 50000,
        work_bytes: 16 * 1024 * 1024,
        wall_seconds: 10,
        output_bytes: 4096,
        proof_seconds: 300,
        dispatch_grace_seconds: 90,
        cleanup_attempts: 3,
        cleanup_retry_seconds: 2,
    }
}
#[test]
fn strict_envelope_rejects_escape_collision_and_resource_expansion() {
    let d = tempfile::tempdir().unwrap();
    let p = sample_policy(d.path());
    p.validate().unwrap();
    let j = p.job(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        vec![InputFile {
            path: "answer.txt".into(),
            digest: store::digest("5\n"),
            bytes: 2,
            executable: false,
        }],
    );
    j.validate().unwrap();
    for path in [
        "../escape",
        "/absolute",
        ".git/config",
        "a//b",
        "dir\\file",
        "NUL.txt",
        "a/COM1",
        "trailing.",
        "x y",
    ] {
        let mut bad = j.clone();
        bad.files[0].path = path.into();
        assert!(bad.validate().is_err(), "{path}");
    }
    let mut bad = j.clone();
    let mut other = bad.files[0].clone();
    other.path = "ANSWER.TXT".into();
    bad.files.push(other);
    assert!(bad.validate().is_err());
    let mut bad = j.clone();
    bad.work_bytes = bad.memory_bytes;
    assert!(bad.validate().is_err());
    let mut bad = j.clone();
    bad.program = "/work/test".into();
    assert!(bad.validate().is_err());
    let mut v = serde_json::to_value(&j).unwrap();
    v["credentials"] = serde_json::json!("forbidden");
    assert!(serde_json::from_value::<WorkerJob>(v).is_err());
}
#[test]
fn fixed_creation_never_inherits_credentials_network_or_host_write_authority() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("input");
    let control = d.path().join("policy");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&control).unwrap();
    let p = sample_policy(&d.path().canonicalize().unwrap());
    let j = p.job(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        vec![InputFile {
            path: "a".into(),
            digest: store::digest("x"),
            bytes: 1,
            executable: false,
        }],
    );
    let docker = Docker::new(p).unwrap();
    let args = docker
        .create_args(
            &j,
            &input.canonicalize().unwrap(),
            &control.canonicalize().unwrap(),
            false,
        )
        .unwrap();
    for arg in [
        "--network=none",
        "--read-only",
        "--cap-drop=ALL",
        "--user=0:0",
        "--security-opt=no-new-privileges:true",
        "--pull=never",
    ] {
        assert!(args.contains(&arg.to_string()), "{arg}");
    }
    assert_eq!(
        args.iter().filter(|a| a.starts_with("type=bind,")).count(),
        2
    );
    assert!(
        args.iter()
            .filter(|a| a.starts_with("type=bind,"))
            .all(|a| a.ends_with(",readonly"))
    );
    assert!(!args.iter().any(|a| a.contains("docker.sock")
        || a.contains("privileged")
        || a.contains("network=host")));
    let mut changed = j.clone();
    changed.wall_seconds += 1;
    assert!(
        docker
            .create_args(&changed, &input, &control, false)
            .is_err()
    );
    assert!(docker.create_args(&j, &input, &input, false).is_err());
}
#[test]
fn inspection_rejects_privilege_mount_image_and_limit_drift() {
    use serde_json::json;
    let d = tempfile::tempdir().unwrap();
    let p = sample_policy(d.path());
    let j = p.job(
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
        vec![InputFile {
            path: "a".into(),
            digest: store::digest("x"),
            bytes: 1,
            executable: false,
        }],
    );
    let input = d.path().join("input");
    let control = d.path().join("policy");
    let v = json!({"Id":"a".repeat(64),"Name":format!("/watchhouse-{}",j.operation),"Image":p.image,"Config":{"Image":p.image,"User":"0:0","WorkingDir":"/work","Entrypoint":["/watchhouse-worker-init"],"Cmd":["run"],"Labels":{"watchhouse.operation":j.operation,"watchhouse.profile":j.profile_digest},"Env":["HOME=/tmp","TMPDIR=/tmp","PATH=/usr/local/bin:/usr/bin:/bin"]},"HostConfig":{"Privileged":false,"ReadonlyRootfs":true,"NetworkMode":"none","IpcMode":"private","CgroupnsMode":"private","PidMode":"","Memory":j.memory_bytes,"MemorySwap":j.memory_bytes,"PidsLimit":j.pids,"CpuPeriod":100000,"CpuQuota":j.cpu_micros,"CapAdd":["SETUID","SETGID"],"CapDrop":["ALL"],"SecurityOpt":["no-new-privileges:true"],"Tmpfs":{"/work":format!("rw,nosuid,nodev,size={},mode=1777",j.work_bytes),"/tmp":"rw,nosuid,nodev,noexec,size=4194304,mode=1777"}},"Mounts":[{"Type":"bind","Destination":"/input","Source":input,"RW":false},{"Type":"bind","Destination":"/policy","Source":control,"RW":false}],"State":{"Status":"created","Running":false,"ExitCode":0,"OOMKilled":false}});
    validate_inspect(&v, &j, &input, &control, false, &p.image).unwrap();
    for (pointer, value) in [
        ("/HostConfig/Privileged", json!(true)),
        ("/HostConfig/NetworkMode", json!("host")),
        ("/HostConfig/PidMode", json!("host")),
        ("/HostConfig/Memory", json!(0)),
        ("/HostConfig/PidsLimit", json!(-1)),
        ("/HostConfig/CapAdd", json!(["SYS_ADMIN"])),
        ("/Config/Env", json!(["SECRET=host"])),
        ("/Config/Image", json!("changed")),
        ("/Mounts/0/RW", json!(true)),
        ("/Mounts/0/Source", json!("/")),
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            validate_inspect(&bad, &j, &input, &control, false, &p.image).is_err(),
            "{pointer}"
        );
    }
}
#[cfg(target_os = "linux")]
mod controller {
    use super::super::{pipeline::*, worker::*};
    use super::*;
    use crate::{
        application::Application, domain::ManualClock, error::Error, intake::FakeSource,
        policy::Policy, repair::*, store::Store,
    };
    use chrono::{Duration, TimeZone, Utc};
    use std::{
        collections::BTreeMap,
        fs,
        process::Command,
        sync::{Arc, Mutex},
    };
    struct Fixture {
        _root: tempfile::TempDir,
        app: Application,
        clock: ManualClock,
        case: String,
        policy: ContainerPolicy,
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
        governing.name = "explicit-fixture-v2".into();
        app.install_policy(governing, "operator", "controller-fixture")
            .unwrap();
        let case = app
            .create_case(FakeSource.manual_command("native-case"))
            .unwrap()
            .id;
        let mut policy = sample_policy(root.path());
        policy.docker_path = fs::canonicalize("/usr/bin/true").unwrap();
        policy.docker_digest = crate::repair::snapshot::file_digest(&policy.docker_path).unwrap();
        app.install_container_policy(policy.clone(), "operator")
            .unwrap();
        Fixture {
            _root: root,
            app,
            clock,
            case,
            policy,
        }
    }
    fn command(f: &Fixture, key: &str, actor: &str) -> OperatorCommand {
        OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: key.into(),
                actor: actor.into(),
                reason: "Controlled synthetic execution test".into(),
            },
            expected_version: f.app.snapshot(&f.case).unwrap().case.version,
        }
    }
    fn qualify(f: &Fixture, key: &str) -> ContainerJob {
        f.app
            .qualify_container(
                &f.case,
                Qualify {
                    command: command(f, key, "operator"),
                    policy_revision: 1,
                },
            )
            .unwrap()
    }
    #[derive(Default)]
    struct State {
        created: usize,
        started: usize,
        cancelled: usize,
        lost_create: bool,
        lost_start: bool,
        lost_cancel: bool,
        malformed: bool,
        exited: bool,
    }
    #[derive(Default)]
    struct FixtureBackend {
        state: Arc<Mutex<State>>,
        native: bool,
    }
    fn identity() -> EngineIdentity {
        EngineIdentity {
            id: "controller-fixture".into(),
            version: "fixture-v1".into(),
            architecture: "amd64".into(),
            cgroup_version: "2".into(),
            kernel_version: "fixture-kernel".into(),
            security_digest: store::digest("fixture-seccomp"),
            host_boot_id: "fixture-boot".into(),
        }
    }
    fn output(r: &Request) -> WorkerOutput {
        let code = if r.probe
            || fs::read_to_string(r.input.join("answer.txt"))
                .ok()
                .as_deref()
                == Some("4\n")
        {
            0
        } else {
            1
        };
        WorkerOutput {
            schema_version: V1,
            operation: r.job.operation.clone(),
            nonce: r.job.nonce.clone(),
            profile_digest: r.job.profile_digest.clone(),
            exit_code: Some(code),
            failure_code: None,
            stdout: "controller fixture only".into(),
            stderr: String::new(),
            input_digest: r.job.input_digest().unwrap(),
            checks: if r.probe {
                CHECKS.iter().map(|k| (k.to_string(), true)).collect()
            } else {
                BTreeMap::new()
            },
        }
    }
    impl Backend for FixtureBackend {
        fn native(&self) -> bool {
            self.native
        }
        fn engine(&self, _: ContainerPolicy) -> Effect<EngineIdentity> {
            Box::pin(async { Ok(identity()) })
        }
        fn create(&self, _: Request) -> Effect<String> {
            let s = self.state.clone();
            Box::pin(async move {
                let mut s = s.lock().unwrap();
                s.created += 1;
                if s.lost_create {
                    Err(Error::Invalid("lost creation acknowledgment".into()))
                } else {
                    Ok("a".repeat(64))
                }
            })
        }
        fn inspect(&self, _: Request) -> Effect<Observation> {
            let exited = self.state.lock().unwrap().exited;
            Box::pin(async move {
                Ok(Observation {
                    id: "a".repeat(64),
                    status: if exited { "exited" } else { "created" }.into(),
                    running: false,
                    exit_code: 0,
                    oom_killed: false,
                })
            })
        }
        fn start(&self, r: Request) -> Effect<WorkerOutput> {
            let s = self.state.clone();
            Box::pin(async move {
                let mut s = s.lock().unwrap();
                s.started += 1;
                if s.lost_start {
                    return Err(Error::Invalid("lost start acknowledgment".into()));
                }
                let mut o = output(&r);
                if s.malformed {
                    o.nonce = "wrong".into();
                }
                Ok(o)
            })
        }
        fn logs(&self, r: Request) -> Effect<WorkerOutput> {
            Box::pin(async move { Ok(output(&r)) })
        }
        fn cancel(&self, _: Request) -> Effect<()> {
            let s = self.state.clone();
            Box::pin(async move {
                let mut s = s.lock().unwrap();
                s.cancelled += 1;
                if s.lost_cancel {
                    Err(Error::Invalid("lost removal acknowledgment".into()))
                } else {
                    Ok(())
                }
            })
        }
    }
    async fn dispatch(f: &Fixture, b: &impl Backend) {
        assert!(f.app.run_container_once(b).await.unwrap());
        assert!(f.app.run_container_once(b).await.unwrap());
    }
    fn manage(f: &Fixture, id: &str, key: &str, action: ManageAction) -> Manage {
        Manage {
            command: command(f, key, "operator"),
            job_id: id.into(),
            action,
            evidence_refs: vec![],
        }
    }
    fn observe(f: &Fixture, key: &str, actor: &str) -> String {
        f.app
            .record_evidence(
                &f.case,
                RecordEvidence {
                    schema_version: V1,
                    idempotency_key: key.into(),
                    actor: actor.into(),
                    reason: "Inspect exact frozen fixture and regression".into(),
                    expected_version: f.app.snapshot(&f.case).unwrap().case.version,
                    scope: "controlled fixture".into(),
                    statement: "Observed exact base/candidate fixture and scoped resources".into(),
                    collection_method: "Independent fixture inspection".into(),
                    observed_at: None,
                    uncertainty: "Synthetic controller evidence only".into(),
                },
            )
            .unwrap()
            .evidence
            .digest
    }
    async fn prepare(f: &Fixture) -> String {
        let repo = f._root.path().join("source");
        fs::create_dir(&repo).unwrap();
        fn git(p: &std::path::Path, args: &[&str]) -> String {
            let o = Command::new("git")
                .arg("-C")
                .arg(p)
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).trim().into()
        }
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("answer.txt"), "5\n").unwrap();
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "Synthetic regression",
            ],
        );
        let root = f._root.path().join("bases");
        fs::create_dir(&root).unwrap();
        let runtime = fs::canonicalize("/usr/bin/git").unwrap();
        f.app
            .install_repair_source(
                RepairSourceConfig {
                    schema_version: V1,
                    revision: 1,
                    repository: "fixture/calculator".into(),
                    source_path: repo.clone(),
                    base_commit: git(&repo, &["rev-parse", "HEAD"]),
                    workspace_root: root,
                    runtime_digest: crate::repair::snapshot::file_digest(&runtime).unwrap(),
                    runtime_path: runtime,
                    max_files: 20,
                    max_blob_bytes: 65536,
                    max_total_bytes: 1048576,
                    preparation_seconds: 30,
                    probe_seconds: 5,
                },
                "operator",
            )
            .await
            .unwrap();
        let evidence = observe(f, "repair-evidence", "operator");
        f.app
            .request_repair(
                &f.case,
                RequestRepair {
                    command: command(f, "repair-request", "operator"),
                    source_revision: 1,
                    objective: "Correct fixture arithmetic".into(),
                    expected_behavior: "answer must equal four".into(),
                    regression_plan: "same fixed regression fails on base and passes on candidate"
                        .into(),
                    evidence_refs: vec![evidence],
                },
            )
            .unwrap()
            .id
    }
    fn freeze(f: &Fixture, repair: &str, key: &str) -> Candidate {
        f.app
            .freeze_candidate(
                &f.case,
                FreezeCandidate {
                    command: command(f, key, "operator"),
                    repair_id: repair.into(),
                    policy_revision: 1,
                    changes: vec![TextChange {
                        path: "answer.txt".into(),
                        before_digest: Some(store::digest("5\n")),
                        after: Some("4\n".into()),
                    }],
                },
            )
            .unwrap()
    }
    #[tokio::test]
    async fn synthetic_backend_never_admits_and_retries_never_repeat_effects() {
        let f = fixture();
        let q = qualify(&f, "qualification");
        let b = FixtureBackend::default();
        dispatch(&f, &b).await;
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "simulated"
        );
        assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
        assert!(!f.app.run_container_once(&b).await.unwrap());
        assert_eq!(b.state.lock().unwrap().created, 1);
        assert_eq!(b.state.lock().unwrap().started, 1);
    }
    #[tokio::test]
    async fn lost_create_reconciles_existing_handle_without_duplicate_create() {
        let f = fixture();
        let q = qualify(&f, "qualification");
        let b = FixtureBackend::default();
        b.state.lock().unwrap().lost_create = true;
        assert!(f.app.run_container_once(&b).await.unwrap());
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "unknown"
        );
        assert!(!f.app.run_container_once(&b).await.unwrap());
        let cmd = manage(&f, &q.id, "inspect", ManageAction::Reconcile);
        let r = f
            .app
            .manage_container(&f.case, cmd.clone(), &b)
            .await
            .unwrap();
        assert_eq!(r.state, "created");
        assert_eq!(
            f.app
                .manage_container(&f.case, cmd, &b)
                .await
                .unwrap()
                .state,
            "created"
        );
        assert!(f.app.run_container_once(&b).await.unwrap());
        assert_eq!(b.state.lock().unwrap().created, 1);
        assert_eq!(b.state.lock().unwrap().started, 1);
    }
    #[tokio::test]
    async fn lost_start_deadline_cleanup_and_late_output_cannot_reopen_authority() {
        let f = fixture();
        let q = qualify(&f, "qualification");
        let b = FixtureBackend::default();
        b.state.lock().unwrap().lost_start = true;
        dispatch(&f, &b).await;
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "unknown"
        );
        assert!(!f.app.run_container_once(&b).await.unwrap());
        f.clock.advance(Duration::seconds(101));
        f.app.supervise_team().unwrap();
        let cur = f.app.container_job(&f.case, &q.id).unwrap();
        assert!(cur.quarantined);
        assert!(cur.cancel_requested);
        assert!(f.app.run_container_once(&b).await.unwrap());
        let cur = f.app.container_job(&f.case, &q.id).unwrap();
        assert_eq!(cur.state, "cancelled");
        assert!(!cur.quarantined);
        assert_eq!(b.state.lock().unwrap().started, 1);
        let r = super::super::worker::request(f.policy.clone(), &cur);
        f.app.finish_container(&cur, output(&r), true).unwrap();
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "cancelled"
        );
        assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
    }
    #[tokio::test]
    async fn malformed_output_is_quarantined_and_cleanup_has_bounded_attempts() {
        let f = fixture();
        let q = qualify(&f, "qualification");
        let b = FixtureBackend::default();
        b.state.lock().unwrap().malformed = true;
        dispatch(&f, &b).await;
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "unknown"
        );
        b.state.lock().unwrap().lost_cancel = true;
        f.clock.advance(Duration::seconds(101));
        f.app.supervise_team().unwrap();
        for _ in 0..3 {
            assert!(f.app.run_container_once(&b).await.unwrap());
            f.clock.advance(Duration::seconds(3));
        }
        assert!(!f.app.run_container_once(&b).await.unwrap());
        assert_eq!(b.state.lock().unwrap().cancelled, 3);
        assert!(
            f.app
                .manage_container(
                    &f.case,
                    manage(&f, &q.id, "cannot-reset-cleanup", ManageAction::Reconcile),
                    &b
                )
                .await
                .is_err()
        );
        assert_eq!(b.state.lock().unwrap().cancelled, 3);
        let mut cmd = manage(&f, &q.id, "reported-cleanup", ManageAction::ReportCleanup);
        assert!(
            f.app
                .manage_container(&f.case, cmd.clone(), &b)
                .await
                .is_err()
        );
        cmd.evidence_refs = vec![observe(&f, "cleanup-inspection", "operator")];
        cmd.command = command(&f, "reported-cleanup", "operator");
        assert_eq!(
            f.app
                .manage_container(&f.case, cmd, &b)
                .await
                .unwrap()
                .state,
            "reported_cleaned"
        );
        assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
    }
    #[tokio::test]
    async fn policy_and_reservations_are_immutable_and_rollback_atomically() {
        let f = fixture();
        let mut bad = f.policy.clone();
        bad.wall_seconds = 11;
        assert!(f.app.install_container_policy(bad, "operator").is_err());
        assert!(
            f.app
                .install_container_policy(f.policy.clone(), "fake-engineer")
                .is_err()
        );
        let db = f.app.store.connect().unwrap();
        db.execute_batch("CREATE TRIGGER fail_native BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='execution.container_changed' BEGIN SELECT RAISE(ABORT,'injected native reservation failure'); END;").unwrap();
        assert!(
            f.app
                .qualify_container(
                    &f.case,
                    Qualify {
                        command: command(&f, "rollback", "operator"),
                        policy_revision: 1
                    }
                )
                .is_err()
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM container_jobs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        db.execute_batch("DROP TRIGGER fail_native").unwrap();
        qualify(&f, "rollback");
    }
    #[tokio::test]
    async fn admission_requires_explicit_case_policy_latest_generation_and_intact_evidence() {
        let f = fixture();
        let b = FixtureBackend {
            native: true,
            ..Default::default()
        };
        let q = qualify(&f, "first-qualification");
        dispatch(&f, &b).await;
        let a = admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).unwrap();
        let db = f.app.store.connect().unwrap();
        db.execute(
            "UPDATE evidence SET content='corrupt' WHERE digest=?1",
            [&a.evidence_digest],
        )
        .unwrap();
        assert!(admission(&db, &f.policy, &f.app.now()).is_err());
        drop(db);
        let q2 = qualify(&f, "new-qualification");
        assert_ne!(q.id, q2.id);
        assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
        dispatch(&f, &b).await;
        admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).unwrap();
        f.clock.advance(Duration::seconds(301));
        assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
        let root = tempfile::tempdir().unwrap();
        let app = Application::new(
            Store::open(root.path().join("db")).unwrap(),
            Arc::new(f.clock.clone()),
        );
        let case = app
            .create_case(FakeSource.manual_command("bootstrap-native"))
            .unwrap();
        assert!(trusted_case_policy(&app.store.connect().unwrap(), &case.id).is_err());
    }
    // This backend only exercises controller gates under injected native-shaped observations.
    // Actual physical acceptance is the separate ignored native Docker test below.
    #[tokio::test]
    async fn exact_regressions_review_independence_staleness_and_shared_budget() {
        let f = fixture();
        let b = FixtureBackend {
            native: true,
            ..Default::default()
        };
        qualify(&f, "qualification");
        dispatch(&f, &b).await;
        admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).unwrap();
        let repair = prepare(&f).await;
        let c = freeze(&f, &repair, "candidate");
        assert!(
            f.app
                .request_candidate_check(
                    &f.case,
                    RequestCheck {
                        command: command(&f, "out-of-order", "operator"),
                        candidate_id: c.id.clone(),
                        side: Side::Candidate
                    }
                )
                .is_err()
        );
        let base = f
            .app
            .request_candidate_check(
                &f.case,
                RequestCheck {
                    command: command(&f, "base", "operator"),
                    candidate_id: c.id.clone(),
                    side: Side::Base,
                },
            )
            .unwrap();
        dispatch(&f, &b).await;
        let candidate = f
            .app
            .request_candidate_check(
                &f.case,
                RequestCheck {
                    command: command(&f, "candidate-check", "operator"),
                    candidate_id: c.id.clone(),
                    side: Side::Candidate,
                },
            )
            .unwrap();
        dispatch(&f, &b).await;
        let mut cmd = ReviewCandidate {
            command: command(&f, "self-review", "operator"),
            candidate_id: c.id.clone(),
            base_check: base.id.clone(),
            candidate_check: candidate.id.clone(),
            approve: true,
            findings: "Exact controlled regression observed".into(),
            evidence_refs: vec![],
        };
        assert!(f.app.review_candidate(&f.case, cmd.clone()).is_err());
        cmd.command = command(&f, "fresh-review", "reviewer");
        assert!(f.app.review_candidate(&f.case, cmd.clone()).is_err());
        cmd.evidence_refs = vec![observe(&f, "review-observation", "reviewer")];
        cmd.command = command(&f, "fresh-review", "reviewer");
        let r = f.app.review_candidate(&f.case, cmd.clone()).unwrap();
        assert!(r.current);
        assert!(f.app.review_candidate(&f.case, cmd).unwrap().current);
        let c2 = freeze(&f, &repair, "candidate-revision");
        assert_eq!(c2.deadline, c.deadline);
        assert!(!f.app.snapshot(&f.case).unwrap().repair_progress.reviews[0].current);
        f.clock.advance(Duration::seconds(301));
        assert!(
            f.app
                .request_candidate_check(
                    &f.case,
                    RequestCheck {
                        command: command(&f, "expired-check", "operator"),
                        candidate_id: c2.id,
                        side: Side::Base
                    }
                )
                .is_err()
        );
        assert!(
            f.app
                .freeze_candidate(
                    &f.case,
                    FreezeCandidate {
                        command: command(&f, "reset-budget", "operator"),
                        repair_id: repair,
                        policy_revision: 1,
                        changes: c.changes
                    }
                )
                .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "requires an explicitly built native container image and local Docker; no model/network access"]
    async fn physical_container_boundary_and_regression() {
        let mut f = fixture();
        let docker = std::env::var("WATCHHOUSE_DOCKER_PATH").expect("native Docker path required");
        let image = std::env::var("WATCHHOUSE_TEST_IMAGE").expect("pinned image required");
        let runner = std::env::var("WATCHHOUSE_TEST_RUNNER").expect("trusted initializer required");
        let mut p = f.policy.clone();
        p.revision = 2;
        p.docker_path = fs::canonicalize(docker).unwrap();
        p.docker_digest = crate::repair::snapshot::file_digest(&p.docker_path).unwrap();
        p.image = image;
        p.runner_digest =
            crate::repair::snapshot::file_digest(std::path::Path::new(&runner)).unwrap();
        f.app
            .install_container_policy(p.clone(), "operator")
            .unwrap();
        f.policy = p;
        let q = f
            .app
            .qualify_container(
                &f.case,
                Qualify {
                    command: command(&f, "physical-qualification", "operator"),
                    policy_revision: 2,
                },
            )
            .unwrap();
        dispatch(&f, &Native).await;
        let q = f.app.container_job(&f.case, &q.id).unwrap();
        if q.state != "passed" {
            // This explicitly configured synthetic drill contains no account/candidate credentials.
            // Read only this operation's logs/state so failed native proof is diagnosable.
            for args in [
                vec!["container", "logs"],
                vec!["container", "inspect", "--format", "{{json .State}}"],
            ] {
                let mut c = Command::new(&f.policy.docker_path);
                c.arg("--host")
                    .arg(format!("unix://{}", f.policy.socket.display()))
                    .args(args)
                    .arg(format!("watchhouse-{}", q.id));
                let out = c.output().unwrap();
                eprintln!(
                    "native drill diagnostic: {} {}",
                    String::from_utf8_lossy(&out.stdout)
                        .chars()
                        .take(12000)
                        .collect::<String>(),
                    String::from_utf8_lossy(&out.stderr)
                        .chars()
                        .take(2000)
                        .collect::<String>()
                );
            }
        }
        assert_eq!(
            q.state,
            "passed",
            "{}",
            serde_json::to_string_pretty(&q).unwrap()
        );
        admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).unwrap();
        let repair = prepare(&f).await;
        let c = f
            .app
            .freeze_candidate(
                &f.case,
                FreezeCandidate {
                    command: command(&f, "physical-candidate", "operator"),
                    repair_id: repair,
                    policy_revision: 2,
                    changes: vec![TextChange {
                        path: "answer.txt".into(),
                        before_digest: Some(store::digest("5\n")),
                        after: Some("4\n".into()),
                    }],
                },
            )
            .unwrap();
        let mut checks = vec![];
        for (key, side) in [
            ("physical-base", Side::Base),
            ("physical-fixed", Side::Candidate),
        ] {
            let job = f
                .app
                .request_candidate_check(
                    &f.case,
                    RequestCheck {
                        command: command(&f, key, "operator"),
                        candidate_id: c.id.clone(),
                        side,
                    },
                )
                .unwrap();
            dispatch(&f, &Native).await;
            let got = f.app.container_job(&f.case, &job.id).unwrap();
            assert_eq!(
                got.state,
                "passed",
                "{}",
                serde_json::to_string_pretty(&got).unwrap()
            );
            checks.push(got);
        }
        let evidence = observe(&f, "physical-review-evidence", "reviewer");
        let r=f.app.review_candidate(&f.case,ReviewCandidate{command:command(&f,"physical-review","reviewer"),candidate_id:c.id,base_check:checks[0].id.clone(),candidate_check:checks[1].id.clone(),approve:true,findings:"Actual fixed regression failed on base and passed on frozen candidate; no model was involved".into(),evidence_refs:vec![evidence]}).unwrap();
        assert!(r.current);
    }
}
