use super::{cgroup::Handle, client::ClientResult, codex::Role, policy::ModelPolicy};
use crate::{
    contracts::V1,
    error::{Error, Result},
    store,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub schema_version: V1,
    pub operation: String,
    pub nonce: String,
    pub deadline: String,
    pub profile: ModelPolicy,
    pub profile_digest: String,
    pub resource: Handle,
    pub role: Role,
    pub prompt: String,
    pub output_schema: Value,
    pub fixture_response: Option<Value>,
    pub qualification: bool,
}
impl Envelope {
    pub fn validate(&self) -> Result<()> {
        self.profile.validate()?;
        chrono::DateTime::parse_from_rfc3339(&self.deadline)
            .map_err(|_| Error::Invalid("client deadline is invalid".into()))?;
        if uuid::Uuid::parse_str(&self.operation).is_err()
            || uuid::Uuid::parse_str(&self.nonce).is_err()
            || self.profile_digest != self.profile.digest()?
            || self.resource.operation != self.operation
            || self.resource.profile_digest != self.profile_digest
            || serde_json::to_value(&self.resource.limits)?
                != serde_json::to_value(&self.profile.limits)?
            || self.prompt.len() > 262144
            || self.prompt.contains('\0')
            || serde_json::to_vec(&self.output_schema)?.len() > 16384
            || self.qualification != matches!(self.role, Role::Qualification)
        {
            return Err(Error::Invalid(
                "model-client envelope/fence/schema mismatch".into(),
            ));
        }
        if !self.qualification
            && !matches!(self.profile.authority, super::policy::Authority::Fixture)
            && self.fixture_response.is_some()
        {
            return Err(Error::Invalid(
                "live client work cannot choose a synthetic response".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub schema_version: V1,
    pub operation: String,
    pub nonce: String,
    pub profile_digest: String,
    pub client: ClientResult,
    pub synthetic: bool,
    pub execution_tools_absent: Option<bool>,
    pub advertised_tools_digest: Option<String>,
    pub descendant_started: bool,
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    let file = fs::File::open(path)
        .map_err(|_| Error::Invalid("sealed client journal unavailable".into()))?;
    let mut bytes = vec![];
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Invalid("sealed journal read failed".into()))?;
    if bytes.len() as u64 > limit {
        return Err(Error::Invalid("sealed journal exceeds bound".into()));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn write_once<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("journal record exceeds 1MiB".into()));
    }
    let pending = path.with_extension("pending");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|_| {
            Error::Conflict(
                "journal already reserved or interrupted; reconcile existing bytes".into(),
            )
        })?;
    file.write_all(&bytes)
        .map_err(|_| Error::Invalid("journal write failed".into()))?;
    file.sync_all()
        .map_err(|_| Error::Invalid("journal fsync failed".into()))?;
    if path.exists() {
        return Err(Error::Conflict("journal outcome already exists".into()));
    }
    fs::rename(pending, path)
        .map_err(|_| Error::Invalid("journal finalization uncertain".into()))?;
    #[cfg(unix)]
    fs::File::open(
        path.parent()
            .ok_or_else(|| Error::Invalid("journal parent missing".into()))?,
    )
    .and_then(|dir| dir.sync_all())
    .map_err(|_| Error::Invalid("journal directory fsync uncertain".into()))?;
    Ok(())
}
pub fn validate_output(job: &Envelope, output: &Output) -> Result<()> {
    if output.operation != job.operation
        || output.nonce != job.nonce
        || output.profile_digest != job.profile_digest
        || output.synthetic
            != (job.qualification
                || matches!(job.profile.authority, super::policy::Authority::Fixture))
    {
        return Err(Error::Conflict(
            "client output/profile/transport fence mismatch".into(),
        ));
    }
    if !crate::real_execution::protocol::sha_valid(&output.client.stderr_digest) {
        return Err(Error::Invalid(
            "client diagnostic provenance missing".into(),
        ));
    }
    Ok(())
}
pub fn digest(job: &Envelope) -> Result<String> {
    Ok(store::digest(&serde_json::to_string(job)?))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CancellationFence {
    pub operation: String,
    pub nonce: String,
    pub profile_digest: String,
}
pub fn cancelled(root: &Path, job: &Envelope) -> Result<bool> {
    if !root.join("cancel.json").exists() {
        return Ok(false);
    }
    let fence: CancellationFence = read_json(&root.join("cancel.json"), 2048)?;
    if fence.operation != job.operation
        || fence.nonce != job.nonce
        || fence.profile_digest != job.profile_digest
    {
        return Err(Error::Conflict("foreign cancellation barrier".into()));
    }
    Ok(true)
}
pub fn revoke(root: &Path, operation: &str, nonce: &str, profile_digest: &str) -> Result<()> {
    let expected = CancellationFence {
        operation: operation.into(),
        nonce: nonce.into(),
        profile_digest: profile_digest.into(),
    };
    if root.join("cancel.json").exists() {
        let found: CancellationFence = read_json(&root.join("cancel.json"), 2048)?;
        if found != expected {
            return Err(Error::Conflict("foreign cancellation barrier".into()));
        }
        return Ok(());
    }
    write_once(&root.join("cancel.json"), &expected)
}
pub fn check_start(root: &Path, job: &Envelope) -> Result<()> {
    let deadline = chrono::DateTime::parse_from_rfc3339(&job.deadline)
        .map_err(|_| Error::Invalid("client deadline invalid".into()))?;
    if chrono::Utc::now() >= deadline || cancelled(root, job)? {
        return Err(Error::Conflict(
            "client authority expired or was cancelled; no model request permitted".into(),
        ));
    }
    Ok(())
}
