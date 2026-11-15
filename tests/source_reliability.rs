mod common;
use chrono::Duration;
use common::*;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{
    sync::{Arc, mpsc},
    time::Duration as WallTime,
};
use watchhouse::{
    application::Application,
    error::{Error, Result},
    incident_artifacts::*,
    incident_ops::*,
    sentry::*,
    store::Store,
};
struct Api {
    status: &'static str,
    count: u32,
    offline: bool,
}
fn issue(status: &str, count: u32) -> Value {
    let mut v: Value = serde_json::from_str(include_str!("fixtures/sentry/issue.json")).unwrap();
    v["status"] = status.into();
    v["count"] = count.to_string().into();
    v
}
fn enrichment(status: &str, count: u32) -> Enrichment {
    Enrichment {
        issue: issue(status, count),
        event: serde_json::from_str(include_str!("fixtures/sentry/event.json")).unwrap(),
    }
}
impl SentryApi for Api {
    fn enrich(&self, _: &SentryConfig, _: &str) -> Result<Enrichment> {
        if self.offline {
            return Err(Error::Invalid("Fixture outage".into()));
        }
        Ok(enrichment(self.status, self.count))
    }
    fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
        Ok(Page {
            issues: vec![],
            next_cursor: None,
        })
    }
}
fn notify(a: &Application, status: &str, count: u32) {
    let cfg: SentryConfig =
        serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();
    let raw =
        serde_json::to_vec(&json!({"action":"resolved","data":{"issue":issue(status,count)}}))
            .unwrap();
    let secret = b"sanitized-fixture-secret-32-bytes-only";
    let mut h = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    h.update(&raw);
    a.receive_sentry_verified(
        &raw,
        &format!("{:x}", h.finalize().into_bytes()),
        secret,
        &cfg,
    )
    .unwrap();
}
#[test]
fn known_issue_changes_outside_discovery_window_are_recovered_without_new_incidents() {
    let (_d, a, t, id) = fixture();
    let incident = a.snapshot(&id).unwrap().incident.unwrap().id;
    t.advance(Duration::days(2));
    let api = Api {
        status: "resolved",
        count: 3,
        offline: false,
    };
    assert_eq!(a.reconcile_sentry(&api).unwrap(), 0);
    a.refresh_known_sentry(&api).unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.source.unwrap().signal.status, "resolved");
    assert_eq!(s.incident.as_ref().unwrap().id, incident);
    assert!(s.incident.unwrap().recovered_at.is_none());
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert_eq!(s.source_freshness.unwrap().state, "current_snapshot");
    assert_eq!(a.refresh_known_sentry(&api).unwrap(), 0);
}
#[test]
fn unchanged_refreshes_advance_freshness_without_spamming_case_events_or_invalidating_review() {
    let (_d, a, t, id) = fixture();
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "offer-reviewer", "operator"),
            action: IncidentAction::OfferRole {
                role: IncidentRole::Commander,
                successor: "reviewer".into(),
                acknowledge_seconds: 10,
            },
        },
    )
    .unwrap();
    let offer = a.snapshot(&id).unwrap().incident_role_offers[0].id.clone();
    a.operate_incident(
        &id,
        IncidentOperation {
            command: command(&a, &id, "accept-reviewer", "reviewer"),
            action: IncidentAction::AcceptRole { offer_id: offer },
        },
    )
    .unwrap();
    let draft = a
        .record_incident_artifact(
            &id,
            ArtifactCommand {
                command: command(&a, &id, "draft", "operator"),
                action: ArtifactAction::DraftCommunication {
                    id: None,
                    expected_revision: None,
                    audience: "internal".into(),
                    body: "Source facts unchanged; recovery remains unknown".into(),
                    evidence_refs: evidence(&a, &id),
                    review_seconds: 30,
                },
            },
        )
        .unwrap();
    a.record_incident_artifact(
        &id,
        ArtifactCommand {
            command: command(&a, &id, "approve", "reviewer"),
            action: ArtifactAction::ApproveCommunication {
                id: draft.id.clone(),
                revision: 1,
                evidence_refs: evidence(&a, &id),
            },
        },
    )
    .unwrap();
    let before = a.snapshot(&id).unwrap();
    t.advance(Duration::seconds(61));
    assert_eq!(
        a.refresh_known_sentry(&Api {
            status: "unresolved",
            count: 3,
            offline: false
        })
        .unwrap(),
        0
    );
    let after = a.snapshot(&id).unwrap();
    assert_eq!(after.case.version, before.case.version);
    assert_eq!(after.events.len(), before.events.len());
    assert!(
        after
            .incident_artifacts
            .iter()
            .find(|a| a.id == draft.id)
            .unwrap()
            .approval_current
    );
    assert_eq!(after.source_freshness.unwrap().age_seconds, Some(0));
}
#[test]
fn outage_and_interrupted_read_recover_after_restart_without_false_freshness() {
    let (d, a, t, id) = fixture();
    let first = a.snapshot(&id).unwrap().source_freshness.unwrap();
    t.advance(Duration::seconds(61));
    assert_eq!(
        a.refresh_known_sentry(&Api {
            status: "resolved",
            count: 3,
            offline: true
        })
        .unwrap(),
        0
    );
    let outage = a.snapshot(&id).unwrap().source_freshness.unwrap();
    assert_eq!(outage.state, "unavailable");
    assert_eq!(outage.last_success_at, first.last_success_at);
    assert_eq!(outage.age_seconds, Some(61));
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(t.clone()),
    );
    t.advance(Duration::seconds(61));
    a.refresh_known_sentry(&Api {
        status: "resolved",
        count: 3,
        offline: false,
    })
    .unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source.unwrap().signal.status,
        "resolved"
    );
    assert_eq!(
        a.snapshot(&id).unwrap().source_freshness.unwrap().state,
        "current_snapshot"
    );
    // A claimed read lost before result storage stays distinguishable from a successful read.
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute(
        "UPDATE source_read_state SET generation=generation+1,error=NULL",
        [],
    )
    .unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source_freshness.unwrap().state,
        "refreshing_or_interrupted"
    );
    t.advance(Duration::seconds(61));
    a.refresh_known_sentry(&Api {
        status: "resolved",
        count: 3,
        offline: false,
    })
    .unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source_freshness.unwrap().state,
        "current_snapshot"
    );
}
#[test]
fn overlapping_read_cannot_overwrite_newer_status_and_its_late_evidence_is_unaccepted() {
    let (_d, a, t, id) = fixture();
    t.advance(Duration::seconds(61));
    let (start_tx, start_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    struct Blocking {
        start: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl SentryApi for Blocking {
        fn enrich(&self, _: &SentryConfig, _: &str) -> Result<Enrichment> {
            self.start.send(()).unwrap();
            self.release.recv_timeout(WallTime::from_secs(10)).unwrap();
            Ok(enrichment("unresolved", 3))
        }
        fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
            unreachable!()
        }
    }
    let old = a.clone();
    let thread = std::thread::spawn(move || {
        old.refresh_known_sentry(&Blocking {
            start: start_tx,
            release: release_rx,
        })
    });
    start_rx.recv_timeout(WallTime::from_secs(10)).unwrap();
    notify(&a, "resolved", 4);
    a.process_sentry_intake(&Api {
        status: "resolved",
        count: 4,
        offline: false,
    })
    .unwrap();
    let generation = a
        .snapshot(&id)
        .unwrap()
        .source_freshness
        .unwrap()
        .last_success_generation;
    release_tx.send(()).unwrap();
    thread.join().unwrap().unwrap();
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.source.unwrap().signal.status, "resolved");
    assert_eq!(
        s.source_freshness.unwrap().last_success_generation,
        generation
    );
    assert!(
        s.qualification
            .evidence
            .iter()
            .any(|e| e.provenance == "sentry-v1:unaccepted-source-read")
    );
    assert_eq!(a.list_cases().unwrap().len(), 1);
}
#[test]
fn legacy_configuration_keeps_canonical_bytes_and_failed_projection_replays_atomically() {
    let (d, a, _t, id) = fixture();
    let cfg: SentryConfig =
        serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();
    assert_eq!(cfg.refresh_seconds, 60);
    assert!(
        !serde_json::to_string(&cfg)
            .unwrap()
            .contains("refresh_seconds")
    );
    let mut invalid = cfg.clone();
    invalid.revision = 2;
    invalid.refresh_seconds = 0;
    assert!(a.configure_sentry(invalid, "operator").is_err());
    notify(&a, "resolved", 4);
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_source_projection BEFORE INSERT ON events WHEN json_extract(NEW.envelope,'$.type')='source.observed' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;").unwrap();
    assert!(
        a.process_sentry_intake(&Api {
            status: "resolved",
            count: 4,
            offline: false
        })
        .is_err()
    );
    assert_eq!(
        a.snapshot(&id).unwrap().source.unwrap().signal.status,
        "unresolved"
    );
    assert_eq!(
        a.snapshot(&id).unwrap().source_freshness.unwrap().state,
        "refreshing_or_interrupted"
    );
    db.execute_batch("DROP TRIGGER fail_source_projection")
        .unwrap();
    a.process_sentry_intake(&Api {
        status: "resolved",
        count: 4,
        offline: false,
    })
    .unwrap();
    assert_eq!(
        a.snapshot(&id).unwrap().source.unwrap().signal.status,
        "resolved"
    );
    assert_eq!(a.list_cases().unwrap().len(), 1);
    assert!(!evidence(&a, &id).is_empty());
}
