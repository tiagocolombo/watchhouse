mod common;
use chrono::Duration;
use common::*;
use std::sync::Arc;
use watchhouse::{application::Application, incident_ops::*, incidents::Severity, store::Store};
fn update(a: &Application, id: &str, key: &str, actor: &str) -> IncidentOperation {
    IncidentOperation {
        command: command(a, id, key, actor),
        action: IncidentAction::Update {
            severity: Severity::Critical,
            impact: "Three failed requests, denominator unknown".into(),
            uncertainty: "No production recovery assessed".into(),
            objective: "Collect discriminating evidence".into(),
            evidence_refs: evidence(a, id),
        },
    }
}
#[test]
fn updates_are_versioned_evidence_bound_and_do_not_rewrite_declaration() {
    let (_d, a, _t, id) = fixture();
    let before = a.snapshot(&id).unwrap();
    let cmd = update(&a, &id, "update", "operator");
    let out = a.operate_incident(&id, cmd.clone()).unwrap();
    assert_eq!(out.revision, 2);
    assert_eq!(a.operate_incident(&id, cmd).unwrap().revision, 2);
    assert_eq!(
        a.snapshot(&id)
            .unwrap()
            .incident
            .unwrap()
            .declaration
            .objective,
        before.incident.unwrap().declaration.objective
    );
    assert!(
        a.incident_markdown(&id)
            .unwrap()
            .contains("Collect discriminating evidence")
    );
    assert!(
        a.snapshot(&id)
            .unwrap()
            .incident
            .unwrap()
            .recovered_at
            .is_none()
    );
    let mut stale = update(&a, &id, "stale", "operator");
    stale.command.expected_version -= 1;
    assert!(a.operate_incident(&id, stale).is_err());
    assert!(
        a.operate_incident(&id, update(&a, &id, "wrong", "not-commander"))
            .is_err()
    );
    let mut missing = update(&a, &id, "missing", "operator");
    if let IncidentAction::Update { evidence_refs, .. } = &mut missing.action {
        evidence_refs.clear()
    }
    assert!(a.operate_incident(&id, missing).is_err());
}
#[test]
fn role_transfer_requires_addressed_acceptance_and_preserves_deadline_across_restart() {
    let (d, a, t, id) = fixture();
    let original = a
        .snapshot(&id)
        .unwrap()
        .incident_operations
        .unwrap()
        .next_update_at;
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "offer", "operator"),
            action: IncidentAction::OfferRole {
                role: IncidentRole::Commander,
                successor: "next-commander".into(),
                acknowledge_seconds: 10,
            },
        },
    )
    .unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.incident_operations.unwrap().roles.commander, "operator");
    let offer = s.incident_role_offers[0].id.clone();
    assert!(
        a.operate_incident(
            &id,
            IncidentOperation {
                command: command(&a, &id, "wrong-accept", "someone-else"),
                action: IncidentAction::AcceptRole {
                    offer_id: offer.clone()
                }
            }
        )
        .is_err()
    );
    let a = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(t));
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "accept", "next-commander"),
            action: IncidentAction::AcceptRole { offer_id: offer },
        },
    )
    .unwrap();
    let ops = a.snapshot(&id).unwrap().incident_operations.unwrap();
    assert_eq!(ops.roles.commander, "next-commander");
    assert_eq!(ops.next_update_at, original);
    assert!(
        a.operate_incident(&id, update(&a, &id, "old-owner-update", "operator"))
            .is_err()
    );
    a.operate_incident(&id, update(&a, &id, "new-owner-update", "next-commander"))
        .unwrap();
}
#[test]
fn deadlines_escalate_each_cycle_without_models_and_updates_never_resolve_past_duties() {
    let (_d, a, t, id) = fixture();
    t.advance(Duration::seconds(1801));
    a.supervise_team().unwrap();
    assert_eq!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .filter(|e| e.source_type == "incident_update")
            .count(),
        1
    );
    assert_eq!(a.supervise_team().unwrap(), 0);
    a.operate_incident(&id, update(&a, &id, "late-update", "operator"))
        .unwrap();
    t.advance(Duration::seconds(901));
    a.supervise_team().unwrap();
    assert_eq!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .filter(|e| e.source_type == "incident_update" && e.state == "open")
            .count(),
        2
    );
    let (_d, b, t, id) = fixture();
    t.advance(Duration::seconds(1801));
    b.operate_incident(&id, update(&b, &id, "late-without-tick", "operator"))
        .unwrap();
    assert_eq!(b.escalation_inbox().unwrap().len(), 1);
}
#[test]
fn expired_role_offer_escalates_without_transfer_and_cannot_admit_fake_roles() {
    let (_d, a, t, id) = fixture();
    assert!(
        a.operate_incident(
            &id,
            IncidentOperation {
                command: command(&a, &id, "fake-offer", "operator"),
                action: IncidentAction::OfferRole {
                    role: IncidentRole::Commander,
                    successor: "fake-verifier".into(),
                    acknowledge_seconds: 10
                }
            }
        )
        .is_err()
    );
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "offer", "operator"),
            action: IncidentAction::OfferRole {
                role: IncidentRole::Commander,
                successor: "successor".into(),
                acknowledge_seconds: 10,
            },
        },
    )
    .unwrap();
    let offer = a.snapshot(&id).unwrap().incident_role_offers[0].id.clone();
    t.advance(Duration::seconds(11));
    a.supervise_team().unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.incident_role_offers[0].state, "expired");
    assert_eq!(s.incident_operations.unwrap().roles.commander, "operator");
    assert!(
        a.operate_incident(
            &id,
            IncidentOperation {
                command: command(&a, &id, "late", "successor"),
                action: IncidentAction::AcceptRole { offer_id: offer }
            }
        )
        .is_err()
    );
}
#[test]
fn concurrent_commands_and_event_failure_preserve_atomic_coordination() {
    let (d, a, _t, id) = fixture();
    let first = update(&a, &id, "one", "operator");
    let second = update(&a, &id, "two", "operator");
    let b = a.clone();
    let c = a.clone();
    let x = id.clone();
    let y = id.clone();
    let one = std::thread::spawn(move || b.operate_incident(&x, first));
    let two = std::thread::spawn(move || c.operate_incident(&y, second));
    assert_eq!(
        [one.join().unwrap().is_ok(), two.join().unwrap().is_ok()]
            .into_iter()
            .filter(|x| *x)
            .count(),
        1
    );
    let before = a.snapshot(&id).unwrap();
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_ops BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='incident.updated' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;").unwrap();
    let cmd = IncidentOperation {
        command: command(&a, &id, "atomic-offer", "operator"),
        action: IncidentAction::OfferRole {
            role: IncidentRole::Operations,
            successor: "ops-human".into(),
            acknowledge_seconds: 10,
        },
    };
    assert!(a.operate_incident(&id, cmd.clone()).is_err());
    let after = a.snapshot(&id).unwrap();
    assert_eq!(after.case.version, before.case.version);
    assert!(after.incident_role_offers.is_empty());
    db.execute_batch("DROP TRIGGER fail_ops").unwrap();
    a.operate_incident(&id, cmd).unwrap();
}
