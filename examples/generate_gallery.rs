//! Reproducible synthetic records for operator review. Never contacts an external service.
#[path = "../tests/common/mod.rs"]
mod common;
use chrono::{Duration, SecondsFormat};
use common::*;
use std::{fs, path::PathBuf};
use watchhouse::{
    contracts::{RecordEvidence, V1},
    domain::Clock,
    incident_artifacts::*,
    recovery::*,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("pass an empty output directory"),
    );
    if out.exists() && fs::read_dir(&out)?.next().is_some() {
        return Err("pass an empty output directory".into());
    }
    fs::create_dir_all(&out)?;
    let (temporary, a, clock, id) = fixture();
    let hypothesis = a.record_incident_artifact(
        &id,
        ArtifactCommand {
            command: command(&a, &id, "gallery-hypothesis", "operator"),
            action: ArtifactAction::Hypothesis {
                id: None,
                expected_revision: None,
                claim: "Checkout submit may exceed the dependency timeout budget".into(),
                scope: "checkout / test / fixture-release-1".into(),
                state: Finding::Proposed,
                supporting: evidence(&a, &id),
                counterevidence: vec![],
                uncertainty:
                    "Shared timeout symptoms do not establish a cause. Synthetic fixture only."
                        .into(),
                next_experiment: "Compare dependency-delay and healthy-control fixture runs".into(),
                experiment_id: None,
            },
        },
    )?;
    a.record_incident_artifact(
        &id,
        ArtifactCommand {
            command: command(&a, &id, "gallery-experiment", "operator"),
            action: ArtifactAction::PlanExperiment {
                hypothesis_id: hypothesis.id,
                hypothesis_revision: 1,
                question: "Does injected dependency latency distinguish the timeout?".into(),
                method: "Compare a delayed fixture with a healthy fixture; record both outcomes"
                    .into(),
                environment: "isolated fixture (execution not yet authorized)".into(),
                expected_outcomes: "Delay hypothesis predicts timeout only in the delayed run"
                    .into(),
                authorization_scope: "Proposal only. No production or real harness execution."
                    .into(),
                risk: "No external action is permitted by this record".into(),
                due_seconds: 3600,
            },
        },
    )?;
    a.record_incident_artifact(&id,ArtifactCommand {command:command(&a,&id,"gallery-draft","operator"),action:ArtifactAction::DraftCommunication {id:None,expected_revision:None,audience:"Internal response team".into(),body:"DRAFT — NOT SENT. We are investigating checkout timeouts in a synthetic test fixture. Three events are recorded; affected users and the traffic denominator are unknown. No mitigation, deployment or recovery is verified. Next step: collect discriminating evidence.".into(),evidence_refs:evidence(&a,&id),review_seconds:1800}})?;
    a.run_fake_worker_once("fake-investigator", true)?;
    let plan = a
        .recovery_command(
            &id,
            RecoveryCommand {
                command: command(&a, &id, "gallery-plan", "operator"),
                action: RecoveryAction::Plan {
                    criteria: Criteria {
                        workflow: "checkout submit".into(),
                        min_requests: 100,
                        min_window_seconds: 60,
                        max_error_ppm: 10000,
                        max_p95_ms: 1000,
                        max_source_lag_seconds: 30,
                    },
                    review_seconds: 3600,
                },
            },
        )?
        .recovery
        .plan
        .unwrap();
    clock.advance(Duration::seconds(61));
    let measured = recovery_report(&a, &id, "gallery-measurement", "operator")?;
    let observation = a
        .recovery_command(
            &id,
            RecoveryCommand {
                command: command(&a, &id, "gallery-observe", "operator"),
                action: RecoveryAction::Observe {
                    plan_id: plan.id,
                    window: Window {
                        started_at: plan.recorded_at,
                        ended_at: clock.now().to_rfc3339_opts(SecondsFormat::Millis, true),
                        requests: 100,
                        errors: 0,
                        p95_ms: Some(500),
                        coverage_complete: true,
                        source_lag_seconds: 0,
                        collection_method: "Invented numeric window for the UI demo".into(),
                        uncertainty: "No live measurement or production recovery".into(),
                        evidence_refs: vec![measured],
                    },
                },
            },
        )?
        .recovery
        .observation
        .unwrap();
    let reviewed = recovery_report(&a, &id, "gallery-review-proof", "reviewer")?;
    let review = a
        .recovery_command(
            &id,
            RecoveryCommand {
                command: command(&a, &id, "gallery-recovery-review", "reviewer"),
                action: RecoveryAction::Review {
                    observation_id: observation.id,
                    evidence_refs: vec![reviewed.clone()],
                },
            },
        )?
        .recovery
        .review
        .unwrap();
    a.recovery_command(&id,RecoveryCommand{command:command(&a,&id,"gallery-action","operator"),action:RecoveryAction::OpenAction{title:"Add a discriminating checkout regression".into(),category:"test".into(),owner:"operator".into(),verifier:"reviewer".into(),acceptance:"Distinguish authenticated submit failure from expected unauthenticated rejection".into(),due_seconds:3600}})?;
    a.recovery_command(
        &id,
        RecoveryCommand {
            command: command(&a, &id, "gallery-resolve", "operator"),
            action: RecoveryAction::Resolve {
                review_id: review.id,
                evidence_refs: vec![reviewed],
            },
        },
    )?;
    fs::write(out.join("recovery-example.md"), a.recovery_markdown(&id)?)?;
    fs::write(out.join("incident-example.md"), a.incident_markdown(&id)?)?;
    fs::write(
        out.join("investigation-example.md"),
        a.investigation_markdown(&id)?,
    )?;
    fs::write(
        out.join("qualification-example.md"),
        a.qualification_markdown(&id)?,
    )?;
    fs::write(
        out.join("structured-record-example.json"),
        serde_json::to_vec_pretty(&a.snapshot(&id)?)?,
    )?;
    let path = out.join("gallery.db");
    if path.exists() {
        return Err("choose an empty output directory".into());
    }
    // VACUUM INTO makes a consistent snapshot; no raw WAL copying or committed database fixture.
    let db = watchhouse::store::Store::open(temporary.path().join("db"))?.connect()?;
    db.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
    println!("{}", id);
    Ok(())
}

fn recovery_report(
    a: &watchhouse::application::Application,
    id: &str,
    key: &str,
    actor: &str,
) -> watchhouse::error::Result<String> {
    Ok(a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: actor.into(),
            reason: "Synthetic recovery gallery evidence".into(),
            expected_version: a.snapshot(id)?.case.version,
            scope: "checkout/test demo".into(),
            statement: format!("{key}: invented 100-request healthy window for demonstration only"),
            collection_method: "Manual fixture report; no production system inspected".into(),
            observed_at: None,
            uncertainty: "Synthetic data; not live recovery".into(),
        },
    )?
    .evidence
    .digest)
}
