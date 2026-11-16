use super::cgroup::Limits;
use crate::{
    contracts::V1,
    error::{Error, Result},
    real_execution::protocol::sha_valid,
    repair::snapshot::file_digest,
    store,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub path: PathBuf,
    pub digest: String,
}
impl Pin {
    pub fn verify(&self) -> Result<()> {
        let metadata = std::fs::metadata(&self.path)
            .map_err(|_| Error::Invalid("pinned binary metadata unavailable".into()))?;
        if !metadata.is_file()
            || metadata.len() > 1024 * 1024 * 1024
            || !self.path.is_absolute()
            || !sha_valid(&self.digest)
            || std::fs::canonicalize(&self.path)
                .map_err(|_| Error::Invalid("pinned runtime/control binary unavailable".into()))?
                != self.path
            || file_digest(&self.path)? != self.digest
        {
            return Err(Error::Conflict(
                "pinned runtime/control binary changed".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authority {
    Fixture,
    OpenaiAuthHome { path: PathBuf },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPolicy {
    pub schema_version: V1,
    pub revision: i64,
    pub container_revision: i64,
    pub dispatch_grace_seconds: u32,
    pub runtime: Pin,
    pub initializer: Pin,
    pub cgroup_root: PathBuf,
    pub jobs_root: PathBuf,
    pub requested_model: String,
    pub authority: Authority,
    pub limits: Limits,
    pub proof_seconds: u32,
    pub cleanup_attempts: u32,
    pub cleanup_retry_seconds: u32,
}
impl ModelPolicy {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        if self.revision < 1
            || self.container_revision < 1
            || !(10..=120).contains(&self.dispatch_grace_seconds)
            || !(1..=3600).contains(&self.proof_seconds)
            || !(1..=10).contains(&self.cleanup_attempts)
            || !(1..=3600).contains(&self.cleanup_retry_seconds)
            || self.requested_model.is_empty()
            || self.requested_model.len() > 128
            || self.requested_model.starts_with('-')
            || !self
                .requested_model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
            || !self.jobs_root.is_absolute()
            || !self.cgroup_root.is_absolute()
            || !self.jobs_root.is_dir()
        {
            return Err(Error::Invalid(
                "invalid versioned text-client policy".into(),
            ));
        }
        if self.runtime.path.starts_with(&self.jobs_root)
            || self.initializer.path.starts_with(&self.jobs_root)
            || self.cgroup_root.starts_with(&self.jobs_root)
            || self.jobs_root.starts_with(&self.cgroup_root)
        {
            return Err(Error::Invalid(
                "runtime/control/cgroup identities must be outside job scratch".into(),
            ));
        }
        if let Authority::OpenaiAuthHome { path } = &self.authority
            && (!path.is_absolute()
                || !path.is_dir()
                || path.starts_with(&self.jobs_root)
                || self.jobs_root.starts_with(path))
        {
            return Err(Error::Invalid(
                "supported harness auth home must remain separate from factory jobs".into(),
            ));
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        Ok(store::digest(&serde_json::to_string(self)?))
    }
    pub fn native_preflight(&self) -> Result<()> {
        self.validate()?;
        self.runtime.verify()?;
        self.initializer.verify()?;
        super::cgroup::validate_root(&self.cgroup_root)?;
        private_dir(&self.jobs_root)?;
        if let Authority::OpenaiAuthHome { path } = &self.authority {
            private_dir(path)?;
        }
        Ok(())
    }
}
pub fn private_dir(path: &Path) -> Result<()> {
    if std::fs::canonicalize(path)
        .map_err(|_| Error::Invalid("operator directory unavailable".into()))?
        != path
    {
        return Err(Error::Invalid(
            "operator directories must be canonical".into(),
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let own = std::fs::metadata("/proc/self")
            .map_err(|_| Error::Invalid("worker identity unavailable".into()))?
            .uid();
        let meta = std::fs::metadata(path)
            .map_err(|_| Error::Invalid("operator directory metadata unavailable".into()))?;
        if own == 0 || meta.uid() != own || meta.mode() & 0o077 != 0 {
            return Err(Error::Invalid(
                "client/job directories must be private and owned by the unprivileged worker"
                    .into(),
            ));
        }
    }
    Ok(())
}
