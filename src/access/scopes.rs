//! Resource authorization uses OS grants and normalized source mappings, never
//! operator-reported impact, an incident title, owner name or a request body.
use super::*;
use crate::{contracts::Snapshot, domain::Case};

const MATCH: &str = "(EXISTS(SELECT 1 FROM json_each(?1) g WHERE g.value=c.id) OR EXISTS(SELECT 1 FROM source_cases s,json_each(?2) g WHERE s.case_id=c.id AND json_extract(s.record,'$.service')=json_extract(g.value,'$.service') AND json_extract(s.record,'$.environment')=json_extract(g.value,'$.environment')))";

impl Session {
    pub(crate) fn case_allowed(&self, db: &Connection, id: &str) -> Result<bool> {
        if self.role != Role::ServiceOwner {
            return Ok(true);
        }
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM cases WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(false);
        }
        if self.case_ids.iter().any(|v| v == id) {
            return Ok(true);
        }
        let row: Option<(String, String)> = db
            .query_row(
                "SELECT record,digest FROM source_cases WHERE case_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((raw, sum)) = row else {
            return Ok(false);
        };
        if store::digest(&raw) != sum {
            return Err(Error::Conflict("source scope integrity failure".into()));
        }
        let source: crate::sentry::SourceObservation = serde_json::from_str(&raw)?;
        Ok(self
            .services
            .iter()
            .any(|s| s.service == source.service && s.environment == source.environment))
    }
    pub(crate) fn resource_allowed(&self, app: &Application, path: &str) -> Result<bool> {
        if self.role != Role::ServiceOwner {
            return Ok(true);
        }
        if path == "/session" || path == "/cases" {
            return Ok(true);
        }
        let db = app.store.connect()?;
        if let Some(tail) = path.strip_prefix("/cases/") {
            return self.case_allowed(&db, tail.split('/').next().unwrap_or(""));
        }
        if let Some(sum) = path.strip_prefix("/evidence/") {
            // Apply the grant in SQL before LIMIT, rather than scanning a global
            // prefix which could hide a later authorized membership.
            let sql = format!(
                "SELECT c.id FROM case_evidence e JOIN cases c ON c.id=e.case_id WHERE e.digest=?3 AND {MATCH}"
            );
            let mut q = db.prepare(&sql)?;
            for id in q.query_map(
                params![
                    serde_json::to_string(&self.case_ids)?,
                    serde_json::to_string(&self.services)?,
                    sum
                ],
                |r| r.get::<_, String>(0),
            )? {
                if self.case_allowed(&db, &id?)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(crate) fn body_allowed(&self, app: &Application, body: &Value) -> Result<bool> {
        if self.role != Role::ServiceOwner {
            return Ok(true);
        }
        // Correlation changes and duplicate dispositions explicitly consume a
        // second case. The domain still validates evidence/version/ownership.
        let db = app.store.connect()?;
        for pointer in ["/target_case", "/plan/target_case"] {
            if let Some(id) = body.pointer(pointer).and_then(Value::as_str)
                && !self.case_allowed(&db, id)?
            {
                return Ok(false);
            }
        }
        if let Some(cases) = body
            .pointer("/action/related_cases")
            .and_then(Value::as_array)
        {
            for id in cases {
                if let Some(id) = id.as_str()
                    && !self.case_allowed(&db, id)?
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    pub(crate) fn current_for_case(
        &self,
        app: &Application,
        secrets: &dyn Secrets,
        id: &str,
    ) -> Result<bool> {
        Ok(self.current(app, secrets)? && self.case_allowed(&app.store.connect()?, id)?)
    }
    pub(crate) fn list(&self, app: &Application) -> Result<Vec<Case>> {
        if self.role != Role::ServiceOwner {
            return app.list_cases();
        }
        let db = app.store.connect()?;
        let sql = format!("SELECT c.id FROM cases c WHERE {MATCH} ORDER BY c.rowid DESC LIMIT 200");
        let mut q = db.prepare(&sql)?;
        let mut result = vec![];
        for id in q.query_map(
            params![
                serde_json::to_string(&self.case_ids)?,
                serde_json::to_string(&self.services)?
            ],
            |r| r.get::<_, String>(0),
        )? {
            let id = id?;
            if self.case_allowed(&db, &id)? {
                result.push(store::case(&db, &id)?);
            }
        }
        Ok(result)
    }
    pub(crate) fn snapshot(&self, app: &Application, id: &str) -> Result<Snapshot> {
        let mut s = app.snapshot(id)?;
        if self.role != Role::ServiceOwner {
            return Ok(s);
        }
        let db = app.store.connect()?;
        if !self.case_allowed(&db, id)? {
            return Err(Error::Conflict(
                "case scope changed while preparing snapshot".into(),
            ));
        }
        s.correlations.topology = None;
        let mut relations = vec![];
        for r in s.correlations.relations {
            if self.case_allowed(&db, &r.source_case)? && self.case_allowed(&db, &r.target_case)? {
                relations.push(r);
            }
        }
        s.correlations.relations = relations;
        let mut candidates = vec![];
        for c in s.correlations.candidates {
            if self.case_allowed(&db, &c.case_id)? {
                candidates.push(c);
            }
        }
        s.correlations.candidates = candidates;
        s.correlations.limitation.push_str(" Service-owner view excludes automatic cross-scope correlation projections and global topology. Historical references and content already incorporated in this authorized case are retained; a reference grants no access to its target.");
        Ok(s)
    }
}
