//! An explicit operator classification, independent of source status and service recovery.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    store,
};
use chrono::{DateTime, Duration, SecondsFormat};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Moderate,
}
impl Severity {
    fn key(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Moderate => "moderate",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentRoles {
    pub commander: String,
    pub operations: String,
    pub communications: String,
    pub scribe: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclareIncident {
    pub command: OperatorCommand,
    pub severity: Severity,
    pub rationale: String,
    pub impact: String,
    pub uncertainty: String,
    pub objective: String,
    pub roles: IncidentRoles,
    pub evidence_refs: Vec<String>,
    pub known_start: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub declared_at: String,
    pub state: String,
    pub policy_provenance: String,
    pub declaration: DeclareIncident,
    pub source_snapshot: crate::sentry::SourceObservation,
    pub next_update_at: String,
    pub mitigation_at: Option<String>,
    pub recovered_at: Option<String>,
    pub recovery_assessment: String,
    pub communication_draft: String,
    pub investigation_register: Vec<String>,
}
pub(crate) fn snapshot(db: &Connection, id: &str) -> Result<Option<Incident>> {
    let raw: Option<String> = db
        .query_row("SELECT record FROM incidents WHERE case_id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
pub(crate) fn supervise(tx: &Transaction<'_>, now: &str) -> Result<usize> {
    Ok(crate::incident_ops::due(tx, now)? + crate::incident_artifacts::due(tx, now)?)
}
pub(crate) fn safe(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' | '\n' => out.push(' '),
            '\\' | '`' | '[' | ']' | '(' | ')' | '*' | '_' | '!' | '#' | '|' => {
                out.push('\\');
                out.push(c)
            }
            _ => out.push(c),
        }
    }
    out
}

impl Application {
    pub fn declare_incident(&self, id: &str, cmd: DeclareIncident) -> Result<Incident> {
        let m = &cmd.command.meta;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        for (name, value) in [
            ("rationale", &cmd.rationale),
            ("impact", &cmd.impact),
            ("uncertainty", &cmd.uncertainty),
            ("objective", &cmd.objective),
        ] {
            text(value, name, 2000)?
        }
        for role in [
            &cmd.roles.commander,
            &cmd.roles.operations,
            &cmd.roles.communications,
            &cmd.roles.scribe,
        ] {
            text(role, "accountable role", 128)?
        }
        let fp = store::digest(&format!("incident:{id}:{}", serde_json::to_string(&cmd)?));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let case = store::case(&tx, id)?;
        if case.version != cmd.command.expected_version {
            return Err(Error::Conflict("stale incident declaration".into()));
        }
        if case.owner != m.actor || m.actor.starts_with("fake-") {
            return Err(Error::Invalid(
                "accountable trusted operator must declare an incident".into(),
            ));
        }
        if snapshot(&tx, id)?.is_some() {
            return Err(Error::Conflict("case already has an incident".into()));
        }
        crate::qualification::evidence_refs(&tx, id, &cmd.evidence_refs, false)?;
        let source = crate::sentry::snapshot(&tx, id)?.ok_or_else(|| {
            Error::Invalid("first incident slice requires an observed source case".into())
        })?;
        let digest: String = tx.query_row(
            "SELECT digest FROM source_cases WHERE case_id=?1",
            [id],
            |r| r.get(0),
        )?;
        if !cmd.evidence_refs.contains(&digest) {
            return Err(Error::Conflict(
                "declaration must reference the current source evidence".into(),
            ));
        }
        if let Some(start) = &cmd.known_start {
            let start = DateTime::parse_from_rfc3339(start)
                .map_err(|_| Error::Invalid("known_start must be RFC3339 or null".into()))?;
            if start > self.clock.now() {
                return Err(Error::Invalid("start cannot be in the future".into()));
            }
        }
        let raw: String = tx.query_row(
            "SELECT record FROM source_configs WHERE revision=?1",
            [source.configuration_revision],
            |r| r.get(0),
        )?;
        let cfg: crate::sentry::SentryConfig = serde_json::from_str(&raw)?;
        let seconds = *cfg
            .incident_update_seconds
            .get(cmd.severity.key())
            .ok_or_else(|| Error::Invalid("missing trusted severity profile".into()))?;
        let due = (self.clock.now() + Duration::seconds(seconds.into()))
            .to_rfc3339_opts(SecondsFormat::Millis, true);
        let incident=Incident{schema_version:V1,id:store::id("inc"),case_id:id.into(),declared_at:self.now(),state:"declared".into(),policy_provenance:crate::policy::for_case(&tx,id)?.provenance(),source_snapshot:source,next_update_at:due.clone(),mitigation_at:None,recovered_at:None,recovery_assessment:"Not assessed. Source status, patch, merge or deployment do not establish recovery. Traffic denominator and observation window remain unknown.".into(),communication_draft:format!("INTERNAL DRAFT — not sent. {}. Impact: {}. Uncertainty: {}. Commander: {}. Next update: {}.",cmd.objective,cmd.impact,cmd.uncertainty,cmd.roles.commander,due),investigation_register:cmd.evidence_refs.clone(),declaration:cmd.clone()};
        tx.execute(
            "INSERT INTO incidents(id,case_id,record,next_update_at) VALUES (?1,?2,?3,?4)",
            params![incident.id, id, serde_json::to_string(&incident)?, due],
        )?;
        store::emit(
            &tx,
            id,
            operator(&m.actor),
            &m.idempotency_key,
            &self.now(),
            EventData::IncidentDeclared {
                incident: Box::new(incident.clone()),
                reason: m.reason.clone(),
            },
        )?;
        save_receipt(&tx, &m.idempotency_key, &fp, &incident)?;
        tx.commit()?;
        Ok(incident)
    }
    pub fn incident_markdown(&self, id: &str) -> Result<String> {
        let db = self.store.connect()?;
        let i = snapshot(&db, id)?.ok_or(Error::NotFound)?;
        let operations = crate::incident_ops::snapshot(&db, id)?.ok_or(Error::NotFound)?;
        let mut d = i.declaration.clone();
        d.severity = operations.severity.clone();
        d.impact = operations.impact.clone();
        d.uncertainty = operations.uncertainty.clone();
        d.objective = operations.objective.clone();
        d.roles = operations.roles.clone();
        let mut report = format!(
            "# Incident {}\n\nDeclaration state: declared. Recovery at declaration: not assessed.\n\n## Current brief\n\n- Objective: {}\n- Severity: {:?}\n- Rationale: {}\n- Impact: {}\n- Uncertainty: {}\n- Service / environment: {} / {}\n- Source: {}\n- Repository: {}\n- Release / deployed revision: {} / {}\n\n## Command roles\n\n| Responsibility | Owner |\n| --- | --- |\n| Commander | {} |\n| Operations | {} |\n| Communications | {} |\n| Scribe | {} |\n\n## Impact and timing\n\nKnown start: {}. Declared: {}. Mitigation and recovery: unknown. Traffic denominator and observation coverage are not established.\n\n## Response and communications\n\n{}\n\nNext update: {}. Draft only; no stakeholder communication sent.\n\n## Recovery assessment\n\n{}\n\n## Investigation evidence\n\n{}\n\n## Live timeline\n\n| Recorded at | Actor | Transition | Reason |\n| --- | --- | --- | --- |\n",
            i.id,
            safe(&d.objective),
            d.severity,
            safe(&d.rationale),
            safe(&d.impact),
            safe(&d.uncertainty),
            safe(&i.source_snapshot.service),
            safe(&i.source_snapshot.environment),
            safe(&i.source_snapshot.source_key),
            safe(&i.source_snapshot.repository),
            safe(i.source_snapshot.release.as_deref().unwrap_or("unknown")),
            safe(
                i.source_snapshot
                    .deployed_revision
                    .as_deref()
                    .unwrap_or("unknown")
            ),
            safe(&d.roles.commander),
            safe(&d.roles.operations),
            safe(&d.roles.communications),
            safe(&d.roles.scribe),
            safe(d.known_start.as_deref().unwrap_or("unknown")),
            i.declared_at,
            safe(&i.communication_draft),
            operations.next_update_at,
            i.recovery_assessment,
            i.investigation_register
                .iter()
                .map(|e| format!("- {}", safe(e)))
                .collect::<Vec<_>>()
                .join("\n")
        );
        for e in self.timeline(id, 0)? {
            let data = serde_json::to_value(&e.data)?;
            report.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                safe(&e.recorded_at),
                safe(&e.actor.id),
                safe(data["type"].as_str().unwrap_or("unknown")),
                safe(data["payload"]["reason"].as_str().unwrap_or("unknown"))
            ));
        }
        report.push_str(&format!("\n## Current coordination revision {}\n\nNext update: {}. Declaration below remains immutable.\n",operations.revision,operations.next_update_at));
        let raw = serde_json::to_string_pretty(&i)?
            .replace('`', "\\u0060")
            .replace('<', "\\u003c");
        report.push_str(&format!("\n## Continuing work\n\nCause, mitigation, deployment validation, recovery observation, postmortem applicability and corrective actions require subsequent evidence-backed records. Declaration closes none of these duties.\n\n## Authoritative structured record\n\n```json\n{raw}\n```\n"));
        report.push_str(&self.investigation_markdown(id)?);
        report.push_str(&self.recovery_markdown(id)?);
        Ok(report)
    }
}
