mod common;
use chrono::Duration;
use common::*;
use std::cell::Cell;
use watchhouse::{
    application::Application,
    contracts::*,
    operations::{gateway::*, *},
};
fn policy() -> Policy {
    Policy {
        schema_version: V1,
        revision: 1,
        service: "checkout".into(),
        environment: "test".into(),
        repository: "example/fixture-service".into(),
        gateway: "https://operations.example.invalid".into(),
        token_env: "WATCHHOUSE_OPERATIONS_FIXTURE_TOKEN".into(),
        paused: false,
        request_seconds: 5,
        action_seconds: 120,
        max_actions: 3,
        runbooks: vec![Runbook {
            id: "rollback-canary".into(),
            target: "checkout-test".into(),
            preconditions: "Canary isolated and previous artifact confirmed".into(),
            stop_conditions: "Stop on missing health or unexpected revision".into(),
            rollback: "Gateway stops at safe checkpoint; independently inspect service".into(),
            verification: "Collect traffic and deployment independently".into(),
        }],
        observation_ids: vec!["checkout-health".into()],
    }
}
fn report(a: &Application, id: &str, key: &str, who: &str) -> String {
    a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: who.into(),
            reason: "Independent runbook fixture inspection".into(),
            expected_version: a.snapshot(id).unwrap().case.version,
            scope: "checkout/test".into(),
            statement: format!("{key}: prerequisites and rollback verified in fixture"),
            collection_method: "Fixture only".into(),
            observed_at: None,
            uncertainty: "No production action".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
fn apply(
    a: &Application,
    id: &str,
    key: &str,
    who: &str,
    action: Action,
) -> watchhouse::error::Result<watchhouse::operations::Snapshot> {
    a.operation_command(
        id,
        Command {
            command: command(a, id, key, who),
            action,
        },
    )
}
fn proposal(a: &Application, id: &str) -> Job {
    let proof = report(a, id, "preconditions", "operator");
    apply(
        a,
        id,
        "propose",
        "operator",
        Action::Propose {
            policy_revision: 1,
            runbook_id: "rollback-canary".into(),
            evidence_refs: vec![proof],
        },
    )
    .unwrap()
    .jobs[0]
        .clone()
}
fn approved(a: &Application, id: &str) -> Job {
    let j = proposal(a, id);
    let proof = report(a, id, "independent-review", "reviewer");
    apply(
        a,
        id,
        "approve",
        "reviewer",
        Action::Approve {
            job_id: j.id,
            evidence_refs: vec![proof],
        },
    )
    .unwrap()
    .jobs[0]
        .clone()
}
struct Gateway {
    writes: Cell<u32>,
    lost: bool,
    wrong: bool,
}
impl Backend for Gateway {
    fn execute(&self, _: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
        self.writes.set(self.writes.get() + 1);
        if self.lost {
            Err(Failure::Unavailable)
        } else {
            self.lookup(&policy(), j)
        }
    }
    fn lookup(&self, _: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
        Ok(ActionResult {
            operation_id: if self.wrong {
                "foreign".into()
            } else {
                j.id.clone()
            },
            runbook_id: j.runbook.id.clone(),
            target: j.runbook.target.clone(),
            state: "succeeded".into(),
        })
    }
    fn observe(
        &self,
        p: &Policy,
        _: &str,
    ) -> std::result::Result<watchhouse::operations::gateway::Observation, Failure> {
        Ok(watchhouse::operations::gateway::Observation::Telemetry {
            service: p.service.clone(),
            environment: p.environment.clone(),
            window: watchhouse::recovery::Window {
                started_at: "2026-11-15T11:59:00Z".into(),
                ended_at: "2026-11-15T12:00:00Z".into(),
                requests: 0,
                errors: 0,
                p95_ms: None,
                coverage_complete: false,
                source_lag_seconds: 120,
                collection_method: "Synthetic gateway metric fixture".into(),
                uncertainty: "Traffic unavailable".into(),
                evidence_refs: vec![],
            },
        })
    }
}
#[test]
fn loss_restart_reconciles_without_replay_or_false_recovery() {
    let (d, a, t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let j = approved(&a, &id);
    let backend = Gateway {
        writes: Cell::new(0),
        lost: true,
        wrong: false,
    };
    a.operation_tick(&backend).unwrap();
    assert_eq!(backend.writes.get(), 1);
    assert_eq!(a.snapshot(&id).unwrap().operations.jobs[0].state, "unknown");
    let a = Application::new(
        watchhouse::store::Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t),
    );
    assert!(!a.operation_tick(&backend).unwrap());
    let r = a.reconcile_operation(&id, &j.id, &backend).unwrap();
    assert_eq!(r.state, "succeeded");
    assert!(!r.native);
    assert_eq!(backend.writes.get(), 1);
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.response_resolved);
    assert!(a.reconcile_operation(&id, &j.id, &backend).is_err());
}

#[test]
fn restore_of_approved_action_reconciles_a_later_original_effect_without_execution_replay() {
    let (d, a, _t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "Restore fixture")
        .unwrap();
    let j = approved(&a, &id);
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "Before original action")
        .unwrap();
    let backend = Gateway {
        writes: Cell::new(0),
        lost: true,
        wrong: false,
    };
    a.operation_tick(&backend).unwrap();
    assert_eq!(backend.writes.get(), 1);
    let output = d.path().join("restored");
    let r = watchhouse::restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "Reconcile original action without replay",
    )
    .unwrap();
    assert_eq!(r.uncertain_external_effects, 1);
    let app = Application::new(
        watchhouse::store::Store::open(output.join("watchhouse.db")).unwrap(),
        std::sync::Arc::new(watchhouse::domain::SystemClock),
    );
    assert_eq!(
        app.snapshot(&id).unwrap().operations.jobs[0].state,
        "unknown"
    );
    assert!(!app.operation_tick(&backend).unwrap());
    let observed = app.reconcile_operation(&id, &j.id, &backend).unwrap();
    assert_eq!(observed.state, "succeeded_stale");
    assert!(!observed.native);
    assert_eq!(backend.writes.get(), 1);
    assert!(
        !app.snapshot(&id)
            .unwrap()
            .recovery
            .recovery
            .response_resolved
    );
}
#[test]
fn independent_review_policy_pause_and_corrupt_evidence_fence_actions() {
    let (_d, a, _t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let j = proposal(&a, &id);
    let proof = report(&a, &id, "self-review", "operator");
    assert!(
        apply(
            &a,
            &id,
            "self",
            "operator",
            Action::Approve {
                job_id: j.id.clone(),
                evidence_refs: vec![proof]
            }
        )
        .is_err()
    );
    let proof = report(&a, &id, "review", "reviewer");
    apply(
        &a,
        &id,
        "approve",
        "reviewer",
        Action::Approve {
            job_id: j.id,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    let mut p = policy();
    p.revision = 2;
    p.paused = true;
    a.install_operation_policy(p, "operator", "pause").unwrap();
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: false,
    };
    a.operation_tick(&b).unwrap();
    assert_eq!(b.writes.get(), 0);
    assert_eq!(a.snapshot(&id).unwrap().operations.jobs[0].state, "blocked");
    assert!(
        a.install_operation_policy(policy(), "fake-engineer", "self")
            .is_err()
    );
}
#[test]
fn wrong_result_unknown_offline_deadline_and_current_policy_read_only_reconciliation() {
    let (_d, a, t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let j = approved(&a, &id);
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: true,
    };
    a.operation_tick(&b).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().operations.jobs[0].state, "unknown");
    t.advance(Duration::seconds(121));
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: false,
    };
    let r = a.reconcile_operation(&id, &j.id, &b).unwrap();
    assert_eq!(r.state, "succeeded_stale");
    assert_eq!(b.writes.get(), 0);
}
#[test]
fn collected_zero_traffic_is_retained_and_cannot_prove_recovery() {
    let (_d, a, t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let p = a
        .recovery_command(
            &id,
            watchhouse::recovery::RecoveryCommand {
                command: command(&a, &id, "plan", "operator"),
                action: watchhouse::recovery::RecoveryAction::Plan {
                    criteria: watchhouse::recovery::Criteria {
                        workflow: "checkout".into(),
                        min_requests: 100,
                        min_window_seconds: 60,
                        max_error_ppm: 10000,
                        max_p95_ms: 1000,
                        max_source_lag_seconds: 30,
                    },
                    review_seconds: 300,
                },
            },
        )
        .unwrap()
        .recovery
        .plan
        .unwrap();
    let cmd = Command {
        command: command(&a, &id, "collect", "operator"),
        action: Action::Collect {
            policy_revision: 1,
            observation_id: "checkout-health".into(),
        },
    };
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: false,
    };
    let o = a
        .collect_operation_observation(&id, cmd.clone(), &b)
        .unwrap();
    assert!(!o.native);
    assert_eq!(
        a.collect_operation_observation(&id, cmd, &b).unwrap().id,
        o.id
    );
    // Retained pre-plan telemetry is rejected even if copied as a recovery observation.
    t.advance(Duration::seconds(61));
    let proof = report(&a, &id, "recovery-proof", "operator");
    let result = a.recovery_command(
        &id,
        watchhouse::recovery::RecoveryCommand {
            command: command(&a, &id, "observe", "operator"),
            action: watchhouse::recovery::RecoveryAction::ObserveCollected {
                plan_id: p.id,
                observation_id: o.id,
                evidence_refs: vec![proof],
            },
        },
    );
    assert!(result.is_err());
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.response_resolved);
    assert_eq!(b.writes.get(), 0);
}
#[test]
fn problem_duty_survives_restart_and_refuses_invented_cause() {
    let (d, a, t, id) = fixture();
    let proof = report(&a, &id, "problem-proof", "operator");
    let action = |cause: &str| Action::Problem {
        id: None,
        expected_revision: 0,
        title: "Checkout intermittent timeout".into(),
        known_error: true,
        cause_status: cause.into(),
        hypothesis_id: None,
        hypothesis_revision: None,
        workaround: "Route through verified alternate checkout".into(),
        invalidation: "Any changed source/release or recurring failures".into(),
        owner: "problem-owner".into(),
        review_seconds: 10,
        related_cases: vec![],
        evidence_refs: vec![proof.clone()],
    };
    assert!(
        apply(
            &a,
            &id,
            "invented",
            "operator",
            action("independently_supported")
        )
        .is_err()
    );
    apply(&a, &id, "problem", "operator", action("unknown")).unwrap();
    t.advance(Duration::seconds(11));
    let a = Application::new(
        watchhouse::store::Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t),
    );
    a.supervise_team().unwrap();
    assert!(a.snapshot(&id).unwrap().operations.problems[0].escalated);
    let count = a.escalation_inbox().unwrap().len();
    a.supervise_team().unwrap();
    assert_eq!(a.escalation_inbox().unwrap().len(), count);
}
#[test]
fn corrupt_review_evidence_blocks_dispatch_and_rollback_keeps_no_proposal() {
    let (d, a, _t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let proof = report(&a, &id, "preconditions", "operator");
    let cmd = Command {
        command: command(&a, &id, "atomic-propose", "operator"),
        action: Action::Propose {
            policy_revision: 1,
            runbook_id: "rollback-canary".into(),
            evidence_refs: vec![proof],
        },
    };
    let db = watchhouse::store::Store::open(d.path().join("db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fail_operation BEFORE INSERT ON operation_jobs BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(a.operation_command(&id, cmd.clone()).is_err());
    assert!(a.snapshot(&id).unwrap().operations.jobs.is_empty());
    db.execute_batch("DROP TRIGGER fail_operation").unwrap();
    let r = a.operation_command(&id, cmd.clone()).unwrap();
    assert_eq!(
        a.operation_command(&id, cmd).unwrap().jobs[0].id,
        r.jobs[0].id
    );
    let proof = report(&a, &id, "review-integrity", "reviewer");
    apply(
        &a,
        &id,
        "approve",
        "reviewer",
        Action::Approve {
            job_id: r.jobs[0].id.clone(),
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap();
    db.execute(
        "UPDATE evidence SET content='corrupt' WHERE digest=?1",
        [proof],
    )
    .unwrap();
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: false,
    };
    a.operation_tick(&b).unwrap();
    assert_eq!(b.writes.get(), 0);
    assert_eq!(a.snapshot(&id).unwrap().operations.jobs[0].state, "blocked");
}

#[test]
fn factory_pause_stops_new_runbook_effects_and_stop_fences_inflight_acknowledgments() {
    struct Stopping<'a> {
        app: &'a Application,
        inner: &'a Gateway,
    }
    impl Backend for Stopping<'_> {
        fn execute(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
            let r = self.inner.execute(p, j);
            self.app
                .control_factory(watchhouse::factory::Command {
                    meta: CommandMeta {
                        schema_version: V1,
                        idempotency_key: "stop-runbook-inflight".into(),
                        actor: "operator".into(),
                        reason: "Synthetic stop while gateway effect is in flight".into(),
                    },
                    expected_version: 0,
                    action: watchhouse::factory::Action::Pause {
                        scope: watchhouse::factory::Scope::Case,
                        target: Some(j.case_id.clone()),
                        effect: watchhouse::factory::Effect::Operation,
                        cancel_running: true,
                    },
                })
                .unwrap();
            r
        }
        fn lookup(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
            self.inner.lookup(p, j)
        }
        fn observe(
            &self,
            p: &Policy,
            id: &str,
        ) -> std::result::Result<watchhouse::operations::gateway::Observation, Failure> {
            self.inner.observe(p, id)
        }
    }
    let (_d, a, _t, id) = fixture();
    a.install_operation_policy(policy(), "operator", "fixture")
        .unwrap();
    let j = approved(&a, &id);
    let b = Gateway {
        writes: Cell::new(0),
        lost: false,
        wrong: false,
    };
    assert!(a.operation_tick(&Stopping { app: &a, inner: &b }).is_err());
    assert_eq!(b.writes.get(), 1);
    assert_eq!(a.snapshot(&id).unwrap().operations.jobs[0].state, "unknown");
    assert!(!a.operation_tick(&b).unwrap());
    a.reconcile_operation(&id, &j.id, &b).unwrap();
    assert_eq!(b.writes.get(), 1);
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.response_resolved);
}
