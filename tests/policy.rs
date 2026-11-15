use chrono::{Duration, TimeZone, Utc};
use std::sync::Arc;
use tempfile::TempDir;
use watchhouse::{
    application::Application, domain::ManualClock, intake::FakeSource, policy::Policy, store::Store,
};
#[test]
fn revisions_are_validated_immutable_and_pinned_across_restart() {
    let d = TempDir::new().unwrap();
    let path = d.path().join("policy.db");
    let clock = ManualClock::new(Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap());
    let a = Application::new(Store::open(&path).unwrap(), Arc::new(clock.clone()));
    let old = a
        .create_case(FakeSource.manual_command("old-policy"))
        .unwrap();
    let before = a.snapshot(&old.id).unwrap();
    let mut p: Policy = serde_json::from_str(include_str!("../policy/local-fake-v1.json")).unwrap();
    p.revision = 2;
    p.name = "operator-fake-profile-v2".into();
    p.lease_seconds = 5;
    p.progress_seconds = 10;
    p.response_seconds = 15;
    p.assignment_seconds = 30;
    p.max_attempts = 2;
    assert!(
        a.install_policy(p.clone(), "fake-investigator", "untrusted candidate")
            .is_err()
    );
    let mut bad = p.clone();
    bad.progress_seconds = 1;
    assert!(a.install_policy(bad, "operator", "fixture config").is_err());
    a.install_policy(p.clone(), "operator", "trusted explicit fixture config")
        .unwrap();
    a.install_policy(p.clone(), "operator", "retry").unwrap();
    let mut changed = p.clone();
    changed.max_attempts = 9;
    assert!(
        a.install_policy(changed, "operator", "changed bytes")
            .is_err()
    );
    let new = a
        .create_case(FakeSource.manual_command("new-policy"))
        .unwrap();
    let snap = a.snapshot(&new.id).unwrap();
    assert_eq!(snap.policy.unwrap().revision, 2);
    assert_eq!(snap.team.control.unwrap().max_attempts, 2);
    assert!(snap.events.iter().all(|e| {
        e.policy_version
            .starts_with("operator-fake-profile-v2@2:sha256:")
    }));
    assert_eq!(a.snapshot(&old.id).unwrap().policy.unwrap().revision, 1);
    assert_eq!(
        a.snapshot(&old.id).unwrap().events[0].policy_version,
        before.events[0].policy_version
    );
    // First claim is old-policy. Second takes the newly configured assignment.
    a.claim().unwrap();
    let claim = a.claim().unwrap().unwrap();
    assert_eq!(claim.case_id, new.id);
    clock.advance(Duration::seconds(11));
    let a = Application::new(Store::open(&path).unwrap(), Arc::new(clock));
    a.supervise_team().unwrap();
    assert_eq!(
        a.snapshot(&new.id).unwrap().team.control.unwrap().state,
        "blocked"
    );
    assert_eq!(
        a.snapshot(&old.id).unwrap().team.control.unwrap().state,
        "running"
    );
    let db = Store::open(&path).unwrap().connect().unwrap();
    assert!(
        db.execute(
            "UPDATE policy_versions SET name='tampered' WHERE revision=2",
            []
        )
        .is_err()
    );
    assert!(
        db.execute(
            "UPDATE case_policy SET revision=2 WHERE case_id=?1",
            [old.id]
        )
        .is_err()
    );
}
#[test]
fn unknown_fields_versions_overflow_and_revision_skips_fail_closed() {
    let mut raw: serde_json::Value =
        serde_json::from_str(include_str!("../policy/local-fake-v1.json")).unwrap();
    raw["grant_production"] = true.into();
    assert!(serde_json::from_value::<Policy>(raw.clone()).is_err());
    raw.as_object_mut().unwrap().remove("grant_production");
    raw["schema_version"] = 2.into();
    assert!(serde_json::from_value::<Policy>(raw.clone()).is_err());
    raw["schema_version"] = 1.into();
    raw["lease_seconds"] = i64::MAX.into();
    assert!(
        serde_json::from_value::<Policy>(raw)
            .unwrap()
            .validate()
            .is_err()
    );
    let d = TempDir::new().unwrap();
    let a = Application::new(
        Store::open(d.path().join("db")).unwrap(),
        Arc::new(watchhouse::domain::SystemClock),
    );
    let mut p: Policy = serde_json::from_str(include_str!("../policy/local-fake-v1.json")).unwrap();
    p.revision = 3;
    assert!(a.install_policy(p, "operator", "skip").is_err());
}
