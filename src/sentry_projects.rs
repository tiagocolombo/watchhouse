//! Independent project mappings and credentials, preserving the legacy default identity.
use crate::{application::{Application,text},error::{Error,Result},sentry::SentryConfig};
use rusqlite::{Connection,OptionalExtension,TransactionBehavior,params};
use serde::{Serialize,Deserialize};
pub(crate) fn config_for(db:&Connection,organization:&str,project:&str)->Result<Option<SentryConfig>> {
    let raw:Option<String>=db.query_row("SELECT c.record FROM source_projects p JOIN source_configs c ON c.revision=p.revision WHERE p.organization=?1 AND p.project_id=?2",params![organization,project],|r|r.get(0)).optional()?;
    raw.map(|s|{let p:SentryConfig=serde_json::from_str(&s)?;p.validate()?;Ok(p)}).transpose()
}
pub(crate) fn default_config(db:&Connection)->Result<Option<SentryConfig>> {
    let pair:Option<(String,String)>=db.query_row("SELECT organization,project_id FROM source_primary WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    pair.map(|(o,p)|config_for(db,&o,&p)).transpose().map(Option::flatten)
}
pub(crate) fn revision_active(db:&Connection,revision:i64)->Result<bool> {Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM source_projects WHERE revision=?1)",[revision],|r|r.get(0))?)}
pub(crate) fn context_current(db:&Connection,case:&str)->Result<bool> {
    let row:Option<(String,i64)>=db.query_row("SELECT source_key,json_extract(record,'$.configuration_revision') FROM source_cases WHERE case_id=?1",[case],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    match row {Some((key,rev)) if key.starts_with("sentry:")=>revision_active(db,rev),_=>Ok(true)}
}
pub(crate) fn select_poll(a:&Application)->Result<Option<SentryConfig>> {
    let mut db=a.store.connect()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let cursor:i64=tx.query_row("SELECT last_revision FROM source_poll_cursor WHERE singleton=1",[],|r|r.get(0))?;
    let raw:Option<String>=tx.query_row("SELECT c.record FROM source_projects p JOIN source_configs c ON c.revision=p.revision LEFT JOIN source_sync s ON s.revision=p.revision WHERE s.revision IS NULL OR s.next_at<=?1 ORDER BY CASE WHEN p.revision>?2 THEN 0 ELSE 1 END,p.revision LIMIT 1",params![a.now(),cursor],|r|r.get(0)).optional()?;
    let Some(raw)=raw else{return Ok(None)};
    let cfg:SentryConfig=serde_json::from_str(&raw)?;cfg.validate()?;
    tx.execute("UPDATE source_poll_cursor SET last_revision=?1 WHERE singleton=1",[cfg.revision])?;tx.commit()?;Ok(Some(cfg))
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ProjectStatus {
    pub configuration:SentryConfig,
    pub installed_by:String,
    pub installed_at:String,
    pub source:String,
    pub read_policy_provenance:String,
    pub api_credential_present:bool,
    pub webhook_credential_present:bool,
}
impl Application {
    pub(crate) fn install_sentry_project(&self,cfg:SentryConfig,who:&str,source:&str,default_only:bool)->Result<SentryConfig> {
        cfg.validate()?;
        if !default_only && cfg.read_policy.is_none(){return Err(Error::Invalid("explicit project installation requires a versioned read_policy".into()))}
        crate::incident_ops::human(who)?;text(source,"source configuration provenance",4096)?;
        let mut db=self.store.connect()?;let tx=db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if default_only && default_config(&tx)?.is_some_and(|p|p.organization!=cfg.organization || p.project_id!=cfg.project_id) {return Err(Error::Conflict("default organization/project cannot be remapped; register an independent project explicitly".into()))}
        let raw=serde_json::to_string(&cfg)?;
        let existing:Option<String>=tx.query_row("SELECT record FROM source_configs WHERE revision=?1",[cfg.revision],|r|r.get(0)).optional()?;
        if let Some(old)=existing {return if old==raw{Ok(cfg)}else{Err(Error::Conflict("source configuration versions are immutable".into()))}}
        let latest:i64=tx.query_row("SELECT COALESCE(MAX(revision),0) FROM source_configs",[],|r|r.get(0))?;
        if cfg.revision!=latest+1{return Err(Error::Conflict("source revision must advance one global configuration version".into()))}
        let count:u32=tx.query_row("SELECT COUNT(*) FROM source_projects",[],|r|r.get(0))?;
        if count>=32 && config_for(&tx,&cfg.organization,&cfg.project_id)?.is_none(){return Err(Error::Invalid("at most 32 independently configured source projects".into()))}
        tx.execute("INSERT INTO source_configs VALUES (?1,?2,?3,?4)",params![cfg.revision,raw,who,self.now()])?;
        tx.execute("INSERT INTO source_config_sources VALUES (?1,?2)",params![cfg.revision,source])?;
        tx.execute("INSERT INTO source_projects VALUES (?1,?2,?3) ON CONFLICT(organization,project_id) DO UPDATE SET revision=excluded.revision",params![cfg.organization,cfg.project_id,cfg.revision])?;
        tx.execute("INSERT OR IGNORE INTO source_primary VALUES (1,?1,?2)",params![cfg.organization,cfg.project_id])?;
        tx.commit()?;Ok(cfg)
    }
    pub fn configure_sentry_project(&self,cfg:SentryConfig,who:&str,source:&str)->Result<SentryConfig> {self.install_sentry_project(cfg,who,source,false)}
    pub fn configure_sentry_with_source(&self,cfg:SentryConfig,who:&str,source:&str)->Result<SentryConfig> {self.install_sentry_project(cfg,who,source,true)}
    pub fn configured_sentry_projects(&self)->Result<Vec<ProjectStatus>> {
        let db=self.store.connect()?;let mut q=db.prepare("SELECT c.record,c.installed_by,c.installed_at,s.source FROM source_projects p JOIN source_configs c ON c.revision=p.revision JOIN source_config_sources s ON s.revision=c.revision ORDER BY c.revision LIMIT 32")?;
        q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.map(|r|{let(raw,installed_by,installed_at,source)=r?;let cfg:SentryConfig=serde_json::from_str(&raw)?;cfg.validate()?;Ok(ProjectStatus{read_policy_provenance:if cfg.read_policy.is_some(){"explicit-versioned-v1"}else{"frozen-legacy-v1-compatibility"}.into(),api_credential_present:std::env::var(&cfg.api_token_env).is_ok_and(|s|!s.trim().is_empty()),webhook_credential_present:std::env::var(&cfg.webhook_secret_env).is_ok_and(|s|s.len()>=32),configuration:cfg,installed_by,installed_at,source})}).collect()
    }
    pub fn receive_sentry_project(&self,organization:&str,project:&str,raw:&[u8],signature:&str)->Result<String> {
        let cfg=config_for(&self.store.connect()?,organization,project)?.ok_or(Error::NotFound)?;
        let secret=std::env::var(&cfg.webhook_secret_env).map_err(|_|Error::Invalid("project webhook credential missing; activation blocked".into()))?;
        self.receive_sentry_verified(raw,signature,secret.as_bytes(),&cfg)
    }
}
