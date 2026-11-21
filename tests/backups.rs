use chrono::{TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, params};
use std::{fs, sync::Arc};
use watchhouse::{
    application::Application,
    backups,
    domain::ManualClock,
    intake::FakeSource,
    store::{Store, digest},
};
fn fixture() -> (tempfile::TempDir, Application) {
    let d = tempfile::tempdir().unwrap();
    let a = Application::new(
        Store::open(d.path().join("state.db")).unwrap(),
        Arc::new(ManualClock::new(
            Utc.with_ymd_and_hms(2026, 11, 21, 12, 0, 0).unwrap(),
        )),
    );
    (d, a)
}
fn readonly(path: &std::path::Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
#[test]
fn live_wal_snapshot_keeps_committed_state_receipts_evidence_and_owned_deadlines() {
    let (d, a) = fixture();
    let command = FakeSource.manual_command("committed before backup");
    let case = a.create_case(command.clone()).unwrap();
    let claim = a.claim().unwrap().unwrap();
    // Keep the WAL live; copying the main database alone would miss these pages.
    let writer = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    let content = "sanitized retained evidence";
    writer
        .execute(
            "INSERT INTO evidence VALUES (?1,'text/plain',?2,'fixture','now')",
            params![digest(content), content],
        )
        .unwrap();
    assert!(fs::metadata(d.path().join("state.db-wal")).unwrap().len() > 0);
    let output = d.path().join("backup");
    let report = a
        .create_backup(&output, "operator", "Retain synthetic obligations")
        .unwrap();
    assert!(report.restore_available);
    assert!(Store::open(output.join("watchhouse.db")).is_err());
    assert!(report.verification.contains("no live integrations"));
    assert_eq!(a.create_case(command).unwrap().id, case.id);
    a.create_case(FakeSource.manual_command("after backup"))
        .unwrap();
    let frozen = readonly(&output.join("watchhouse.db"));
    assert_eq!(
        frozen
            .query_row("SELECT COUNT(*) FROM cases", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        frozen
            .query_row(
                "SELECT content FROM evidence WHERE digest=?1",
                [digest(content)],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        content
    );
    assert!(
        frozen
            .query_row("SELECT EXISTS(SELECT 1 FROM commands)", [], |r| r
                .get::<_, bool>(0))
            .unwrap()
    );
    let run: String = frozen
        .query_row(
            "SELECT progress_deadline FROM run_control WHERE run_id=?1",
            [&claim.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        run,
        writer
            .query_row(
                "SELECT progress_deadline FROM run_control WHERE run_id=?1",
                [&claim.run_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap()
    );
    assert!(
        a.factory_status()
            .unwrap()
            .audit
            .iter()
            .any(|e| e["backup_id"] == report.id)
    );
    assert!(
        a.create_backup(&output, "operator", "Never overwrite")
            .is_err()
    );
    assert_eq!(
        backups::inspect(&output, Some(&report.manifest_digest))
            .unwrap()
            .manifest_digest,
        report.manifest_digest
    );
    assert!(backups::inspect(&output, Some("sha256:wrong")).is_err());
    fs::write(output.join("unexpected"), "not declared").unwrap();
    assert!(backups::inspect(&output, None).is_err());
    fs::remove_file(output.join("unexpected")).unwrap();
    fs::write(output.join("watchhouse.db"), "corrupted").unwrap();
    assert!(backups::inspect(&output, None).is_err());
}
fn frozen_artifacts(d: &tempfile::TempDir, a: &Application) {
    use watchhouse::{
        contracts::V1,
        real_execution::{pipeline::Candidate, protocol::InputFile},
        repair::{BaseSnapshot, RepairSource, RepairSourceConfig, RuntimeProbe, SnapshotFile},
    };
    let root = d.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("code.txt"), "base\n").unwrap();
    // A neighboring private worker file must never enter the backup.
    fs::write(
        root.join("credential-copy"),
        "fixture credential placeholder",
    )
    .unwrap();
    let cfg = RepairSourceConfig {
        schema_version: V1,
        revision: 1,
        repository: "fixture/repo".into(),
        source_path: d.path().join("repo"),
        base_commit: "a".repeat(40),
        workspace_root: d.path().join("work"),
        runtime_path: d.path().join("runtime"),
        runtime_digest: digest("fixture executable"),
        max_files: 100,
        max_blob_bytes: 1024,
        max_total_bytes: 1024,
        preparation_seconds: 5,
        probe_seconds: 5,
    };
    let snapshot = BaseSnapshot {
        base_commit: cfg.base_commit.clone(),
        tree: "b".repeat(40),
        files: vec![SnapshotFile {
            path: "code.txt".into(),
            git_mode: "100644".into(),
            git_blob: "c".repeat(40),
            bytes: 5,
            digest: digest("base\n"),
        }],
        total_bytes: 5,
    };
    let source = RepairSource {
        schema_version: V1,
        config_digest: digest(&serde_json::to_string(&cfg).unwrap()),
        config: cfg,
        manifest_digest: digest(&serde_json::to_string(&snapshot).unwrap()),
        snapshot,
        snapshot_path: root,
        probe: RuntimeProbe {
            adapter: "fixture".into(),
            binary_digest: None,
            version: None,
            version_output_digest: None,
            help_output_digest: None,
            advertised: vec![],
            verified: vec![],
            error: None,
        },
        operator: "operator".into(),
        recorded_at: "2026-11-21T12:00:00.000Z".into(),
    };
    let db = Store::open(d.path().join("state.db"))
        .unwrap()
        .connect()
        .unwrap();
    let raw = serde_json::to_string(&source).unwrap();
    db.execute(
        "INSERT INTO repair_sources VALUES (1,?1,?2)",
        params![digest(&raw), raw],
    )
    .unwrap();
    let case = a
        .create_case(FakeSource.manual_command("artifact case"))
        .unwrap();
    db.execute(
        "INSERT INTO repair_requests VALUES ('repair',?1,'{}')",
        [&case.id],
    )
    .unwrap();
    let root = d.path().join("candidate");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("code.txt"), "fixed\n").unwrap();
    let files = vec![InputFile {
        path: "code.txt".into(),
        bytes: 6,
        digest: digest("fixed\n"),
        executable: true,
    }];
    let c = Candidate {
        schema_version: V1,
        id: "candidate-fixture".into(),
        case_id: case.id,
        repair_id: "repair".into(),
        author: "operator".into(),
        producer: "fixture".into(),
        model_job: None,
        recorded_at: source.recorded_at,
        recorded_version: 1,
        source_revision: 1,
        source_digest: None,
        policy_revision: 1,
        repository: source.config.repository,
        base_commit: source.config.base_commit,
        base_tree: source.snapshot.tree,
        digest: digest(&serde_json::to_string(&files).unwrap()),
        files,
        root,
        changes: vec![],
        deadline: "2026-11-21T13:00:00.000Z".into(),
    };
    db.execute(
        "INSERT INTO repair_candidates VALUES (?1,?2,?3,?4)",
        params![
            c.id,
            c.case_id,
            c.repair_id,
            serde_json::to_string(&c).unwrap()
        ],
    )
    .unwrap();
}
#[test]
fn frozen_source_and_candidate_are_complete_inert_and_independent_of_original_paths() {
    let (d, a) = fixture();
    frozen_artifacts(&d, &a);
    let output = d.path().join("backup");
    let report = a
        .create_backup(&output, "operator", "Preserve frozen artifacts")
        .unwrap();
    assert_eq!(report.artifacts, 2);
    assert_eq!(report.files, 3);
    fs::remove_dir_all(d.path().join("source")).unwrap();
    fs::remove_dir_all(d.path().join("candidate")).unwrap();
    assert!(backups::inspect(&output, Some(&report.manifest_digest)).is_ok());
    let manifest: backups::Manifest =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert!(
        manifest
            .artifacts
            .iter()
            .find(|a| a.kind == "candidate")
            .unwrap()
            .files[0]
            .executable
    );
    assert!(!format!("{manifest:?}").contains("credential-copy"));
    assert_eq!(
        fs::read_to_string(output.join("artifacts/source-1/code.txt")).unwrap(),
        "base\n"
    );
    fs::write(
        output.join("artifacts/candidate-fixture/code.txt"),
        "wrong\n",
    )
    .unwrap();
    assert!(backups::inspect(&output, None).is_err());
}
#[test]
fn missing_or_changed_frozen_files_leave_incomplete_non_overwritable_exports() {
    let (d, a) = fixture();
    frozen_artifacts(&d, &a);
    fs::remove_file(d.path().join("source/code.txt")).unwrap();
    let output = d.path().join("incomplete");
    assert!(
        a.create_backup(&output, "operator", "Missing frozen artifact")
            .is_err()
    );
    assert!(backups::inspect(&output, None).is_err());
    assert!(
        a.create_backup(&output, "operator", "No overwrite")
            .is_err()
    );
    assert!(
        !a.factory_status()
            .unwrap()
            .audit
            .iter()
            .any(|e| e["type"] == "backup_created")
    );
}
#[test]
fn semantic_evidence_and_migration_tampering_are_rejected_even_with_rehashed_file_manifest() {
    let (d, a) = fixture();
    let output = d.path().join("backup");
    a.create_backup(&output, "operator", "Integrity fixture")
        .unwrap();
    let path = output.join("watchhouse.db");
    let db = Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO evidence VALUES ('sha256:forged','text/plain','changed','fixture','now')",
        [],
    )
    .unwrap();
    drop(db);
    let manifest_path = output.join("manifest.json");
    let mut m: backups::Manifest =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let bytes = fs::read(&path).unwrap();
    use sha2::{Digest, Sha256};
    let e = m
        .entries
        .iter_mut()
        .find(|e| e.path == "watchhouse.db")
        .unwrap();
    e.bytes = bytes.len() as u64;
    e.digest = format!("sha256:{:x}", Sha256::digest(bytes));
    fs::write(&manifest_path, serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(
        backups::inspect(&output, None)
            .unwrap_err()
            .to_string()
            .contains("evidence content")
    );
    let db = Connection::open(&path).unwrap();
    db.execute("DELETE FROM evidence", []).unwrap();
    db.execute(
        "UPDATE schema_migrations SET digest='changed' WHERE version=1",
        [],
    )
    .unwrap();
    drop(db);
    let bytes = fs::read(&path).unwrap();
    m.entries[0].bytes = bytes.len() as u64;
    m.entries[0].digest = format!("sha256:{:x}", Sha256::digest(bytes));
    fs::write(&manifest_path, serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(
        backups::inspect(&output, None)
            .unwrap_err()
            .to_string()
            .contains("migration history")
    );
}
#[cfg(unix)]
#[test]
fn symlinked_inputs_and_backup_paths_are_rejected_and_completed_files_are_private() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (d, a) = fixture();
    frozen_artifacts(&d, &a);
    let output = d.path().join("backup");
    a.create_backup(&output, "operator", "Private files")
        .unwrap();
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(output.join("manifest.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fs::remove_file(output.join("artifacts/source-1/code.txt")).unwrap();
    symlink(
        d.path().join("source/code.txt"),
        output.join("artifacts/source-1/code.txt"),
    )
    .unwrap();
    assert!(backups::inspect(&output, None).is_err());
    fs::remove_file(d.path().join("source/code.txt")).unwrap();
    symlink(
        d.path().join("candidate/code.txt"),
        d.path().join("source/code.txt"),
    )
    .unwrap();
    assert!(
        a.create_backup(&d.path().join("linked"), "operator", "Reject linked input")
            .is_err()
    );
}
#[test]
fn inspect_cli_does_not_create_or_migrate_a_coordinator_database() {
    let (d, a) = fixture();
    let output = d.path().join("backup");
    let r = a.create_backup(&output, "operator", "CLI fixture").unwrap();
    let absent = d.path().join("must-not-exist.db");
    let p = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .args([
            "--db",
            absent.to_str().unwrap(),
            "backup-inspect",
            "--directory",
            output.to_str().unwrap(),
            "--manifest-digest",
            &r.manifest_digest,
        ])
        .output()
        .unwrap();
    assert!(p.status.success(), "{}", String::from_utf8_lossy(&p.stderr));
    assert!(!absent.exists());
}

#[test]
fn untrusted_manifest_paths_and_case_aliases_cannot_select_external_files() {
    let (d, a) = fixture();
    let output = d.path().join("backup");
    a.create_backup(&output, "operator", "Path fixture")
        .unwrap();
    let path = output.join("manifest.json");
    let original = fs::read(&path).unwrap();
    let mut m: backups::Manifest = serde_json::from_slice(&original).unwrap();
    m.entries[0].path = "../state.db".into();
    fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(
        backups::inspect(&output, None)
            .unwrap_err()
            .to_string()
            .contains("unsafe manifest entry")
    );
    let mut m: backups::Manifest = serde_json::from_slice(&original).unwrap();
    for name in ["artifacts/Foo/a", "artifacts/foo/b"] {
        m.entries.push(backups::Entry {
            path: name.into(),
            bytes: 0,
            digest: digest(""),
        });
    }
    fs::write(&path, serde_json::to_vec(&m).unwrap()).unwrap();
    assert!(
        backups::inspect(&output, None)
            .unwrap_err()
            .to_string()
            .contains("case-colliding")
    );
    fs::write(path, original).unwrap();
    assert!(backups::inspect(&output, None).is_ok());
    assert!(a.list_cases().unwrap().is_empty());
}

#[test]
fn restore_preserves_receipts_and_deadlines_but_old_fake_proofs_cannot_resume() {
    use watchhouse::{
        contracts::*,
        factory::{Action, Command},
    };
    let (d, a) = fixture();
    let create = FakeSource.manual_command("restore lease fixture");
    let case = a.create_case(create.clone()).unwrap();
    let claim = a.claim().unwrap().unwrap();
    let before = a.snapshot(&case.id).unwrap();
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "Before restore fixture")
        .unwrap();
    let output = d.path().join("restored");
    let r = watchhouse::restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "Isolated fixture recovery",
    )
    .unwrap();
    let clock = ManualClock::new(
        chrono::DateTime::parse_from_rfc3339(&r.restored_at)
            .unwrap()
            .with_timezone(&Utc)
            + chrono::Duration::seconds(1),
    );
    let restored = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(clock),
    );
    assert_eq!(restored.create_case(create).unwrap().id, case.id);
    assert_eq!(
        restored.snapshot(&case.id).unwrap().events.len(),
        before.events.len()
    );
    assert_eq!(
        restored
            .snapshot(&case.id)
            .unwrap()
            .team
            .control
            .as_ref()
            .unwrap()
            .overall_deadline,
        before.team.control.as_ref().unwrap().overall_deadline
    );
    let heartbeat = RunCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: "stale-after-restore".into(),
            actor: claim.executor,
            reason: "Old proof must not renew".into(),
        },
        proof: RunProof {
            run_id: claim.run_id,
            fence: claim.fence,
        },
    };
    assert!(restored.heartbeat(heartbeat.clone()).is_err());
    let factory = restored.factory_status().unwrap();
    restored
        .control_factory(Command {
            meta: CommandMeta {
                schema_version: V1,
                idempotency_key: "resume-restore".into(),
                actor: "operator".into(),
                reason: "Explicitly reviewed fixture".into(),
            },
            expected_version: factory.version,
            action: Action::Resume {
                pause_id: r.pause_id,
            },
        })
        .unwrap();
    assert!(restored.heartbeat(heartbeat).is_err());
    restored.supervise_team().unwrap();
    assert!(!restored.snapshot(&case.id).unwrap().team.blocks.is_empty());
    assert_eq!(
        a.snapshot(&case.id).unwrap().case.version,
        before.case.version
    );
    assert!(
        watchhouse::restore::restore(
            &backup,
            &output,
            &b.manifest_digest,
            "operator",
            "No overwrite"
        )
        .is_err()
    );
}

#[test]
fn repeated_restore_materializes_frozen_files_without_rewriting_immutable_records() {
    let (d, a) = fixture();
    frozen_artifacts(&d, &a);
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "First backup")
        .unwrap();
    assert!(
        watchhouse::restore::restore(
            &backup,
            &backup.join("not-allowed"),
            &b.manifest_digest,
            "operator",
            "Preserve input backup"
        )
        .is_err()
    );
    assert!(!backup.join("not-allowed").exists());
    fs::remove_dir_all(d.path().join("source")).unwrap();
    fs::remove_dir_all(d.path().join("candidate")).unwrap();
    let output = d.path().join("restore-one");
    watchhouse::restore::restore(
        &backup,
        &output,
        &b.manifest_digest,
        "operator",
        "First restore",
    )
    .unwrap();
    let restored = Application::new(
        Store::open(output.join("watchhouse.db")).unwrap(),
        Arc::new(ManualClock::new(Utc::now())),
    );
    let source = restored.repair_source_status().unwrap().unwrap();
    assert!(source.snapshot_path.starts_with(&output));
    assert_eq!(
        fs::read_to_string(source.snapshot_path.join("code.txt")).unwrap(),
        "base\n"
    );
    let db = Store::open(output.join("watchhouse.db"))
        .unwrap()
        .connect()
        .unwrap();
    let old: String = readonly(&backup.join("watchhouse.db"))
        .query_row(
            "SELECT record FROM repair_sources WHERE revision=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        old,
        db.query_row(
            "SELECT record FROM repair_sources WHERE revision=1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
    );
    let second_backup = d.path().join("backup-two");
    let b2 = restored
        .create_backup(&second_backup, "operator", "Rebackup relocated artifacts")
        .unwrap();
    let second = d.path().join("restore-two");
    watchhouse::restore::restore(
        &second_backup,
        &second,
        &b2.manifest_digest,
        "operator",
        "Repeated recovery",
    )
    .unwrap();
    let again = Application::new(
        Store::open(second.join("watchhouse.db")).unwrap(),
        Arc::new(ManualClock::new(Utc::now())),
    );
    assert!(
        again
            .repair_source_status()
            .unwrap()
            .unwrap()
            .snapshot_path
            .starts_with(&second)
    );
    assert_eq!(
        fs::read_to_string(second.join("artifacts/candidate-fixture/code.txt")).unwrap(),
        "fixed\n"
    );
    let db = Store::open(second.join("watchhouse.db"))
        .unwrap()
        .connect()
        .unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM restore_runs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(db.execute("DELETE FROM restore_fences", []).is_err());
    assert!(
        db.execute("UPDATE restore_artifacts SET path='elsewhere'", [])
            .is_err()
    );
}

#[test]
fn incomplete_restore_denies_coordinator_start_and_corrupt_input_creates_no_destination() {
    let (d, a) = fixture();
    let output = d.path().join("interrupted");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("restore-incomplete"), "interrupted fixture").unwrap();
    assert!(Store::open(output.join("watchhouse.db")).is_err());
    assert!(!output.join("watchhouse.db").exists());
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .args([
            "--db",
            output.join("watchhouse.db").to_str().unwrap(),
            "serve",
            "--port",
            "0",
        ])
        .output()
        .unwrap();
    assert!(!child.status.success());
    assert!(String::from_utf8_lossy(&child.stderr).contains("restore is incomplete"));
    let backup = d.path().join("backup");
    let b = a
        .create_backup(&backup, "operator", "Corruption fixture")
        .unwrap();
    fs::write(backup.join("watchhouse.db"), "changed").unwrap();
    let absent = d.path().join("not-created");
    assert!(
        watchhouse::restore::restore(
            &backup,
            &absent,
            &b.manifest_digest,
            "operator",
            "Reject corrupt input"
        )
        .is_err()
    );
    assert!(!absent.exists());
}
