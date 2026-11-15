mod common;
use chrono::Duration;
use common::*;
use watchhouse::{
    application::Application, incident_artifacts::*, incident_ops::*, incidents::Severity,
    store::Store,
};
fn apply(
    a: &Application,
    id: &str,
    key: &str,
    actor: &str,
    action: ArtifactAction,
) -> watchhouse::error::Result<Artifact> {
    a.record_incident_artifact(
        id,
        ArtifactCommand {
            command: command(a, id, key, actor),
            action,
        },
    )
}
fn hypothesis(
    id: Option<String>,
    revision: Option<u32>,
    state: Finding,
    experiment_id: Option<String>,
    refs: Vec<String>,
) -> ArtifactAction {
    ArtifactAction::Hypothesis {
        id,
        expected_revision: revision,
        claim: "The database causes fixture latency".into(),
        scope: "fixture checkout / test".into(),
        state,
        supporting: vec![],
        counterevidence: refs,
        uncertainty: "No production conclusion".into(),
        next_experiment: "Compare a discriminating fixture".into(),
        experiment_id,
    }
}
fn plan(h: &Artifact, seconds: u32) -> ArtifactAction {
    ArtifactAction::PlanExperiment {
        hypothesis_id: h.id.clone(),
        hypothesis_revision: h.revision,
        question: "Does latency persist without database access?".into(),
        method: "Compare sanitized fixture runs".into(),
        environment: "test".into(),
        expected_outcomes: "Database hypothesis predicts only the database path is slow".into(),
        authorization_scope: "Read-only proposal; no execution authority".into(),
        risk: "No external effect".into(),
        due_seconds: seconds,
    }
}
fn draft(id: Option<String>, revision: Option<u32>, refs: Vec<String>) -> ArtifactAction {
    ArtifactAction::DraftCommunication {
        id,
        expected_revision: revision,
        audience: "internal responders".into(),
        body: "<script>untrusted draft</script> Impact remains under investigation.".into(),
        evidence_refs: refs,
        review_seconds: 30,
    }
}
#[test]
fn experiments_bind_exact_claims_and_failed_hypotheses_remain_in_history() {
    let (_d, a, _t, id) = fixture();
    let h = apply(
        &a,
        &id,
        "hyp",
        "operator",
        hypothesis(None, None, Finding::Proposed, None, vec![]),
    )
    .unwrap();
    assert!(
        apply(
            &a,
            &id,
            "unsupported",
            "operator",
            hypothesis(
                Some(h.id.clone()),
                Some(1),
                Finding::Supported,
                None,
                evidence(&a, &id)
            )
        )
        .is_err()
    );
    let exp = apply(&a, &id, "plan", "operator", plan(&h, 30)).unwrap();
    let complete = apply(
        &a,
        &id,
        "result",
        "operator",
        ArtifactAction::FinishExperiment {
            id: exp.id.clone(),
            expected_revision: 1,
            outcome: Finding::Refuted,
            evidence_refs: evidence(&a, &id),
            interpretation: "The fixture is slow on both paths".into(),
            uncertainty: "Fixture only".into(),
        },
    )
    .unwrap();
    assert_eq!(complete.revision, 2);
    let mut changed = hypothesis(
        Some(h.id.clone()),
        Some(1),
        Finding::Refuted,
        Some(exp.id.clone()),
        evidence(&a, &id),
    );
    if let ArtifactAction::Hypothesis { claim, .. } = &mut changed {
        *claim = "An unrelated new causal claim".into()
    }
    assert!(apply(&a, &id, "changed-claim", "operator", changed).is_err());
    let refuted = apply(
        &a,
        &id,
        "refuted",
        "operator",
        hypothesis(
            Some(h.id.clone()),
            Some(1),
            Finding::Refuted,
            Some(exp.id),
            evidence(&a, &id),
        ),
    )
    .unwrap();
    assert_eq!(refuted.revision, 2);
    let md = a.investigation_markdown(&id).unwrap();
    assert!(md.contains("Refuted"));
    assert!(md.contains("No production conclusion"));
    assert!(
        apply(
            &a,
            &id,
            "stale-hyp",
            "operator",
            hypothesis(Some(h.id), Some(1), Finding::Testing, None, vec![])
        )
        .is_err()
    );
}
#[test]
fn communication_review_is_independent_and_exact_context_bound_without_delivery() {
    let (_d, a, t, id) = fixture();
    let own = apply(
        &a,
        &id,
        "own-draft",
        "operator",
        draft(None, None, evidence(&a, &id)),
    )
    .unwrap();
    assert!(
        apply(
            &a,
            &id,
            "self-approve",
            "operator",
            ArtifactAction::ApproveCommunication {
                id: own.id,
                revision: 1,
                evidence_refs: evidence(&a, &id)
            }
        )
        .is_err()
    );
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "offer-writer", "operator"),
            action: IncidentAction::OfferRole {
                role: IncidentRole::Communications,
                successor: "writer".into(),
                acknowledge_seconds: 10,
            },
        },
    )
    .unwrap();
    let offer = a.snapshot(&id).unwrap().incident_role_offers[0].id.clone();
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "accept-writer", "writer"),
            action: IncidentAction::AcceptRole { offer_id: offer },
        },
    )
    .unwrap();
    let d = apply(
        &a,
        &id,
        "draft",
        "writer",
        draft(None, None, evidence(&a, &id)),
    )
    .unwrap();
    t.advance(Duration::seconds(31));
    let approve = ArtifactCommand {
        command: command(&a, &id, "approve", "operator"),
        action: ArtifactAction::ApproveCommunication {
            id: d.id.clone(),
            revision: d.revision,
            evidence_refs: evidence(&a, &id),
        },
    };
    let approved = a.record_incident_artifact(&id, approve.clone()).unwrap();
    assert!(approved.approval_current);
    assert!(
        a.record_incident_artifact(&id, approve)
            .unwrap()
            .approval_current
    );
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "context-change", "operator"),
            action: IncidentAction::Update {
                severity: Severity::High,
                impact: "Updated impact".into(),
                uncertainty: "Unknown recovery".into(),
                objective: "Review changed context".into(),
                evidence_refs: evidence(&a, &id),
            },
        },
    )
    .unwrap();
    assert!(
        !a.snapshot(&id)
            .unwrap()
            .incident_artifacts
            .iter()
            .find(|a| a.id == d.id)
            .unwrap()
            .approval_current
    );
    let revised = apply(
        &a,
        &id,
        "revise",
        "writer",
        draft(Some(d.id.clone()), Some(1), evidence(&a, &id)),
    )
    .unwrap();
    assert_eq!(revised.revision, 2);
    assert!(revised.approval.is_none());
    assert!(
        apply(
            &a,
            &id,
            "stale-review",
            "operator",
            ArtifactAction::ApproveCommunication {
                id: d.id,
                revision: 1,
                evidence_refs: evidence(&a, &id)
            }
        )
        .is_err()
    );
    assert!(a.snapshot(&id).unwrap().notifications.is_empty());
}
#[test]
fn artifact_duties_escalate_without_models_and_late_completion_is_not_resolution() {
    let (_d, a, t, id) = fixture();
    let h = apply(
        &a,
        &id,
        "h",
        "operator",
        hypothesis(None, None, Finding::Proposed, None, vec![]),
    )
    .unwrap();
    let exp = apply(&a, &id, "plan", "operator", plan(&h, 1)).unwrap();
    a.run_fake_worker_once("fake-investigator", false).unwrap();
    let verifier=a.record_evidence(&id,serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":"verifier-proof","actor":"fake-verifier","reason":"Independent fixture assessment","expected_version":a.snapshot(&id).unwrap().case.version,"scope":"fixture","statement":"Current fixture observations checked","collection_method":"Separate observation","observed_at":null,"uncertainty":"Investigation duty remains open"})).unwrap()).unwrap().evidence.digest;
    a.record_gate(
        &id,
        watchhouse::challenges::GateCommand {
            command: command(&a, &id, "validate", "fake-verifier"),
            close: false,
            evidence_refs: vec![verifier.clone()],
        },
    )
    .unwrap();
    assert!(
        a.record_gate(
            &id,
            watchhouse::challenges::GateCommand {
                command: command(&a, &id, "close", "operator"),
                close: true,
                evidence_refs: vec![verifier]
            }
        )
        .is_err()
    );
    t.advance(Duration::seconds(2));
    a.supervise_team().unwrap();
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_type == "incident_artifact")
    );
    assert_eq!(a.supervise_team().unwrap(), 0);
    apply(
        &a,
        &id,
        "finish",
        "operator",
        ArtifactAction::FinishExperiment {
            id: exp.id.clone(),
            expected_revision: 1,
            outcome: Finding::Inconclusive,
            evidence_refs: evidence(&a, &id),
            interpretation: "Traffic insufficient".into(),
            uncertainty: "No discriminating outcome".into(),
        },
    )
    .unwrap();
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_type == "incident_artifact" && e.state == "open")
    );
    assert!(
        apply(
            &a,
            &id,
            "finish-twice",
            "operator",
            ArtifactAction::FinishExperiment {
                id: exp.id,
                expected_revision: 2,
                outcome: Finding::Supported,
                evidence_refs: evidence(&a, &id),
                interpretation: "Cannot overwrite".into(),
                uncertainty: "Unknown".into()
            }
        )
        .is_err()
    );
}
#[test]
fn artifact_failures_and_competing_versions_leave_no_partial_obligation() {
    let (d, a, _t, id) = fixture();
    let h = apply(
        &a,
        &id,
        "h",
        "operator",
        hypothesis(None, None, Finding::Proposed, None, vec![]),
    )
    .unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_artifact BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='incident.artifact_recorded' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;").unwrap();
    let cmd = ArtifactCommand {
        command: command(&a, &id, "atomic-plan", "operator"),
        action: plan(&h, 30),
    };
    assert!(a.record_incident_artifact(&id, cmd.clone()).is_err());
    assert_eq!(a.snapshot(&id).unwrap().incident_artifacts.len(), 1);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM artifact_obligations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_artifact").unwrap();
    a.record_incident_artifact(&id, cmd).unwrap();
    let x = ArtifactCommand {
        command: command(&a, &id, "one", "operator"),
        action: hypothesis(Some(h.id.clone()), Some(1), Finding::Testing, None, vec![]),
    };
    let y = ArtifactCommand {
        command: command(&a, &id, "two", "operator"),
        action: hypothesis(Some(h.id), Some(1), Finding::Inconclusive, None, vec![]),
    };
    let b = a.clone();
    let c = a.clone();
    let i = id.clone();
    let j = id.clone();
    let one = std::thread::spawn(move || b.record_incident_artifact(&i, x));
    let two = std::thread::spawn(move || c.record_incident_artifact(&j, y));
    assert_eq!(
        [one.join().unwrap().is_ok(), two.join().unwrap().is_ok()]
            .into_iter()
            .filter(|x| *x)
            .count(),
        1
    );
}
#[test]
fn fake_roles_and_foreign_evidence_are_not_admitted_and_records_are_immutable() {
    let (d, a, _t, id) = fixture();
    assert!(
        apply(
            &a,
            &id,
            "fake",
            "fake-investigator",
            hypothesis(None, None, Finding::Proposed, None, vec![])
        )
        .is_err()
    );
    let other = a
        .create_case(watchhouse::intake::FakeSource.manual_command("other-case"))
        .unwrap();
    let proof=a.record_evidence(&other.id,serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":"other-evidence","actor":"operator","reason":"Separate scope","expected_version":other.version,"scope":"other","statement":"Other case proof","collection_method":"Fixture","observed_at":null,"uncertainty":"Separate case"})).unwrap()).unwrap().evidence.digest;
    assert!(
        apply(
            &a,
            &id,
            "foreign",
            "operator",
            hypothesis(None, None, Finding::Refuted, None, vec![proof])
        )
        .is_err()
    );
    let h = apply(
        &a,
        &id,
        "h",
        "operator",
        hypothesis(None, None, Finding::Proposed, None, vec![]),
    )
    .unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert!(
        db.execute(
            "UPDATE incident_artifacts SET record='{}' WHERE id=?1",
            [h.id]
        )
        .is_err()
    );
    assert!(!evidence(&a, &id).is_empty());
}
