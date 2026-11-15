//! Reproducible synthetic records for operator review. Never contacts an external service.
#[path = "../tests/common/mod.rs"]
mod common;
use common::*;
use std::{fs, path::PathBuf};
use watchhouse::incident_artifacts::*;
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
    let (temporary, a, _clock, id) = fixture();
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
