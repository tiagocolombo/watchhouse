use std::sync::Arc;
use watchhouse::{
    application::Application, contracts::*, domain::SystemClock, hallmark::*, intake::FakeSource,
    store::Store,
};
fn setup() -> (tempfile::TempDir, Application, String) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(SystemClock),
    );
    let id = a
        .create_case(FakeSource.manual_command("hallmark-fixture"))
        .unwrap()
        .id;
    (d, a, id)
}
fn command(a: &Application, id: &str, key: &str) -> ImportCommand {
    ImportCommand {
        command: OperatorCommand {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: key.into(),
                actor: "operator".into(),
                reason: "Import sanitized Hallmark format fixture as untrusted data".into(),
            },
            expected_version: a.snapshot(id).unwrap().case.version,
        },
        source_namespace: "hallmark:fixture/retries".into(),
        payload: serde_json::from_str(include_str!("fixtures/hallmark/library.json")).unwrap(),
    }
}
#[test]
fn actual_hallmark_wire_shapes_are_supported_but_malformed_or_oversized_entries_fail() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/hallmark/library.json")).unwrap();
    let expected = parse_export(&value).unwrap();
    assert_eq!(parse_export(&value["learnings"]).unwrap(), expected);
    assert_eq!(parse_export(&serde_json::json!({"action":"export","count":1,"ids":["L-0007"],"out":null,"learnings":value["learnings"],"next":[]})).unwrap(),expected);
    for bad in [
        serde_json::json!({"format":"other","version":1,"learnings":value["learnings"]}),
        serde_json::json!({"format":"hallmark-library","version":2,"learnings":value["learnings"]}),
        serde_json::json!({"learnings":[{"path":"../escape.md","content":"untrusted"}]}),
        serde_json::json!({"learnings":[{"path":"L-1.md","content":"x"},{"path":"l-1.md","content":"y"}]}),
        serde_json::json!({"learnings":[{"path":"L-1.md","content":"x".repeat(24577)}]}),
        serde_json::json!({"learnings":[]}),
    ] {
        assert!(parse_export(&bad).is_err(), "{bad}");
    }
}
#[test]
fn imports_roundtrip_exact_bytes_replay_and_revisions_without_accepting_external_authority() {
    let (d, a, id) = setup();
    let cmd = command(&a, &id, "import");
    let result = a.import_hallmark(&id, cmd.clone()).unwrap();
    assert_eq!(result.added, 1);
    assert_eq!(
        a.import_hallmark(&id, cmd).unwrap().entries[0].id,
        result.entries[0].id
    );
    assert!(a.snapshot(&id).unwrap().learning.entries.is_empty());
    let original = result.entries[0].clone();
    let exported = a
        .export_hallmark(
            &id,
            ExportSelection::Imported {
                ids: vec![original.id.clone()],
            },
        )
        .unwrap();
    assert_eq!(exported.learnings[0].content, original.content);
    assert_eq!(exported.learnings[0].path, original.source_path);
    assert!(exported.learnings[0].content.contains("status: accepted"));
    assert_eq!(
        parse_export(&serde_json::to_value(exported).unwrap()).unwrap()[0].content,
        original.content
    );
    let repeat = a
        .import_hallmark(&id, command(&a, &id, "another-key"))
        .unwrap();
    assert_eq!((repeat.added, repeat.already_present), (0, 1));
    let mut changed = command(&a, &id, "revision");
    changed.payload["learnings"][0]["content"] =
        serde_json::json!(original.content.to_owned() + "\nChanged source revision\n");
    let revision = a.import_hallmark(&id, changed).unwrap().entries[0].clone();
    assert_ne!(original.id, revision.id);
    assert_eq!(a.snapshot(&id).unwrap().knowledge_imports.len(), 2);
    assert!(
        a.export_hallmark(
            &id,
            ExportSelection::Imported {
                ids: vec![original.id.clone(), revision.id]
            }
        )
        .is_err()
    );
    assert_eq!(
        a.export_hallmark(
            &id,
            ExportSelection::Imported {
                ids: vec![original.id]
            }
        )
        .unwrap()
        .learnings[0]
            .content,
        original.content
    );
    let reopened = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(SystemClock),
    );
    assert_eq!(reopened.snapshot(&id).unwrap().knowledge_imports.len(), 2);
    assert!(reopened.snapshot(&id).unwrap().learning.entries.is_empty());
}
#[test]
fn imports_and_receipts_rollback_with_events_and_never_cross_case_authority() {
    let (d, a, id) = setup();
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_import BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='knowledge.imported' BEGIN SELECT RAISE(ABORT,'injected import audit failure'); END;").unwrap();
    let cmd = command(&a, &id, "atomic-import");
    assert!(a.import_hallmark(&id, cmd.clone()).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM knowledge_imports", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM commands WHERE idempotency_key='atomic-import'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER fail_import").unwrap();
    let record = a.import_hallmark(&id, cmd.clone()).unwrap().entries[0].clone();
    let foreign = a
        .create_case(FakeSource.manual_command("foreign-hallmark"))
        .unwrap()
        .id;
    assert!(
        a.export_hallmark(
            &foreign,
            ExportSelection::Imported {
                ids: vec![record.id]
            }
        )
        .is_err()
    );
    let mut wrong = command(&a, &id, "wrong-owner");
    wrong.command.meta.actor = "other-operator".into();
    assert!(a.import_hallmark(&id, wrong).is_err());
    let mut changed = cmd;
    changed.source_namespace = "other".into();
    assert!(a.import_hallmark(&id, changed).is_err());
    assert!(
        db.execute(
            "UPDATE knowledge_imports SET source_namespace='changed'",
            []
        )
        .is_err()
    );
    assert!(db.execute("DELETE FROM knowledge_imports", []).is_err());
}
#[tokio::test]
async fn import_export_share_authenticated_http_and_snapshot_records() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (_d, a, id) = setup();
    let token = "hallmark-local-test-token-32-bytes";
    let router = watchhouse::http::router(a.clone(), token.into(), "127.0.0.1:8787".into());
    let req = |path: &str, body: serde_json::Value, auth: bool| {
        let mut b = Request::builder()
            .method("POST")
            .uri(path)
            .header("Host", "127.0.0.1:8787")
            .header("Content-Type", "application/json");
        if auth {
            b = b.header("Authorization", format!("Bearer {token}"));
        }
        b.body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let path = format!("/api/v1/cases/{id}/knowledge-imports");
    let cmd = serde_json::to_value(command(&a, &id, "http-import")).unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(req(&path, cmd.clone(), false))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let r = router.clone().oneshot(req(&path, cmd, true)).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let result: ImportResult =
        serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let path = format!("/api/v1/cases/{id}/hallmark-export");
    let r = router
        .oneshot(req(
            &path,
            serde_json::to_value(ExportSelection::Imported {
                ids: vec![result.entries[0].id.clone()],
            })
            .unwrap(),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let file: ExportFile =
        serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(file.learnings[0].content, result.entries[0].content);
}

#[test]
fn separate_cli_processes_import_replay_and_export_the_same_frozen_bytes() {
    let (root, a, id) = setup();
    let path = root.path().join("import.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&command(&a, &id, "cli-import")).unwrap(),
    )
    .unwrap();
    let run = |action: &str, file: &std::path::Path| {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
            .arg("--db")
            .arg(root.path().join("db"))
            .arg("case")
            .arg(action)
            .arg(&id)
            .arg("--file")
            .arg(file)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()
    };
    let result = run("import-hallmark", &path);
    assert_eq!(run("import-hallmark", &path), result);
    let source = result["entries"][0].clone();
    let selection = root.path().join("selection.json");
    std::fs::write(
        &selection,
        serde_json::to_vec(&serde_json::json!({"type":"imported","ids":[source["id"]]})).unwrap(),
    )
    .unwrap();
    let exported = run("export-hallmark", &selection);
    assert_eq!(exported["learnings"][0]["content"], source["content"]);
    assert_eq!(a.snapshot(&id).unwrap().knowledge_imports.len(), 1);
}
