//! Multi-project contract fixtures only. No live source/model/notification request is made.
use chrono::{Duration, TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use watchhouse::{
    application::Application, contracts::*, domain::ManualClock, error::{Error, Result},
    sentry::*, store::Store,
};
fn profile(revision:i64, project:&str)->SentryConfig {
    let mut p:SentryConfig=serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();
    p.revision=revision;p.project_id=project.into();
    if project!="42" {p.service="payments".into();p.repository="example/payments-fixture".into();p.webhook_secret_env="WATCHHOUSE_SENTRY_PROJECT_B_WEBHOOK_SECRET".into();p.api_token_env="WATCHHOUSE_SENTRY_PROJECT_B_UNSET_TOKEN".into();}
    p.read_policy=Some(ReadPolicy{schema_version:V1,request_seconds:5,lookback_seconds:86400,overlap_seconds:300,max_pages:3});
    p
}
fn fixture()->(tempfile::TempDir,Application,ManualClock) {
    let d=tempfile::tempdir().unwrap();let t=ManualClock::new(Utc.with_ymd_and_hms(2026,11,16,12,0,0).unwrap());
    let a=Application::new(Store::open(d.path().join("db")).unwrap(),Arc::new(t.clone()));
    a.configure_sentry(profile(1,"42"),"operator").unwrap();
    a.configure_sentry_project(profile(2,"43"),"operator","independent-project-fixture").unwrap();
    (d,a,t)
}
fn issue(p:&SentryConfig,id:&str,count:u32)->Value {
    let mut v:Value=serde_json::from_str(include_str!("fixtures/sentry/issue.json")).unwrap();
    v["id"]=json!(id);v["project"]["id"]=json!(p.project_id);v["count"]=json!(count.to_string());v
}
fn event()->Value {serde_json::from_str(include_str!("fixtures/sentry/event.json")).unwrap()}
fn signed(a:&Application,p:&SentryConfig,id:&str,count:u32) {
    let raw=serde_json::to_vec(&json!({"action":"created","data":{"issue":issue(p,id,count)}})).unwrap();
    let secret=b"project-contract-synthetic-secret-32-bytes";
    let mut h=Hmac::<Sha256>::new_from_slice(secret).unwrap();h.update(&raw);
    a.receive_sentry_verified(&raw,&format!("{:x}",h.finalize().into_bytes()),secret,p).unwrap();
}
#[derive(Default)]
struct Api {reads:Mutex<Vec<(i64,String,String)>>,pages:Mutex<Vec<String>>,continuing:bool,offline_project:Option<String>}
impl SentryApi for Api {
    fn enrich(&self,p:&SentryConfig,id:&str)->Result<Enrichment> {
        self.reads.lock().unwrap().push((p.revision,p.project_id.clone(),id.into()));
        if self.offline_project.as_deref()==Some(p.project_id.as_str()){return Err(Error::Invalid("synthetic project outage".into()))}
        Ok(Enrichment{issue:issue(p,id,5),event:event()})
    }
    fn page(&self,p:&SentryConfig,_:&str,_:&str,cursor:Option<&str>)->Result<Page> {
        self.pages.lock().unwrap().push(p.project_id.clone());
        if self.offline_project.as_deref()==Some(p.project_id.as_str()){return Err(Error::Invalid("synthetic project outage".into()))}
        Ok(Page{issues:vec![issue(p,"1001",5)],next_cursor:if self.continuing{Some(format!("{}-{}",p.project_id,cursor.unwrap_or("first")))}else{None}})
    }
}
#[test]
fn independent_projects_survive_restart_duplicate_ids_out_of_order_and_missed_delivery() {
    let(d,a,t)=fixture();let api=Api::default();
    // Discovery finds missed webhook deliveries in each independent project.
    assert_eq!(a.reconcile_sentry(&api).unwrap(),1);assert_eq!(a.reconcile_sentry(&api).unwrap(),1);
    assert_eq!(*api.pages.lock().unwrap(),vec!["42","43"]);
    a.process_sentry_intake(&api).unwrap();a.process_sentry_intake(&api).unwrap();
    assert_eq!(a.list_cases().unwrap().len(),2);
    let observations:Vec<_>=a.list_cases().unwrap().iter().map(|c|a.snapshot(&c.id).unwrap().source.unwrap()).collect();
    assert!(observations.iter().any(|o|o.source_key=="sentry:fixture-org/42/1001"));
    assert!(observations.iter().any(|o|o.source_key=="sentry:fixture-org/43/1001"));
    signed(&a,&profile(1,"42"),"1001",1);signed(&a,&profile(1,"42"),"1001",1);
    a.process_sentry_intake(&api).unwrap();assert_eq!(a.list_cases().unwrap().len(),2);
    let restarted=Application::new(Store::open(d.path().join("db")).unwrap(),Arc::new(t.clone()));
    t.advance(Duration::days(2));restarted.refresh_known_sentry(&api).unwrap();restarted.refresh_known_sentry(&api).unwrap();
    let reads=api.reads.lock().unwrap();assert!(reads.iter().any(|(_,p,_)|p=="42"));assert!(reads.iter().any(|(_,p,_)|p=="43"));
    for c in restarted.list_cases().unwrap() {let s=restarted.snapshot(&c.id).unwrap();assert!(s.source.unwrap().authoritative);assert!(s.incident.is_none());assert!(s.model_jobs.is_empty());}
    let health=restarted.sentry_health().unwrap();assert_eq!(health["configuration"]["project_id"],"42");assert_eq!(health["projects"].as_array().unwrap().len(),2);
}
#[test]
fn discovery_rotates_while_pages_continue_and_one_project_is_offline() {
    let(d,a,t)=fixture();let api=Api{continuing:true,..Default::default()};
    for _ in 0..6 {assert_eq!(a.reconcile_sentry(&api).unwrap(),1)}
    assert_eq!(*api.pages.lock().unwrap(),vec!["42","43","42","43","42","43"]);
    // Three pages are the configured cap. Another read cannot silently reset that window.
    assert!(a.reconcile_sentry(&api).is_err());
    let restarted=Application::new(Store::open(d.path().join("db")).unwrap(),Arc::new(t.clone()));
    assert!(restarted.reconcile_sentry(&api).is_err());
    assert_eq!(api.pages.lock().unwrap().len(),6);
    let mut one=profile(3,"42");one.read_policy.as_mut().unwrap().max_pages=2;a.configure_sentry(one,"operator").unwrap();
    let mut two=profile(4,"43");two.read_policy.as_mut().unwrap().max_pages=2;a.configure_sentry_project(two,"operator","new-project-budget").unwrap();
    let partial=Api{offline_project:Some("42".into()),..Default::default()};
    a.reconcile_sentry(&partial).unwrap();a.reconcile_sentry(&partial).unwrap();
    assert!(partial.pages.lock().unwrap().contains(&"43".into()));
    let h=a.sentry_health().unwrap();assert!(h["reconciliation"].as_array().unwrap().iter().any(|r|r["error"].as_str().is_some_and(|s|s.contains("unavailable"))));
}
#[test]
fn changed_project_configuration_fences_inflight_reads_and_preserves_other_projects() {
    let(_d,a,_t)=fixture();let api=Api::default();signed(&a,&profile(1,"42"),"1001",3);signed(&a,&profile(2,"43"),"1001",3);
    a.process_sentry_intake(&api).unwrap();a.process_sentry_intake(&api).unwrap();
    let cases=a.list_cases().unwrap();let left=cases.iter().find(|c|a.snapshot(&c.id).unwrap().source.unwrap().signal.project_id=="42").unwrap().id.clone();
    let right=cases.iter().find(|c|a.snapshot(&c.id).unwrap().source.unwrap().signal.project_id=="43").unwrap().id.clone();
    let mut next=profile(3,"42");next.service="checkout-v2".into();a.configure_sentry(next,"operator").unwrap();
    let s=a.snapshot(&left).unwrap().source_freshness.unwrap();assert_eq!(s.state,"configuration_changed");assert_eq!(s.active_configuration_revision,Some(3));assert_eq!(a.snapshot(&right).unwrap().source_freshness.unwrap().state,"current_snapshot");
    a.refresh_known_sentry(&api).unwrap();assert_eq!(a.snapshot(&left).unwrap().source.unwrap().service,"checkout-v2");
    struct Rotate {a:Application}
    impl SentryApi for Rotate {
        fn enrich(&self,p:&SentryConfig,id:&str)->Result<Enrichment> {let mut next=profile(4,"43");next.service="payments-v2".into();self.a.configure_sentry_project(next,"operator","rotation-during-read").unwrap();Ok(Enrichment{issue:issue(p,id,5),event:event()})}
        fn page(&self,_:&SentryConfig,_:&str,_:&str,_:Option<&str>)->Result<Page>{unreachable!()}
    }
    signed(&a,&profile(2,"43"),"1001",6);
    a.process_sentry_intake(&Rotate{a:a.clone()}).unwrap();
    assert_eq!(a.snapshot(&right).unwrap().source.unwrap().configuration_revision,2);
    assert_eq!(a.snapshot(&right).unwrap().source_freshness.unwrap().state,"configuration_changed");
    a.process_sentry_intake(&api).unwrap();
    assert_eq!(a.snapshot(&right).unwrap().source.unwrap().configuration_revision,4);
    assert!(a.snapshot(&right).unwrap().qualification.evidence.iter().any(|e|e.provenance=="sentry-v1:unaccepted-source-read"));
}
#[test]
fn registration_is_explicit_immutable_scoped_and_keeps_provenance() {
    let(d,a,_t)=fixture();
    let mut extra=profile(3,"44");extra.read_policy=None;
    assert!(a.configure_sentry_project(extra.clone(),"operator","missing-explicit-budget").is_err());
    extra.read_policy=profile(2,"43").read_policy;
    assert!(a.configure_sentry(extra.clone(),"operator").is_err());
    assert!(a.configure_sentry_project(extra.clone(),"fake-engineer","self-edit").is_err());
    a.configure_sentry_project(extra.clone(),"operator","canonical-fixture-policy").unwrap();
    a.configure_sentry_project(extra,"operator","replay-cannot-rewrite-source").unwrap();
    let profiles=a.configured_sentry_projects().unwrap();assert_eq!(profiles.iter().find(|p|p.configuration.revision==3).unwrap().source,"canonical-fixture-policy");
    let db=rusqlite::Connection::open(d.path().join("db")).unwrap();assert!(db.execute("UPDATE source_config_sources SET source='changed'",[]).is_err());assert!(db.execute("UPDATE source_primary SET project_id='43'",[]).is_err());
    let mut raw:Value=serde_json::from_str(include_str!("fixtures/sentry/config.json")).unwrap();raw.as_object_mut().unwrap().remove("read_policy");let legacy:SentryConfig=serde_json::from_value(raw.clone()).unwrap();assert_eq!(serde_json::to_value(legacy).unwrap(),raw);
}

#[test]
fn offline_gaps_are_replayed_in_bounded_windows_without_jumping_the_watermark() {
    let(_d,a,t)=fixture();let api=Api::default();a.reconcile_sentry(&api).unwrap();a.reconcile_sentry(&api).unwrap();
    t.advance(Duration::days(3));a.reconcile_sentry(&api).unwrap();
    let h=a.sentry_health().unwrap();let r=h["reconciliation"].as_array().unwrap().iter().find(|r|r["revision"]==1).unwrap();
    let start=chrono::DateTime::parse_from_rfc3339(r["window_start"].as_str().unwrap()).unwrap();let end=chrono::DateTime::parse_from_rfc3339(r["window_end"].as_str().unwrap()).unwrap();
    assert!(end-start<=Duration::seconds(86400));assert!(end<chrono::DateTime::parse_from_rfc3339("2026-11-19T12:00:00Z").unwrap());
    assert_eq!(r["covered_until"],r["window_end"]);
}
#[test]
fn a_configuration_change_during_discovery_cannot_queue_the_old_page() {
    let(_d,a,_t)=fixture();
    struct Rotate(Application);
    impl SentryApi for Rotate {
        fn enrich(&self,_:&SentryConfig,_:&str)->Result<Enrichment>{unreachable!()}
        fn page(&self,p:&SentryConfig,_:&str,_:&str,_:Option<&str>)->Result<Page>{
            self.0.configure_sentry(profile(3,"42"),"operator").unwrap();
            Ok(Page{issues:vec![issue(p,"1001",5)],next_cursor:None})
        }
    }
    assert!(a.reconcile_sentry(&Rotate(a.clone())).is_err());
    assert_eq!(a.process_sentry_intake(&Api::default()).unwrap(),0);assert!(a.list_cases().unwrap().is_empty());
}
#[test]
fn project_change_invalidates_reported_impact_before_the_next_source_read() {
    let(_d,a,_t)=fixture();let api=Api::default();signed(&a,&profile(1,"42"),"1001",3);a.process_sentry_intake(&api).unwrap();let id=a.list_cases().unwrap()[0].id.clone();
    let report=a.record_evidence(&id,RecordEvidence{schema_version:V1,idempotency_key:"impact-before-rotation".into(),actor:"operator".into(),reason:"Synthetic report".into(),expected_version:a.snapshot(&id).unwrap().case.version,scope:"checkout / test".into(),statement:"Unknown impact denominator".into(),collection_method:"Fixture report".into(),observed_at:None,uncertainty:"No live measurement".into()}).unwrap().evidence.digest;
    a.assess_impact(&id,watchhouse::impact::Assess{command:OperatorCommand{meta:CommandMeta{schema_version:V1,idempotency_key:"impact".into(),actor:"operator".into(),reason:"Retain impact collection".into()},expected_version:a.snapshot(&id).unwrap().case.version},expected_revision:0,service:"checkout".into(),environment:"test".into(),scope:"Test requests".into(),window:None,critical_workflow:"Submit order".into(),impact_statement:"Unknown".into(),uncertainty:"Fixture only".into(),collection_owner:"impact-owner".into(),collection_task:"Collect denominator".into(),review_seconds:60,evidence_refs:vec![report]}).unwrap();
    a.configure_sentry(profile(3,"42"),"operator").unwrap();
    assert!(!a.snapshot(&id).unwrap().impact[0].context_current);a.supervise_team().unwrap();assert!(a.snapshot(&id).unwrap().impact[0].due);
}
#[test]
fn signed_project_http_child() {
    if std::env::var_os("WATCHHOUSE_PROJECT_ROUTING_TEST_CHILD").is_none(){return}
    let(_d,a,_t)=fixture();
    let mut first=profile(3,"42");first.webhook_secret_env="WATCHHOUSE_SENTRY_PROJECT_A_WEBHOOK_SECRET".into();a.configure_sentry(first.clone(),"operator").unwrap();
    let router=watchhouse::http::router(a.clone(),"unused-operator-token-32-bytes-long".into(),"127.0.0.1:8787".into());
    tokio::runtime::Runtime::new().unwrap().block_on(async move {
        use axum::{body::Body,http::{Request,StatusCode}};
        use tower::ServiceExt;
        let request=|path:&str,p:&SentryConfig,secret:&[u8]|{
            let raw=serde_json::to_vec(&json!({"action":"created","data":{"issue":issue(p,"1001",3)}})).unwrap();
            let mut h=Hmac::<Sha256>::new_from_slice(secret).unwrap();h.update(&raw);
            Request::builder().method("POST").uri(path).header("Host","127.0.0.1:8787").header("Content-Type","application/json").header("sentry-hook-signature",format!("{:x}",h.finalize().into_bytes())).body(Body::from(raw)).unwrap()
        };
        let secret_a=b"project-A-synthetic-routing-secret-32-bytes";
        let secret_b=b"project-B-synthetic-routing-secret-32-bytes";
        let second=profile(2,"43");
        assert_eq!(router.clone().oneshot(request("/integrations/sentry/fixture-org/43",&second,secret_a)).await.unwrap().status(),StatusCode::BAD_REQUEST);
        assert_eq!(router.clone().oneshot(request("/integrations/sentry/fixture-org/43",&second,secret_b)).await.unwrap().status(),StatusCode::ACCEPTED);
        assert_eq!(router.clone().oneshot(request("/integrations/sentry/fixture-org/42",&second,secret_a)).await.unwrap().status(),StatusCode::BAD_REQUEST);
        assert_eq!(router.clone().oneshot(request("/integrations/sentry",&first,secret_b)).await.unwrap().status(),StatusCode::BAD_REQUEST);
        assert_eq!(router.clone().oneshot(request("/integrations/sentry",&first,secret_a)).await.unwrap().status(),StatusCode::ACCEPTED);
        assert_eq!(router.oneshot(request("/integrations/sentry/fixture-org/99",&first,secret_a)).await.unwrap().status(),StatusCode::NOT_FOUND);
        let api=Api::default();a.process_sentry_intake(&api).unwrap();a.process_sentry_intake(&api).unwrap();assert_eq!(a.list_cases().unwrap().len(),2);
    });
}
#[test]
fn project_http_selects_its_own_secret_and_rejects_cross_project_payloads() {
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","signed_project_http_child","--nocapture"])
        .env("WATCHHOUSE_PROJECT_ROUTING_TEST_CHILD","1")
        .env("WATCHHOUSE_SENTRY_PROJECT_A_WEBHOOK_SECRET","project-A-synthetic-routing-secret-32-bytes")
        .env("WATCHHOUSE_SENTRY_PROJECT_B_WEBHOOK_SECRET","project-B-synthetic-routing-secret-32-bytes")
        .output().unwrap();
    assert!(output.status.success(),"{} {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
}
