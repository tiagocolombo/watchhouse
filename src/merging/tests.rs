//! Injected remote/server observations; no live repository or merge authority is exercised.
use super::*;
use github::*;
use std::cell::RefCell;
fn policy(repository: &str) -> Policy {
    Policy {
        schema_version: V1,
        revision: 1,
        repository: repository.into(),
        base_branch: "main".into(),
        token_env: "WATCHHOUSE_FIXTURE_MERGER_TOKEN".into(),
        paused: false,
        allowed_paths: vec![
            "src/".into(),
            "test.txt".into(),
            "value.txt".into(),
            "fixture.txt".into(),
        ],
        denied_paths: vec!["src/auth/".into()],
        required_checks: vec![Check {
            name: "CI".into(),
            app_id: 123,
        }],
        required_approvals: 1,
        max_changed_bytes: 100_000,
        request_seconds: 5,
        job_seconds: 120,
    }
}
struct Mock {
    o: RefCell<Observation>,
    writes: RefCell<u32>,
    lost: bool,
    merged: RefCell<Option<Merged>>,
    native: bool,
}
impl Mock {
    fn new(j: &Job) -> Self {
        Self {
            o: RefCell::new(Observation {
                repository: j.repository.clone(),
                branch: j.branch.clone(),
                base_branch: j.base_branch.clone(),
                head: j.head.clone(),
                base: j.base.clone(),
                tree: j.tree.clone(),
                open: true,
                draft: false,
                mergeable: true,
                clean: true,
                strict_protection: true,
                required_checks: vec![Check {
                    name: "CI".into(),
                    app_id: 123,
                }],
                successful_checks: vec![Check {
                    name: "CI".into(),
                    app_id: 123,
                }],
                all_checks_pass: true,
                current_approvals: 1,
                changes_requested: false,
                required_approvals: 1,
                paths: j.paths.clone(),
            }),
            writes: RefCell::new(0),
            lost: false,
            merged: RefCell::new(None),
            native: false,
        }
    }
}
impl Backend for Mock {
    fn native(&self) -> bool {
        self.native
    }
    fn inspect(&self, _: &Policy, _: &Job) -> std::result::Result<Observation, Failure> {
        Ok(self.o.borrow().clone())
    }
    fn merge(&self, p: &Policy, j: &Job) -> std::result::Result<Merged, Failure> {
        eligible(p, j, &self.o.borrow())?;
        *self.writes.borrow_mut() += 1;
        let m = Merged {
            commit: "f".repeat(40),
            head: j.head.clone(),
            base: j.base.clone(),
            tree: j.tree.clone(),
        };
        *self.merged.borrow_mut() = Some(m.clone());
        if self.lost {
            Err(Failure::Unavailable)
        } else {
            Ok(m)
        }
    }
    fn reconcile(&self, _: &Policy, _: &Job) -> std::result::Result<Merged, Failure> {
        self.merged.borrow().clone().ok_or(Failure::Unavailable)
    }
}
async fn fixture() -> (
    tempfile::TempDir,
    Application,
    crate::domain::ManualClock,
    Job,
) {
    let (root, a, t, published) = crate::delivery::tests::published_fixture().await;
    let c = crate::real_execution::pipeline::get(
        &a.store.connect().unwrap(),
        &published.plan.candidate_id,
    )
    .unwrap();
    let mut p = policy(&published.plan.repository);
    p.allowed_paths = c.changes.iter().map(|c| c.path.clone()).collect();
    a.install_merge_policy(p, "operator", "fixture only")
        .unwrap();
    let j = a
        .enqueue_merge(
            &published.case_id,
            Enqueue {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "merge-fixture".into(),
                        actor: "operator".into(),
                        reason: "Synthetic merge admission".into(),
                    },
                    expected_version: a.snapshot(&published.case_id).unwrap().case.version,
                },
                publication_id: published.id,
                policy_revision: 1,
            },
        )
        .unwrap();
    (root, a, t, j)
}
#[tokio::test]
async fn eligible_fixture_and_unknown_merge_reconcile_without_replay() {
    let (_d, a, _t, j) = fixture().await;
    let b = Mock::new(&j);
    assert!(a.run_merge_once(&b).unwrap());
    assert_eq!(*b.writes.borrow(), 1);
    let r = get(&a.store.connect().unwrap(), &j.id).unwrap();
    assert_eq!(r.state, "simulated");
    assert!(!r.native);
    assert!(
        !a.snapshot(&j.case_id)
            .unwrap()
            .recovery
            .recovery
            .response_resolved
    );
    let (d, a, t, j) = fixture().await;
    let mut b = Mock::new(&j);
    b.lost = true;
    assert!(a.run_merge_once(&b).unwrap());
    assert_eq!(
        get(&a.store.connect().unwrap(), &j.id).unwrap().state,
        "unknown"
    );
    let reopened = Application::new(
        crate::store::Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t),
    );
    assert!(!reopened.run_merge_once(&b).unwrap());
    assert_eq!(
        reopened
            .reconcile_merge(&j.case_id, &j.id, &b)
            .unwrap()
            .state,
        "simulated"
    );
    assert_eq!(*b.writes.borrow(), 1);
}
#[tokio::test]
async fn each_external_gate_denies_before_put() {
    let (_d, a, _t, j) = fixture().await;
    let b = Mock::new(&j);
    let p = a.merge_policy().unwrap();
    let original = b.o.borrow().clone();
    let variants: Vec<fn(&mut Observation)> = vec![
        |o| o.head = "a".repeat(40),
        |o| o.base = "b".repeat(40),
        |o| o.tree = "c".repeat(40),
        |o| o.all_checks_pass = false,
        |o| o.successful_checks.clear(),
        |o| o.current_approvals = 0,
        |o| o.changes_requested = true,
        |o| o.strict_protection = false,
        |o| o.clean = false,
        |o| o.mergeable = false,
        |o| o.draft = true,
        |o| o.paths.push(".github/workflows/ci.yml".into()),
        |o| o.successful_checks[0].app_id = 999,
    ];
    for mutate in variants {
        let mut o = original.clone();
        mutate(&mut o);
        assert!(eligible(&p, &j, &o).is_err());
    }
    b.o.borrow_mut().all_checks_pass = false;
    a.run_merge_once(&b).unwrap();
    assert_eq!(*b.writes.borrow(), 0);
    assert_eq!(
        get(&a.store.connect().unwrap(), &j.id).unwrap().state,
        "blocked"
    );
}
#[tokio::test]
async fn pause_context_deadline_and_atomicity_prevent_authority_reuse() {
    let (_d, a, t, j) = fixture().await;
    let b = Mock::new(&j);
    let mut p = a.merge_policy().unwrap();
    p.revision = 2;
    p.paused = true;
    a.install_merge_policy(p, "operator", "pause").unwrap();
    a.run_merge_once(&b).unwrap();
    assert_eq!(*b.writes.borrow(), 0);
    t.advance(Duration::seconds(121));
    a.supervise_team().unwrap();
    assert_eq!(
        get(&a.store.connect().unwrap(), &j.id).unwrap().state,
        "blocked"
    );
    let (_d, a, _t, j) = fixture().await;
    let b = Mock::new(&j);
    let db = a.store.connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_merge BEFORE UPDATE ON merge_jobs BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(a.run_merge_once(&b).is_err());
    assert_eq!(*b.writes.borrow(), 0);
    assert_eq!(get(&db, &j.id).unwrap().state, "ready");
    db.execute_batch("DROP TRIGGER fail_merge").unwrap();
    a.run_merge_once(&b).unwrap();
    assert_eq!(*b.writes.borrow(), 1);
}
#[test]
fn sensitive_scope_cannot_be_overridden_by_allowed_prefix() {
    let mut p = policy("example/repo");
    p.allowed_paths = vec![
        ".github/".into(),
        "policy/".into(),
        "AGENTS.md".into(),
        "src/".into(),
    ];
    for path in [
        ".github/workflows/ci.yml",
        "policy/merge.json",
        "AGENTS.md",
        "src/auth/login.rs",
        "src/.env",
        "src/private.key",
        "src/../safe.rs",
    ] {
        assert!(!p.permits(path));
    }
    assert!(p.permits("src/fix.rs"));
}
#[tokio::test]
async fn definitive_denial_has_bounded_retry_but_uncertainty_never_does() {
    let (_d, a, _t, j) = fixture().await;
    let b = Mock::new(&j);
    b.o.borrow_mut().all_checks_pass = false;
    for attempt in 1..=3 {
        a.run_merge_once(&b).unwrap();
        let state = get(&a.store.connect().unwrap(), &j.id).unwrap();
        assert_eq!(state.attempts, attempt);
        assert_eq!(state.state, "blocked");
        let retry = Retry {
            command: OperatorCommand {
                meta: CommandMeta {
                    schema_version: V1,
                    idempotency_key: format!("retry-{attempt}"),
                    actor: j.owner.clone(),
                    reason: "Recheck definitively denied prerequisites".into(),
                },
                expected_version: a.snapshot(&j.case_id).unwrap().case.version,
            },
            job_id: j.id.clone(),
        };
        if attempt < 3 {
            let next = a.retry_merge(&j.case_id, retry.clone()).unwrap();
            assert_eq!(next.deadline, j.deadline);
            assert_eq!(a.retry_merge(&j.case_id, retry).unwrap().fence, next.fence);
        } else {
            assert!(a.retry_merge(&j.case_id, retry).is_err());
        }
    }
    assert_eq!(*b.writes.borrow(), 0);
    let (_d, a, _t, j) = fixture().await;
    let mut b = Mock::new(&j);
    b.lost = true;
    a.run_merge_once(&b).unwrap();
    let old = b.merged.borrow_mut().take();
    assert_eq!(
        a.reconcile_merge(&j.case_id, &j.id, &b).unwrap().state,
        "unknown"
    );
    *b.merged.borrow_mut() = old;
    let retry = Retry {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "unknown-retry".into(),
                actor: j.owner.clone(),
                reason: "Cannot replay uncertainty".into(),
            },
            expected_version: a.snapshot(&j.case_id).unwrap().case.version,
        },
        job_id: j.id.clone(),
    };
    assert!(a.retry_merge(&j.case_id, retry).is_err());
    assert_eq!(*b.writes.borrow(), 1);
}
