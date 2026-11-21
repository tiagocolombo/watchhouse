//! OS-owned logical evidence removal with immutable tombstones and proof epochs.
use crate::{
    application::{Application, text},
    error::{Error, Result},
    store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tombstone {
    pub digest: String,
    pub redacted_at: String,
    pub operator: String,
    pub reason: String,
    pub original_bytes: u64,
    pub provenance: String,
    pub limitation: String,
}
pub(crate) fn epoch(db: &Connection, case: &str) -> Result<i64> {
    Ok(db.query_row(
        "SELECT COALESCE(MAX(sequence),0) FROM retention_invalidations WHERE case_id=?1",
        [case],
        |r| r.get(0),
    )?)
}
pub(crate) fn bind(db: &Connection, case: &str, kind: &str, id: &str) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO retention_proofs VALUES (?1,?2,?3,?4)",
        params![kind, id, case, epoch(db, case)?],
    )?;
    Ok(())
}
pub(crate) fn proof_current(db: &Connection, case: &str, kind: &str, id: &str) -> Result<bool> {
    let bound: Option<(String, i64)> = db
        .query_row(
            "SELECT case_id,epoch FROM retention_proofs WHERE kind=?1 AND identity=?2",
            [kind, id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let epoch = epoch(db, case)?;
    Ok(bound.is_some_and(|(c, e)| c == case && e == epoch))
}
pub(crate) fn tombstone(db: &Connection, sum: &str) -> Result<Option<Tombstone>> {
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,record_digest FROM evidence_redactions WHERE digest=?1",
            [sum],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    row.map(|(raw, digest)| {
        if store::digest(&raw) != digest {
            return Err(Error::Conflict("redaction audit integrity failure".into()));
        }
        let t: Tombstone = serde_json::from_str(&raw)?;
        if t.digest != sum {
            return Err(Error::Conflict("redaction identity mismatch".into()));
        }
        Ok(t)
    })
    .transpose()
}
pub(crate) fn usable(db: &Connection, sum: &str) -> Result<bool> {
    Ok(!db.query_row(
        "SELECT EXISTS(SELECT 1 FROM evidence_redactions WHERE digest=?1)",
        [sum],
        |r| r.get::<_, bool>(0),
    )?)
}
pub(crate) fn validate_operator(who: &str, reason: &str) -> Result<()> {
    crate::incident_ops::human(who)?;
    if who.starts_with("model-") {
        return Err(Error::Invalid("OS operator required".into()));
    }
    text(reason, "redaction reason", 2000)
}
impl Application {
    pub fn evidence_redactions(&self, case: &str) -> Result<Vec<Tombstone>> {
        let db = self.store.connect()?;
        store::case(&db, case)?;
        for_case(&db, case)
    }

    /// Logical removal of this evidence-store body. Other retained records,
    /// journals, exports and historical backups are explicitly separate copies.
    pub fn redact_evidence(&self, sum: &str, who: &str, reason: &str) -> Result<Tombstone> {
        validate_operator(who, reason)?;
        if !crate::real_execution::protocol::sha_valid(sum) {
            return Err(Error::Invalid("exact evidence digest required".into()));
        }
        let mut db = self.store.connect()?;
        // Avoid retaining the replaced body in newly freed SQLite cells. This
        // does not promise filesystem/WAL/snapshot secure physical erasure.
        db.pragma_update(None, "secure_delete", true)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(old) = tombstone(&tx, sum)? {
            return Ok(old);
        }
        let row: Option<(String, String)> = tx
            .query_row(
                "SELECT content,provenance FROM evidence WHERE digest=?1",
                [sum],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (body, provenance) = row.ok_or(Error::NotFound)?;
        if store::digest(&body) != sum {
            return Err(Error::Conflict(
                "evidence integrity failure; targeted corruption review required".into(),
            ));
        }
        let now = self.now();
        let t=Tombstone{digest:sum.into(),redacted_at:now.clone(),operator:who.into(),reason:reason.into(),original_bytes:body.len() as u64,provenance,limitation:"This evidence-store body is logically removed; its digest, audit, case records and other derived copies remain. Existing exports/backups, native journals and filesystem snapshots are separate retention scopes; physical secure erasure is not claimed. Dependent current proof is invalidated; historical facts and receipts are retained.".into()};
        let raw = serde_json::to_string(&t)?;
        tx.execute(
            "INSERT INTO evidence_redactions VALUES (?1,?2,?3)",
            params![sum, raw, store::digest(&raw)],
        )?;
        tx.execute("UPDATE evidence SET content='' WHERE digest=?1", [sum])?;
        // A temporary key set keeps memory bounded and preserves all direct
        // memberships, including source projections not in case_evidence.
        tx.execute_batch("CREATE TEMP TABLE redaction_cases(case_id TEXT PRIMARY KEY)")?;
        tx.execute("INSERT INTO redaction_cases SELECT case_id FROM case_evidence WHERE digest=?1 UNION SELECT case_id FROM source_cases WHERE digest=?1",[sum])?;
        tx.execute("INSERT OR IGNORE INTO redaction_cases SELECT case_id FROM dispositions WHERE json_extract(record,'$.plan.target_case') IN (SELECT case_id FROM redaction_cases)",[])?;
        let mut cases = tx.prepare("SELECT case_id FROM redaction_cases ORDER BY case_id")?;
        for case in cases.query_map([], |r| r.get::<_, String>(0))? {
            let case = case?;
            tx.execute(
                "INSERT INTO retention_invalidations(digest,case_id,recorded_at) VALUES (?1,?2,?3)",
                params![sum, case, now],
            )?;
            let epoch = epoch(&tx, &case)?;
            crate::recovery::invalidate_redacted(&tx, &case)?;
            tx.execute(
                "UPDATE impact_assessments SET due=1 WHERE case_id=?1",
                [&case],
            )?;
            let pause = store::id("pause");
            tx.execute(
                "INSERT INTO factory_pauses VALUES (?1,'case',?2,'all',?3,?4,1,'active',?5,NULL)",
                params![
                    pause,
                    case,
                    who,
                    "Evidence redacted; reassess dependencies and revalidate before resuming",
                    now
                ],
            )?;
            crate::factory::stop_case(&tx, &case, &pause, &now, crate::factory::Effect::All)?;
            tx.execute(
                "UPDATE dispositions SET state='reopened' WHERE case_id=?1 AND state='active'",
                [&case],
            )?;
            tx.execute("UPDATE obligations SET state='due',triggered_at=COALESCE(triggered_at,?1) WHERE disposition_id IN (SELECT id FROM dispositions WHERE case_id=?2) AND state='pending'",params![now,case])?;
            // Reopen current fulfilled actions conservatively. Their immutable
            // historical submission/verification records remain available.
            let mut actions=tx.prepare("SELECT record FROM corrective_actions WHERE case_id=?1 AND state IN ('submitted','complete','completed','verified','waived')")?;
            for raw in actions.query_map([&case], |r| r.get::<_, String>(0))? {
                let mut a: crate::recovery::CorrectiveAction = serde_json::from_str(&raw?)?;
                a.revision += 1;
                a.state = "open".into();
                a.submitted_at = None;
                a.submitted_version = None;
                a.submission_refs.clear();
                a.verification_refs.clear();
                a.residual_risk = None;
                a.escalated = true;
                tx.execute(
                    "UPDATE corrective_actions SET state='open',escalated=1,record=?1 WHERE id=?2",
                    params![serde_json::to_string(&a)?, a.id],
                )?;
            }
            store::emit(
                &tx,
                &case,
                crate::application::operator(who),
                sum,
                &now,
                crate::contracts::EventData::EvidenceRedacted {
                    digest: sum.into(),
                    epoch,
                    reason: reason.into(),
                },
            )?;
            crate::team::escalate(
                &tx,
                &case,
                "evidence_redaction",
                sum,
                "Evidence removed; owner must reassess missing proof, collect replacement evidence and revalidate dependent claims before explicitly resuming",
                &now,
            )?;
        }
        drop(cases);
        tx.execute("UPDATE factory_state SET version=version+1 WHERE id=1", [])?;
        tx.execute("INSERT INTO factory_events(record) VALUES (?1)",[serde_json::json!({"type":"evidence_redacted","tombstone":t,"affected_cases":tx.query_row("SELECT COUNT(*) FROM redaction_cases",[],|r|r.get::<_,i64>(0))?}).to_string()])?;
        tx.commit()?;
        Ok(t)
    }
}

pub(crate) fn for_case(db: &Connection, case: &str) -> Result<Vec<Tombstone>> {
    let mut q=db.prepare("SELECT DISTINCT r.digest FROM evidence_redactions r JOIN retention_invalidations i ON i.digest=r.digest WHERE i.case_id=?1 ORDER BY r.rowid")?;
    q.query_map([case], |r| r.get::<_, String>(0))?
        .map(|r| tombstone(db, &r?)?.ok_or(Error::NotFound))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn equal_clock_time_and_repeated_binding_cannot_resurrect_old_proof() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("state.db");
        let clock = crate::domain::ManualClock::new(
            chrono::DateTime::parse_from_rfc3339("2026-11-21T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        let a = Application::new(crate::store::Store::open(&path).unwrap(), Arc::new(clock));
        let case = a
            .create_case(crate::intake::FakeSource.manual_command("epoch fixture"))
            .unwrap();
        let db = crate::store::Store::open(&path).unwrap().connect().unwrap();
        let body = "Synthetic proof";
        let sum = store::digest(body);
        db.execute(
            "INSERT INTO evidence VALUES (?1,'text/plain',?2,'fixture','2026-11-21T12:00:00.000Z')",
            params![sum, body],
        )
        .unwrap();
        db.execute(
            "INSERT INTO case_evidence VALUES (?1,?2)",
            params![case.id, sum],
        )
        .unwrap();
        bind(&db, &case.id, "fixture", "old").unwrap();
        assert!(proof_current(&db, &case.id, "fixture", "old").unwrap());
        a.redact_evidence(
            &sum,
            "operator",
            "Invalidate without advancing the fixture clock",
        )
        .unwrap();
        bind(&db, &case.id, "fixture", "old").unwrap();
        assert!(!proof_current(&db, &case.id, "fixture", "old").unwrap());
        bind(&db, &case.id, "fixture", "fresh").unwrap();
        assert!(proof_current(&db, &case.id, "fixture", "fresh").unwrap());
        assert!(
            db.execute("UPDATE retention_proofs SET epoch=100", [])
                .is_err()
        );
        assert_eq!(
            db.query_row(
                "SELECT epoch FROM retention_proofs WHERE identity='old'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
