mod common;
use chrono::Duration;
use common::*;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::Arc;
use watchhouse::{
    application::Application, contracts::*, correlation::*, error::Result, execution::FakeAdapter,
    sentry::*, store::Store,
};
fn cfg() -> SentryConfig {
    serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap()
}
fn event() -> Value {
    serde_json::from_str(include_str!("fixtures/sentry/event.json")).unwrap()
}
struct Api {
    issue: Value,
    event: Value,
}
impl SentryApi for Api {
    fn enrich(&self, _: &SentryConfig, _: &str) -> Result<Enrichment> {
        Ok(Enrichment {
            issue: self.issue.clone(),
            event: self.event.clone(),
        })
    }
    fn page(&self, _: &SentryConfig, _: &str, _: &str, _: Option<&str>) -> Result<Page> {
        Ok(Page {
            issues: vec![],
            next_cursor: None,
        })
    }
}
fn ingest_with_cfg(
    a: &Application,
    n: &str,
    event: Value,
    last_seen: Option<&str>,
    config: &SentryConfig,
) -> String {
    let mut issue: Value =
        serde_json::from_str(include_str!("fixtures/sentry/issue.json")).unwrap();
    issue["id"] = n.into();
    issue["project"]["id"] = config.project_id.clone().into();
    if a.list_cases()
        .unwrap()
        .iter()
        .any(|c| c.title == format!("Sentry issue {n}"))
    {
        issue["count"] = "4".into();
    }
    if let Some(t) = last_seen {
        issue["lastSeen"] = t.into();
    }
    let raw = serde_json::to_vec(&json!({"action":"created","data":{"issue":issue}})).unwrap();
    let secret = b"sanitized-fixture-secret-32-bytes-only";
    let mut h = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    h.update(&raw);
    a.receive_sentry_verified(
        &raw,
        &format!("{:x}", h.finalize().into_bytes()),
        secret,
        config,
    )
    .unwrap();
    a.process_sentry_intake(&Api { issue, event }).unwrap();
    a.list_cases()
        .unwrap()
        .iter()
        .find(|c| c.title == format!("Sentry issue {n}"))
        .unwrap()
        .id
        .clone()
}
fn ingest(a: &Application, n: &str, event: Value, last_seen: Option<&str>) -> String {
    ingest_with_cfg(a, n, event, last_seen, &cfg())
}
fn source_digest(a: &Application, id: &str) -> String {
    let s = a.snapshot(id).unwrap();
    s.events
        .iter()
        .rev()
        .find_map(|e| {
            if let EventData::SourceObserved {
                evidence_refs,
                authoritative: true,
                ..
            } = &e.data
            {
                Some(evidence_refs[0].clone())
            } else {
                None
            }
        })
        .unwrap()
}
fn link(a: &Application, id: &str, target: &str, key: &str) -> RelationCommand {
    RelationCommand {
        command: command(a, id, key, "operator"),
        target_case: target.into(),
        expected_target_version: a.snapshot(target).unwrap().case.version,
        expected_revision: 0,
        expected_topology_revision: None,
        action: RelationAction::Link {
            kind: RelationKind::Related,
            source_evidence: vec![source_digest(a, id)],
            target_evidence: vec![source_digest(a, target)],
            uncertainty: "Only fixture evidence; common cause not established".into(),
        },
    }
}
fn report(a: &Application, id: &str, key: &str, statement: &str) -> String {
    a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: "operator".into(),
            reason: "Evaluate scoped fixture contract".into(),
            expected_version: a.snapshot(id).unwrap().case.version,
            scope: "checkout / test / fixture-release-1".into(),
            statement: statement.into(),
            collection_method: "Manual synthetic fixture replay".into(),
            observed_at: None,
            uncertainty: "Not a live observation".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
#[test]
fn suggestions_are_bounded_scoped_fresh_and_never_merge_or_declare() {
    let (_d, a, t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.correlations.candidates.len(), 1);
    assert_eq!(s.correlations.candidates[0].case_id, target);
    assert!(a.snapshot(&target).unwrap().incident.is_none());
    let mut other = event();
    other["entries"][0]["data"]["values"][0]["stacktrace"]["frames"][0]["function"] =
        "another".into();
    ingest(&a, "1003", other, None);
    let mut unknown = event();
    unknown["release"]["version"] = "unmapped".into();
    ingest(&a, "1004", unknown, None);
    let mut wrong_env = event();
    wrong_env["environment"] = "production".into();
    ingest(&a, "1005", wrong_env, None);
    ingest(&a, "1006", event(), Some("2026-11-14T09:00:00Z"));
    assert_eq!(a.snapshot(&id).unwrap().correlations.candidates.len(), 1);
    let before = a.snapshot(&id).unwrap().case.version;
    a.snapshot(&id).unwrap();
    assert_eq!(a.snapshot(&id).unwrap().case.version, before);
    t.advance(Duration::seconds(61));
    assert!(a.snapshot(&id).unwrap().correlations.candidates.is_empty());
    assert_eq!(a.list_cases().unwrap().len(), 6);
}
#[test]
fn rich_evidence_is_allowlisted_bounded_and_legacy_compatible() {
    let (_d, a, _t, _) = fixture();
    let mut e = event();
    let frame = json!({"module":"checkout","function":"submit","vars":{"secret":"OMIT"},"filename":"/private/OMIT.rs","context_line":"OMIT"});
    e["entries"][0]["data"]["values"][0]["stacktrace"]["frames"] = Value::Array(vec![frame; 100]);
    e["entries"][1]["data"]["values"] = json!([{"category":"http","message":"OMIT"},{"category":"secret-token-OMIT"},{"category":"http"}]);
    let id = ingest(&a, "1002", e, None);
    let s = a.snapshot(&id).unwrap();
    let source = s.source.unwrap();
    let d = source.diagnostic.as_ref().unwrap();
    assert_eq!(d.frames.len(), 20);
    assert_eq!(d.breadcrumb_categories, vec!["http"]);
    assert!(d.event_id.is_some());
    let content = a.evidence(&source_digest(&a, &id)).unwrap();
    assert!(!serde_json::to_string(&content).unwrap().contains("OMIT"));
    let mut legacy = serde_json::to_value(&source).unwrap();
    legacy.as_object_mut().unwrap().remove("diagnostic");
    let old: SourceObservation = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(old).unwrap(), legacy);
    let mut evil = event();
    evil["entries"][0]["data"]["values"][0]["stacktrace"]["frames"][0]["module"] =
        "https://example.invalid/private?token=OMIT".into();
    let x = ingest(&a, "1003", evil, None);
    assert!(
        a.snapshot(&x)
            .unwrap()
            .source
            .unwrap()
            .diagnostic
            .unwrap()
            .frames
            .is_empty()
    );
}
#[test]
fn links_are_atomic_versioned_reversible_and_do_not_transfer_duties() {
    let (d, a, t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let before = a.snapshot(&id).unwrap();
    let other_before = a.snapshot(&target).unwrap();
    let cmd = link(&a, &id, &target, "link");
    let r = a.correlate_case(&id, cmd.clone()).unwrap();
    assert!(r.context_current);
    assert_eq!(a.correlate_case(&id, cmd).unwrap().revision, 1);
    let after = a.snapshot(&id).unwrap();
    let other = a.snapshot(&target).unwrap();
    assert_eq!(after.case.version, before.case.version + 1);
    assert_eq!(other.case.version, other_before.case.version + 1);
    assert_eq!(
        after.incident_operations.unwrap().next_update_at,
        before.incident_operations.unwrap().next_update_at
    );
    assert!(other.incident.is_none());
    assert!(after.correlations.candidates.is_empty());
    let restarted = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(t));
    let mut revoke = link(&restarted, &target, &id, "revoke");
    revoke.expected_revision = 1;
    revoke.action = RelationAction::Revoke;
    assert!(
        !restarted
            .correlate_case(&target, revoke.clone())
            .unwrap()
            .active
    );
    assert_eq!(
        restarted.correlate_case(&target, revoke).unwrap().revision,
        2
    );
    assert!(!restarted.snapshot(&id).unwrap().correlations.relations[0].active);
    assert_eq!(
        restarted
            .snapshot(&id)
            .unwrap()
            .correlations
            .candidates
            .len(),
        1
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert!(
        db.execute("UPDATE case_correlations SET revision=99", [])
            .is_err()
    );
    assert!(db.execute("DELETE FROM case_correlations", []).is_err());
    assert!(
        restarted
            .qualification_markdown(&id)
            .unwrap()
            .contains("Correlations")
    );
}
#[test]
fn stale_conflicting_and_foreign_evidence_cannot_link_and_event_failure_rolls_back_both_cases() {
    let (d, a, _t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let cmd = link(&a, &id, &target, "atomic");
    let mut stale = cmd.clone();
    stale.expected_target_version -= 1;
    assert!(a.correlate_case(&id, stale).is_err());
    let mut foreign = cmd.clone();
    if let RelationAction::Link {
        source_evidence, ..
    } = &mut foreign.action
    {
        *source_evidence = vec![source_digest(&a, &target)]
    };
    assert!(a.correlate_case(&id, foreign).is_err());
    let mut fake = cmd.clone();
    fake.command.meta.actor = "fake-investigator".into();
    assert!(a.correlate_case(&id, fake).is_err());
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch(&format!("CREATE TRIGGER reject_target BEFORE INSERT ON events WHEN NEW.case_id='{target}' AND json_extract(NEW.envelope,'$.type')='case.correlated' BEGIN SELECT RAISE(ABORT,'fixture rollback'); END;")).unwrap();
    assert!(a.correlate_case(&id, cmd.clone()).is_err());
    assert!(a.snapshot(&id).unwrap().correlations.relations.is_empty());
    assert_eq!(
        a.snapshot(&id).unwrap().case.version,
        cmd.command.expected_version
    );
    assert_eq!(
        a.snapshot(&target).unwrap().case.version,
        cmd.expected_target_version
    );
    db.execute_batch("DROP TRIGGER reject_target").unwrap();
    a.correlate_case(&id, cmd).unwrap();
    let mut conflict = link(&a, &target, &id, "reverse");
    conflict.expected_revision = 0;
    assert!(a.correlate_case(&target, conflict).is_err());
}
#[test]
fn changed_facts_invalidate_links_but_replay_cannot_rewrite_or_close_them() {
    let (_d, a, _t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let cmd = link(&a, &id, &target, "first");
    a.correlate_case(&id, cmd.clone()).unwrap();
    let mut changed = event();
    changed["release"]["version"] = "unknown-build".into();
    ingest(&a, "1002", changed, None);
    let s = a.snapshot(&id).unwrap();
    assert!(s.correlations.relations[0].active);
    assert!(!s.correlations.relations[0].context_current);
    assert!(
        a.snapshot(&id)
            .unwrap()
            .incident
            .unwrap()
            .recovered_at
            .is_none()
    );
    assert_eq!(a.correlate_case(&id, cmd).unwrap().revision, 1);
    assert!(!a.snapshot(&id).unwrap().correlations.relations[0].context_current);
    let mut renewed = link(&a, &id, &target, "renew");
    renewed.expected_revision = 1;
    a.correlate_case(&id, renewed).unwrap();
    assert!(a.snapshot(&id).unwrap().correlations.relations[0].context_current);
}
#[test]
fn checkout_fixture_qualifies_expected_behavior_without_treating_status_or_synthetic_results_as_proof()
 {
    let (_d, a, t, id) = fixture();
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/sentry/checkout-qualification.json")).unwrap();
    let target = ingest(&a, "1002", event(), None);
    let mut duplicate = link(&a, &target, &id, "duplicate");
    if let RelationAction::Link { kind, .. } = &mut duplicate.action {
        *kind = RelationKind::Duplicate;
    }
    assert!(a.correlate_case(&target, duplicate).is_err());
    let left = report(
        &a,
        &target,
        "left-contract",
        corpus["defect"]["statement"].as_str().unwrap(),
    );
    let right = report(
        &a,
        &id,
        "right-contract",
        corpus["defect"]["statement"].as_str().unwrap(),
    );
    let mut duplicate = link(&a, &target, &id, "duplicate-reviewed");
    if let RelationAction::Link {
        kind,
        source_evidence,
        target_evidence,
        ..
    } = &mut duplicate.action
    {
        *kind = RelationKind::Duplicate;
        source_evidence.push(left);
        target_evidence.push(right);
    }
    a.correlate_case(&target, duplicate).unwrap();
    let mut expected = event();
    expected["entries"][0]["data"]["values"][0]["type"] =
        corpus["expected"]["exception_type"].clone();
    expected["entries"][0]["data"]["values"][0]["stacktrace"]["frames"][0]["module"] =
        corpus["expected"]["module"].clone();
    expected["entries"][0]["data"]["values"][0]["stacktrace"]["frames"][0]["function"] =
        corpus["expected"]["function"].clone();
    let auth = ingest(&a, "1003", expected, None);
    while a.run_once(&FakeAdapter).unwrap() {}
    let make_decision = |refs: Vec<String>, key: &str| RecordDisposition {
        schema_version: V1,
        idempotency_key: key.into(),
        actor: "operator".into(),
        reason: "Contract-specific rejection, not inferred from HTTP status".into(),
        expected_version: a.snapshot(&auth).unwrap().case.version,
        owner: "operator".into(),
        scope: corpus["scope"].as_str().unwrap().into(),
        uncertainty: corpus["expected"]["uncertainty"].as_str().unwrap().into(),
        evidence_refs: refs,
        reconsideration_conditions: "Reassess changed request contract or evidence".into(),
        reopen_after_occurrences: 1,
        plan: QualificationPlan::ExpectedBehavior { after_seconds: 10 },
    };
    assert!(
        a.record_disposition(
            &auth,
            make_decision(vec![source_digest(&a, &auth)], "source-alone")
        )
        .is_err()
    );
    let evidence = report(
        &a,
        &auth,
        "auth-contract",
        corpus["expected"]["statement"].as_str().unwrap(),
    );
    a.record_disposition(
        &auth,
        make_decision(vec![source_digest(&a, &auth), evidence], "qualified"),
    )
    .unwrap();
    assert!(
        a.snapshot(&auth)
            .unwrap()
            .correlations
            .candidates
            .is_empty()
    );
    t.advance(Duration::seconds(11));
    a.process_due_obligations().unwrap();
    assert_eq!(
        a.snapshot(&auth).unwrap().qualification.dispositions[0].state,
        DispositionState::Reopened
    );
    assert_eq!(a.list_cases().unwrap().len(), 3);
}
#[test]
fn different_trusted_scope_cannot_be_linked_and_competing_commands_serialize() {
    let (_d, a, _t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let first = link(&a, &id, &target, "compete-one");
    let second = link(&a, &id, &target, "compete-two");
    let b = a.clone();
    let c = a.clone();
    let x = id.clone();
    let y = id.clone();
    let one = std::thread::spawn(move || b.correlate_case(&x, first));
    let two = std::thread::spawn(move || c.correlate_case(&y, second));
    assert_eq!(
        [one.join().unwrap().is_ok(), two.join().unwrap().is_ok()]
            .iter()
            .filter(|x| **x)
            .count(),
        1
    );
    let mut config = cfg();
    config.revision = 2;
    config.environment = "production".into();
    config.service = "another-service".into();
    a.configure_sentry(config.clone(), "operator").unwrap();
    let mut e = event();
    e["environment"] = "production".into();
    let other = ingest_with_cfg(&a, "1003", e, None, &config);
    assert!(
        a.snapshot(&other)
            .unwrap()
            .correlations
            .candidates
            .is_empty()
    );
    assert!(
        a.correlate_case(&id, link(&a, &id, &other, "wrong-scope"))
            .is_err()
    );
}
#[test]
fn duplicate_disposition_reopens_when_the_exact_catalogue_judgment_changes() {
    let (_d, a, _t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    while a.run_once(&FakeAdapter).unwrap() {}
    let left = report(
        &a,
        &id,
        "left-duplicate-report",
        "Reported same fixture failure",
    );
    let right = report(
        &a,
        &target,
        "right-duplicate-report",
        "Reported same fixture failure",
    );
    let mut cmd = link(&a, &id, &target, "duplicate-link");
    if let RelationAction::Link {
        kind,
        source_evidence,
        target_evidence,
        ..
    } = &mut cmd.action
    {
        *kind = RelationKind::Duplicate;
        source_evidence.push(left.clone());
        target_evidence.push(right);
    }
    a.correlate_case(&id, cmd).unwrap();
    let d = RecordDisposition {
        schema_version: V1,
        idempotency_key: "duplicate-disposition".into(),
        actor: "operator".into(),
        reason: "Exact accepted duplicate relation".into(),
        expected_version: a.snapshot(&id).unwrap().case.version,
        owner: "operator".into(),
        scope: "checkout / test".into(),
        uncertainty: "Separate impact/incident and follow-up duties remain".into(),
        evidence_refs: vec![source_digest(&a, &id), left],
        reconsideration_conditions: "Changed or revoked relation, new evidence or deadline".into(),
        reopen_after_occurrences: 1,
        plan: QualificationPlan::Duplicate {
            after_seconds: 60,
            target_case: target.clone(),
            relation_revision: 1,
        },
    };
    a.record_disposition(&id, d).unwrap();
    assert_eq!(a.process_due_obligations().unwrap(), 0);
    let mut revoke = link(&a, &id, &target, "revoke-duplicate");
    revoke.expected_revision = 1;
    revoke.action = RelationAction::Revoke;
    a.correlate_case(&id, revoke).unwrap();
    assert_eq!(a.process_due_obligations().unwrap(), 1);
    assert_eq!(a.process_due_obligations().unwrap(), 0);
    assert_eq!(
        a.snapshot(&id).unwrap().qualification.dispositions[0].state,
        DispositionState::Reopened
    );
    assert!(
        a.snapshot(&id)
            .unwrap()
            .incident
            .unwrap()
            .recovered_at
            .is_none()
    );
}
#[test]
fn cross_service_hints_require_explicit_topology_and_never_become_duplicate_or_causal_authority() {
    use watchhouse::topology::{Dependency, Policy, Scope};
    let (d, a, t, id) = fixture();
    let mut config = cfg();
    config.revision = 2;
    config.service = "payments".into();
    config.repository = "example/payments-fixture".into();
    config.project_id = "43".into();
    config.read_policy = Some(ReadPolicy {
        schema_version: V1,
        request_seconds: 5,
        lookback_seconds: 86400,
        overlap_seconds: 300,
        max_pages: 10,
    });
    a.configure_sentry_project(
        config.clone(),
        "operator",
        "independent-project-contract-fixture",
    )
    .unwrap();
    let target = ingest_with_cfg(&a, "1002", event(), None, &config);
    assert!(a.snapshot(&id).unwrap().correlations.candidates.is_empty());
    let p = Policy {
        schema_version: V1,
        revision: 1,
        paused: false,
        max_gap_seconds: 300,
        dependencies: vec![Dependency {
            caller: Scope {
                service: "checkout".into(),
                environment: "test".into(),
                repository: "example/fixture-service".into(),
            },
            dependency: Scope {
                service: "payments".into(),
                environment: "test".into(),
                repository: "example/payments-fixture".into(),
            },
            rationale: "Sanitized contract mapping; not live topology discovery".into(),
        }],
    };
    assert!(
        a.install_service_topology(p.clone(), "fake-engineer", "self-edit")
            .is_err()
    );
    a.install_service_topology(p.clone(), "operator", "synthetic-dependency-map")
        .unwrap();
    let hints = a.snapshot(&id).unwrap().correlations.candidates;
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].case_id, target);
    assert_eq!(hints[0].topology_revision, Some(1));
    assert!(hints[0].reason.contains("not a shared stack signature"));
    let mut cmd = link(&a, &id, &target, "cross-review");
    assert!(a.correlate_case(&id, cmd.clone()).is_err());
    cmd.expected_topology_revision = Some(1);
    let mut duplicate = cmd.clone();
    duplicate.command.meta.idempotency_key = "cross-duplicate-denied".into();
    if let RelationAction::Link { kind, .. } = &mut duplicate.action {
        *kind = RelationKind::Duplicate;
    }
    assert!(a.correlate_case(&id, duplicate).is_err());
    let accepted = a.correlate_case(&id, cmd).unwrap();
    assert!(accepted.context_current);
    assert_eq!(accepted.topology_revision, Some(1));
    let mut paused = p.clone();
    paused.revision = 2;
    paused.paused = true;
    a.install_service_topology(paused, "operator", "pause-dependency-review")
        .unwrap();
    assert!(!a.snapshot(&id).unwrap().correlations.relations[0].context_current);
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    assert!(db.execute("DELETE FROM service_topologies", []).is_err());
    assert!(
        a.install_service_topology(p, "operator", "idempotent-replay")
            .is_ok()
    );
    let restarted = Application::new(Store::open(d.path().join("db")).unwrap(), Arc::new(t));
    assert_eq!(restarted.service_topology().unwrap().unwrap().revision, 2);
    assert_eq!(restarted.snapshot(&target).unwrap().case.owner, "operator");
    // Independent project transports/mappings are exercised by the project adapter fixtures.
    // This injected API result is not a live multi-service activation claim.
}
#[test]
fn candidate_output_is_capped_and_blocking_challenges_prevent_acceptance_but_not_revocation() {
    use watchhouse::challenges::*;
    let (_d, a, _t, id) = fixture();
    for n in 2000..2022 {
        ingest(&a, &n.to_string(), event(), None);
    }
    let candidates = a.snapshot(&id).unwrap().correlations.candidates;
    assert_eq!(candidates.len(), 20);
    let target = &candidates[0].case_id;
    a.correlate_case(&id, link(&a, &id, target, "linked"))
        .unwrap();
    let claim = a.snapshot(target).unwrap().events[0].clone();
    a.challenge(
        target,
        ChallengeCommand {
            command: command(&a, target, "challenge", "fake-challenger"),
            action: ChallengeAction::Open {
                target_event_id: claim.event_id,
                target_version: claim.aggregate_version,
                recipient: "fake-investigator".into(),
                arbiter: "fake-verifier".into(),
                requested_evidence: "Distinct failure or common cause?".into(),
                uncertainty: "Diagnostic signature is not causal proof".into(),
                blocking: true,
                acknowledge_seconds: 10,
                response_seconds: 30,
            },
        },
    )
    .unwrap();
    let mut renewed = link(&a, &id, target, "blocked");
    renewed.expected_revision = 1;
    assert!(a.correlate_case(&id, renewed).is_err());
    let mut revoke = link(&a, &id, target, "revoke-challenged");
    revoke.expected_revision = 1;
    revoke.action = RelationAction::Revoke;
    assert!(!a.correlate_case(&id, revoke).unwrap().active);
}
#[tokio::test]
async fn http_and_cli_share_correlation_versions_receipts_and_literal_reports() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let (d, a, _t, id) = fixture();
    let target = ingest(&a, "1002", event(), None);
    let mut cmd = link(&a, &id, &target, "http-link");
    cmd.command.meta.reason = "<script>window.linkInjected=true</script> `fixture`".into();
    let token = "test-correlation-token-32-bytes-only";
    let router = watchhouse::http::router(a.clone(), token.into(), "127.0.0.1:8787".into());
    let path = format!("/api/v1/cases/{id}/correlations");
    let request = |method: &str, path: &str, body: String, authenticated: bool| {
        let mut r = Request::builder()
            .method(method)
            .uri(path)
            .header("Host", "127.0.0.1:8787")
            .header("Content-Type", "application/json");
        if authenticated {
            r = r.header("Authorization", format!("Bearer {token}"));
        }
        r.body(Body::from(body)).unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(request(
                "POST",
                &path,
                serde_json::to_string(&cmd).unwrap(),
                false
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let posted = router
        .clone()
        .oneshot(request(
            "POST",
            &path,
            serde_json::to_string(&cmd).unwrap(),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(posted.status(), StatusCode::OK);
    let bytes = posted.into_body().collect().await.unwrap().to_bytes();
    let r: Relation = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(r.revision, 1);
    let file = d.path().join("correlation.json");
    std::fs::write(&file, serde_json::to_vec(&cmd).unwrap()).unwrap();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_watchhouse"))
        .arg("--db")
        .arg(d.path().join("db"))
        .args(["case", "correlate", &id, "--file"])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Relation>(&run.stdout)
            .unwrap()
            .revision,
        1
    );
    let report = router
        .oneshot(request(
            "GET",
            &format!("/api/v1/cases/{id}/qualification/markdown"),
            String::new(),
            true,
        ))
        .await
        .unwrap();
    assert_eq!(report.status(), StatusCode::OK);
    assert_eq!(
        report.headers()["content-type"],
        "text/markdown; charset=utf-8"
    );
    let text = String::from_utf8(
        report
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(!text.contains("<script>"));
    assert!(text.contains("linkInjected"));
}
#[test]
fn snapshot_link_cap_does_not_resuggest_an_older_active_pair() {
    let (_d, a, _t, id) = fixture();
    for n in 3000..3101 {
        let target = ingest(&a, &n.to_string(), event(), None);
        a.correlate_case(&id, link(&a, &id, &target, &format!("pair-{n}")))
            .unwrap();
    }
    let s = a.snapshot(&id).unwrap();
    assert_eq!(s.correlations.relations.len(), 100);
    assert!(s.correlations.candidates.is_empty());
}
