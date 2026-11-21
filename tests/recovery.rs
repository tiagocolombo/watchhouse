mod common;
use chrono::{Duration, SecondsFormat};
use common::*;
use watchhouse::{application::Application, contracts::*, recovery::*, store::Store};
fn apply(
    a: &Application,
    id: &str,
    key: &str,
    who: &str,
    action: RecoveryAction,
) -> watchhouse::error::Result<RecoverySnapshot> {
    a.recovery_command(
        id,
        RecoveryCommand {
            command: command(a, id, key, who),
            action,
        },
    )
}
fn report(a: &Application, id: &str, key: &str, who: &str) -> String {
    a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: who.into(),
            reason: "Report sanitized manual observation".into(),
            expected_version: a.snapshot(id).unwrap().case.version,
            scope: "checkout/test".into(),
            statement: format!("{key}: synthetic manual test result; not live telemetry"),
            collection_method: "Independent fixture inspection".into(),
            observed_at: None,
            uncertainty: "Fixture only".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
fn criteria() -> Criteria {
    Criteria {
        workflow: "checkout submit".into(),
        min_requests: 100,
        min_window_seconds: 60,
        max_error_ppm: 10000,
        max_p95_ms: 1000,
        max_source_lag_seconds: 30,
    }
}
fn plan(a: &Application, id: &str) -> Plan {
    apply(
        a,
        id,
        "plan",
        "operator",
        RecoveryAction::Plan {
            criteria: criteria(),
            review_seconds: 300,
        },
    )
    .unwrap()
    .recovery
    .plan
    .unwrap()
}
fn window(p: &Plan, end: String, refs: Vec<String>) -> Window {
    Window {
        started_at: p.recorded_at.clone(),
        ended_at: end,
        requests: 100,
        errors: 0,
        p95_ms: Some(500),
        coverage_complete: true,
        source_lag_seconds: 0,
        collection_method: "Manual fixture measurement".into(),
        uncertainty: "Operator-reported, not a live integration".into(),
        evidence_refs: refs,
    }
}
fn met(a: &Application, id: &str, t: &watchhouse::domain::ManualClock) -> Observation {
    let p = plan(a, id);
    t.advance(Duration::seconds(61));
    let refs = vec![report(a, id, "measurement", "operator")];
    apply(
        a,
        id,
        "observe",
        "operator",
        RecoveryAction::Observe {
            plan_id: p.id.clone(),
            window: window(
                &p,
                t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
                refs,
            ),
        },
    )
    .unwrap()
    .recovery
    .observation
    .unwrap()
}
use watchhouse::domain::Clock;
#[test]
fn evidence_removal_invalidates_the_plan_and_requires_a_new_prospective_plan() {
    let (_d, a, t, id) = fixture();
    let observation = met(&a, &id, &t);
    let old_plan = a.snapshot(&id).unwrap().recovery.recovery.plan.unwrap();
    a.redact_evidence(
        &observation.window.evidence_refs[0],
        "operator",
        "Remove the synthetic recovery measurement body",
    )
    .unwrap();
    let current = a.snapshot(&id).unwrap().recovery.recovery;
    assert!(!current.context_current);
    assert!(!current.response_resolved);
    assert!(current.review.is_none());
    let proof = report(&a, &id, "post-redaction-review", "reviewer");
    assert!(
        apply(
            &a,
            &id,
            "cannot-review-old-plan",
            "reviewer",
            RecoveryAction::Review {
                observation_id: observation.id,
                evidence_refs: vec![proof]
            }
        )
        .is_err()
    );
    let new = apply(
        &a,
        &id,
        "fresh-prospective-plan",
        "operator",
        RecoveryAction::Plan {
            criteria: criteria(),
            review_seconds: 300,
        },
    )
    .unwrap()
    .recovery;
    assert!(new.context_current);
    assert_ne!(new.plan.unwrap().id, old_plan.id);
    assert!(new.observation.is_none());
    assert!(
        a.factory_status()
            .unwrap()
            .pauses
            .iter()
            .any(|p| p["target"] == id)
    );
}
#[test]
fn reported_recovery_requires_planned_window_independent_review_and_keeps_followups() {
    let (_d, a, t, id) = fixture();
    let o = met(&a, &id, &t);
    assert_eq!(o.outcome, Outcome::Met);
    assert!(
        apply(
            &a,
            &id,
            "self-review",
            "operator",
            RecoveryAction::Review {
                observation_id: o.id.clone(),
                evidence_refs: o.window.evidence_refs.clone()
            }
        )
        .is_err()
    );
    let proof = report(&a, &id, "review-observation", "reviewer");
    let r = apply(
        &a,
        &id,
        "review",
        "reviewer",
        RecoveryAction::Review {
            observation_id: o.id,
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap()
    .recovery
    .review
    .unwrap();
    let open = apply(
        &a,
        &id,
        "followup",
        "operator",
        RecoveryAction::OpenAction {
            title: "Regression monitor".into(),
            category: "detect".into(),
            owner: "operator".into(),
            verifier: "reviewer".into(),
            acceptance: "Alert on a failing authenticated request".into(),
            due_seconds: 30,
        },
    )
    .unwrap()
    .actions
    .remove(0);
    let done = apply(
        &a,
        &id,
        "resolve",
        "operator",
        RecoveryAction::Resolve {
            review_id: r.id,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    assert!(done.recovery.response_resolved);
    assert_eq!(done.actions[0].state, "open");
    t.advance(Duration::seconds(31));
    a.supervise_team().unwrap();
    let s = a.snapshot(&id).unwrap();
    assert!(s.recovery.recovery.response_resolved);
    assert!(s.recovery.actions[0].escalated);
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_id == open.id)
    );
    assert!(
        a.recovery_markdown(&id)
            .unwrap()
            .contains("operator_reported_review")
    );
}
#[test]
fn low_traffic_missing_coverage_lag_or_latency_cannot_establish_recovery() {
    for n in 0..5 {
        let (_d, a, t, id) = fixture();
        let p = plan(&a, &id);
        t.advance(Duration::seconds(61));
        let refs = vec![report(&a, &id, "reported", "operator")];
        let mut w = window(
            &p,
            t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
            refs,
        );
        match n {
            0 => w.requests = 0,
            1 => w.coverage_complete = false,
            2 => w.source_lag_seconds = 31,
            3 => w.p95_ms = None,
            _ => {
                w.errors = 100;
                w.p95_ms = Some(2000);
            }
        }
        let o = apply(
            &a,
            &id,
            "observe",
            "operator",
            RecoveryAction::Observe {
                plan_id: p.id,
                window: w,
            },
        )
        .unwrap()
        .recovery
        .observation
        .unwrap();
        assert_eq!(
            o.outcome,
            if n == 4 {
                Outcome::Ineffective
            } else {
                Outcome::Inconclusive
            }
        );
        assert!(
            apply(
                &a,
                &id,
                "resolve",
                "operator",
                RecoveryAction::Resolve {
                    review_id: "missing".into(),
                    evidence_refs: evidence(&a, &id)
                }
            )
            .is_err()
        );
    }
}
#[test]
fn action_completion_needs_assigned_independent_new_proof_and_waivers_keep_risk() {
    let (_d, a, _t, id) = fixture();
    let action = apply(
        &a,
        &id,
        "open",
        "operator",
        RecoveryAction::OpenAction {
            title: "Add regression".into(),
            category: "test".into(),
            owner: "engineer".into(),
            verifier: "reviewer".into(),
            acceptance: "Fails on broken fixture".into(),
            due_seconds: 60,
        },
    )
    .unwrap()
    .actions
    .remove(0);
    let owner = report(&a, &id, "engineer-proof", "engineer");
    let submitted = apply(
        &a,
        &id,
        "submit",
        "engineer",
        RecoveryAction::SubmitAction {
            id: action.id.clone(),
            revision: 1,
            evidence_refs: vec![owner.clone()],
        },
    )
    .unwrap()
    .actions
    .remove(0);
    assert!(
        apply(
            &a,
            &id,
            "bad-verify",
            "engineer",
            RecoveryAction::VerifyAction {
                id: action.id.clone(),
                revision: submitted.revision,
                evidence_refs: vec![owner.clone()]
            }
        )
        .is_err()
    );
    assert!(
        apply(
            &a,
            &id,
            "same-proof",
            "reviewer",
            RecoveryAction::VerifyAction {
                id: action.id.clone(),
                revision: submitted.revision,
                evidence_refs: vec![owner]
            }
        )
        .is_err()
    );
    let proof = report(&a, &id, "verifier-proof", "reviewer");
    let verified = apply(
        &a,
        &id,
        "verify",
        "reviewer",
        RecoveryAction::VerifyAction {
            id: action.id,
            revision: submitted.revision,
            evidence_refs: vec![proof],
        },
    )
    .unwrap()
    .actions
    .remove(0);
    assert_eq!(verified.state, "verified");
    let other = apply(
        &a,
        &id,
        "other",
        "operator",
        RecoveryAction::OpenAction {
            title: "Runbook".into(),
            category: "runbook".into(),
            owner: "engineer".into(),
            verifier: "reviewer".into(),
            acceptance: "Document fallback".into(),
            due_seconds: 60,
        },
    )
    .unwrap()
    .actions
    .remove(0);
    let proof = report(&a, &id, "risk", "operator");
    let waived = apply(
        &a,
        &id,
        "waive",
        "operator",
        RecoveryAction::WaiveAction {
            id: other.id,
            revision: 1,
            residual_risk: "Manual response remains slower".into(),
            evidence_refs: vec![proof],
        },
    )
    .unwrap()
    .actions
    .remove(0);
    assert_eq!(waived.state, "waived");
    assert!(waived.residual_risk.is_some());
}
#[test]
fn plan_expiry_stale_commands_foreign_evidence_and_deployments_do_not_bypass_gates() {
    let (_d, a, t, id) = fixture();
    let p = plan(&a, &id);
    let stale = RecoveryCommand {
        command: command(&a, &id, "stale", "operator"),
        action: RecoveryAction::Reopen {
            evidence_refs: evidence(&a, &id),
        },
    };
    let proof = report(&a, &id, "deployment", "operator");
    let before = a.snapshot(&id).unwrap().recovery.recovery;
    apply(
        &a,
        &id,
        "deploy",
        "operator",
        RecoveryAction::RecordDeployment {
            repository: "example/checkout".into(),
            revision: "a".repeat(40),
            environment: "production".into(),
            observed_at: t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
            evidence_refs: vec![proof],
        },
    )
    .unwrap_err();
    assert_eq!(
        a.snapshot(&id).unwrap().recovery.recovery.response_resolved,
        before.response_resolved
    );
    assert!(a.recovery_command(&id, stale).is_err());
    t.advance(Duration::seconds(301));
    a.supervise_team().unwrap();
    a.supervise_team().unwrap();
    assert_eq!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .filter(|e| e.source_id == p.id)
            .count(),
        1
    );
    let w = window(
        &p,
        t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
        evidence(&a, &id),
    );
    assert!(
        apply(
            &a,
            &id,
            "expired",
            "operator",
            RecoveryAction::Observe {
                plan_id: p.id,
                window: w
            }
        )
        .is_err()
    );
}
#[test]
fn rollback_restart_and_immutable_record_history_preserve_recovery() {
    let (d, a, t, id) = fixture();
    let p = plan(&a, &id);
    let store = Store::open(d.path().join("db")).unwrap();
    let db = store.connect().unwrap();
    assert!(
        db.execute("UPDATE recovery_records SET kind='changed'", [])
            .is_err()
    );
    assert!(db.execute("DELETE FROM recovery_records", []).is_err());
    let cmd = RecoveryCommand {
        command: command(&a, &id, "retry", "operator"),
        action: RecoveryAction::OpenAction {
            title: "Test rollback".into(),
            category: "test".into(),
            owner: "operator".into(),
            verifier: "reviewer".into(),
            acceptance: "Retain duty".into(),
            due_seconds: 1,
        },
    };
    db.execute_batch("CREATE TRIGGER fail_recovery BEFORE INSERT ON recovery_records BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(a.recovery_command(&id, cmd.clone()).is_err());
    assert!(a.snapshot(&id).unwrap().recovery.actions.is_empty());
    db.execute_batch("DROP TRIGGER fail_recovery").unwrap();
    let first = a.recovery_command(&id, cmd.clone()).unwrap();
    assert_eq!(
        a.recovery_command(&id, cmd).unwrap().actions[0].id,
        first.actions[0].id
    );
    let b = Application::new(store, std::sync::Arc::new(t.clone()));
    assert_eq!(
        b.snapshot(&id).unwrap().recovery.recovery.plan.unwrap().id,
        p.id
    );
    t.advance(Duration::seconds(2));
    b.supervise_team().unwrap();
    assert!(b.snapshot(&id).unwrap().recovery.actions[0].escalated);
}
#[test]
fn mitigation_is_only_a_blocked_proposal_with_owned_escalation() {
    let (_d, a, _t, id) = fixture();
    let result = apply(
        &a,
        &id,
        "mitigate",
        "operator",
        RecoveryAction::ProposeMitigation {
            runbook: "restart@1".into(),
            target: "checkout/test".into(),
            steps: "<script>untrusted</script> never executed".into(),
            preconditions: "Read-only fixture".into(),
            stop_conditions: "Any unexpected output".into(),
            rollback: "Restore previous configuration".into(),
            risk: "No execution authorized".into(),
            evidence_refs: evidence(&a, &id),
        },
    )
    .unwrap();
    assert_eq!(result.records[0]["data"]["executed"], false);
    assert!(!result.recovery.response_resolved);
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_type == "mitigation_authority")
    );
    assert!(!a.recovery_markdown(&id).unwrap().contains("<script>"));
}
#[test]
fn later_context_changes_reopen_response_and_new_plan_invalidates_review() {
    let (_d, a, t, id) = fixture();
    let o = met(&a, &id, &t);
    let proof = report(&a, &id, "review-proof", "reviewer");
    let review = apply(
        &a,
        &id,
        "review",
        "reviewer",
        RecoveryAction::Review {
            observation_id: o.id,
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap()
    .recovery
    .review
    .unwrap();
    apply(
        &a,
        &id,
        "resolve",
        "operator",
        RecoveryAction::Resolve {
            review_id: review.id,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    a.operate_incident(
        &id,
        watchhouse::incident_ops::IncidentOperation {
            command: command(&a, &id, "context-change", "operator"),
            action: watchhouse::incident_ops::IncidentAction::Update {
                severity: watchhouse::incidents::Severity::Critical,
                impact: "Recurrence requires new assessment".into(),
                uncertainty: "New context".into(),
                objective: "Reassess".into(),
                evidence_refs: evidence(&a, &id),
            },
        },
    )
    .unwrap();
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.context_current);
    a.supervise_team().unwrap();
    assert!(!a.snapshot(&id).unwrap().recovery.recovery.response_resolved);
    assert!(a.snapshot(&id).unwrap().recovery.recovery.review.is_none());
    a.supervise_team().unwrap();
    assert_eq!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .filter(|e| e.source_type == "recovery_reassessment")
            .count(),
        1
    );
}
#[test]
fn partial_observation_and_early_or_synthetic_only_reports_never_pass() {
    let (_d, a, t, id) = fixture();
    let p = plan(&a, &id);
    t.advance(Duration::seconds(61));
    let mut w = window(
        &p,
        t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
        evidence(&a, &id),
    );
    assert!(
        apply(
            &a,
            &id,
            "source-only",
            "operator",
            RecoveryAction::Observe {
                plan_id: p.id.clone(),
                window: w.clone()
            }
        )
        .is_err()
    );
    w.evidence_refs = vec![report(&a, &id, "report", "operator")];
    w.errors = 5;
    let observed = apply(
        &a,
        &id,
        "partial",
        "operator",
        RecoveryAction::Observe {
            plan_id: p.id,
            window: w,
        },
    )
    .unwrap();
    assert_eq!(
        observed.recovery.observation.unwrap().outcome,
        Outcome::Partial
    );
}
