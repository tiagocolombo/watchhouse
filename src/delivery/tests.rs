//! Synthetic remote state; these tests never call GitHub or grant live publication proof.
use super::github::Failure;
use super::{lifecycle::*, policy::PublisherPolicy};
use crate::{
    application::Application,
    contracts::*,
    domain::ManualClock,
    real_execution::pipeline::{Candidate, Review},
};
use chrono::Duration;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex};
struct Fixture {
    _root: tempfile::TempDir,
    app: Application,
    clock: ManualClock,
    c: Candidate,
    r: Review,
    policy: PublisherPolicy,
}
impl Fixture {
    async fn new() -> Self {
        let (root, app, clock, c, r) =
            crate::real_execution::tests::controller::reviewed_fixture().await;
        let p = PublisherPolicy {
            schema_version: V1,
            revision: 1,
            repository: c.repository.clone(),
            base_branch: "main".into(),
            token_env: "WATCHHOUSE_FIXTURE_PUBLISHER_TOKEN".into(),
            author_name: "Fixture".into(),
            author_email: "fixture@example.invalid".into(),
            draft: true,
            paused: false,
            job_seconds: 120,
            request_seconds: 5,
            reconciliation_attempts: 3,
            max_revisions: 2,
            retry_seconds: 2,
        };
        app.install_publisher_policy(p.clone(), "operator", "synthetic-publisher-fixture")
            .unwrap();
        Self {
            _root: root,
            app,
            clock,
            c,
            r,
            policy: p,
        }
    }
    fn command(&self, key: &str) -> Enqueue {
        Enqueue {
            command: OperatorCommand {
                meta: CommandMeta {
                    schema_version: V1,
                    idempotency_key: key.into(),
                    actor: "operator".into(),
                    reason: "Synthetic publisher verification".into(),
                },
                expected_version: self.app.snapshot(&self.c.case_id).unwrap().case.version,
            },
            candidate_id: self.c.id.clone(),
            review_id: self.r.id.clone(),
            policy_revision: 1,
        }
    }
    fn enqueue(&self, key: &str) -> Job {
        self.app
            .enqueue_publication(&self.c.case_id, self.command(key))
            .unwrap()
    }
    fn job(&self, id: &str) -> Job {
        self.app
            .publication_jobs(&self.c.case_id)
            .unwrap()
            .into_iter()
            .find(|j| j.id == id)
            .unwrap()
    }
    fn tick(&self, b: &Mock) {
        assert!(self.app.run_publisher_once(b).unwrap());
    }
}
struct Remote {
    expected: Job,
    values: BTreeMap<String, Value>,
    writes: Vec<String>,
    lose: Option<String>,
    hidden: bool,
    missing_credentials: bool,
    duplicates: bool,
    native: bool,
}
struct Mock(Mutex<Remote>);
/// Completes the adopted engineer -> independent model review -> publication fixture.
/// All remote/kernel/model observations remain injected; this is not live acceptance.
pub(crate) fn exercise_model_lineage_publication(app: &Application, c: &Candidate, r: &Review) {
    assert!(c.model_job.is_some());
    assert!(r.model_job.is_some());
    assert_ne!(c.author, r.author);
    let p:PublisherPolicy=serde_json::from_value(json!({"schema_version":1,"revision":1,"repository":c.repository,"base_branch":"main","token_env":"WATCHHOUSE_FIXTURE_PUBLISHER_TOKEN","author_name":"Fixture","author_email":"fixture@example.invalid","draft":true,"paused":false,"job_seconds":120,"request_seconds":5,"reconciliation_attempts":3,"max_revisions":2,"retry_seconds":2})).unwrap();
    app.install_publisher_policy(p, "operator", "model-to-publication-fixture")
        .unwrap();
    let j = app
        .enqueue_publication(
            &c.case_id,
            Enqueue {
                command: OperatorCommand {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "model-publication-fixture".into(),
                        actor: "operator".into(),
                        reason:
                            "Exercise exact adopted model lineage through synthetic publication"
                                .into(),
                    },
                    expected_version: app.snapshot(&c.case_id).unwrap().case.version,
                },
                candidate_id: c.id.clone(),
                review_id: r.id.clone(),
                policy_revision: 1,
            },
        )
        .unwrap();
    let b = Mock::new(&j);
    for _ in 0..4 {
        assert!(app.run_publisher_once(&b).unwrap())
    }
    let jobs = app.publication_jobs(&c.case_id).unwrap();
    assert_eq!(jobs[0].state, "simulated");
    assert_eq!(jobs[0].pr, Some(7));
    assert_eq!(
        b.0.lock()
            .unwrap()
            .writes
            .iter()
            .filter(|p| *p == "/pulls")
            .count(),
        1
    );
}
impl Mock {
    fn new(j: &Job) -> Self {
        let mut values = BTreeMap::new();
        values.insert(
            "/git/ref/heads/main".into(),
            json!({"object":{"sha":j.plan.base_commit}}),
        );
        Self(Mutex::new(Remote {
            expected: j.clone(),
            values,
            writes: vec![],
            lose: None,
            hidden: false,
            missing_credentials: false,
            duplicates: false,
            native: false,
        }))
    }
}

#[tokio::test]
async fn restore_before_publication_follows_only_existing_stages_without_any_repeated_write() {
    let f = Fixture::new().await;
    let j = f.enqueue("restore-publisher");
    let backup = f._root.path().join("backup");
    let b = f
        .app
        .create_backup(&backup, "operator", "Before original publication")
        .unwrap();
    let remote = Mock::new(&j);
    remote.0.lock().unwrap().native = true;
    for _ in 0..4 {
        f.tick(&remote);
    }
    assert_eq!(f.job(&j.id).state, "published");
    let writes = remote.0.lock().unwrap().writes.len();
    let output = f._root.path().join("restored");
    crate::restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "Read complete original publication only",
    )
    .unwrap();
    let app = Application::new(
        crate::store::Store::open(output.join("watchhouse.db")).unwrap(),
        std::sync::Arc::new(crate::domain::SystemClock),
    );
    assert!(!app.run_publisher_once(&remote).unwrap());
    app.reconcile_publication(&j.case_id, &j.id, "operator")
        .unwrap();
    for _ in 0..4 {
        assert!(app.run_publisher_once(&remote).unwrap());
    }
    let observed = app
        .publication_jobs(&j.case_id)
        .unwrap()
        .into_iter()
        .find(|v| v.id == j.id)
        .unwrap();
    assert_eq!(observed.state, "published_stale");
    assert_eq!(observed.pr, Some(7));
    assert_eq!(remote.0.lock().unwrap().writes.len(), writes);
    assert!(
        !app.snapshot(&j.case_id)
            .unwrap()
            .recovery
            .recovery
            .response_resolved
    );
}
impl Backend for Mock {
    fn native(&self) -> bool {
        self.0.lock().unwrap().native
    }
    fn get(&self, _: &PublisherPolicy, path: &str) -> super::github::Result<Option<Value>> {
        let s = self.0.lock().unwrap();
        if s.missing_credentials {
            return Err(Failure::MissingCredentials);
        }
        if path.starts_with("/pulls?") {
            let rows = if s.hidden {
                vec![]
            } else {
                s.values
                    .get("/pulls/7")
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>()
            };
            return Ok(Some(if s.duplicates {
                json!([rows[0], rows[0]])
            } else {
                json!(rows)
            }));
        }
        Ok(s.values.get(path).cloned())
    }
    fn write(
        &self,
        p: &PublisherPolicy,
        path: &str,
        body: &Value,
        update: bool,
    ) -> super::github::Result<Value> {
        let mut s = self.0.lock().unwrap();
        let j = s.expected.clone();
        s.writes.push(path.into());
        let (key, value) = match path {
            "/git/trees" => {
                assert_eq!(*body, j.plan.tree.request);
                (
                    format!("/git/trees/{}", j.plan.tree.sha),
                    json!({"sha":j.plan.tree.sha}),
                )
            }
            "/git/commits" => {
                assert_eq!(*body, j.commit_request);
                (
                    format!("/git/commits/{}", j.commit),
                    json!({"sha":j.commit}),
                )
            }
            "/git/refs" => {
                assert!(!update);
                assert_eq!(body["ref"], format!("refs/heads/{}", j.branch));
                (
                    format!("/git/ref/heads/{}", j.branch),
                    json!({"object":{"sha":body["sha"]}}),
                )
            }
            path if path.starts_with("/git/refs/heads/watchhouse/") => {
                assert!(update);
                assert_eq!(body["force"], false);
                (
                    format!("/git/ref/heads/{}", j.branch),
                    json!({"object":{"sha":body["sha"]}}),
                )
            }
            "/pulls" | "/pulls/7" => {
                if path == "/pulls" {
                    assert!(!update);
                    assert!(!s.values.contains_key("/pulls/7"));
                    assert_eq!(body["draft"], p.draft);
                } else {
                    assert!(update)
                }
                (
                    "/pulls/7".into(),
                    json!({"number":7,"state":"open","body":body["body"],"head":{"sha":j.commit,"ref":j.branch,"repo":{"full_name":p.repository}},"base":{"ref":p.base_branch,"repo":{"full_name":p.repository}}}),
                )
            }
            _ => panic!("forbidden write endpoint {path}"),
        };
        if key.starts_with("/git/ref/heads/watchhouse/")
            && let Some(pr) = s.values.get_mut("/pulls/7")
        {
            pr["head"]["sha"] = value["object"]["sha"].clone();
        }
        s.values.insert(key, value.clone());
        if s.lose.as_deref() == Some(path) {
            s.lose = None;
            Err(Failure::Unavailable)
        } else {
            Ok(value)
        }
    }
}
#[tokio::test]
async fn ordered_effects_and_revisions_reuse_one_branch_and_pr_without_force() {
    let f = Fixture::new().await;
    let cmd = f.command("publish");
    let j = f
        .app
        .enqueue_publication(&f.c.case_id, cmd.clone())
        .unwrap();
    assert_eq!(
        f.app.enqueue_publication(&f.c.case_id, cmd).unwrap().id,
        j.id
    );
    let b = Mock::new(&j);
    for _ in 0..4 {
        f.tick(&b)
    }
    assert_eq!(f.job(&j.id).state, "simulated");
    assert!(!f.job(&j.id).native);
    assert!(!f.app.run_publisher_once(&b).unwrap());
    let revision = f.enqueue("reviewed-revision");
    assert_eq!(revision.branch, j.branch);
    assert_eq!(revision.pr, Some(7));
    assert_eq!(revision.previous_head, Some(j.commit.clone()));
    assert_eq!(revision.deadline, j.deadline);
    {
        let mut s = b.0.lock().unwrap();
        s.expected = revision.clone();
    }
    // The unchanged tree already exists; the new deterministic commit and PR body do not.
    for _ in 0..4 {
        f.tick(&b)
    }
    assert_eq!(f.job(&revision.id).state, "simulated");
    assert!(
        f.app
            .enqueue_publication(&f.c.case_id, f.command("cannot-reset-revision-budget"))
            .is_err()
    );
    let view = serde_json::to_string(&f.app.snapshot(&f.c.case_id).unwrap().publications).unwrap();
    assert!(!view.contains("fence"));
    assert!(!view.contains("commit_request"));
    assert!(!view.contains("token_env"));
    let s = b.0.lock().unwrap();
    assert_eq!(s.writes.iter().filter(|p| *p == "/pulls").count(), 1);
    assert_eq!(s.writes.iter().filter(|p| *p == "/pulls/7").count(), 1);
    assert_eq!(s.writes.iter().filter(|p| *p == "/git/refs").count(), 1);
}
#[tokio::test]
async fn each_lost_write_acknowledgment_is_reconciled_without_replaying() {
    for lost in ["/git/trees", "/git/commits", "/git/refs", "/pulls"] {
        let f = Fixture::new().await;
        let j = f.enqueue("lost-ack");
        let b = Mock::new(&j);
        b.0.lock().unwrap().lose = Some(lost.into());
        for _ in 0..4 {
            f.tick(&b);
            if f.job(&j.id).state == "unknown" {
                break;
            }
        }
        assert_eq!(f.job(&j.id).state, "unknown");
        assert!(
            f.app
                .enqueue_publication(&f.c.case_id, f.command("duplicate"))
                .is_err()
        );
        for _ in 0..5 {
            if f.job(&j.id).state == "simulated" {
                break;
            }
            f.tick(&b)
        }
        assert_eq!(f.job(&j.id).state, "simulated");
        assert_eq!(
            b.0.lock()
                .unwrap()
                .writes
                .iter()
                .filter(|p| p.as_str() == lost)
                .count(),
            1
        );
        assert!(
            !f.app
                .snapshot(&f.c.case_id)
                .unwrap()
                .team
                .escalations
                .is_empty()
        );
    }
}
#[tokio::test]
async fn delayed_and_duplicate_pr_observations_never_create_a_second_pr() {
    let f = Fixture::new().await;
    let j = f.enqueue("delayed");
    let b = Mock::new(&j);
    b.0.lock().unwrap().lose = Some("/pulls".into());
    for _ in 0..4 {
        f.tick(&b)
    }
    b.0.lock().unwrap().hidden = true;
    for _ in 0..3 {
        f.clock.advance(Duration::seconds(2));
        f.tick(&b)
    }
    assert_eq!(f.job(&j.id).state, "quarantined");
    assert_eq!(
        b.0.lock()
            .unwrap()
            .writes
            .iter()
            .filter(|p| *p == "/pulls")
            .count(),
        1
    );
    b.0.lock().unwrap().hidden = false;
    b.0.lock().unwrap().duplicates = true;
    f.app
        .reconcile_publication(&f.c.case_id, &j.id, "operator")
        .unwrap();
    f.clock.advance(Duration::seconds(2));
    f.tick(&b);
    assert_eq!(f.job(&j.id).state, "quarantined");
    b.0.lock().unwrap().duplicates = false;
    f.app
        .reconcile_publication(&f.c.case_id, &j.id, "operator")
        .unwrap();
    f.tick(&b);
    assert_eq!(f.job(&j.id).pr, Some(7));
    assert_eq!(f.job(&j.id).state, "simulated");
}
#[tokio::test]
async fn pause_stale_files_and_missing_credentials_are_owned_blocks_without_writes() {
    for failure in ["pause", "files", "credentials", "base"] {
        let f = Fixture::new().await;
        let j = f.enqueue("blocked");
        let b = Mock::new(&j);
        match failure {
            "pause" => {
                let mut p = f.policy.clone();
                p.revision = 2;
                p.paused = true;
                f.app
                    .install_publisher_policy(p, "operator", "pause-fixture")
                    .unwrap();
            }
            "files" => std::fs::write(f.c.root.join("answer.txt"), "tampered").unwrap(),
            "credentials" => b.0.lock().unwrap().missing_credentials = true,
            "base" => {
                b.0.lock().unwrap().values.insert(
                    "/git/ref/heads/main".into(),
                    json!({"object":{"sha":"b".repeat(40)}}),
                );
            }
            _ => unreachable!(),
        }
        f.tick(&b);
        assert!(
            ["blocked", "quarantined"].contains(&f.job(&j.id).state.as_str()),
            "{failure}"
        );
        assert!(b.0.lock().unwrap().writes.is_empty());
        assert!(
            !f.app
                .snapshot(&f.c.case_id)
                .unwrap()
                .team
                .escalations
                .is_empty()
        );
    }
}
#[tokio::test]
async fn crashed_write_reservation_and_late_success_survive_restart_without_dispatch() {
    let f = Fixture::new().await;
    let j = f.enqueue("crash");
    let b = Mock::new(&j);
    for _ in 0..3 {
        f.tick(&b)
    }
    let mut reserved = f.job(&j.id);
    reserved.state = "writing".into();
    let db = f.app.store.connect().unwrap();
    db.execute(
        "UPDATE publication_jobs SET state='writing',record=?1 WHERE id=?2",
        rusqlite::params![serde_json::to_string(&reserved).unwrap(), j.id],
    )
    .unwrap();
    drop(db);
    f.clock.advance(Duration::seconds(121));
    for _ in 0..3 {
        f.tick(&b);
        f.clock.advance(Duration::seconds(2));
    }
    assert_eq!(f.job(&j.id).state, "quarantined");
    assert_eq!(b.0.lock().unwrap().writes.len(), 3);
    // Simulate the original PR's delayed visibility after a lost coordinator response.
    {
        let mut s = b.0.lock().unwrap();
        s.values.insert("/pulls/7".into(),json!({"number":7,"state":"open","body":super::lifecycle::pr_body(&j),"head":{"sha":j.commit,"ref":j.branch,"repo":{"full_name":f.policy.repository}},"base":{"ref":"main","repo":{"full_name":f.policy.repository}}}));
    }
    // Inject native-shaped observations to exercise stale classification; no live proof.
    b.0.lock().unwrap().native = true;
    f.app
        .reconcile_publication(&f.c.case_id, &j.id, "operator")
        .unwrap();
    f.tick(&b);
    assert_eq!(f.job(&j.id).pr, Some(7));
    assert_eq!(f.job(&j.id).state, "published_stale");
    assert_eq!(b.0.lock().unwrap().writes.len(), 3);
}
#[tokio::test]
async fn reservation_rollback_and_competing_enqueues_cannot_dispatch_extra_effects() {
    let f = Fixture::new().await;
    let cmd = f.command("rollback");
    let db = f.app.store.connect().unwrap();
    db.execute_batch("CREATE TRIGGER reject_publication BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='publication.changed' BEGIN SELECT RAISE(ABORT,'injected publication reservation failure'); END;").unwrap();
    assert!(
        f.app
            .enqueue_publication(&f.c.case_id, cmd.clone())
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM publication_slots", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM publication_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_publication").unwrap();
    drop(db);
    let jobs = std::thread::scope(|scope| {
        let a = scope.spawn(|| f.app.enqueue_publication(&f.c.case_id, cmd.clone()));
        let b = scope.spawn(|| f.app.enqueue_publication(&f.c.case_id, cmd.clone()));
        (a.join().unwrap().unwrap(), b.join().unwrap().unwrap())
    });
    assert_eq!(jobs.0.id, jobs.1.id);
    let b = Mock::new(&jobs.0);
    let db = f.app.store.connect().unwrap();
    db.execute_batch("CREATE TRIGGER reject_write BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='publication.changed' AND json_extract(NEW.envelope,'$.payload.state')='writing' BEGIN SELECT RAISE(ABORT,'injected write reservation failure'); END;").unwrap();
    drop(db);
    assert!(f.app.run_publisher_once(&b).is_err());
    assert!(b.0.lock().unwrap().writes.is_empty());
    assert_eq!(f.job(&jobs.0.id).state, "reading");
}

/// Injected native-shaped publication for merge-controller tests; never real GitHub proof.
pub(crate) async fn published_fixture() -> (tempfile::TempDir, Application, ManualClock, Job) {
    let f = Fixture::new().await;
    let j = f.enqueue("merge-controller-publication");
    let b = Mock::new(&j);
    b.0.lock().unwrap().native = true;
    for _ in 0..4 {
        f.tick(&b);
    }
    let j = f.job(&j.id);
    assert_eq!(j.state, "published");
    (f._root, f.app, f.clock, j)
}

#[tokio::test]
async fn factory_pause_between_read_and_write_and_stop_after_write_never_replay() {
    struct Hook<'a> {
        app: &'a Application,
        inner: &'a Mock,
        job: &'a Job,
        read: bool,
        called: std::cell::Cell<bool>,
    }
    impl Hook<'_> {
        fn hold(&self) {
            if self.called.replace(true) {
                return;
            }
            self.app
                .control_factory(crate::factory::Command {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "publisher-factory-hook".into(),
                        actor: "operator".into(),
                        reason: "Synthetic factory publication race".into(),
                    },
                    expected_version: 0,
                    action: crate::factory::Action::Pause {
                        scope: crate::factory::Scope::Case,
                        target: Some(self.job.case_id.clone()),
                        effect: crate::factory::Effect::Publication,
                        cancel_running: !self.read,
                    },
                })
                .unwrap();
        }
    }
    impl Backend for Hook<'_> {
        fn get(&self, p: &PublisherPolicy, path: &str) -> super::github::Result<Option<Value>> {
            let r = self.inner.get(p, path);
            if self.read {
                self.hold();
            }
            r
        }
        fn write(
            &self,
            p: &PublisherPolicy,
            path: &str,
            body: &Value,
            update: bool,
        ) -> super::github::Result<Value> {
            let r = self.inner.write(p, path, body, update);
            if !self.read {
                self.hold();
            }
            r
        }
    }
    let f = Fixture::new().await;
    let j = f.enqueue("publisher-factory-pause");
    let b = Mock::new(&j);
    let h = Hook {
        app: &f.app,
        inner: &b,
        job: &j,
        read: true,
        called: std::cell::Cell::new(false),
    };
    assert!(f.app.run_publisher_once(&h).unwrap());
    assert!(b.0.lock().unwrap().writes.is_empty());
    assert_eq!(f.job(&j.id).state, "ready");
    assert!(!f.app.run_publisher_once(&b).unwrap());
    let f = Fixture::new().await;
    let j = f.enqueue("publisher-factory-stop");
    let b = Mock::new(&j);
    let h = Hook {
        app: &f.app,
        inner: &b,
        job: &j,
        read: false,
        called: std::cell::Cell::new(false),
    };
    assert!(f.app.run_publisher_once(&h).is_err());
    assert_eq!(b.0.lock().unwrap().writes.len(), 1);
    assert_eq!(f.job(&j.id).state, "unknown");
    f.app
        .reconcile_publication(&j.case_id, &j.id, "operator")
        .unwrap();
    assert!(f.app.run_publisher_once(&b).unwrap());
    assert_eq!(b.0.lock().unwrap().writes.len(), 1);
    assert!(!f.app.run_publisher_once(&b).unwrap());
}
