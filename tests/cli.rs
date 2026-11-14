use std::process::{Command, Output};
use tempfile::TempDir;
fn run(db: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn cli_uses_durable_application_services_across_processes() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("cli.db");
    assert!(run(&db, &["migrate"]).status.success());
    let args = [
        "case",
        "create",
        "--title",
        "CLI fixture",
        "--key",
        "cli-fixture-1",
    ];
    let first = run(&db, &args);
    assert!(first.status.success());
    let value: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    let id = value["id"].as_str().unwrap();
    assert_eq!(first.stdout, run(&db, &args).stdout);
    assert!(run(&db, &["worker", "--once"]).status.success());
    let output = run(&db, &["case", "show", id]);
    assert!(output.status.success());
    let state: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(state["case"]["status"], "awaiting_review");
    assert_eq!(state["events"].as_array().unwrap().len(), 4);
    assert_eq!(state["results"].as_array().unwrap().len(), 1);
    let conflict = run(
        &db,
        &[
            "case",
            "rename",
            id,
            "--title",
            "Changed",
            "--key",
            "rename-1",
            "--reason",
            "Fix title",
            "--expected-version",
            "2",
        ],
    );
    assert!(!conflict.status.success());
    assert!(
        String::from_utf8(conflict.stderr)
            .unwrap()
            .contains("current version 4")
    );
}
#[test]
fn cli_records_evidence_dispositions_and_explicit_reconsideration() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("qualification.db");
    let output = run(
        &db,
        &[
            "case",
            "create",
            "--title",
            "Qualification CLI",
            "--key",
            "qualification-cli",
        ],
    );
    let case: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = case["id"].as_str().unwrap();
    assert!(run(&db, &["worker", "--once"]).status.success());
    let evidence = serde_json::json!({"schema_version":1,"idempotency_key":"cli-evidence","actor":"operator","reason":"Record sanitized fixture","expected_version":4,"scope":"staging checkout","statement":"Operator reported a timeout","collection_method":"Manual fixture report","observed_at":null,"uncertainty":"Impact and recovery unknown"});
    let file = dir.path().join("evidence.json");
    std::fs::write(&file, evidence.to_string()).unwrap();
    let output = run(
        &db,
        &[
            "case",
            "record-evidence",
            id,
            "--file",
            file.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let decision = serde_json::json!({"schema_version":1,"idempotency_key":"cli-disposition","actor":"operator","reason":"Request more evidence","expected_version":5,"owner":"operator","scope":"staging checkout","uncertainty":"Cause not established","evidence_refs":[evidence["evidence"]["digest"]],"reconsideration_conditions":"New evidence or deadline","reopen_after_occurrences":1,"plan":{"kind":"insufficient_evidence","after_seconds":60,"collection_task":"Collect sanitized trace"}});
    let file = dir.path().join("decision.json");
    std::fs::write(&file, decision.to_string()).unwrap();
    let args = [
        "case",
        "record-disposition",
        id,
        "--file",
        file.to_str().unwrap(),
    ];
    let first = run(&db, &args);
    assert!(first.status.success());
    assert_eq!(first.stdout, run(&db, &args).stdout);
    assert!(run(&db, &["tick"]).status.success());
    let s: serde_json::Value =
        serde_json::from_slice(&run(&db, &["case", "show", id]).stdout).unwrap();
    assert_eq!(s["qualification"]["obligations"][0]["state"], "pending");
    let review = serde_json::json!({"schema_version":1,"idempotency_key":"cli-reconsider","actor":"operator","reason":"Reassess current report","expected_version":7,"disposition_id":s["qualification"]["dispositions"][0]["id"],"trigger":"operator_review","evidence_refs":[evidence["evidence"]["digest"]]});
    let file = dir.path().join("review.json");
    std::fs::write(&file, review.to_string()).unwrap();
    assert!(
        run(
            &db,
            &["case", "reconsider", id, "--file", file.to_str().unwrap()]
        )
        .status
        .success()
    );
    let s: serde_json::Value =
        serde_json::from_slice(&run(&db, &["case", "show", id]).stdout).unwrap();
    assert_eq!(s["qualification"]["dispositions"][0]["state"], "reopened");
    assert_eq!(s["case"]["status"], "awaiting_review");
}
#[test]
fn headless_fake_workers_block_handoff_accept_and_complete_without_a_server() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("team.db");
    let initial = run(
        &db,
        &[
            "case",
            "create",
            "--title",
            "Headless teamwork",
            "--key",
            "headless-team",
        ],
    );
    assert!(initial.status.success());
    let case: serde_json::Value = serde_json::from_slice(&initial.stdout).unwrap();
    let id = case["id"].as_str().unwrap();
    assert!(run(&db, &["worker", "--once", "--block"]).status.success());
    let before: serde_json::Value =
        serde_json::from_slice(&run(&db, &["case", "show", id]).stdout).unwrap();
    assert_eq!(before["team"]["control"]["state"], "blocked");
    assert_eq!(before["case"]["owner"], "operator");
    let offer = serde_json::json!({"command":{"meta":{"schema_version":1,"idempotency_key":"headless-offer","actor":"operator","reason":"Send durable checkpoint to a second fake investigator"},"expected_version":before["case"]["version"]},"block_id":before["team"]["blocks"][0]["id"],"successor":"fake-specialist","next_owner":"fake-specialist","acknowledge_within_seconds":60});
    let file = dir.path().join("handoff.json");
    std::fs::write(&file, offer.to_string()).unwrap();
    let output = run(
        &db,
        &[
            "team",
            "offer-handoff",
            id,
            "--file",
            file.to_str().unwrap(),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pending: serde_json::Value =
        serde_json::from_slice(&run(&db, &["case", "show", id]).stdout).unwrap();
    assert_eq!(pending["case"]["owner"], "operator");
    assert_eq!(pending["team"]["handoffs"][0]["state"], "proposed");
    let output = run(&db, &["worker", "--once", "--identity", "fake-specialist"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let done: serde_json::Value =
        serde_json::from_slice(&run(&db, &["case", "show", id]).stdout).unwrap();
    assert_eq!(done["case"]["owner"], "fake-specialist");
    assert_eq!(done["case"]["status"], "awaiting_review");
    assert_eq!(done["team"]["handoffs"][0]["state"], "accepted");
    assert_eq!(done["team"]["runs"].as_array().unwrap().len(), 2);
    assert_eq!(done["results"].as_array().unwrap().len(), 1);
    assert_eq!(
        done["team"]["control"]["overall_deadline"],
        before["team"]["control"]["overall_deadline"]
    );
    assert!(run(&db, &["team", "inbox"]).status.success());
}
