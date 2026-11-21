use super::*;
use rusqlite::types::ValueRef;

const DIRECT: &[&str] = &[
    "cases",
    "assignments",
    "case_evidence",
    "case_gates",
    "case_policy",
    "challenges",
    "escalations",
    "events",
    "incidents",
    "incident_operations",
    "incident_role_offers",
    "incident_artifacts",
    "artifact_obligations",
    "dispositions",
    "recovery_state",
    "recovery_records",
    "corrective_actions",
    "learning_documents",
    "knowledge_imports",
    "impact_assessments",
    "repair_requests",
    "repair_candidates",
    "candidate_reviews",
    "container_jobs",
    "model_jobs",
    "publication_jobs",
    "merge_jobs",
    "operation_jobs",
    "operation_observations",
    "problem_revisions",
    "adapter_trials",
    "worker_qualifications",
    "team_threads",
    "source_cases",
    "retention_invalidations",
    "retention_proofs",
    "case_command_refs",
    "case_exports",
];
const INDIRECT: &[(&str, &str)] = &[
    (
        "assignment_control",
        "assignment_id IN (SELECT id FROM assignments WHERE case_id=?1)",
    ),
    (
        "jobs",
        "assignment_id IN (SELECT id FROM assignments WHERE case_id=?1)",
    ),
    (
        "runs",
        "assignment_id IN (SELECT id FROM assignments WHERE case_id=?1)",
    ),
    (
        "run_control",
        "run_id IN (SELECT r.id FROM runs r JOIN assignments a ON a.id=r.assignment_id WHERE a.case_id=?1)",
    ),
    (
        "progress_evidence",
        "assignment_id IN (SELECT id FROM assignments WHERE case_id=?1)",
    ),
    (
        "late_results",
        "run_id IN (SELECT r.id FROM runs r JOIN assignments a ON a.id=r.assignment_id WHERE a.case_id=?1)",
    ),
    (
        "blocks",
        "assignment_id IN (SELECT id FROM assignments WHERE case_id=?1)",
    ),
    (
        "handoffs",
        "block_id IN (SELECT b.id FROM blocks b JOIN assignments a ON a.id=b.assignment_id WHERE a.case_id=?1)",
    ),
    (
        "team_messages",
        "thread_id IN (SELECT id FROM team_threads WHERE case_id=?1)",
    ),
    (
        "message_ack",
        "message_id IN (SELECT m.id FROM team_messages m JOIN team_threads t ON t.id=m.thread_id WHERE t.case_id=?1)",
    ),
    (
        "escalation_resolutions",
        "escalation_id IN (SELECT id FROM escalations WHERE case_id=?1)",
    ),
    (
        "escalation_routes",
        "escalation_id IN (SELECT id FROM escalations WHERE case_id=?1)",
    ),
    (
        "notification_deliveries",
        "escalation_id IN (SELECT id FROM escalations WHERE case_id=?1)",
    ),
    (
        "obligations",
        "disposition_id IN (SELECT id FROM dispositions WHERE case_id=?1)",
    ),
    (
        "reconsideration_evidence",
        "disposition_id IN (SELECT id FROM dispositions WHERE case_id=?1)",
    ),
    (
        "communication_approvals",
        "id IN (SELECT id FROM incident_artifacts WHERE case_id=?1)",
    ),
    (
        "hypothesis_approvals",
        "id IN (SELECT id FROM incident_artifacts WHERE case_id=?1)",
    ),
    (
        "learning_acceptances",
        "id IN (SELECT id FROM learning_documents WHERE case_id=?1)",
    ),
    (
        "knowledge_retirements",
        "id IN (SELECT id FROM learning_documents WHERE case_id=?1)",
    ),
    (
        "repair_execution_budgets",
        "repair_id IN (SELECT id FROM repair_requests WHERE case_id=?1)",
    ),
    (
        "publication_slots",
        "repair_id IN (SELECT id FROM repair_requests WHERE case_id=?1)",
    ),
    (
        "adapter_trial_events",
        "trial_id IN (SELECT id FROM adapter_trials WHERE case_id=?1)",
    ),
    (
        "fake_adapter_sessions",
        "id IN (SELECT id FROM adapter_trials WHERE case_id=?1)",
    ),
    (
        "worker_qualification_late",
        "id IN (SELECT id FROM worker_qualifications WHERE case_id=?1)",
    ),
    (
        "source_read_state",
        "source_key IN (SELECT source_key FROM source_cases WHERE case_id=?1)",
    ),
    (
        "datadog_reads",
        "source_key IN (SELECT source_key FROM source_cases WHERE case_id=?1)",
    ),
    (
        "worker_bindings",
        "(kind='model' AND job_id IN (SELECT id FROM model_jobs WHERE case_id=?1)) OR (kind='container' AND job_id IN (SELECT id FROM container_jobs WHERE case_id=?1))",
    ),
    (
        "factory_resource_scopes",
        "(kind='model' AND job_id IN (SELECT id FROM model_jobs WHERE case_id=?1)) OR (kind='container' AND job_id IN (SELECT id FROM container_jobs WHERE case_id=?1))",
    ),
    ("factory_pauses", "scope='case' AND target=?1"),
    (
        "commands",
        "idempotency_key IN (SELECT idempotency_key FROM case_command_refs WHERE case_id=?1)",
    ),
    (
        "factory_events",
        "json_extract(record,'$.case_id')=?1 OR (json_extract(record,'$.scope')='case' AND json_extract(record,'$.target')=?1)",
    ),
];
const PRIVATE_KEYS: &[&str] = &[
    "fence",
    "nonce",
    "root",
    "snapshot_path",
    "auth_home",
    "control_root",
    "cgroup_root",
    "resource",
    "fixture_response",
    "prompt",
    "authorization",
    "api_key",
    "password",
    "proxy_token",
    "access_token",
    "refresh_token",
    "client_secret",
];
const JSON_COLUMNS: &[&str] = &[
    "record",
    "envelope",
    "briefing",
    "response",
    "result",
    "evidence_refs",
    "config",
];

pub(super) fn sanitize(value: &mut Value, path: &str, omitted: &mut Vec<String>, patches: bool) {
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if PRIVATE_KEYS.contains(&key.as_str()) || (!patches && key == "changes") {
                    map.remove(&key);
                    omitted.push(next);
                } else if let Some(child) = map.get_mut(&key) {
                    sanitize(child, &next, omitted, patches);
                }
            }
        }
        Value::Array(values) => {
            for (i, child) in values.iter_mut().enumerate() {
                sanitize(child, &format!("{path}[{i}]"), omitted, patches);
            }
        }
        _ => {}
    }
}

fn rows(
    db: &Connection,
    table: &str,
    predicate: &str,
    case: &str,
    options: &Options,
    emit: &mut dyn FnMut(&str, Value) -> Result<()>,
) -> Result<()> {
    // Identifiers and predicates are fixed by this module, never supplied by a caller.
    let mut q = db.prepare(&format!(
        "SELECT rowid AS export_rowid,* FROM {table} WHERE {predicate} ORDER BY rowid"
    ))?;
    let names: Vec<String> = q
        .column_names()
        .iter()
        .skip(1)
        .map(|s| (*s).to_owned())
        .collect();
    let mut rows = q.query([case])?;
    while let Some(row) = rows.next()? {
        let mut raw = serde_json::Map::new();
        let mut bytes = 0usize;
        for (i, name) in names.iter().enumerate() {
            let value = match row.get_ref(i + 1)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(s) => {
                    bytes = bytes.saturating_add(s.len());
                    if bytes > ROW_LIMIT {
                        return Err(Error::Backpressure("case export row exceeds 8MiB".into()));
                    }
                    Value::String(
                        std::str::from_utf8(s)
                            .map_err(|_| invalid("non-UTF8 case record"))?
                            .to_owned(),
                    )
                }
                ValueRef::Blob(_) => {
                    return Err(invalid(
                        "binary SQL field requires an explicit export adapter",
                    ));
                }
            };
            raw.insert(name.clone(), value);
        }
        let original = Value::Object(raw);
        let source_digest = store::digest(&serde_json::to_string(&original)?);
        let mut structured = original.clone();
        if let Some(map) = structured.as_object_mut() {
            for name in JSON_COLUMNS {
                if let Some(Value::String(text)) = map.get(*name) {
                    let parsed = serde_json::from_str::<Value>(text)
                        .map_err(|_| invalid("malformed structured column; export refused"))?;
                    map.insert((*name).into(), parsed);
                }
            }
        }
        let mut omitted = vec![];
        sanitize(&mut structured, "", &mut omitted, options.include_patches);
        let exported_digest = store::digest(&serde_json::to_string(&structured)?);
        emit(
            table,
            json!({
                "kind":"record_view", "source_rowid":row.get::<_,i64>(0)?,
                "source_digest":source_digest, "exported_digest":exported_digest,
                "transformed":structured!=original, "omitted_fields":omitted, "record":structured
            }),
        )?;
    }
    Ok(())
}

pub(super) fn capture(
    db: &Connection,
    case: &str,
    options: &Options,
    scope: Option<&crate::access::Session>,
    emit: &mut dyn FnMut(&str, Value) -> Result<()>,
) -> Result<()> {
    for table in DIRECT {
        rows(
            db,
            table,
            if *table == "cases" {
                "id=?1"
            } else {
                "case_id=?1"
            },
            case,
            options,
            emit,
        )?;
    }
    for (table, predicate) in INDIRECT {
        rows(db, table, predicate, case, options, emit)?;
    }
    let mut q = db.prepare("SELECT left_case,right_case,record FROM case_correlations WHERE left_case=?1 OR right_case=?1 ORDER BY rowid")?;
    let mut linked = q.query([case])?;
    while let Some(row) = linked.next()? {
        let left: String = row.get(0)?;
        let right: String = row.get(1)?;
        if let Some(scope) = scope
            && (!scope.case_allowed(db, &left)? || !scope.case_allowed(db, &right)?)
        {
            continue;
        }
        // No automatic neighbor projections or global topology is exported.
        let raw: String = row.get(2)?;
        if raw.len() > ROW_LIMIT {
            return Err(invalid("correlation exceeds row bound"));
        }
        let mut value: Value = serde_json::from_str(&raw)?;
        let original = value.clone();
        let mut omitted = vec![];
        sanitize(&mut value, "", &mut omitted, options.include_patches);
        emit(
            "case_correlations",
            json!({"kind":"record_view","source_digest":store::digest(&serde_json::to_string(&original)?),"exported_digest":store::digest(&serde_json::to_string(&value)?),"transformed":value!=original,"omitted_fields":omitted,"record":value}),
        )?;
    }
    let mut q = db.prepare("SELECT c.digest,e.media_type,e.content,e.provenance,e.recorded_at FROM case_evidence c LEFT JOIN evidence e ON e.digest=c.digest WHERE c.case_id=?1 ORDER BY c.digest")?;
    let mut evidence = q.query([case])?;
    while let Some(row) = evidence.next()? {
        let sum: String = row.get(0)?;
        let body: Option<String> = match row.get_ref(2)? {
            ValueRef::Null => None,
            ValueRef::Text(bytes) => {
                if bytes.len() > ROW_LIMIT {
                    return Err(Error::Backpressure(
                        "evidence exceeds the 8MiB export row bound".into(),
                    ));
                }
                Some(
                    std::str::from_utf8(bytes)
                        .map_err(|_| invalid("non-UTF8 evidence"))?
                        .to_owned(),
                )
            }
            _ => return Err(invalid("unexpected evidence storage type")),
        };
        let removed = crate::retention::tombstone(db, &sum)?;
        let actual = body.as_ref().map(|body| store::digest(body));
        let state = if removed.is_some() {
            "redacted"
        } else if body.is_none() {
            "missing"
        } else if actual.as_deref() != Some(&sum) {
            "corrupt"
        } else {
            "present"
        };
        let included = options.include_evidence && removed.is_none() && body.is_some();
        emit(
            "evidence_manifest",
            json!({
                "kind":"evidence", "digest":sum, "state":state, "included":included,
                "observed_digest":actual, "bytes":body.as_ref().map(String::len),
                "media_type":row.get::<_,Option<String>>(1)?, "provenance":row.get::<_,Option<String>>(3)?,
                "recorded_at":row.get::<_,Option<String>>(4)?, "tombstone":removed,
                "body":if included{body}else{None}
            }),
        )?;
    }
    Ok(())
}
