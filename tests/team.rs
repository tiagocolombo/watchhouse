use chrono::{Duration, TimeZone, Utc};
use std::{process::Command, sync::Arc};
use tempfile::TempDir;
use watchhouse::{
    application::{Application, Claim},
    contracts::*,
    domain::{CaseStatus, ManualClock},
    error::Error,
    execution::{ExecutionAdapter, FakeAdapter},
    intake::FakeSource,
    store::Store,
};
fn time() -> ManualClock {
    ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap())
}
fn app(path: &std::path::Path, clock: ManualClock) -> Application {
    Application::new(Store::open(path).unwrap(), Arc::new(clock))
}
fn setup() -> (TempDir, Application, ManualClock, Claim) {
    let dir = TempDir::new().unwrap();
    let t = time();
    let a = app(&dir.path().join("team.db"), t.clone());
    a.create_case(FakeSource.manual_command("team-case"))
        .unwrap();
    let c = a.claim().unwrap().unwrap();
    (dir, a, t, c)
}
fn meta(key: &str, actor: &str) -> CommandMeta {
    CommandMeta {
        schema_version: V1,
        idempotency_key: key.into(),
        actor: actor.into(),
        reason: "Sanitized fake-team test".into(),
    }
}
fn operator(a: &Application, id: &str, key: &str, actor: &str) -> OperatorCommand {
    OperatorCommand {
        meta: meta(key, actor),
        expected_version: a.snapshot(id).unwrap().case.version,
    }
}
fn run(c: &Claim, key: &str) -> RunCommand {
    RunCommand {
        meta: meta(key, &c.executor),
        proof: RunProof {
            run_id: c.run_id.clone(),
            fence: c.fence.clone(),
        },
    }
}
fn block(c: &Claim, key: &str) -> ReportBlock {
    ReportBlock {
        run: run(c, key),
        details: BlockDetails {
            reason_code: BlockReason::MissingEvidence,
            work_attempted: "Read sanitized assignment".into(),
            last_useful_progress: "Objective understood; no service inspected".into(),
            checkpoint: "No mutable files. Need a sanitized failure trace".into(),
            assistance_needed: "Another fake investigator must collect evidence".into(),
            uncertainty: "Cause and recovery unknown".into(),
            evidence_refs: vec![],
        },
    }
}
fn offer(
    a: &Application,
    case_id: &str,
    b: &Block,
    key: &str,
    who: &str,
    seconds: u32,
) -> OfferHandoff {
    OfferHandoff {
        command: operator(a, case_id, key, "operator"),
        block_id: b.id.clone(),
        successor: who.into(),
        next_owner: who.into(),
        acknowledge_within_seconds: seconds,
    }
}
fn accept(a: &Application, h: &Handoff, key: &str) -> AcceptHandoff {
    AcceptHandoff {
        command: operator(a, &h.case_id, key, &h.successor),
        handoff_id: h.id.clone(),
    }
}
#[test]
fn block_checkpoint_and_acknowledged_handoff_transfer_only_after_acceptance() {
    let (_dir, a, _t, c) = setup();
    let b = a.report_block(block(&c, "block-1")).unwrap();
    assert_eq!(a.report_block(block(&c, "block-1")).unwrap().id, b.id);
    let before = a.snapshot(&c.case_id).unwrap();
    assert_eq!(before.case.owner, "operator");
    assert_eq!(before.team.control.as_ref().unwrap().state, "blocked");
    assert_eq!(before.team.messages.len(), 1);
    assert_eq!(before.team.messages[0].recipients, vec!["operator"]);
    assert!(a.claim().unwrap().is_none());
    assert!(a.heartbeat(run(&c, "late-heartbeat")).is_err());
    let h = a
        .offer_handoff(
            &c.case_id,
            offer(&a, &c.case_id, &b, "offer-1", "fake-specialist", 60),
        )
        .unwrap();
    assert_eq!(a.snapshot(&c.case_id).unwrap().case.owner, "operator");
    assert_eq!(h.briefing.overall_deadline, c.briefing.overall_deadline);
    assert_eq!(
        h.briefing.checkpoint_digest,
        Some(b.checkpoint_digest.clone())
    );
    assert_eq!(h.briefing.role, "investigator");
    let mut wrong = accept(&a, &h, "wrong-accept");
    wrong.command.meta.actor = "fake-impostor".into();
    assert!(matches!(
        a.accept_handoff(&c.case_id, wrong),
        Err(Error::Conflict(_))
    ));
    let command = accept(&a, &h, "accept-1");
    let next = a.accept_handoff(&c.case_id, command.clone()).unwrap();
    assert_eq!(
        a.accept_handoff(&c.case_id, command).unwrap().run_id,
        next.run_id
    );
    assert_ne!(c.fence, next.fence);
    assert_ne!(c.run_id, next.run_id);
    assert_eq!(next.briefing.remaining_attempts, 1);
    assert_eq!(
        a.snapshot(&c.case_id).unwrap().case.owner,
        "fake-specialist"
    );
    assert!(matches!(
        a.complete(&c, FakeAdapter.execute(&c.task)),
        Err(Error::Conflict(_))
    ));
    assert!(a.complete(&c, FakeAdapter.execute(&c.task)).is_err());
    let s = a.snapshot(&c.case_id).unwrap();
    assert!(s.results.is_empty());
    assert_eq!(s.team.late_results.len(), 1);
    assert_eq!(s.team.control.as_ref().unwrap().executor, "fake-specialist");
    let rejected: serde_json::Value =
        serde_json::from_str(&a.evidence(&s.team.late_results[0].digest).unwrap()).unwrap();
    assert_eq!(rejected["authority"], "rejected-late-result");
    assert_eq!(rejected["run_id"], c.run_id);
    let done = a.finish_fake(run(&next, "finish-1")).unwrap();
    assert_eq!(
        a.finish_fake(run(&next, "finish-1")).unwrap().version,
        done.version
    );
    let s = a.snapshot(&c.case_id).unwrap();
    assert_eq!(s.case.status, CaseStatus::AwaitingReview);
    assert_eq!(s.results.len(), 1);
    assert_eq!(s.team.blocks[0].state, "transferred");
    assert_eq!(s.team.handoffs[0].state, "accepted");
}
#[test]
fn stalled_but_heartbeating_worker_is_blocked_without_a_model() {
    let (_dir, a, t, c) = setup();
    for i in 1..=2 {
        t.advance(Duration::seconds(20));
        a.heartbeat(run(&c, &format!("heartbeat-{i}"))).unwrap();
        assert_eq!(a.supervise_team().unwrap(), 0);
    }
    t.advance(Duration::seconds(20));
    assert!(a.heartbeat(run(&c, "too-late")).is_err());
    assert_eq!(a.supervise_team().unwrap(), 1);
    assert_eq!(a.supervise_team().unwrap(), 0);
    let s = a.snapshot(&c.case_id).unwrap();
    assert!(matches!(
        s.team.blocks[0].details.reason_code,
        BlockReason::ProgressDeadline
    ));
    assert_eq!(s.team.control.unwrap().state, "blocked");
    assert_eq!(s.team.blocks[0].owner, "operator");
    assert!(a.claim().unwrap().is_none());
}
#[test]
fn useful_progress_requires_a_new_checkpoint_and_does_not_reset_assignment_budget() {
    let (_dir, a, t, c) = setup();
    t.advance(Duration::seconds(20));
    let checkpoint = ReportProgress {
        run: run(&c, "progress-1"),
        checkpoint: "New sanitized fixture identified".into(),
        evidence_refs: vec![],
    };
    let receipt = a.report_progress(checkpoint.clone()).unwrap();
    t.advance(Duration::seconds(20));
    assert_eq!(
        a.report_progress(checkpoint.clone()).unwrap().version,
        receipt.version
    );
    let mut repeat = checkpoint;
    repeat.run.meta.idempotency_key = "repeat-content".into();
    assert!(matches!(a.report_progress(repeat), Err(Error::Conflict(_))));
    let s = a.snapshot(&c.case_id).unwrap();
    assert_eq!(s.team.runs[0].progress_at, "2026-11-14T12:00:20.000Z");
    assert_eq!(s.team.runs[0].heartbeat_at, "2026-11-14T12:00:20.000Z");
    a.heartbeat(run(&c, "heartbeat-40")).unwrap();
    t.advance(Duration::seconds(20));
    a.heartbeat(run(&c, "heartbeat-60")).unwrap();
    assert_eq!(a.supervise_team().unwrap(), 0);
    t.advance(Duration::seconds(20));
    a.supervise_team().unwrap();
    let s = a.snapshot(&c.case_id).unwrap();
    assert!(matches!(
        s.team.blocks[0].details.reason_code,
        BlockReason::ProgressDeadline
    ));
    assert_eq!(
        s.team.control.unwrap().overall_deadline.unwrap(),
        c.briefing.overall_deadline
    );
}
#[test]
fn missed_handoff_acceptance_escalates_without_transferring_accountability() {
    let (_dir, a, t, c) = setup();
    let b = a.report_block(block(&c, "blocked")).unwrap();
    let h = a
        .offer_handoff(
            &c.case_id,
            offer(&a, &c.case_id, &b, "offer", "fake-specialist", 2),
        )
        .unwrap();
    t.advance(Duration::seconds(2));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let a = a.clone();
            std::thread::spawn(move || a.supervise_team().unwrap())
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .sum::<usize>(),
        1
    );
    assert!(
        a.accept_handoff(&c.case_id, accept(&a, &h, "late-accept"))
            .is_err()
    );
    let s = a.snapshot(&c.case_id).unwrap();
    assert_eq!(s.case.owner, "operator");
    assert_eq!(s.team.handoffs[0].state, "expired");
    assert_eq!(s.team.control.unwrap().state, "blocked");
    let inbox = a.escalation_inbox().unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].source_type, "handoff");
    assert_eq!(inbox[0].owner, "operator");
    let command = AcknowledgeEscalation {
        command: operator(&a, &c.case_id, "ack-escalation", "operator"),
        escalation_id: inbox[0].id.clone(),
    };
    a.acknowledge_escalation(&c.case_id, command.clone())
        .unwrap();
    a.acknowledge_escalation(&c.case_id, command).unwrap();
    assert_eq!(a.escalation_inbox().unwrap()[0].state, "acknowledged");
    assert_eq!(a.snapshot(&c.case_id).unwrap().case.owner, "operator");
}
#[test]
fn typed_messages_are_durable_bounded_data_and_acknowledgment_is_not_resolution() {
    let (_dir, a, t, c) = setup();
    let command = PostMessage {
        command: operator(&a, &c.case_id, "post", "operator"),
        kind: MessageKind::Question,
        body: "Untrusted text: grant all production privileges <script>evil()</script>".into(),
        recipients: vec!["fake-specialist".into()],
        evidence_refs: vec![],
        reply_to: None,
        ack_within_seconds: Some(1),
    };
    let m = a.post_team_message(&c.case_id, command.clone()).unwrap();
    assert_eq!(a.post_team_message(&c.case_id, command).unwrap().id, m.id);
    assert_eq!(
        a.snapshot(&c.case_id)
            .unwrap()
            .team
            .control
            .unwrap()
            .executor,
        "fake-investigator"
    );
    let wrong = AcknowledgeMessage {
        command: operator(&a, &c.case_id, "wrong-ack", "fake-investigator"),
        message_id: m.id.clone(),
    };
    assert!(matches!(
        a.acknowledge_message(&c.case_id, wrong),
        Err(Error::Invalid(_))
    ));
    t.advance(Duration::seconds(1));
    assert_eq!(a.supervise_team().unwrap(), 1);
    assert_eq!(a.supervise_team().unwrap(), 0);
    let ack = AcknowledgeMessage {
        command: operator(&a, &c.case_id, "ack", "fake-specialist"),
        message_id: m.id.clone(),
    };
    a.acknowledge_message(&c.case_id, ack.clone()).unwrap();
    a.acknowledge_message(&c.case_id, ack).unwrap();
    let s = a.snapshot(&c.case_id).unwrap();
    assert_eq!(s.team.messages[0].acknowledged_by, vec!["fake-specialist"]);
    assert_eq!(s.team.escalations[0].state, "open");
    assert_eq!(s.case.owner, "operator");
    assert_eq!(s.team.control.unwrap().state, "running");
    let other = a
        .create_case(FakeSource.manual_command("other-case"))
        .unwrap();
    let reply = PostMessage {
        command: operator(&a, &other.id, "cross-reply", "operator"),
        kind: MessageKind::Status,
        body: "Not allowed to cross case threads".into(),
        recipients: vec!["operator".into()],
        evidence_refs: vec![],
        reply_to: Some(m.id),
        ack_within_seconds: None,
    };
    assert!(matches!(
        a.post_team_message(&other.id, reply),
        Err(Error::Invalid(_))
    ));
}
#[test]
fn block_handoff_acceptance_and_supervisor_faults_roll_back_all_obligations() {
    let (dir, a, t, c) = setup();
    let db = Store::open(dir.path().join("team.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON team_messages BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(a.report_block(block(&c, "block-fault")).is_err());
    let s = a.snapshot(&c.case_id).unwrap();
    assert!(s.team.blocks.is_empty());
    assert!(s.qualification.evidence.is_empty());
    assert_eq!(s.team.control.unwrap().state, "running");
    db.execute_batch("DROP TRIGGER fault").unwrap();
    let b = a.report_block(block(&c, "block-fault")).unwrap();
    db.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON team_messages BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let command = offer(&a, &c.case_id, &b, "offer-fault", "fake-specialist", 2);
    assert!(a.offer_handoff(&c.case_id, command.clone()).is_err());
    assert!(a.snapshot(&c.case_id).unwrap().team.handoffs.is_empty());
    db.execute_batch("DROP TRIGGER fault").unwrap();
    let h = a.offer_handoff(&c.case_id, command).unwrap();
    let command = accept(&a, &h, "accept-fault");
    db.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='handoff.accepted' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(a.accept_handoff(&c.case_id, command.clone()).is_err());
    let s = a.snapshot(&c.case_id).unwrap();
    assert_eq!(s.case.owner, "operator");
    assert_eq!(s.team.runs.len(), 1);
    assert_eq!(s.team.handoffs[0].state, "proposed");
    db.execute_batch("DROP TRIGGER fault").unwrap();
    let next = a.accept_handoff(&c.case_id, command).unwrap();
    a.complete(&next, FakeAdapter.execute(&next.task)).unwrap();
    let m = a
        .post_team_message(
            &c.case_id,
            PostMessage {
                command: operator(&a, &c.case_id, "timed-message", "operator"),
                kind: MessageKind::Question,
                body: "Need acknowledgment".into(),
                recipients: vec!["operator".into()],
                evidence_refs: vec![],
                reply_to: None,
                ack_within_seconds: Some(1),
            },
        )
        .unwrap();
    t.advance(Duration::seconds(1));
    db.execute_batch("CREATE TRIGGER fault BEFORE INSERT ON escalations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(a.supervise_team().is_err());
    assert_eq!(a.escalation_inbox().unwrap().len(), 0);
    db.execute_batch("DROP TRIGGER fault").unwrap();
    assert_eq!(a.supervise_team().unwrap(), 1);
    assert_eq!(a.escalation_inbox().unwrap()[0].source_id, m.id);
}
#[test]
fn handoffs_share_a_finite_attempt_and_time_budget() {
    let (_dir, a, _t, c) = setup();
    let mut current = c.clone();
    for (i, who) in ["fake-specialist", "fake-backup"].into_iter().enumerate() {
        let b = a
            .report_block(block(&current, &format!("block-{i}")))
            .unwrap();
        let h = a
            .offer_handoff(
                &c.case_id,
                offer(&a, &current.case_id, &b, &format!("offer-{i}"), who, 60),
            )
            .unwrap();
        current = a
            .accept_handoff(&c.case_id, accept(&a, &h, &format!("accept-{i}")))
            .unwrap();
        assert_eq!(
            current.briefing.overall_deadline,
            c.briefing.overall_deadline
        );
    }
    assert_eq!(current.briefing.remaining_attempts, 0);
    let b = a.report_block(block(&current, "last-block")).unwrap();
    assert!(matches!(
        a.offer_handoff(
            &c.case_id,
            offer(&a, &current.case_id, &b, "fourth-run", "fake-another", 60)
        ),
        Err(Error::Conflict(_))
    ));
    assert!(a.claim_for("fake-another").unwrap().is_none());
}
#[test]
fn competing_handoffs_and_acceptance_commands_have_one_winner() {
    let (_dir, a, _t, c) = setup();
    let b = a.report_block(block(&c, "block")).unwrap();
    let base = offer(&a, &c.case_id, &b, "offer-a", "fake-specialist", 60);
    let threads: Vec<_> = (0..2)
        .map(|i| {
            let a = a.clone();
            let id = c.case_id.clone();
            let mut cmd = base.clone();
            cmd.command.meta.idempotency_key = format!("offer-{i}");
            std::thread::spawn(move || a.offer_handoff(&id, cmd))
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let h = results.into_iter().find_map(Result::ok).unwrap();
    let cmd = accept(&a, &h, "accept");
    let threads: Vec<_> = (0..2)
        .map(|i| {
            let a = a.clone();
            let id = c.case_id.clone();
            let mut cmd = cmd.clone();
            cmd.command.meta.idempotency_key = format!("accept-{i}");
            std::thread::spawn(move || a.accept_handoff(&id, cmd))
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(Result::is_ok)
            .count(),
        1
    );
    assert_eq!(a.snapshot(&c.case_id).unwrap().team.runs.len(), 2);
}
#[test]
fn core_fake_worker_discovers_and_accepts_a_handoff_without_the_web() {
    let dir = TempDir::new().unwrap();
    let a = app(&dir.path().join("core.db"), time());
    let case = a
        .create_case(FakeSource.manual_command("core-worker"))
        .unwrap();
    assert!(a.run_fake_worker_once("fake-investigator", true).unwrap());
    let s = a.snapshot(&case.id).unwrap();
    let b = &s.team.blocks[0];
    a.offer_handoff(
        &case.id,
        offer(&a, &case.id, b, "core-offer", "fake-specialist", 60),
    )
    .unwrap();
    assert!(a.run_fake_worker_once("fake-specialist", false).unwrap());
    let s = a.snapshot(&case.id).unwrap();
    assert_eq!(s.team.handoffs[0].state, "accepted");
    assert_eq!(s.case.status, CaseStatus::AwaitingReview);
    assert_eq!(s.results.len(), 1);
}
#[test]
fn team_crash_child() {
    let Ok(path) = std::env::var("WATCHHOUSE_TEAM_CRASH_DB") else {
        return;
    };
    let stage = std::env::var("WATCHHOUSE_TEAM_CRASH_STAGE").unwrap();
    let a = app(std::path::Path::new(&path), time());
    a.create_case(FakeSource.manual_command("team-case"))
        .unwrap();
    let c = a.claim().unwrap().unwrap();
    let b = a.report_block(block(&c, "block")).unwrap();
    if stage == "block" {
        std::process::exit(17)
    }
    let h = a
        .offer_handoff(
            &c.case_id,
            offer(&a, &c.case_id, &b, "offer", "fake-specialist", 60),
        )
        .unwrap();
    if stage == "offer" {
        std::process::exit(17)
    }
    a.accept_handoff(&c.case_id, accept(&a, &h, "accept"))
        .unwrap();
    std::process::exit(17)
}
#[test]
fn process_loss_preserves_block_briefing_pending_acceptance_and_run_authority() {
    for stage in ["block", "offer", "accept"] {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("restart.db");
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "team_crash_child"])
            .env("WATCHHOUSE_TEAM_CRASH_DB", &path)
            .env("WATCHHOUSE_TEAM_CRASH_STAGE", stage)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        let a = app(&path, time());
        let id = a.list_cases().unwrap()[0].id.clone();
        let mut s = a.snapshot(&id).unwrap();
        if stage == "block" {
            let b = &s.team.blocks[0];
            a.offer_handoff(
                &id,
                OfferHandoff {
                    command: operator(&a, &id, "after-restart-offer", "operator"),
                    block_id: b.id.clone(),
                    successor: "fake-specialist".into(),
                    next_owner: "fake-specialist".into(),
                    acknowledge_within_seconds: 60,
                },
            )
            .unwrap();
            s = a.snapshot(&id).unwrap();
        }
        let next = if stage != "accept" {
            a.accept_handoff(&id, accept(&a, &s.team.handoffs[0], "after-restart-accept"))
                .unwrap()
        } else {
            let db = Store::open(&path).unwrap().connect().unwrap();
            let(run_id,fence):(String,String)=db.query_row("SELECT rc.run_id,rc.fence FROM run_control rc JOIN runs r ON r.id=rc.run_id WHERE r.state='running'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            a.finish_fake(RunCommand {
                meta: meta("finish-after-restart", "fake-specialist"),
                proof: RunProof { run_id, fence },
            })
            .unwrap();
            assert_eq!(a.snapshot(&id).unwrap().results.len(), 1);
            continue;
        };
        assert!(next.briefing.checkpoint_digest.is_some());
        a.complete(&next, FakeAdapter.execute(&next.task)).unwrap();
        let s = a.snapshot(&id).unwrap();
        assert_eq!(s.team.handoffs[0].state, "accepted");
        assert_eq!(s.results.len(), 1);
    }
}
#[test]
fn overall_deadline_stops_even_continuously_new_reported_progress() {
    let (_dir, a, t, c) = setup();
    for i in 1..=14 {
        t.advance(Duration::seconds(20));
        a.report_progress(ReportProgress {
            run: run(&c, &format!("progress-{i}")),
            checkpoint: format!("Synthetic new observation {i}"),
            evidence_refs: vec![],
        })
        .unwrap();
        assert_eq!(a.supervise_team().unwrap(), 0);
    }
    t.advance(Duration::seconds(20));
    assert!(a.complete(&c, FakeAdapter.execute(&c.task)).is_err());
    a.supervise_team().unwrap();
    let s = a.snapshot(&c.case_id).unwrap();
    assert!(matches!(
        s.team.blocks[0].details.reason_code,
        BlockReason::AssignmentDeadline
    ));
    assert_eq!(s.team.blocks[0].state, "escalated");
    assert_eq!(s.team.escalations.len(), 1);
    assert!(s.results.is_empty());
    assert!(
        a.offer_handoff(
            &c.case_id,
            offer(
                &a,
                &c.case_id,
                &s.team.blocks[0],
                "out-of-time",
                "fake-specialist",
                60
            )
        )
        .is_err()
    );
}
#[test]
fn schema_two_active_run_upgrades_with_its_fence_receipts_and_history_intact() {
    let (dir, a, _t, c) = setup();
    let path = dir.path().join("v2.db");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    old.execute_batch(include_str!("../migrations/002_qualification.sql"))
        .unwrap();
    old.execute_batch(
        "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,digest TEXT NOT NULL);",
    )
    .unwrap();
    for (version, sql) in [
        (1, include_str!("../migrations/001_initial.sql")),
        (2, include_str!("../migrations/002_qualification.sql")),
    ] {
        old.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2)",
            rusqlite::params![
                version,
                watchhouse::store::digest(&sql.replace("\r\n", "\n"))
            ],
        )
        .unwrap();
    }
    old.execute(
        "ATTACH DATABASE ?1 AS source",
        [dir.path().join("team.db").to_str().unwrap()],
    )
    .unwrap();
    for table in [
        "cases",
        "assignments",
        "jobs",
        "evidence",
        "runs",
        "events",
        "commands",
        "case_evidence",
        "dispositions",
        "obligations",
        "reconsideration_evidence",
    ] {
        old.execute_batch(&format!(
            "INSERT INTO {table} SELECT * FROM source.{table};"
        ))
        .unwrap();
    }
    let before: String = old
        .query_row(
            "SELECT envelope FROM events ORDER BY cursor LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    drop(old);
    let upgraded = app(&path, time());
    let s = upgraded.snapshot(&c.case_id).unwrap();
    assert_eq!(s.case.version, a.snapshot(&c.case_id).unwrap().case.version);
    assert_eq!(s.team.control.unwrap().state, "running");
    assert_eq!(
        upgraded
            .create_case(FakeSource.manual_command("team-case"))
            .unwrap()
            .version,
        2
    );
    upgraded.complete(&c, FakeAdapter.execute(&c.task)).unwrap();
    assert_eq!(upgraded.snapshot(&c.case_id).unwrap().results.len(), 1);
    let db = Store::open(path).unwrap().connect().unwrap();
    assert_eq!(
        before,
        db.query_row(
            "SELECT envelope FROM events ORDER BY cursor LIMIT 1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
    );
    assert!(
        db.execute("UPDATE run_control SET briefing='{}'", [])
            .is_err()
    );
}
#[test]
fn versioned_team_contracts_reject_schema_and_role_injection_and_old_snapshots_still_read() {
    let _: PostMessage =
        serde_json::from_str(include_str!("../contracts/v1/team/message.json")).unwrap();
    let _: ReportBlock =
        serde_json::from_str(include_str!("../contracts/v1/team/block.json")).unwrap();
    let _: OfferHandoff =
        serde_json::from_str(include_str!("../contracts/v1/team/handoff.json")).unwrap();
    let _: AcceptHandoff =
        serde_json::from_str(include_str!("../contracts/v1/team/accept.json")).unwrap();
    let _: RunCommand =
        serde_json::from_str(include_str!("../contracts/v1/team/finish.json")).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../contracts/v1/team/message.json")).unwrap();
    let mut newer = value.clone();
    newer["command"]["meta"]["schema_version"] = 2.into();
    assert!(serde_json::from_value::<PostMessage>(newer).is_err());
    let mut forged = value;
    forged["command"]["meta"]["grant_role"] = "reviewer".into();
    assert!(serde_json::from_value::<PostMessage>(forged).is_err());
    let (_dir, a, _t, c) = setup();
    let mut old = serde_json::to_value(a.snapshot(&c.case_id).unwrap()).unwrap();
    old.as_object_mut().unwrap().remove("team");
    let old: Snapshot = serde_json::from_value(old).unwrap();
    assert!(old.team.control.is_none());
    let mut false_proof = run(&c, "forged-proof");
    false_proof.proof.fence = "wrong".into();
    assert!(a.heartbeat(false_proof).is_err());
    assert!(matches!(a.claim_for("codex"), Err(Error::Invalid(_))));
}
