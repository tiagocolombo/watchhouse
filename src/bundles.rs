//! Internal case exports are record views, never imported operational authority.
mod io;
mod records;

use crate::{
    application::{Application, text},
    contracts::V1,
    error::{Error, Result},
    store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub use io::{Inspection, inspect};

const ROW_LIMIT: usize = 8 * 1024 * 1024;
const FRAME_LIMIT: usize = 8 * 1024 * 1024;
const EXPORT_LIMIT: u64 = 16 * 1024 * 1024 * 1024;
const RECORD_LIMIT: u64 = 10_000_000;

fn invalid(message: &str) -> Error { Error::Invalid(format!("case bundle: {message}")) }
fn io_error(_: std::io::Error) -> Error { invalid("filesystem operation failed; incomplete destination retained") }

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default)]
    pub include_evidence: bool,
    #[serde(default)]
    pub include_patches: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub case_version: i64,
    pub cursor: i64,
    pub created_at: String,
    pub actor: String,
    pub identity_source: String,
    pub audience: String,
    pub scope: String,
    pub options: Options,
    pub evidence_epoch: i64,
    pub source_context_current: bool,
    pub source_freshness: Option<Value>,
    pub recovery_currency: Value,
    pub exclusions: Vec<String>,
    pub limitation: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub rows: u64,
    pub sections: BTreeMap<String,u64>,
    pub evidence_present: u64,
    pub evidence_redacted: u64,
    pub evidence_missing: u64,
    pub evidence_corrupt: u64,
    pub evidence_included: u64,
}
impl Counts {
    fn observe(&mut self, section: &str, value: &Value) -> Result<()> {
        self.rows = self.rows.checked_add(1).ok_or_else(|| invalid("record count overflow"))?;
        if self.rows > RECORD_LIMIT { return Err(Error::Backpressure("case export exceeds ten million records".into())); }
        *self.sections.entry(section.into()).or_default() += 1;
        if value["kind"] == "evidence" {
            match value["state"].as_str() {
                Some("present") => self.evidence_present += 1,
                Some("redacted") => self.evidence_redacted += 1,
                Some("missing") => self.evidence_missing += 1,
                Some("corrupt") => self.evidence_corrupt += 1,
                _ => return Err(invalid("unknown evidence state")),
            }
            if value["included"] == true { self.evidence_included += 1; }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub header: Header,
    pub counts: Counts,
    pub sections: BTreeMap<String,Vec<Value>>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub actor: String,
    pub owner: String,
    pub created_at: String,
    pub updated_at: String,
    pub kind: String,
    pub state: String,
    pub case_version: Option<i64>,
    pub cursor: Option<i64>,
    pub manifest_digest: Option<String>,
    pub counts: Option<Counts>,
    pub reason: String,
    pub detail: String,
}
fn frame_digest(header: &Header, counts: &Counts, sections: &BTreeMap<String,Vec<Value>>) -> Result<String> {
    Ok(store::digest(&serde_json::to_string(&json!({"header":header,"counts":counts,"sections":sections}))?))
}
fn header(app: &Application, db: &Connection, case: &str, id: &str, who: &str, options: Options, scope: Option<&crate::access::Session>) -> Result<Header> {
    let c = store::case(db,case)?;
    if let Some(scope) = scope {
        if !scope.case_allowed(db,case)? { return Err(Error::Conflict("case export scope changed".into())); }
    }
    let recovery = crate::recovery::state(db,case)?;
    Ok(Header {
        schema_version:V1, id:id.into(), case_id:case.into(), case_version:c.version,
        cursor:db.query_row("SELECT COALESCE(MAX(cursor),0) FROM events WHERE case_id=?1",[case],|r|r.get(0))?,
        created_at:app.now(), actor:who.into(),
        identity_source:scope.map_or("OS operator attribution",|s|if s.revision.is_some(){"OS-configured access credential"}else{"shared local operator attribution"}).into(),
        audience:"internal_case".into(),
        scope:if scope.is_some_and(|s|s.role==crate::access::Role::ServiceOwner){"granted case records; automatic cross-scope correlations excluded"}else{"owned case records and their retained references"}.into(),
        options, evidence_epoch:crate::retention::epoch(db,case)?,
        source_context_current:crate::sentry_projects::context_current(db,case)?,
        source_freshness:crate::sentry_refresh::snapshot(db,case,app.clock.now())?.map(serde_json::to_value).transpose()?,
        recovery_currency:json!({"response_resolved":recovery.response_resolved,"context_current":recovery.context_current,"plan_id":recovery.plan.map(|p|p.id),"observation_id":recovery.observation.map(|o|o.id)}),
        exclusions:vec!["global configuration, secrets and provider queues/cursors".into(),"private runtime homes, control directories and raw harness transcripts".into(),"frozen repository filesystem snapshots and working directories".into(),"automatic neighbor suggestions and ungranted correlation rows".into(),"transport/control fields and optional code changes listed per record view".into()],
        limitation:"Complete record views within the declared case scope, not a coordinator restore or public summary. Structured JSON columns are decoded and control fields omitted; source and exported-view hashes are distinct. Retained evidence bytes are optional and may be corrupt or missing as explicitly marked. Included hashes prove export integrity, not factual correctness, native execution, deployment or recovery. Existing references and incorporated prose do not grant access to another case. Previously downloaded copies are outside current-store redaction.".into(),
    })
}
fn save_export(db: &Connection, e: &Export) -> Result<()> {
    db.execute("INSERT INTO case_exports VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET state=excluded.state,record=excluded.record",params![e.id,e.case_id,e.state,serde_json::to_string(e)?])?;
    db.execute("INSERT INTO factory_events(record) VALUES (?1)",[json!({"type":"case_export_state","case_id":e.case_id,"export_id":e.id,"state":e.state,"at":e.updated_at,"actor":e.actor,"manifest_digest":e.manifest_digest,"detail":e.detail}).to_string()])?;
    Ok(())
}
impl Application {
    pub fn case_exports(&self, case: &str) -> Result<Vec<Export>> {
        let db=self.store.connect()?;store::case(&db,case)?;
        let mut q=db.prepare("SELECT record FROM case_exports WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100")?;
        q.query_map([case],|r|r.get::<_,String>(0))?.map(|s|Ok(serde_json::from_str(&s?)?)).collect()
    }
    pub fn case_bundle(&self, case: &str, options: Options, who: &str) -> Result<Frame> {
        crate::retention::validate_operator(who,"Prepare an internal case export")?;
        self.capture_frame(case,options,who,None)
    }
    pub(crate) fn scoped_case_bundle(&self, case: &str, options: Options, scope: &crate::access::Session) -> Result<Frame> {
        self.capture_frame(case,options,&scope.id,Some(scope))
    }
    fn capture_frame(&self, case: &str, options: Options, who: &str, scope: Option<&crate::access::Session>) -> Result<Frame> {
        let id=store::id("export");let mut db=self.store.connect()?;let tx=db.transaction()?;
        let h=header(self,&tx,case,&id,who,options,scope)?;
        let owner=store::case(&tx,case)?.owner;
        let mut sections=BTreeMap::<String,Vec<Value>>::new();let mut counts=Counts::default();let mut bytes=0usize;
        records::capture(&tx,case,&options,scope,&mut |section,value|{
            let encoded=serde_json::to_vec(&value)?;
            if encoded.len()>ROW_LIMIT {return Err(Error::Backpressure("case export row exceeds 8MiB".into()));}
            bytes=bytes.saturating_add(encoded.len());
            if bytes>FRAME_LIMIT {return Err(Error::Backpressure("case bundle exceeds browser limit; use the streaming OS directory export".into()));}
            counts.observe(section,&value)?;
            sections.entry(section.into()).or_default().push(value);Ok(())
        })?;
        let digest=frame_digest(&h,&counts,&sections)?;
        let frame=Frame{header:h,counts,sections,digest};
        if serde_json::to_vec(&frame)?.len()>FRAME_LIMIT {return Err(Error::Backpressure("case bundle exceeds browser limit; use the streaming OS directory export".into()));}
        drop(tx);
        let e=Export{schema_version:V1,id,case_id:case.into(),actor:who.into(),owner,created_at:frame.header.created_at.clone(),updated_at:self.now(),kind:"download".into(),state:"prepared".into(),case_version:Some(frame.header.case_version),cursor:Some(frame.header.cursor),manifest_digest:Some(frame.digest.clone()),counts:Some(frame.counts.clone()),reason:"Authenticated internal case export".into(),detail:"Response prepared; client receipt/delivery is not established".into()};
        let audit=db.unchecked_transaction()?;save_export(&audit,&e)?;audit.commit()?;
        Ok(frame)
    }
    pub fn export_case(&self, case: &str, output: &Path, options: Options, who: &str, reason: &str) -> Result<Export> {
        crate::retention::validate_operator(who,reason)?;
        text(reason,"export reason",2000)?;
        if output.exists() || std::fs::symlink_metadata(output).is_ok() {return Err(Error::Conflict("export destination already exists; no overwrite".into()));}
        let parent=output.parent().filter(|p|!p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let root=std::fs::canonicalize(parent).map_err(io_error)?.join(output.file_name().ok_or_else(||invalid("new destination directory required"))?);
        let mut db=self.store.connect()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let c=store::case(&tx,case)?;let now=self.now();
        let mut e=Export{schema_version:V1,id:store::id("export"),case_id:case.into(),actor:who.into(),owner:c.owner,created_at:now.clone(),updated_at:now.clone(),kind:"directory".into(),state:"preparing".into(),case_version:None,cursor:None,manifest_digest:None,counts:None,reason:reason.into(),detail:"OS exporter owns completion or inspection of an interrupted private destination".into()};
        save_export(&tx,&e)?;
        tx.execute("INSERT INTO case_export_locations VALUES (?1,?2,?3)",params![e.id,root.to_string_lossy(),now])?;tx.commit()?;
        let result=(||->Result<(Header,Counts,String)>{
            crate::backups::private_directory(&root)?;
            let mut marker=crate::backups::create_file(&root.join("export-incomplete.json"))?;
            use std::io::Write;
            marker.write_all(serde_json::to_string(&json!({"format":"watchhouse-case-export","id":e.id,"case_id":case,"owner":e.owner,"actor":who,"at":now,"next_action":"Inspect or reconcile a complete hash-pinned manifest; retry incomplete exports in a new directory without overwriting"}))?.as_bytes()).map_err(io_error)?;
            marker.sync_all().map_err(io_error)?;
            let tx=db.transaction()?;
            let h=header(self,&tx,case,&e.id,who,options,None)?;
            let mut sink=io::Writer::new(&root);
            let mut counts=Counts::default();
            records::capture(&tx,case,&options,None,&mut |section,value|{
                counts.observe(section,&value)?;sink.emit(section,&value)
            })?;
            let sum=sink.finish(&h,&counts)?;
            drop(tx);
            Ok((h,counts,sum))
        })();
        match result {
            Ok((h,counts,sum))=>{
                // A complete manifest can be independently inspected/reconciled if
                // the final metadata write or marker removal is interrupted.
                e.case_version=Some(h.case_version);e.cursor=Some(h.cursor);e.counts=Some(counts);e.manifest_digest=Some(sum);
                e.state="complete".into();e.updated_at=self.now();e.detail="Internal directory bytes and manifest prepared; not a live recovery or delivery assertion".into();
                save_export(&db,&e)?;
                std::fs::remove_file(root.join("export-incomplete.json")).map_err(io_error)?;
                Ok(e)
            }
            Err(error)=>{
                e.state="failed".into();e.updated_at=self.now();e.detail="Export interrupted; any incomplete private destination is retained without overwrite. Operator must inspect or retry in a new directory".into();
                save_export(&db,&e)?;
                Err(error)
            }
        }
    }
    pub fn reconcile_case_export(&self, id: &str, directory: &Path, expected: &str, who: &str, reason: &str) -> Result<Export> {
        crate::retention::validate_operator(who,reason)?;
        let mut db=self.store.connect()?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if crate::restore::fenced(&tx,"case_export",id)? {
            return Err(Error::Conflict("restored export locations are forensic only; create a fresh export".into()));
        }
        let raw:String=tx.query_row("SELECT record FROM case_exports WHERE id=?1",[id],|r|r.get(0)).optional()?.ok_or(Error::NotFound)?;
        let mut e:Export=serde_json::from_str(&raw)?;
        let registered:String=tx.query_row("SELECT root FROM case_export_locations WHERE export_id=?1",[id],|r|r.get(0))?;
        // Only the explicit operator-supplied directory is read, never an old registered path.
        let canonical=std::fs::canonicalize(directory).map_err(io_error)?;
        if canonical.to_string_lossy()!=registered {return Err(Error::Conflict("export destination does not match".into()));}
        let checked=io::inspect_inner(directory,Some(expected),true)?;
        if checked.header.id!=id || checked.header.case_id!=e.case_id {return Err(Error::Conflict("export identity does not match".into()));}
        let marker=directory.join("export-incomplete.json");
        if std::fs::symlink_metadata(&marker).is_ok() {
            let path=crate::backups::contained(directory,"export-incomplete.json")?;
            let meta=std::fs::symlink_metadata(&path).map_err(io_error)?;
            if !meta.is_file() || meta.len()>16384 {return Err(invalid("invalid incomplete marker"));}
            let value:Value=serde_json::from_slice(&std::fs::read(path).map_err(io_error)?)?;
            if value["id"]!=id || value["case_id"]!=e.case_id {return Err(invalid("incomplete marker identity mismatch"));}
            std::fs::remove_file(marker).map_err(io_error)?;
        }
        e.state="complete".into();e.updated_at=self.now();e.case_version=Some(checked.header.case_version);e.cursor=Some(checked.header.cursor);e.manifest_digest=Some(checked.manifest_digest);e.counts=Some(checked.counts);e.detail=format!("OS operator {who} independently inspected the pinned complete export: {reason}");
        save_export(&tx,&e)?;tx.commit()?;
        Ok(e)
    }
}
pub fn inspect_frame(frame: &Frame) -> Result<Counts> {
    if frame.header.audience!="internal_case" || frame.digest!=frame_digest(&frame.header,&frame.counts,&frame.sections)? {return Err(invalid("frame identity or digest mismatch"));}
    let mut counts=Counts::default();
    for (section,values) in &frame.sections {
        for value in values {validate_row(section,value,&frame.header)?;counts.observe(section,value)?;}
    }
    if counts.sections.get("cases")!=Some(&1) || counts!=frame.counts {return Err(invalid("frame counts mismatch"));}
    Ok(counts)
}
fn validate_row(section: &str, value: &Value, header: &Header) -> Result<()> {
    match value["kind"].as_str() {
        Some("record_view")=>{
            let data=&value["record"];
            if value["exported_digest"].as_str()!=Some(store::digest(&serde_json::to_string(data)?).as_str()) {return Err(invalid("record view digest mismatch"));}
            if !value["source_digest"].as_str().is_some_and(crate::real_execution::protocol::sha_valid) {return Err(invalid("invalid source row digest"));}
            if value["transformed"]==false && value["source_digest"]!=value["exported_digest"] {return Err(invalid("untransformed row identity changed"));}
            if section=="cases" && (data["id"]!=header.case_id || data["version"]!=header.case_version) {return Err(invalid("case identity/version mismatch"));}
            if data.get("case_id").is_some_and(|id|id!=&json!(header.case_id)) {return Err(invalid("foreign case row in bundle"));}
        }
        Some("evidence")=>{
            let included=value["included"].as_bool().ok_or_else(||invalid("evidence inclusion flag missing"))?;
            let state=value["state"].as_str().ok_or_else(||invalid("evidence state missing"))?;
            if included {
                if !header.options.include_evidence || !matches!(state,"present"|"corrupt") {return Err(invalid("unpermitted evidence body"));}
                let body=value["body"].as_str().ok_or_else(||invalid("included body missing"))?;
                let actual=store::digest(body);
                if value["observed_digest"]!=actual || value["bytes"]!=body.len() {return Err(invalid("included evidence bytes changed"));}
                if (state=="present")!=(value["digest"]==actual) {return Err(invalid("evidence integrity label is false"));}
            } else if !value["body"].is_null() {return Err(invalid("omitted body unexpectedly included"));}
            if state=="redacted" && (included || value["tombstone"].is_null()) {return Err(invalid("redacted evidence rehydrated"));}
        }
        _=>return Err(invalid("unknown bundle row type")),
    }
    Ok(())
}
