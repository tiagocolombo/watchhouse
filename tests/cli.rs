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
