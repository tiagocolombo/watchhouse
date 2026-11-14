use chrono::{Duration, TimeZone, Utc};
use std::sync::Arc;
use tempfile::TempDir;
use watchhouse::{
    application::Application, challenges::*, contracts::*, domain::ManualClock, intake::FakeSource,
    store::Store,
};
fn cmd(a: &Application, id: &str, key: &str, actor: &str) -> OperatorCommand {
    OperatorCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: actor.into(),
            reason: "Sanitized independent review".into(),
        },
        expected_version: a.snapshot(id).unwrap().case.version,
    }
}
fn evidence(a: &Application, id: &str, key: &str, actor: &str) -> String {
    a.record_evidence(id,serde_json::from_value(serde_json::json!({"schema_version":1,"idempotency_key":key,"actor":actor,"reason":"Independent experiment","expected_version":a.snapshot(id).unwrap().case.version,"scope":"fixture","statement":key,"collection_method":"Separate fixture observation","observed_at":null,"uncertainty":"No live service checked"})).unwrap()).unwrap().evidence.digest
}
fn setup() -> (TempDir, Application, ManualClock, String) {
    let d = TempDir::new().unwrap();
    let t = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap());
    let a = Application::new(
        Store::open(d.path().join("test.db")).unwrap(),
        Arc::new(t.clone()),
    );
    let id = a
        .create_case(FakeSource.manual_command("fixture"))
        .unwrap()
        .id;
    (d, a, t, id)
}
fn open(a: &Application, id: &str) -> ChallengeCommand {
    let target = a.snapshot(id).unwrap().events[0].clone();
    ChallengeCommand {
        command: cmd(a, id, "open", "fake-challenger"),
        action: ChallengeAction::Open {
            target_event_id: target.event_id,
            target_version: target.aggregate_version,
            recipient: "fake-investigator".into(),
            arbiter: "fake-verifier".into(),
            requested_evidence: "Reproduce exact claim".into(),
            uncertainty: "Fixture only".into(),
            blocking: true,
            acknowledge_seconds: 10,
            response_seconds: 30,
        },
    }
}
#[test]
fn independent_lifecycle_and_version_bound_closure() {
    let (_d, a, _t, id) = setup();
    let claim = a.claim().unwrap().unwrap();
    let finish = RunCommand {
        meta: CommandMeta {
            schema_version: V1,
            idempotency_key: "finish-after-challenge".into(),
            actor: claim.executor.clone(),
            reason: "Synthetic result".into(),
        },
        proof: RunProof {
            run_id: claim.run_id,
            fence: claim.fence,
        },
    };
    let op = open(&a, &id);
    let c = a.challenge(&id, op.clone()).unwrap();
    assert_eq!(a.challenge(&id, op).unwrap().id, c.id);
    assert!(a.finish_fake(finish.clone()).is_err());
    let mut stale = cmd(&a, &id, "ack", "fake-investigator");
    stale.expected_version -= 1;
    assert!(
        a.challenge(
            &id,
            ChallengeCommand {
                command: stale,
                action: ChallengeAction::Acknowledge { id: c.id.clone() }
            }
        )
        .is_err()
    );
    for (key, action) in [
        ("ack", ChallengeAction::Acknowledge { id: c.id.clone() }),
        (
            "investigate",
            ChallengeAction::Investigate { id: c.id.clone() },
        ),
    ] {
        a.challenge(
            &id,
            ChallengeCommand {
                command: cmd(&a, &id, key, "fake-investigator"),
                action,
            },
        )
        .unwrap();
    }
    let proof = evidence(&a, &id, "respondent-observation", "fake-investigator");
    a.challenge(
        &id,
        ChallengeCommand {
            command: cmd(&a, &id, "answer", "fake-investigator"),
            action: ChallengeAction::Answer {
                id: c.id.clone(),
                answer: "Observed fixture result".into(),
                evidence_refs: vec![proof.clone()],
            },
        },
    )
    .unwrap();
    let resolve = ChallengeAction::Resolve {
        id: c.id.clone(),
        disposition: "Claim bounded to fixture".into(),
        uncertainty: "No production evidence".into(),
        evidence_refs: vec![proof],
    };
    assert!(
        a.challenge(
            &id,
            ChallengeCommand {
                command: cmd(&a, &id, "self-resolve", "fake-investigator"),
                action: resolve.clone()
            }
        )
        .is_err()
    );
    assert!(
        a.challenge(
            &id,
            ChallengeCommand {
                command: cmd(&a, &id, "repeat-answer", "fake-verifier"),
                action: resolve
            }
        )
        .is_err()
    );
    let independent = evidence(&a, &id, "verifier-experiment", "fake-verifier");
    a.challenge(
        &id,
        ChallengeCommand {
            command: cmd(&a, &id, "resolve", "fake-verifier"),
            action: ChallengeAction::Resolve {
                id: c.id,
                disposition: "Fixture experiment confirmed".into(),
                uncertainty: "Service outcome unknown".into(),
                evidence_refs: vec![independent.clone()],
            },
        },
    )
    .unwrap();
    a.finish_fake(finish).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().challenges[0].state, "resolved");
    assert!(
        a.challenge(
            &id,
            ChallengeCommand {
                command: cmd(&a, &id, "reopen-resolved", "fake-investigator"),
                action: ChallengeAction::Acknowledge {
                    id: a.snapshot(&id).unwrap().challenges[0].id.clone()
                }
            }
        )
        .is_err()
    );
}
#[test]
fn deadline_restart_and_independent_gate() {
    let (d, a, t, id) = setup();
    let c = a.challenge(&id, open(&a, &id)).unwrap();
    t.advance(Duration::seconds(11));
    let a = Application::new(Store::open(d.path().join("test.db")).unwrap(), Arc::new(t));
    assert!(a.supervise_team().unwrap() > 0);
    assert_eq!(a.supervise_team().unwrap(), 0);
    assert_eq!(a.snapshot(&id).unwrap().challenges[0].state, "escalated");
    assert_eq!(a.escalation_inbox().unwrap()[0].source_id, c.id);
    let (_d, a, _t, id) = setup();
    a.run_fake_worker_once("fake-investigator", false).unwrap();
    let proof = evidence(&a, &id, "independent", "fake-verifier");
    a.record_gate(
        &id,
        GateCommand {
            command: cmd(&a, &id, "validate", "fake-verifier"),
            close: false,
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap();
    evidence(&a, &id, "changed-claim", "operator");
    assert!(
        a.record_gate(
            &id,
            GateCommand {
                command: cmd(&a, &id, "stale-close", "operator"),
                close: true,
                evidence_refs: vec![proof.clone()]
            }
        )
        .is_err()
    );
    a.record_gate(
        &id,
        GateCommand {
            command: cmd(&a, &id, "revalidate", "fake-verifier"),
            close: false,
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap();
    let g = a
        .record_gate(
            &id,
            GateCommand {
                command: cmd(&a, &id, "close", "operator"),
                close: true,
                evidence_refs: vec![proof],
            },
        )
        .unwrap();
    assert_eq!(
        g.closed_version,
        Some(a.snapshot(&id).unwrap().case.version)
    );
}
#[test]
fn competing_commands_and_injected_failure_preserve_atomic_review() {
    let (d, a, _t, id) = setup();
    let one = open(&a, &id);
    let mut two = one.clone();
    two.command.meta.idempotency_key = "competing-open".into();
    let first = a.clone();
    let other = a.clone();
    let x = id.clone();
    let y = id.clone();
    let t = std::thread::spawn(move || first.challenge(&x, one));
    let u = std::thread::spawn(move || other.challenge(&y, two));
    assert_eq!(
        [t.join().unwrap().is_ok(), u.join().unwrap().is_ok()]
            .into_iter()
            .filter(|v| *v)
            .count(),
        1
    );
    let c = a.snapshot(&id).unwrap().challenges[0].clone();
    let before = a.snapshot(&id).unwrap().case.version;
    let db = Store::open(d.path().join("test.db"))
        .unwrap()
        .connect()
        .unwrap();
    db.execute_batch("CREATE TRIGGER fail_review BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='challenge.changed' BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    let ack = ChallengeCommand {
        command: cmd(&a, &id, "atomic-ack", "fake-investigator"),
        action: ChallengeAction::Acknowledge { id: c.id },
    };
    assert!(a.challenge(&id, ack.clone()).is_err());
    assert_eq!(a.snapshot(&id).unwrap().case.version, before);
    assert_eq!(a.snapshot(&id).unwrap().challenges[0].state, "open");
    db.execute_batch("DROP TRIGGER fail_review").unwrap();
    a.challenge(&id, ack).unwrap();
}
