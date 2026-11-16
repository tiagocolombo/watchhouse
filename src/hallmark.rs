//! Bounded Hallmark JSON exchange. Imported Markdown is untrusted data, never approval policy.
use crate::{
    application::{Application, operator, receipt, save_receipt, text},
    contracts::*,
    error::{Error, Result},
    learning, store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
const FORMAT: &str = "hallmark-library";
const PREFIX: &str = "hallmark-import:";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortableEntry {
    pub path: String,
    pub content: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub format: String,
    pub version: u32,
    pub learnings: Vec<PortableEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportCommand {
    pub command: OperatorCommand,
    pub source_namespace: String,
    pub payload: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Imported {
    pub schema_version: V1,
    pub id: String,
    pub case_id: String,
    pub source_namespace: String,
    pub source_path: String,
    pub digest: String,
    pub content: String,
    pub author: String,
    pub recorded_at: String,
    pub provenance: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub entries: Vec<Imported>,
    pub added: usize,
    pub already_present: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExportSelection {
    Imported { ids: Vec<String> },
    Knowledge { ids: Vec<String> },
}
pub fn parse_export(value: &Value) -> Result<Vec<PortableEntry>> {
    if serde_json::to_vec(value)?.len() > 24576 {
        return Err(Error::Invalid(
            "Hallmark import exceeds 24KiB; split the export into bounded batches".into(),
        ));
    }
    let entries = if value.is_array() {
        value.clone()
    } else {
        let o = value.as_object().ok_or_else(|| {
            Error::Invalid("Hallmark export must be an object or entry list".into())
        })?;
        if o.get("format").is_some_and(|f| f != FORMAT)
            || o.get("version").is_some_and(|v| v != 1)
            || o.get("action").is_some_and(|a| a != "export")
        {
            return Err(Error::Invalid(
                "unsupported Hallmark export format/version/action".into(),
            ));
        }
        o.get("learnings")
            .cloned()
            .ok_or_else(|| Error::Invalid("Hallmark export is missing learnings".into()))?
    };
    let entries: Vec<PortableEntry> = serde_json::from_value(entries)?;
    if entries.is_empty() || entries.len() > 20 {
        return Err(Error::Invalid(
            "Hallmark import requires 1..20 entries".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for entry in &entries {
        let path = entry.path.replace('\\', "/");
        if !crate::real_execution::protocol::portable(&path)
            || !path.ends_with(".md")
            || !seen.insert(path.to_ascii_lowercase())
        {
            return Err(Error::Invalid(
                "unsafe or duplicate/case-colliding Hallmark source path".into(),
            ));
        }
        text(&entry.content, "Hallmark Markdown", 16384)?;
    }
    Ok(entries)
}
pub(crate) fn get(db: &Connection, case: &str, id: &str) -> Result<Imported> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,record_digest FROM knowledge_imports WHERE id=?1 AND case_id=?2",
            params![id, case],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if store::digest(&raw) != digest {
        return Err(Error::Conflict(
            "import provenance record integrity failure".into(),
        ));
    }
    let entry: Imported = serde_json::from_str(&raw)?;
    if entry.case_id != case
        || entry.id != id
        || store::digest(&entry.content) != entry.digest
        || entry.provenance != format!("{PREFIX}{}", entry.id)
    {
        return Err(Error::Conflict(
            "import content/provenance integrity failure".into(),
        ));
    }
    Ok(entry)
}
pub(crate) fn validate_origin(db: &Connection, case: &str, origin: Option<&str>) -> Result<()> {
    if let Some(id) = origin.and_then(|s| s.strip_prefix(PREFIX)) {
        get(db, case, id)?;
    }
    Ok(())
}
pub(crate) fn snapshot(db: &Connection, case: &str) -> Result<Vec<Imported>> {
    let mut q = db.prepare(
        "SELECT id FROM knowledge_imports WHERE case_id=?1 ORDER BY rowid DESC LIMIT 100",
    )?;
    q.query_map([case], |r| r.get::<_, String>(0))?
        .map(|r| get(db, case, &r?))
        .collect()
}
impl Application {
    pub fn import_hallmark(&self, case: &str, cmd: ImportCommand) -> Result<ImportResult> {
        let m = &cmd.command.meta;
        crate::incident_ops::human(&m.actor)?;
        text(&m.idempotency_key, "key", 128)?;
        text(&m.reason, "reason", 2000)?;
        text(&cmd.source_namespace, "source namespace", 256)?;
        if cmd.source_namespace.contains(['\n', '\r']) {
            return Err(Error::Invalid(
                "source namespace must be a single scoped identifier".into(),
            ));
        }
        let entries = parse_export(&cmd.payload)?;
        let fp = store::digest(&format!(
            "hallmark-import:{case}:{}",
            serde_json::to_string(&cmd)?
        ));
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(r) = receipt(&tx, &m.idempotency_key, &fp)? {
            return Ok(r);
        }
        let c = store::case(&tx, case)?;
        if c.owner != m.actor || c.version != cmd.command.expected_version {
            return Err(Error::Conflict(
                "reload the owned case before importing untrusted knowledge".into(),
            ));
        }
        let mut result = ImportResult {
            entries: vec![],
            added: 0,
            already_present: 0,
        };
        for entry in entries {
            let digest = store::digest(&entry.content);
            let normalized = entry.path.replace('\\', "/").to_ascii_lowercase();
            let existing:Option<String>=tx.query_row("SELECT id FROM knowledge_imports WHERE case_id=?1 AND source_namespace=?2 AND normalized_path=?3 AND content_digest=?4",params![case,cmd.source_namespace,normalized,digest],|r|r.get(0)).optional()?;
            if let Some(id) = existing {
                result.entries.push(get(&tx, case, &id)?);
                result.already_present += 1;
                continue;
            }
            let id = store::id("import");
            let record = Imported {
                schema_version: V1,
                id: id.clone(),
                case_id: case.into(),
                source_namespace: cmd.source_namespace.clone(),
                source_path: entry.path,
                digest,
                content: entry.content,
                author: m.actor.clone(),
                recorded_at: self.now(),
                provenance: format!("{PREFIX}{id}"),
            };
            let raw = serde_json::to_string(&record)?;
            tx.execute(
                "INSERT INTO knowledge_imports VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    case,
                    record.source_namespace,
                    normalized,
                    record.digest,
                    store::digest(&raw),
                    raw
                ],
            )?;
            store::emit(
                &tx,
                case,
                operator(&m.actor),
                &m.idempotency_key,
                &self.now(),
                EventData::KnowledgeImported {
                    import_id: id,
                    source_namespace: record.source_namespace.clone(),
                    content_digest: record.digest.clone(),
                    reason: m.reason.clone(),
                },
            )?;
            result.entries.push(record);
            result.added += 1;
        }
        save_receipt(&tx, &m.idempotency_key, &fp, &result)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn export_hallmark(&self, case: &str, selection: ExportSelection) -> Result<ExportFile> {
        let db = self.store.connect()?;
        let tx = db.unchecked_transaction()?;
        store::case(&tx, case)?;
        let (ids, imported) = match selection {
            ExportSelection::Imported { ids } => (ids, true),
            ExportSelection::Knowledge { ids } => (ids, false),
        };
        if ids.is_empty()
            || ids.len() > 20
            || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
        {
            return Err(Error::Invalid(
                "export requires 1..20 distinct explicit record IDs".into(),
            ));
        }
        let mut learnings = vec![];
        let mut paths = std::collections::HashSet::new();
        let mut namespace = None;
        for (index, id) in ids.iter().enumerate() {
            let entry = if imported {
                let source = get(&tx, case, id)?;
                if namespace
                    .as_ref()
                    .is_some_and(|n| n != &source.source_namespace)
                {
                    return Err(Error::Invalid(
                        "export one source namespace at a time".into(),
                    ));
                }
                namespace = Some(source.source_namespace);
                PortableEntry {
                    path: source.source_path,
                    content: source.content,
                }
            } else {
                let revision:Option<u32>=tx.query_row("SELECT MAX(a.revision) FROM learning_acceptances a JOIN learning_documents d ON d.id=a.id AND d.revision=a.revision WHERE d.id=?1 AND d.case_id=?2",params![id,case],|r|r.get(0))?;
                let entry =
                    learning::entry(&tx, id, revision.ok_or(Error::NotFound)?, &self.now())?;
                if !entry.accepted_current {
                    return Err(Error::Conflict(
                        "only current independently accepted Watchhouse knowledge can be exported"
                            .into(),
                    ));
                }
                let learning::Content::Knowledge { draft } = entry.document.content else {
                    return Err(Error::Invalid(
                        "only knowledge documents use the Hallmark learning format".into(),
                    ));
                };
                let id = format!("L-{:04}", index + 1);
                let quote = |s: &str| serde_json::to_string(s).unwrap();
                let kind = match draft.kind.as_str() {
                    "service_map" => "map",
                    "decision" => "decision",
                    _ => "rule",
                };
                // External applicability is not authenticated by a local acceptance. Hallmark must
                // review this proposal in its own repository; the source identity remains explicit.
                let body = format!(
                    "---\nid: {id}\nkind: {kind}\nscope: []\nsource: [{}]\nstatus: proposed\nused: 0\nwatchhouse_document: {}\nwatchhouse_digest: {}\nwatchhouse_service: {}\nwatchhouse_environment: {}\nwatchhouse_repository: {}\nwatchhouse_revision: {}\nwatchhouse_kind: {}\nwatchhouse_expires_at: {}\nwatchhouse_import_origin: {}\n---\n# {}\n\n{}\n\n## Applicability\n\n{}\n\n## Invalidation conditions\n\n{}\n",
                    quote(&format!(
                        "watchhouse:{case}/{}@{}",
                        entry.document.id, entry.document.revision
                    )),
                    quote(&entry.document.id),
                    quote(&entry.digest),
                    quote(&entry.document.service),
                    quote(&entry.document.environment),
                    quote(&entry.document.repository),
                    entry.document.revision,
                    quote(&draft.kind),
                    quote(&entry.document.review_by),
                    quote(
                        draft
                            .imported_from
                            .as_deref()
                            .unwrap_or("native-watchhouse")
                    ),
                    draft.title.replace(['\n', '\r'], " "),
                    draft.body,
                    draft.applicability,
                    draft.invalidation_conditions
                );
                PortableEntry {
                    path: format!("{id}-watchhouse.md"),
                    content: body,
                }
            };
            if !paths.insert(entry.path.replace('\\', "/").to_ascii_lowercase()) {
                return Err(Error::Conflict(
                    "selected source revisions collide; export one revision per source path".into(),
                ));
            }
            learnings.push(entry);
        }
        Ok(ExportFile {
            format: FORMAT.into(),
            version: 1,
            learnings,
        })
    }
}
