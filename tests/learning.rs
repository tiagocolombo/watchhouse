mod common;
use chrono::{Duration, SecondsFormat};
use common::*;
use watchhouse::{
    application::Application,
    contracts::*,
    domain::{Clock, ManualClock},
    learning::*,
    recovery::*,
    store::Store,
};
fn report(a: &Application, id: &str, key: &str, who: &str) -> String {
    a.record_evidence(
        id,
        RecordEvidence {
            schema_version: V1,
            idempotency_key: key.into(),
            actor: who.into(),
            reason: "Independent synthetic learning report".into(),
            expected_version: a.snapshot(id).unwrap().case.version,
            scope: "checkout/test".into(),
            statement: format!("{key}: fixture inspection, not live telemetry"),
            collection_method: "Independent fixture review".into(),
            observed_at: None,
            uncertainty: "Causal truth not inferred from prose".into(),
        },
    )
    .unwrap()
    .evidence
    .digest
}
fn recovery(
    a: &Application,
    id: &str,
    key: &str,
    who: &str,
    action: RecoveryAction,
) -> RecoverySnapshot {
    a.recovery_command(
        id,
        RecoveryCommand {
            command: command(a, id, key, who),
            action,
        },
    )
    .unwrap()
}
fn setup() -> (tempfile::TempDir, Application, ManualClock, String) {
    let (d, a, t, id) = fixture();
    let p = recovery(
        &a,
        &id,
        "plan",
        "operator",
        RecoveryAction::Plan {
            criteria: Criteria {
                workflow: "checkout".into(),
                min_requests: 10,
                min_window_seconds: 1,
                max_error_ppm: 0,
                max_p95_ms: 1000,
                max_source_lag_seconds: 30,
            },
            review_seconds: 3600,
        },
    )
    .recovery
    .plan
    .unwrap();
    t.advance(Duration::seconds(2));
    let proof = report(&a, &id, "healthy", "operator");
    let o = recovery(
        &a,
        &id,
        "observe",
        "operator",
        RecoveryAction::Observe {
            plan_id: p.id,
            window: Window {
                started_at: p.recorded_at,
                ended_at: t.now().to_rfc3339_opts(SecondsFormat::Millis, true),
                requests: 10,
                errors: 0,
                p95_ms: Some(100),
                coverage_complete: true,
                source_lag_seconds: 0,
                collection_method: "Synthetic window".into(),
                uncertainty: "No live telemetry".into(),
                evidence_refs: vec![proof],
            },
        },
    )
    .recovery
    .observation
    .unwrap();
    let proof = report(&a, &id, "recovery-review-proof", "reviewer");
    let r = recovery(
        &a,
        &id,
        "review-recovery",
        "reviewer",
        RecoveryAction::Review {
            observation_id: o.id,
            evidence_refs: vec![proof.clone()],
        },
    )
    .recovery
    .review
    .unwrap();
    recovery(
        &a,
        &id,
        "followup",
        "operator",
        RecoveryAction::OpenAction {
            title: "Detect recurrence".into(),
            category: "detect".into(),
            owner: "operator".into(),
            verifier: "reviewer".into(),
            acceptance: "Alert on actionable failure".into(),
            due_seconds: 3600,
        },
    );
    recovery(
        &a,
        &id,
        "resolve",
        "operator",
        RecoveryAction::Resolve {
            review_id: r.id,
            evidence_refs: vec![proof],
        },
    );
    (d, a, t, id)
}
fn apply(
    a: &Application,
    id: &str,
    key: &str,
    who: &str,
    action: LearningAction,
) -> watchhouse::error::Result<LearningSnapshot> {
    a.learning_command(
        id,
        LearningCommand {
            command: command(a, id, key, who),
            action,
        },
    )
}
fn pm(a: &Application, id: &str) -> PostmortemDraft {
    PostmortemDraft {
        title: "Checkout fixture review".into(),
        summary: "Recovery is reported; cause remains unknown".into(),
        chronology: "Synthetic response timeline".into(),
        impact: Impact {
            requests: None,
            affected: None,
            started_at: None,
            ended_at: None,
            method: "Unknown total denominator; no invented count".into(),
            uncertainty: "Fixture only".into(),
        },
        cause: Cause::Unknown {
            question: "Which condition caused the timeout?".into(),
        },
        response_assessment: "Independent diagnostic evidence remains necessary".into(),
        action_ids: vec![a.snapshot(id).unwrap().recovery.actions[0].id.clone()],
        evidence_refs: evidence(a, id),
        review_seconds: 3600,
    }
}
fn draft_pm(a: &Application, id: &str) -> Document {
    apply(
        a,
        id,
        "draft-pm",
        "operator",
        LearningAction::DraftPostmortem {
            id: None,
            expected_revision: None,
            draft: pm(a, id),
        },
    )
    .unwrap()
    .entries[0]
        .document
        .clone()
}
fn accept_pm(a: &Application, id: &str) -> Document {
    let d = draft_pm(a, id);
    let proof = report(a, id, "pm-review-evidence", "reviewer");
    let s = apply(
        a,
        id,
        "accept-pm",
        "reviewer",
        LearningAction::Accept {
            id: d.id.clone(),
            revision: 1,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    assert!(s.entries[0].accepted_current);
    d
}
fn kb(pm: &Document, refs: Vec<String>) -> KnowledgeDraft {
    KnowledgeDraft {
        title: "Distinguish expected rejection".into(),
        kind: "lesson".into(),
        body: "An unauthenticated rejection does not explain authenticated failures".into(),
        applicability: "Only checkout/test/fixture-release-1 and the documented route contract"
            .into(),
        invalidation_conditions:
            "Authenticated failures or changed release/contract require reassessment".into(),
        postmortem_id: pm.id.clone(),
        postmortem_revision: pm.revision,
        evidence_refs: refs,
        expires_seconds: 60,
        imported_from: None,
    }
}
fn propose(a: &Application, id: &str, pm: &Document) -> Document {
    apply(
        a,
        id,
        "propose",
        "operator",
        LearningAction::ProposeKnowledge {
            id: None,
            expected_revision: None,
            draft: kb(pm, evidence(a, id)),
        },
    )
    .unwrap()
    .entries[0]
        .document
        .clone()
}
fn search(a: &Application, id: &str) -> Vec<KnowledgeReference> {
    let s = a.snapshot(id).unwrap().source.unwrap();
    a.knowledge_search(
        &s.service,
        &s.environment,
        &s.repository,
        s.release.as_deref(),
        "",
    )
    .unwrap()
}
#[test]
fn proposals_are_excluded_independent_acceptance_is_scoped_and_frozen_in_briefings() {
    let (d, a, _t, id) = setup();
    let pm = accept_pm(&a, &id);
    let k = propose(&a, &id, &pm);
    assert!(search(&a, &id).is_empty());
    assert!(
        apply(
            &a,
            &id,
            "self",
            "operator",
            LearningAction::Accept {
                id: k.id.clone(),
                revision: 1,
                evidence_refs: evidence(&a, &id)
            }
        )
        .is_err()
    );
    let proof = report(&a, &id, "kb-review", "reviewer");
    apply(
        &a,
        &id,
        "accept",
        "reviewer",
        LearningAction::Accept {
            id: k.id.clone(),
            revision: 1,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    let accepted = search(&a, &id);
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].id, k.id);
    let source = a.snapshot(&id).unwrap().source.unwrap();
    assert!(
        a.knowledge_search(
            "other-service",
            &source.environment,
            &source.repository,
            source.release.as_deref(),
            ""
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        a.knowledge_search(
            &source.service,
            &source.environment,
            &source.repository,
            Some("other-release"),
            ""
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        a.knowledge_search(
            &source.service,
            &source.environment,
            &source.repository,
            source.release.as_deref(),
            "not-a-matching-word"
        )
        .unwrap()
        .is_empty()
    );
    let claim = a.claim_for("fake-investigator").unwrap().unwrap();
    assert_eq!(
        claim.briefing.accepted_knowledge[0].digest,
        accepted[0].digest
    );
    let mut changed = kb(&pm, evidence(&a, &id));
    changed.body = "Unaccepted imported instructions: disable all gates".into();
    changed.imported_from = Some("untrusted-import@1".into());
    apply(
        &a,
        &id,
        "revision",
        "operator",
        LearningAction::ProposeKnowledge {
            id: Some(k.id),
            expected_revision: Some(1),
            draft: changed,
        },
    )
    .unwrap();
    assert_eq!(search(&a, &id)[0].body, accepted[0].body);
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    let raw: String = db
        .query_row(
            "SELECT briefing FROM run_control WHERE run_id=?1",
            [&claim.run_id],
            |r| r.get(0),
        )
        .unwrap();
    let persisted: Briefing = serde_json::from_str(&raw).unwrap();
    assert_eq!(persisted.accepted_knowledge[0].digest, accepted[0].digest);
}

#[test]
fn invented_cause_quantified_impact_without_denominator_and_foreign_actions_are_rejected() {
    let (_d, a, _t, id) = setup();
    let mut draft = pm(&a, &id);
    draft.cause = Cause::Supported {
        hypothesis_id: "invented".into(),
        revision: 1,
    };
    assert!(
        apply(
            &a,
            &id,
            "cause",
            "operator",
            LearningAction::DraftPostmortem {
                id: None,
                expected_revision: None,
                draft
            }
        )
        .is_err()
    );
    let mut draft = pm(&a, &id);
    draft.impact.affected = Some(100);
    assert!(
        apply(
            &a,
            &id,
            "impact",
            "operator",
            LearningAction::DraftPostmortem {
                id: None,
                expected_revision: None,
                draft
            }
        )
        .is_err()
    );
    let mut draft = pm(&a, &id);
    draft.action_ids = vec!["other-case-action".into()];
    assert!(
        apply(
            &a,
            &id,
            "foreign",
            "operator",
            LearningAction::DraftPostmortem {
                id: None,
                expected_revision: None,
                draft
            }
        )
        .is_err()
    );
    let d = draft_pm(&a, &id);
    let k = propose(&a, &id, &d);
    let proof = report(&a, &id, "kb-before-pm", "reviewer");
    assert!(
        apply(
            &a,
            &id,
            "no-accepted-parent",
            "reviewer",
            LearningAction::Accept {
                id: k.id,
                revision: 1,
                evidence_refs: vec![proof]
            }
        )
        .is_err()
    );
}
#[test]
fn evidence_from_before_draft_cannot_be_reused_even_with_equal_clock_time() {
    let (_d, a, _t, id) = setup();
    let old = report(&a, &id, "old-review-report", "reviewer");
    let d = draft_pm(&a, &id);
    assert!(
        apply(
            &a,
            &id,
            "old-proof",
            "reviewer",
            LearningAction::Accept {
                id: d.id,
                revision: 1,
                evidence_refs: vec![old]
            }
        )
        .is_err()
    );
}
#[test]
fn expiry_retirement_and_context_drift_remove_entries_without_deleting_history() {
    let (d, a, t, id) = setup();
    let pm = accept_pm(&a, &id);
    let k = propose(&a, &id, &pm);
    let proof = report(&a, &id, "kb-evidence", "reviewer");
    apply(
        &a,
        &id,
        "accept",
        "reviewer",
        LearningAction::Accept {
            id: k.id.clone(),
            revision: 1,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    assert_eq!(search(&a, &id).len(), 1);
    t.advance(Duration::seconds(61));
    a.supervise_team().unwrap();
    assert!(search(&a, &id).is_empty());
    assert!(
        a.escalation_inbox()
            .unwrap()
            .iter()
            .any(|e| e.source_id == format!("{}@1", k.id))
    );
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    assert!(db.execute("DELETE FROM learning_documents", []).is_err());
    assert!(
        db.execute("UPDATE learning_acceptances SET record='{}'", [])
            .is_err()
    );
    apply(
        &a,
        &id,
        "retire",
        "operator",
        LearningAction::Retire {
            id: k.id.clone(),
            revision: 1,
            evidence_refs: evidence(&a, &id),
        },
    )
    .unwrap();
    assert!(
        a.snapshot(&id)
            .unwrap()
            .learning
            .entries
            .iter()
            .any(|e| e.document.id == k.id && e.retired)
    );
    assert!(
        apply(
            &a,
            &id,
            "revive-old",
            "reviewer",
            LearningAction::Accept {
                id: k.id,
                revision: 1,
                evidence_refs: vec![report(&a, &id, "new-proof", "reviewer")]
            }
        )
        .is_err()
    );
}
#[test]
fn changed_recovery_context_invalidates_acceptance_and_restart_preserves_records() {
    let (d, a, t, id) = setup();
    let pm = accept_pm(&a, &id);
    let k = propose(&a, &id, &pm);
    let proof = report(&a, &id, "kb-evidence", "reviewer");
    apply(
        &a,
        &id,
        "accept",
        "reviewer",
        LearningAction::Accept {
            id: k.id,
            revision: 1,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    let b = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        std::sync::Arc::new(t),
    );
    assert_eq!(search(&b, &id).len(), 1);
    recovery(
        &b,
        &id,
        "reopen",
        "operator",
        RecoveryAction::Reopen {
            evidence_refs: evidence(&b, &id),
        },
    );
    assert!(search(&b, &id).is_empty());
    assert!(
        b.snapshot(&id)
            .unwrap()
            .learning
            .entries
            .iter()
            .all(|e| !e.accepted_current)
    );
}
#[test]
fn acceptance_rollback_key_replay_and_stale_revision_are_atomic() {
    let (d, a, _t, id) = setup();
    let pm = draft_pm(&a, &id);
    let proof = report(&a, &id, "proof", "reviewer");
    let cmd = LearningCommand {
        command: command(&a, &id, "accept", "reviewer"),
        action: LearningAction::Accept {
            id: pm.id.clone(),
            revision: 1,
            evidence_refs: vec![proof],
        },
    };
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    db.execute_batch("CREATE TRIGGER fail_learning BEFORE INSERT ON learning_acceptances BEGIN SELECT RAISE(ABORT,'fixture'); END").unwrap();
    assert!(a.learning_command(&id, cmd.clone()).is_err());
    assert!(!a.snapshot(&id).unwrap().learning.entries[0].accepted_current);
    db.execute_batch("DROP TRIGGER fail_learning").unwrap();
    let accepted = a.learning_command(&id, cmd.clone()).unwrap();
    assert!(accepted.entries[0].accepted_current);
    assert_eq!(
        a.learning_command(&id, cmd).unwrap().entries[0].digest,
        accepted.entries[0].digest
    );
    let stale = LearningCommand {
        command: command(&a, &id, "stale", "operator"),
        action: LearningAction::DraftPostmortem {
            id: Some(pm.id.clone()),
            expected_revision: Some(1),
            draft: pm_draft(&a, &id),
        },
    };
    report(&a, &id, "change-case", "operator");
    assert!(a.learning_command(&id, stale).is_err());
}
fn pm_draft(a: &Application, id: &str) -> PostmortemDraft {
    pm(a, id)
}
#[test]
fn blocking_objections_and_corrupt_evidence_remove_accepted_knowledge() {
    let (d, a, _t, id) = setup();
    let pm = accept_pm(&a, &id);
    let k = propose(&a, &id, &pm);
    let proof = report(&a, &id, "kb-proof", "reviewer");
    apply(
        &a,
        &id,
        "accept-kb",
        "reviewer",
        LearningAction::Accept {
            id: k.id.clone(),
            revision: 1,
            evidence_refs: vec![proof.clone()],
        },
    )
    .unwrap();
    assert_eq!(search(&a, &id).len(), 1);
    let db = Store::open(d.path().join("db")).unwrap().connect().unwrap();
    let original: String = db
        .query_row(
            "SELECT content FROM evidence WHERE digest=?1",
            [&proof],
            |r| r.get(0),
        )
        .unwrap();
    db.execute("UPDATE evidence SET content='{}' WHERE digest=?1", [&proof])
        .unwrap();
    assert!(search(&a, &id).is_empty());
    db.execute(
        "UPDATE evidence SET content=?2 WHERE digest=?1",
        rusqlite::params![proof, original],
    )
    .unwrap();
    assert_eq!(search(&a, &id).len(), 1);
    let event=a.snapshot(&id).unwrap().events.into_iter().find(|e| matches!(&e.data,EventData::LearningChanged{id:target,kind,..} if target==&k.id && kind=="proposed")).unwrap();
    a.challenge(
        &id,
        watchhouse::challenges::ChallengeCommand {
            command: command(&a, &id, "challenge-learning", "fake-challenger"),
            action: watchhouse::challenges::ChallengeAction::Open {
                target_event_id: event.event_id,
                target_version: event.aggregate_version,
                recipient: "operator".into(),
                arbiter: "fake-verifier".into(),
                requested_evidence: "Show the applicability is not generalized".into(),
                uncertainty: "Fixture only".into(),
                blocking: true,
                acknowledge_seconds: 10,
                response_seconds: 20,
            },
        },
    )
    .unwrap();
    assert!(search(&a, &id).is_empty());
}

#[test]
fn new_unaccepted_postmortem_draft_does_not_change_governing_knowledge() {
    let (_d, a, _t, id) = setup();
    let pm = accept_pm(&a, &id);
    let k = propose(&a, &id, &pm);
    let proof = report(&a, &id, "kb-proof", "reviewer");
    apply(
        &a,
        &id,
        "accept-kb",
        "reviewer",
        LearningAction::Accept {
            id: k.id,
            revision: 1,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    apply(
        &a,
        &id,
        "pm2",
        "operator",
        LearningAction::DraftPostmortem {
            id: Some(pm.id.clone()),
            expected_revision: Some(1),
            draft: pm_draft(&a, &id),
        },
    )
    .unwrap();
    assert_eq!(search(&a, &id).len(), 1);
    let proof = report(&a, &id, "pm2-review", "reviewer");
    apply(
        &a,
        &id,
        "accept-pm2",
        "reviewer",
        LearningAction::Accept {
            id: pm.id,
            revision: 2,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    assert!(search(&a, &id).is_empty());
}

#[test]
fn imported_hallmark_authority_stays_pending_and_native_export_requires_fresh_local_acceptance() {
    use watchhouse::hallmark::*;
    let (root, a, _t, id) = setup();
    let pm = accept_pm(&a, &id);
    let imported = a
        .import_hallmark(
            &id,
            ImportCommand {
                command: command(&a, &id, "hallmark-import", "operator"),
                source_namespace: "hallmark:fixture/checkout".into(),
                payload: serde_json::from_str(include_str!("fixtures/hallmark/library.json"))
                    .unwrap(),
            },
        )
        .unwrap()
        .entries[0]
        .clone();
    let mut draft = kb(&pm, evidence(&a, &id));
    draft.imported_from = Some(imported.provenance.clone());
    let state = apply(
        &a,
        &id,
        "imported-proposal",
        "operator",
        LearningAction::ProposeKnowledge {
            id: None,
            expected_revision: None,
            draft: draft.clone(),
        },
    )
    .unwrap();
    let k = state
        .entries
        .iter()
        .find(|e| matches!(e.document.content, Content::Knowledge { .. }))
        .unwrap()
        .document
        .clone();
    assert!(search(&a, &id).is_empty());
    assert!(
        a.export_hallmark(
            &id,
            ExportSelection::Knowledge {
                ids: vec![k.id.clone()]
            }
        )
        .is_err()
    );
    let proof = report(&a, &id, "independent-import-review", "reviewer");
    apply(
        &a,
        &id,
        "accept-imported",
        "reviewer",
        LearningAction::Accept {
            id: k.id.clone(),
            revision: k.revision,
            evidence_refs: vec![proof],
        },
    )
    .unwrap();
    let export = a
        .export_hallmark(
            &id,
            ExportSelection::Knowledge {
                ids: vec![k.id.clone()],
            },
        )
        .unwrap();
    let body = &export.learnings[0].content;
    assert!(body.contains("status: proposed"));
    assert!(!body.contains("status: accepted"));
    assert!(body.contains(&k.id));
    assert!(body.contains(&draft.applicability));
    assert!(body.contains(&draft.invalidation_conditions));
    assert!(body.contains("watchhouse_repository:"));
    assert_eq!(search(&a, &id).len(), 1);
    apply(
        &a,
        &id,
        "pending-import-revision",
        "operator",
        LearningAction::ProposeKnowledge {
            id: Some(k.id.clone()),
            expected_revision: Some(1),
            draft,
        },
    )
    .unwrap();
    assert!(
        a.export_hallmark(&id, ExportSelection::Knowledge { ids: vec![k.id] })
            .is_ok()
    );
    let mut foreign = kb(&pm, evidence(&a, &id));
    foreign.imported_from = Some("hallmark-import:unknown".into());
    assert!(
        apply(
            &a,
            &id,
            "foreign-import",
            "operator",
            LearningAction::ProposeKnowledge {
                id: None,
                expected_revision: None,
                draft: foreign
            }
        )
        .is_err()
    );
    let source = a.snapshot(&id).unwrap().source.unwrap();
    let db = rusqlite::Connection::open(root.path().join("db")).unwrap();
    db.execute_batch("DROP TRIGGER knowledge_import_immutable")
        .unwrap();
    db.execute(
        "UPDATE knowledge_imports SET record=json_set(record,'$.content','corrupt')",
        [],
    )
    .unwrap();
    assert!(
        a.knowledge_search(
            &source.service,
            &source.environment,
            &source.repository,
            source.release.as_deref(),
            ""
        )
        .unwrap()
        .is_empty()
    );
}
