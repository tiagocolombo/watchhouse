//! Controller fixtures prove state handling, not physical containment or live model access.
use super::{docker::*, protocol::*};
use crate::{contracts::*, store};
fn sample_policy(root: &std::path::Path) -> ContainerPolicy {
    ContainerPolicy {
        schema_version: V1,
        revision: 1,
        docker_path: std::env::current_exe().unwrap(),
        docker_digest: store::digest("client"),
        socket: root
            .parent()
            .unwrap()
            .join("watchhouse-fixture-docker.sock"),
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
    let mut canonical = v.clone();
    canonical["HostConfig"]["CapAdd"] = json!(["CAP_SETGID", "CAP_SETUID"]);
    validate_inspect(&canonical, &j, &input, &control, false, &p.image).unwrap();
    canonical["HostConfig"]["CapAdd"] = json!(["CAP_SETUID", "CAP_SYS_ADMIN"]);
    assert!(validate_inspect(&canonical, &j, &input, &control, false, &p.image).is_err());
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
pub(crate) mod controller {
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
        policy.socket = std::path::PathBuf::from("/var/run/docker.sock");
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
    /// Only injected native-shaped observations, never kernel or live provider proof.
    pub(crate) async fn reviewed_fixture() -> (
        tempfile::TempDir,
        Application,
        ManualClock,
        Candidate,
        Review,
    ) {
        let f = fixture();
        let b = FixtureBackend {
            native: true,
            ..Default::default()
        };
        qualify(&f, "publisher-qualification");
        dispatch(&f, &b).await;
        let repair = prepare(&f).await;
        let c = freeze(&f, &repair, "publisher-candidate");
        let base = f
            .app
            .request_candidate_check(
                &f.case,
                RequestCheck {
                    command: command(&f, "publisher-base", "operator"),
                    candidate_id: c.id.clone(),
                    side: Side::Base,
                },
            )
            .unwrap();
        dispatch(&f, &b).await;
        let fixed = f
            .app
            .request_candidate_check(
                &f.case,
                RequestCheck {
                    command: command(&f, "publisher-fixed", "operator"),
                    candidate_id: c.id.clone(),
                    side: Side::Candidate,
                },
            )
            .unwrap();
        dispatch(&f, &b).await;
        let evidence = observe(&f, "publisher-evidence", "reviewer");
        let r = f
            .app
            .review_candidate(
                &f.case,
                ReviewCandidate {
                    model_job: None,
                    command: command(&f, "publisher-review", "reviewer"),
                    candidate_id: c.id.clone(),
                    base_check: base.id,
                    candidate_check: fixed.id,
                    approve: true,
                    findings: "Injected controller fixture; not native proof".into(),
                    evidence_refs: vec![evidence],
                },
            )
            .unwrap();
        (f._root, f.app, f.clock, c, r)
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
        dispatch_on(f, b, None).await;
    }
    async fn dispatch_on(f: &Fixture, b: &impl Backend, worker: Option<&str>) {
        for stage in ["create", "start"] {
            let worked = f.app.run_container_on(b, worker).await.unwrap();
            if !worked {
                let db = f.app.store.connect().unwrap();
                let mut q = db.prepare("SELECT id,state,json_extract(record,'$.reason') FROM container_jobs ORDER BY rowid").unwrap();
                let jobs = q
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap();
                panic!("No container work during {stage}; synthetic fixture jobs: {jobs:?}");
            }
        }
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
                    model_job: None,
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
    #[cfg(target_os = "linux")]
    async fn routed_container_cleanup_remains_on_original_worker_after_route_removal() {
        use crate::routing::{Kind, Policy as Routes, Target, Worker};
        let f = fixture();
        let q = qualify(&f, "routed-qualification");
        let b = FixtureBackend::default();
        let routes = Routes {
            schema_version: V1,
            revision: 1,
            workers: vec![Worker {
                id: "container-a".into(),
                platform: "linux".into(),
                architecture: std::env::consts::ARCH.into(),
                paused: false,
                targets: vec![Target {
                    kind: Kind::Container,
                    policy_revision: 1,
                }],
            }],
        };
        f.app
            .install_worker_routes(routes, "operator", "container routing fixture")
            .unwrap();
        assert!(!f.app.run_container_once(&b).await.unwrap());
        assert!(
            !f.app
                .run_container_on(&b, Some("container-b"))
                .await
                .unwrap()
        );
        assert!(
            f.app
                .run_container_on(&b, Some("container-a"))
                .await
                .unwrap()
        );
        f.app
            .install_worker_routes(
                Routes {
                    schema_version: V1,
                    revision: 2,
                    workers: vec![],
                },
                "operator",
                "removed route",
            )
            .unwrap();
        assert!(
            !f.app
                .run_container_on(&b, Some("container-a"))
                .await
                .unwrap()
        );
        f.clock.advance(Duration::seconds(101));
        f.app.supervise_team().unwrap();
        assert!(
            !f.app
                .run_container_on(&b, Some("container-b"))
                .await
                .unwrap()
        );
        assert!(
            f.app
                .run_container_on(&b, Some("container-a"))
                .await
                .unwrap()
        );
        let state = b.state.lock().unwrap();
        assert_eq!(state.created, 1);
        assert_eq!(state.started, 0);
        assert_eq!(state.cancelled, 1);
        assert_eq!(
            f.app.container_job(&f.case, &q.id).unwrap().state,
            "cancelled"
        );
    }
    #[tokio::test]
    async fn evidence_capacity_never_accepts_container_proof_or_repeats_start() {
        for native in [false, true] {
            let f = fixture();
            let q = qualify(&f, "quota-qualification");
            let b = FixtureBackend {
                native,
                ..Default::default()
            };
            let p = crate::capacity::Limits {
                revision: 2,
                max_evidence_bytes: 1024,
                ..Default::default()
            };
            f.app
                .install_factory_limits(p, "operator", "container quota fixture")
                .unwrap();
            f.app
                .store
                .connect()
                .unwrap()
                .execute(
                    "INSERT INTO evidence VALUES ('full-capacity','text/plain',?1,'fixture','now')",
                    ["x".repeat(1024)],
                )
                .unwrap();
            dispatch(&f, &b).await;
            let current = f.app.container_job(&f.case, &q.id).unwrap();
            assert!(current.evidence_digest.is_none());
            assert!(admission(&f.app.store.connect().unwrap(), &f.policy, &f.app.now()).is_err());
            if !native {
                assert_eq!(current.state, "unknown");
                assert!(current.cancel_requested);
                assert!(f.app.run_container_once(&b).await.unwrap());
            }
            assert_eq!(
                f.app.container_job(&f.case, &q.id).unwrap().state,
                "cancelled"
            );
            let s = b.state.lock().unwrap();
            assert_eq!(s.created, 1);
            assert_eq!(s.started, 1);
        }
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
            model_job: None,
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
        let plan = f.app.prepare_publication(&f.case, &c.id, &r.id).unwrap();
        assert!(!plan.external_writes_authorized);
        assert_eq!(plan.tree.request["base_tree"], c.base_tree);
        assert!(crate::delivery::objects::oid(&plan.tree.sha));
        fs::write(c.root.join("answer.txt"), "6\n").unwrap();
        assert!(f.app.prepare_publication(&f.case, &c.id, &r.id).is_err());
        fs::write(c.root.join("answer.txt"), "4\n").unwrap();
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
                        model_job: None,
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
        p.wall_seconds = 30;
        p.docker_path = fs::canonicalize(docker).unwrap();
        p.docker_digest = crate::repair::snapshot::file_digest(&p.docker_path).unwrap();
        p.image = image;
        p.runner_digest =
            crate::repair::snapshot::file_digest(std::path::Path::new(&runner)).unwrap();
        f.app
            .install_container_policy(p.clone(), "operator")
            .unwrap();
        f.policy = p;
        f.app
            .install_worker_routes(
                crate::routing::Policy {
                    schema_version: V1,
                    revision: 1,
                    workers: vec![crate::routing::Worker {
                        id: "native-container".into(),
                        platform: "linux".into(),
                        architecture: std::env::consts::ARCH.into(),
                        paused: false,
                        targets: vec![crate::routing::Target {
                            kind: crate::routing::Kind::Container,
                            policy_revision: 2,
                        }],
                    }],
                },
                "operator",
                "native routed regression",
            )
            .unwrap();
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
        dispatch_on(&f, &Native, Some("native-container")).await;
        let q = f.app.container_job(&f.case, &q.id).unwrap();
        if q.state != "passed" {
            // This explicitly configured synthetic drill contains no account/candidate credentials.
            // Read only this operation's logs/state so failed native proof is diagnosable.
            for args in [
                vec!["container", "logs"],
                vec!["container", "inspect", "--format", "{{json .State}}"],
                vec![
                    "container",
                    "inspect",
                    "--format",
                    "caps={{json .HostConfig.CapAdd}} drop={{json .HostConfig.CapDrop}} security={{json .HostConfig.SecurityOpt}}",
                ],
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
                    model_job: None,
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
            dispatch_on(&f, &Native, Some("native-container")).await;
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
        let r=f.app.review_candidate(&f.case,ReviewCandidate{model_job:None,command:command(&f,"physical-review","reviewer"),candidate_id:c.id,base_check:checks[0].id.clone(),candidate_check:checks[1].id.clone(),approve:true,findings:"Actual fixed regression failed on base and passed on frozen candidate; no model was involved".into(),evidence_refs:vec![evidence]}).unwrap();
        assert!(r.current);
        // No controller cancellation is sent until after observing expiry. The trusted PID 1
        // watchdog must terminate the sleeping test and all namespace descendants itself.
        let mut expired = f.policy.clone();
        expired.wall_seconds = 2;
        expired.args = vec!["fixture-stall".into()];
        let source = f._root.path().join("source");
        let job = expired.job(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
            vec![InputFile {
                path: "answer.txt".into(),
                digest: store::digest("5\n"),
                bytes: 2,
                executable: false,
            }],
        );
        let (root, job) = super::super::worker::stage(&expired, job, &source).unwrap();
        let docker = Docker::new(expired.clone()).unwrap();
        let input = root.path().join("input");
        let control = root.path().join("policy");
        let id = docker.create(&job, &input, &control, false).await.unwrap();
        let observed = docker.inspect(&job, &input, &control, false).await.unwrap();
        assert_eq!(observed.id, id);
        let started = Command::new(&expired.docker_path)
            .arg("--host")
            .arg(format!("unix://{}", expired.socket.display()))
            .args(["container", "start", &id])
            .output()
            .unwrap();
        assert!(started.status.success());
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let ended = docker.inspect(&job, &input, &control, false).await.unwrap();
        docker.cancel(&job, &input, &control, false).await.unwrap();
        assert!(!ended.running);
        assert_eq!(
            ended.exit_code, 124,
            "watchdog must enforce wall time with no coordinator cancellation"
        );
    } // These injected native-shaped observations test controller lineage only. They execute no
    // client/model or kernel resource. Physical adapter acceptance is a separate CI drill.
    struct RoleFixture {
        session: String,
        changes: Vec<TextChange>,
    }
    impl crate::model_runtime::ops::Backend for RoleFixture {
        fn native(&self) -> bool {
            true
        }
        fn preflight(
            &self,
            _: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<()> {
            Box::pin(async { Ok(()) })
        }
        fn create(
            &self,
            j: crate::model_runtime::ops::Job,
            p: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<crate::model_runtime::cgroup::Handle> {
            Box::pin(async move {
                Ok(crate::model_runtime::cgroup::Handle {
                    operation: j.id.clone(),
                    directory: p.cgroup_root.join(format!("watchhouse-{}", j.id)),
                    inode: 1,
                    boot_id: crate::model_runtime::cgroup::boot()?,
                    limits: p.limits,
                    profile_digest: j.profile_digest,
                })
            })
        }
        fn recover(
            &self,
            _: crate::model_runtime::ops::Job,
            _: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<crate::model_runtime::cgroup::Handle> {
            Box::pin(async { Err(Error::Conflict("not a physical resource".into())) })
        }
        fn start(
            &self,
            _: crate::model_runtime::ops::Job,
            _: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<()> {
            Box::pin(async { Ok(()) })
        }
        fn cancel(
            &self,
            _: crate::model_runtime::ops::Job,
            _: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<bool> {
            Box::pin(async { Ok(true) })
        }
        fn output(
            &self,
            j: crate::model_runtime::ops::Job,
            _: crate::model_runtime::policy::ModelPolicy,
        ) -> crate::model_runtime::ops::Effect<Option<crate::model_runtime::protocol::Output>>
        {
            use crate::model_runtime::{client::ClientResult, codex::*, protocol::Output};
            let session = self.session.clone();
            let changes = self.changes.clone();
            Box::pin(async move {
                let output=match j.plan.role {Role::Qualification=>RoleOutput::Qualification(Probe{schema_version:V1,nonce:j.nonce.clone()}),Role::Engineer=>RoleOutput::Engineer(Proposal{schema_version:V1,summary:"Correct the controlled fixture".into(),changes}),Role::Reviewer=>RoleOutput::Reviewer(Assessment{schema_version:V1,approve:true,findings:"The independently supplied exact regression fails on base and passes on this candidate".into()})};
                Ok(Some(Output {
                    schema_version: V1,
                    operation: j.id,
                    nonce: j.nonce,
                    profile_digest: j.profile_digest,
                    client: ClientResult {
                        parsed: Some(Parsed {
                            output,
                            session,
                            actual_model: None,
                            usage: Usage {
                                input_tokens: None,
                                output_tokens: None,
                                source: "unknown".into(),
                            },
                            events: vec!["fixture-only-native-shape".into()],
                        }),
                        failure_code: None,
                        stderr_digest: store::digest(""),
                    },
                    synthetic: j.plan.qualification,
                    execution_tools_absent: Some(true),
                    advertised_tools_digest: Some(store::digest("[]")),
                    descendant_started: true,
                }))
            })
        }
    }
    fn model_profile(f: &Fixture) -> tempfile::TempDir {
        use crate::model_runtime::{cgroup::Limits, policy::*};
        let control = tempfile::tempdir().unwrap();
        let jobs = control.path().join("jobs");
        let auth = control.path().join("auth");
        fs::create_dir(&jobs).unwrap();
        fs::create_dir(&auth).unwrap();
        use std::os::unix::fs::PermissionsExt;
        for dir in [&jobs, &auth] {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let binary = control.path().join("never-executed-client");
        fs::write(&binary, b"fixture-identity").unwrap();
        let pin = Pin {
            path: binary,
            digest: store::digest("fixture-identity"),
        };
        f.app
            .install_model_policy(
                ModelPolicy {
                    profile: "default".into(),
                    adapter: crate::model_runtime::policy::Adapter::Codex,
                    schema_version: V1,
                    revision: 1,
                    container_revision: 1,
                    dispatch_grace_seconds: 10,
                    runtime: pin.clone(),
                    initializer: pin,
                    cgroup_root: control.path().join("not-a-kernel-group"),
                    jobs_root: jobs,
                    requested_model: "fixture-no-network-or-account".into(),
                    authority: Authority::OpenaiAuthHome { path: auth },
                    limits: Limits {
                        memory_bytes: 268435456,
                        pids: 32,
                        cpu_micros: 50000,
                        wall_seconds: 10,
                        output_bytes: 65536,
                    },
                    proof_seconds: 300,
                    cleanup_attempts: 2,
                    cleanup_retry_seconds: 1,
                },
                "operator",
                "injected controller proof only",
            )
            .unwrap();
        control
    }
    async fn model_run(
        f: &Fixture,
        b: &RoleFixture,
        key: &str,
        action: crate::model_runtime::ops::Action,
    ) -> crate::model_runtime::ops::Job {
        model_run_profile(f, b, key, action, 1).await
    }
    async fn model_run_profile(
        f: &Fixture,
        b: &RoleFixture,
        key: &str,
        action: crate::model_runtime::ops::Action,
        policy_revision: i64,
    ) -> crate::model_runtime::ops::Job {
        use crate::model_runtime::ops::*;
        let j = f
            .app
            .request_model(
                &f.case,
                Request {
                    command: command(f, key, "operator"),
                    policy_revision,
                    action,
                    fixture_response: None,
                },
            )
            .unwrap();
        for _ in 0..4 {
            f.app.run_model_once(b).await.unwrap();
        }
        f.app.model_job(&f.case, &j.id).unwrap()
    }
    #[tokio::test]
    async fn model_adoption_requires_exact_proposal_evidence_and_independent_session() {
        use crate::model_runtime::{adoption::*, ops::Action};
        for (same_session, cross_harness) in [(false, false), (true, false), (false, true)] {
            let f = fixture();
            let container = FixtureBackend {
                native: true,
                ..Default::default()
            };
            qualify(&f, "container-qualification");
            dispatch(&f, &container).await;
            let repair = prepare(&f).await;
            let _control = model_profile(&f);
            let mut roles = RoleFixture {
                session: "engineer-session".into(),
                changes: vec![TextChange {
                    path: "answer.txt".into(),
                    before_digest: Some(store::digest("5\n")),
                    after: Some("4\n".into()),
                }],
            };
            assert_eq!(
                model_run(&f, &roles, "model-qualification", Action::Qualification)
                    .await
                    .state,
                "qualified"
            );
            let engineer = model_run(
                &f,
                &roles,
                "engineer",
                Action::Engineer {
                    repair_id: repair.clone(),
                    paths: vec!["answer.txt".into()],
                },
            )
            .await;
            assert_eq!(engineer.state, "completed");
            let mut tampered = FreezeCandidate {
                command: command(&f, "tampered-proposal", "operator"),
                repair_id: repair,
                policy_revision: 1,
                changes: roles.changes.clone(),
                model_job: Some(engineer.id.clone()),
            };
            tampered.changes[0].after = Some("6\n".into());
            assert!(f.app.freeze_candidate(&f.case, tampered).is_err());
            assert!(
                f.app
                    .snapshot(&f.case)
                    .unwrap()
                    .repair_progress
                    .candidates
                    .is_empty()
            );
            let adopt = AdoptProposal {
                command: command(&f, "adopt-proposal", "operator"),
                job_id: engineer.id.clone(),
                container_revision: 1,
            };
            let candidate = f.app.adopt_model_proposal(&f.case, adopt.clone()).unwrap();
            assert_eq!(candidate.author, "model-session:engineer-session");
            assert_eq!(candidate.producer, "model-role-v1");
            assert_eq!(candidate.model_job, Some(engineer.id));
            assert_eq!(
                f.app.adopt_model_proposal(&f.case, adopt).unwrap().id,
                candidate.id
            );
            let base = f
                .app
                .request_candidate_check(
                    &f.case,
                    RequestCheck {
                        command: command(&f, "model-base", "operator"),
                        candidate_id: candidate.id.clone(),
                        side: Side::Base,
                    },
                )
                .unwrap();
            dispatch(&f, &container).await;
            let checked = f
                .app
                .request_candidate_check(
                    &f.case,
                    RequestCheck {
                        command: command(&f, "model-candidate", "operator"),
                        candidate_id: candidate.id.clone(),
                        side: Side::Candidate,
                    },
                )
                .unwrap();
            dispatch(&f, &container).await;
            if !same_session {
                roles.session = "reviewer-session".into();
            }
            if cross_harness {
                let mut profile =
                    crate::model_runtime::ops::policy(&f.app.store.connect().unwrap(), 1).unwrap();
                profile.revision = 2;
                profile.adapter = crate::model_runtime::policy::Adapter::ClaudeCode;
                profile.profile = "independent-review".into();
                profile.authority = crate::model_runtime::policy::Authority::ClaudeApiKeyEnv {
                    token_env: "WATCHHOUSE_CLAUDE_FIXTURE_UNSET_KEY".into(),
                };
                f.app
                    .install_model_policy(
                        profile,
                        "operator",
                        "injected cross-harness controller fixture only",
                    )
                    .unwrap();
                roles.session = "claude:00000000-0000-4000-8000-000000000002".into();
                assert_eq!(
                    model_run_profile(
                        &f,
                        &roles,
                        "second-adapter-qualification",
                        Action::Qualification,
                        2
                    )
                    .await
                    .state,
                    "qualified"
                );
            }
            let reviewer = model_run_profile(
                &f,
                &roles,
                "reviewer",
                Action::Reviewer {
                    candidate_id: candidate.id.clone(),
                    base_check: base.id,
                    candidate_check: checked.id,
                },
                if cross_harness { 2 } else { 1 },
            )
            .await;
            assert_eq!(reviewer.state, "completed");
            let cmd = AdoptReview {
                command: command(&f, "adopt-review", "operator"),
                job_id: reviewer.id.clone(),
            };
            if same_session {
                assert!(f.app.adopt_model_review(&f.case, cmd).is_err());
            } else {
                let r = f.app.adopt_model_review(&f.case, cmd.clone()).unwrap();
                assert!(r.current);
                assert_eq!(r.author, format!("model-session:{}", roles.session));
                assert_eq!(f.app.adopt_model_review(&f.case, cmd).unwrap().id, r.id);
                crate::delivery::tests::exercise_model_lineage_publication(&f.app, &candidate, &r);
                // Corruption of the engineer evidence invalidates both its candidate and the dependent review.
                f.app
                    .store
                    .connect()
                    .unwrap()
                    .execute(
                        "UPDATE evidence SET content='corrupted-role' WHERE digest=?1",
                        [engineer.evidence_digest.unwrap()],
                    )
                    .unwrap();
                assert!(!f.app.snapshot(&f.case).unwrap().repair_progress.reviews[0].current);
            }
        }
    }
    #[test]
    fn absent_model_lineage_preserves_legacy_command_receipt_fingerprints() {
        let command = serde_json::json!({"command":{"meta":{"schema_version":1,"idempotency_key":"legacy","actor":"operator","reason":"legacy operator proposal"},"expected_version":1},"repair_id":"repair-fixture","policy_revision":1,"changes":[{"path":"answer.txt","before_digest":null,"after":"4\n"}]});
        let parsed: FreezeCandidate = serde_json::from_value(command.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), command);
    }
}
