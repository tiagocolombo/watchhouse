use chrono::{TimeZone, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use tempfile::TempDir;
use watchhouse::{
    application::Application, contracts::*, domain::ManualClock, execution::FakeAdapter,
    intake::FakeSource, repair::*, store::Store,
};
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}
fn git_binary() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|p| p.is_file())
        .unwrap()
        .canonicalize()
        .unwrap()
}
fn setup() -> (TempDir, Application, RepairSourceConfig, String) {
    let d = TempDir::new().unwrap();
    let repo = d.path().join("source");
    fs::create_dir(&repo).unwrap();
    let root = d.path().join("bases");
    fs::create_dir(&root).unwrap();
    git(&repo, &["init", "-q"]);
    fs::create_dir(repo.join("src")).unwrap();
    fs::write(repo.join("src/logic.txt"), "fixture bug: add(2,2)=5\n").unwrap();
    fs::write(repo.join(".gitattributes"), "*.txt filter=forbidden\n").unwrap();
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
            "Sanitized fixture",
        ],
    );
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(
        &repo,
        &[
            "config",
            "filter.forbidden.smudge",
            "must-never-run-the-filter",
        ],
    );
    fs::write(repo.join("untracked-secret.txt"), "OMIT host-only data").unwrap();
    let binary = git_binary();
    let digest = format!("sha256:{:x}", Sha256::digest(fs::read(&binary).unwrap()));
    let cfg = RepairSourceConfig {
        schema_version: V1,
        revision: 1,
        repository: "fixture/checkout".into(),
        source_path: repo.canonicalize().unwrap(),
        base_commit: base,
        workspace_root: root.canonicalize().unwrap(),
        runtime_path: binary,
        runtime_digest: digest,
        max_files: 20,
        max_blob_bytes: 65536,
        max_total_bytes: 1048576,
        preparation_seconds: 30,
        probe_seconds: 5,
    };
    let clock = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap());
    let a = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(clock));
    let id = a
        .create_case(FakeSource.manual_command("repair-fixture"))
        .unwrap()
        .id;
    a.record_evidence(&id,RecordEvidence {schema_version:V1,idempotency_key:"repair-observation".into(),actor:"operator".into(),reason:"Record fixture reproduction expectation".into(),expected_version:a.snapshot(&id).unwrap().case.version,scope:"fixture checkout".into(),statement:"Fixture add(2,2) returned 5; expected 4. This is an operator report, not a verified regression run".into(),collection_method:"Manual synthetic fixture inspection".into(),observed_at:None,uncertainty:"No real harness or regression executed".into()}).unwrap();
    (d, a, cfg, id)
}
fn request(a: &Application, id: &str, key: &str) -> RequestRepair {
    RequestRepair {command:OperatorCommand {meta:CommandMeta {schema_version:V1,idempotency_key:key.into(),actor:"operator".into(),reason:"Prepare bounded fixture repair".into()},expected_version:a.snapshot(id).unwrap().case.version},source_revision:1,objective:"Correct the fixture addition result".into(),expected_behavior:"add(2,2) returns 4".into(),regression_plan:"Demonstrate failure at pinned base and pass at a frozen candidate using independent checks".into(),evidence_refs:a.snapshot(id).unwrap().qualification.evidence.iter().map(|e|e.digest.clone()).collect()}
}
#[tokio::test]
async fn object_only_snapshot_pins_committed_bytes_without_git_metadata_filters_or_untracked_data()
{
    let (_d, a, cfg, _id) = setup();
    let source = a
        .install_repair_source(cfg.clone(), "operator")
        .await
        .unwrap();
    assert_eq!(source.snapshot.base_commit, cfg.base_commit);
    assert_eq!(
        fs::read_to_string(source.snapshot_path.join("src/logic.txt")).unwrap(),
        "fixture bug: add(2,2)=5\n"
    );
    assert!(!source.snapshot_path.join(".git").exists());
    assert!(!source.snapshot_path.join("untracked-secret.txt").exists());
    assert!(source.probe.version.is_none());
    assert!(source.probe.verified.is_empty());
    fs::write(
        cfg.source_path.join("src/logic.txt"),
        "working tree changed\n",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(source.snapshot_path.join("src/logic.txt")).unwrap(),
        "fixture bug: add(2,2)=5\n"
    );
    assert_eq!(
        a.install_repair_source(cfg, "operator")
            .await
            .unwrap()
            .snapshot_path,
        source.snapshot_path
    );
}
#[tokio::test]
async fn blocked_admission_is_durable_owned_idempotent_and_never_enters_the_fake_execution_path() {
    let (d, a, cfg, id) = setup();
    a.install_repair_source(cfg, "operator").await.unwrap();
    let cmd = request(&a, &id, "repair-request");
    let r = a.request_repair(&id, cmd.clone()).unwrap();
    assert_eq!(r.state, "blocked");
    assert_eq!(a.request_repair(&id, cmd).unwrap().id, r.id);
    assert_eq!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .filter(|e| e.source_type == "repair_admission")
            .count(),
        1
    );
    assert!(r.denials.iter().any(|d| d.contains("isolated worker")));
    a.run_once(&FakeAdapter).unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.repairs.len(), 1);
    assert_eq!(s.repairs[0].state, "blocked");
    assert_eq!(s.results.len(), 1);
    assert!(s.results[0].actual_model.is_none());
    let reopened = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 15, 12, 1, 0).unwrap(),
        )),
    );
    assert_eq!(reopened.snapshot(&id).unwrap().repairs[0].id, r.id);
}
#[tokio::test]
async fn config_is_immutable_sequential_outside_agent_control_and_uses_no_http_install_route() {
    let (_d, a, cfg, id) = setup();
    assert!(
        a.install_repair_source(cfg.clone(), "fake-engineer")
            .await
            .is_err()
    );
    let source = a
        .install_repair_source(cfg.clone(), "operator")
        .await
        .unwrap();
    let mut changed = cfg.clone();
    changed.max_files += 1;
    assert!(a.install_repair_source(changed, "operator").await.is_err());
    let mut skip = cfg.clone();
    skip.revision = 3;
    assert!(a.install_repair_source(skip, "operator").await.is_err());
    assert_eq!(
        a.repair_source_status().unwrap().unwrap().manifest_digest,
        source.manifest_digest
    );
    let mut fake = request(&a, &id, "fake-repair");
    fake.command.meta.actor = "fake-engineer".into();
    assert!(a.request_repair(&id, fake).is_err());
    let mut missing = request(&a, &id, "no-evidence");
    missing.evidence_refs.clear();
    assert!(a.request_repair(&id, missing).is_err());
    let mut stale = request(&a, &id, "stale");
    stale.command.expected_version -= 1;
    assert!(a.request_repair(&id, stale).is_err());
    let mut schema = serde_json::to_value(request(&a, &id, "schema")).unwrap();
    schema["command"]["meta"]["schema_version"] = 2.into();
    assert!(serde_json::from_value::<RequestRepair>(schema).is_err());
}
#[tokio::test]
async fn frozen_snapshot_drift_is_visible_and_new_configuration_does_not_rewrite_old_requests() {
    let (_d, a, cfg, id) = setup();
    let old = a
        .install_repair_source(cfg.clone(), "operator")
        .await
        .unwrap();
    let first = a.request_repair(&id, request(&a, &id, "first")).unwrap();
    fs::write(
        old.snapshot_path.join("unexpected.txt"),
        "unaccepted candidate content",
    )
    .unwrap();
    let drift = a.request_repair(&id, request(&a, &id, "drift")).unwrap();
    assert!(
        drift
            .denials
            .iter()
            .any(|d| d.contains("Frozen base snapshot"))
    );
    let mut next = cfg;
    next.revision = 2;
    next.runtime_digest = format!("sha256:{}", "0".repeat(64));
    let new = a.install_repair_source(next, "operator").await.unwrap();
    assert!(new.probe.error.unwrap().contains("not invoked"));
    assert_eq!(first.source_revision, 1);
    assert_eq!(
        a.snapshot(&id)
            .unwrap()
            .repairs
            .last()
            .unwrap()
            .config_digest,
        old.config_digest
    );
}
#[tokio::test]
async fn denied_paths_submodules_and_byte_limits_fail_before_a_source_is_registered() {
    let (_d, a, mut cfg, _) = setup();
    cfg.max_files = 1;
    assert!(
        a.install_repair_source(cfg.clone(), "operator")
            .await
            .is_err()
    );
    assert!(a.repair_source_status().unwrap().is_none());
    cfg.max_files = 20;
    cfg.max_blob_bytes = 1;
    assert!(
        a.install_repair_source(cfg.clone(), "operator")
            .await
            .is_err()
    );
    cfg.max_blob_bytes = 65536;
    git(
        &cfg.source_path,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},submodule", cfg.base_commit),
        ],
    );
    git(
        &cfg.source_path,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "Unsupported gitlink",
        ],
    );
    cfg.base_commit = git(&cfg.source_path, &["rev-parse", "HEAD"]);
    assert!(
        a.install_repair_source(cfg.clone(), "operator")
            .await
            .is_err()
    );
    assert!(a.repair_source_status().unwrap().is_none());
    cfg.workspace_root = cfg.source_path.clone();
    assert!(a.install_repair_source(cfg, "operator").await.is_err());
}
#[tokio::test]
async fn request_failure_rolls_back_the_request_event_escalation_and_receipt_and_concurrent_requests_conflict()
 {
    let (d, a, cfg, id) = setup();
    a.install_repair_source(cfg, "operator").await.unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_repair BEFORE INSERT ON escalations WHEN NEW.source_type='repair_admission' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    let cmd = request(&a, &id, "atomic");
    let version = cmd.command.expected_version;
    assert!(a.request_repair(&id, cmd.clone()).is_err());
    assert!(a.snapshot(&id).unwrap().repairs.is_empty());
    assert_eq!(a.snapshot(&id).unwrap().case.version, version);
    db.execute_batch("DROP TRIGGER fail_repair").unwrap();
    a.request_repair(&id, cmd).unwrap();
    let first = request(&a, &id, "compete-one");
    let second = request(&a, &id, "compete-two");
    let b = a.clone();
    let c = a.clone();
    let x = id.clone();
    let y = id.clone();
    let one = std::thread::spawn(move || b.request_repair(&x, first));
    let two = std::thread::spawn(move || c.request_repair(&y, second));
    assert_eq!(
        [one.join().unwrap().is_ok(), two.join().unwrap().is_ok()]
            .iter()
            .filter(|x| **x)
            .count(),
        1
    );
}
#[tokio::test]
async fn api_cli_reports_and_auth_preserve_the_same_denied_admission_record() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (d, a, cfg, id) = setup();
    a.install_repair_source(cfg, "operator").await.unwrap();
    let mut cmd = request(&a, &id, "api-repair");
    cmd.objective = "<script>window.repairInjected=true</script> Fixture objective".into();
    let token = "repair-fixture-operator-token-32bytes";
    let router = watchhouse::http::router(a.clone(), token.into(), "127.0.0.1:8787".into());
    let post = |auth: bool| {
        let mut r = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/cases/{id}/repairs"))
            .header("Host", "127.0.0.1:8787")
            .header("Content-Type", "application/json");
        if auth {
            r = r.header("Authorization", format!("Bearer {token}"));
        }
        r.body(Body::from(serde_json::to_vec(&cmd).unwrap()))
            .unwrap()
    };
    assert_eq!(
        router.clone().oneshot(post(false)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let response = router.clone().oneshot(post(true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let r: RepairRequest =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(r.state, "blocked");
    let markdown = a.repair_markdown(&id).unwrap();
    assert!(!markdown.contains("<script>"));
    assert!(markdown.contains("repairInjected"));
    assert!(markdown.contains("No real worker"));
    let file = d.path().join("request.json");
    fs::write(&file, serde_json::to_vec(&cmd).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .arg("--db")
        .arg(d.path().join("db"))
        .args(["case", "request-repair", &id, "--file"])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<RepairRequest>(&result.stdout)
            .unwrap()
            .id,
        r.id
    );
    let legacy = json!({"command":{},"source_revision":1,"objective":"no"});
    assert!(serde_json::from_value::<RequestRepair>(legacy).is_err());
}
#[tokio::test]
async fn unresolved_challenges_cannot_be_bypassed_by_requesting_repair() {
    use watchhouse::challenges::*;
    let (_d, a, cfg, id) = setup();
    a.install_repair_source(cfg, "operator").await.unwrap();
    let claim = a.snapshot(&id).unwrap().events[0].clone();
    let mut op = request(&a, &id, "open").command;
    op.meta.actor = "fake-challenger".into();
    a.challenge(
        &id,
        ChallengeCommand {
            command: op,
            action: ChallengeAction::Open {
                target_event_id: claim.event_id,
                target_version: claim.aggregate_version,
                recipient: "fake-investigator".into(),
                arbiter: "fake-verifier".into(),
                requested_evidence: "Reproduce the exact fixture claim".into(),
                uncertainty: "Operator evidence is not independent regression validation".into(),
                blocking: true,
                acknowledge_seconds: 10,
                response_seconds: 30,
            },
        },
    )
    .unwrap();
    assert!(
        a.request_repair(&id, request(&a, &id, "challenged"))
            .is_err()
    );
    assert!(a.snapshot(&id).unwrap().repairs.is_empty());
}
#[tokio::test]
async fn corrupted_git_object_cannot_be_frozen_under_an_unrelated_blob_identity() {
    let (_d, a, cfg, _) = setup();
    let original = git(
        &cfg.source_path,
        &["rev-parse", &format!("{}:src/logic.txt", cfg.base_commit)],
    );
    fs::write(
        cfg.source_path.join("forged.txt"),
        "fixture bug: add(2,2)=9\n",
    )
    .unwrap();
    let forged = git(&cfg.source_path, &["hash-object", "-w", "forged.txt"]);
    let object = |oid: &str| {
        cfg.source_path
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..])
    };
    let original_path = object(&original);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&original_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(windows)]
    {
        let mut permissions = fs::metadata(&original_path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&original_path, permissions).unwrap();
    }
    fs::copy(object(&forged), &original_path).unwrap();
    assert!(a.install_repair_source(cfg, "operator").await.is_err());
    assert!(a.repair_source_status().unwrap().is_none());
}
#[path = "common/mod.rs"]
mod common;
#[tokio::test]
async fn observed_source_case_requires_an_exact_explicit_base_mapping() {
    let (_d, _a, mut cfg, _) = setup();
    let (_source_dir, a, _t, id) = common::fixture();
    cfg.repository = a.snapshot(&id).unwrap().source.unwrap().repository;
    a.record_evidence(
        &id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: "mapped-observation".into(),
            actor: "operator".into(),
            reason: "Scoped synthetic fixture report".into(),
            expected_version: a.snapshot(&id).unwrap().case.version,
            scope: "fixture".into(),
            statement: "Reproduction proposal, no verified regression".into(),
            collection_method: "Manual fixture review".into(),
            observed_at: None,
            uncertainty: "Mapping must be exact".into(),
        },
    )
    .unwrap();
    a.install_repair_source(cfg, "operator").await.unwrap();
    let error = a
        .request_repair(&id, request(&a, &id, "wrong-base"))
        .unwrap_err();
    assert!(error.to_string().contains("base does not match"));
    assert!(a.snapshot(&id).unwrap().repairs.is_empty());
}
