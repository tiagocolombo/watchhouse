//! OS-controlled publisher policy. No factory/model command can install or expand it.
use crate::{
    contracts::V1,
    error::{Error, Result},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherPolicy {
    pub schema_version: V1,
    pub revision: i64,
    pub repository: String,
    pub base_branch: String,
    pub token_env: String,
    pub author_name: String,
    pub author_email: String,
    pub draft: bool,
    pub paused: bool,
    pub job_seconds: u32,
    pub request_seconds: u32,
    pub reconciliation_attempts: u32,
    pub max_revisions: u32,
    pub retry_seconds: u32,
}
pub fn branch(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.contains("..")
        && !s.contains("@{")
        && s.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && !p.starts_with('-')
                && !p.ends_with('.')
                && !p.ends_with(".lock")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        })
}
impl PublisherPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.revision < 1
            || !self.repository.split_once('/').is_some_and(|(a, b)| {
                [a, b].iter().all(|s| {
                    !s.is_empty()
                        && ![".", ".."].contains(s)
                        && s.len() <= 128
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                })
            })
            || !branch(&self.base_branch)
            || !self.token_env.starts_with("WATCHHOUSE_")
            || self.token_env.len() > 128
            || !self
                .token_env
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            || self.author_name.trim().is_empty()
            || self.author_name.trim() != self.author_name
            || self.author_name.len() > 128
            || self
                .author_name
                .chars()
                .any(|c| c.is_control() || "<>".contains(c))
            || self.author_email.len() > 200
            || !self
                .author_email
                .split_once('@')
                .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
            || self.author_email.matches('@').count() != 1
            || self
                .author_email
                .bytes()
                .any(|b| !b.is_ascii_graphic() || b"<>".contains(&b))
            || !(1..=300).contains(&self.job_seconds)
            || self.request_seconds > self.job_seconds
            || !(1..=30).contains(&self.request_seconds)
            || !(1..=10).contains(&self.max_revisions)
            || !(1..=10).contains(&self.reconciliation_attempts)
            || !(1..=3600).contains(&self.retry_seconds)
        {
            return Err(Error::Invalid(
                "publisher policy exceeds explicit trusted bounds".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn policy(
    db: &rusqlite::Connection,
    revision: i64,
) -> crate::error::Result<PublisherPolicy> {
    use rusqlite::OptionalExtension;
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT record,digest FROM publisher_policies WHERE revision=?1",
            [revision],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (raw, digest) = row.ok_or(Error::NotFound)?;
    if crate::store::digest(&raw) != digest {
        return Err(Error::Conflict("publisher policy integrity failure".into()));
    }
    let p: PublisherPolicy = serde_json::from_str(&raw)?;
    p.validate()?;
    Ok(p)
}
impl crate::application::Application {
    pub fn install_publisher_policy(
        &self,
        p: PublisherPolicy,
        who: &str,
        source: &str,
    ) -> crate::error::Result<PublisherPolicy> {
        crate::incident_ops::human(who)?;
        crate::application::text(source, "publisher policy source", 4096)?;
        p.validate()?;
        let mut db = self.store.connect()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let latest: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM publisher_policies",
            [],
            |r| r.get(0),
        )?;
        let raw = serde_json::to_string(&p)?;
        if p.revision <= latest {
            if serde_json::to_string(&policy(&tx, p.revision)?)? == raw {
                return Ok(p);
            }
            return Err(Error::Conflict(
                "publisher policy versions are immutable".into(),
            ));
        }
        if p.revision != latest + 1 {
            return Err(Error::Conflict(
                "publisher policy must advance one revision".into(),
            ));
        }
        tx.execute(
            "INSERT INTO publisher_policies VALUES (?1,?2,?3,?4,?5,?6)",
            rusqlite::params![
                p.revision,
                crate::store::digest(&raw),
                raw,
                who,
                source,
                self.now()
            ],
        )?;
        tx.commit()?;
        Ok(p)
    }
    pub fn publisher_policies(&self) -> crate::error::Result<Vec<PublisherPolicy>> {
        let db = self.store.connect()?;
        let mut q =
            db.prepare("SELECT revision FROM publisher_policies ORDER BY revision DESC LIMIT 100")?;
        q.query_map([], |r| r.get::<_, i64>(0))?
            .map(|r| policy(&db, r?))
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn sample() -> PublisherPolicy {
        PublisherPolicy {
            schema_version: V1,
            revision: 1,
            repository: "fixture/repo".into(),
            base_branch: "main".into(),
            token_env: "WATCHHOUSE_GITHUB_FIXTURE_TOKEN".into(),
            author_name: "Watchhouse Fixture".into(),
            author_email: "fixture@example.invalid".into(),
            draft: true,
            paused: false,
            job_seconds: 120,
            request_seconds: 10,
            reconciliation_attempts: 3,
            max_revisions: 3,
            retry_seconds: 5,
        }
    }
    #[test]
    fn immutable_policy_has_provenance_without_loading_credentials() {
        let root = tempfile::tempdir().unwrap();
        let app = crate::application::Application::new(
            crate::store::Store::open(root.path().join("db")).unwrap(),
            Arc::new(crate::domain::SystemClock),
        );
        let p = sample();
        app.install_publisher_policy(p.clone(), "operator", "fixture-only-policy")
            .unwrap();
        assert!(
            app.install_publisher_policy(p.clone(), "fake-engineer", "self-modification")
                .is_err()
        );
        let mut altered = p.clone();
        altered.base_branch = "another".into();
        assert!(
            app.install_publisher_policy(altered, "operator", "replacement")
                .is_err()
        );
        app.install_publisher_policy(p, "operator", "idempotent-replay")
            .unwrap();
        let db = app.store.connect().unwrap();
        assert!(db.execute("DELETE FROM publisher_policies", []).is_err());
        assert_eq!(app.publisher_policies().unwrap().len(), 1);
        let row: (String, String) = db
            .query_row("SELECT operator,source FROM publisher_policies", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(row, ("operator".into(), "fixture-only-policy".into()));
    }
    #[test]
    fn unsupported_refs_secret_sources_and_budgets_are_rejected() {
        let mut p = sample();
        p.token_env = "HOME".into();
        assert!(p.validate().is_err());
        let mut p = sample();
        p.base_branch = "main/../other".into();
        assert!(p.validate().is_err());
        let mut p = sample();
        p.request_seconds = 100;
        assert!(p.validate().is_err());
        let mut p = sample();
        p.author_name = "bad\nidentity".into();
        assert!(p.validate().is_err());
    }
}
