//! Portable credential-free worker protocol. Providers do not own workflow authority.
use crate::{
    contracts::V1,
    error::{Error, Result},
    store,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    #[serde(default)]
    pub executable: bool,
    pub path: String,
    pub digest: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerJob {
    pub schema_version: V1,
    pub operation: String,
    pub nonce: String,
    pub profile_digest: String,
    pub runner_digest: String,
    pub files: Vec<InputFile>,
    pub program: String,
    pub args: Vec<String>,
    pub wall_seconds: u32,
    pub output_bytes: u32,
    pub memory_bytes: u64,
    pub pids: u32,
    pub cpu_micros: u32,
    pub work_bytes: u64,
    pub host_net_namespace: String,
    pub host_pid_namespace: String,
    pub canary_port: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerOutput {
    pub schema_version: V1,
    pub operation: String,
    pub nonce: String,
    pub profile_digest: String,
    pub exit_code: Option<i32>,
    pub failure_code: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub input_digest: String,
    pub checks: std::collections::BTreeMap<String, bool>,
}
pub fn portable(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.len() <= 128
                && !p.ends_with('.')
                && !p.eq_ignore_ascii_case(".git")
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
                && !["CON", "PRN", "AUX", "NUL"]
                    .contains(&p.split('.').next().unwrap().to_ascii_uppercase().as_str())
                && !{
                    let s = p.split('.').next().unwrap().to_ascii_uppercase();
                    s.len() == 4
                        && (s.starts_with("COM") || s.starts_with("LPT"))
                        && s.as_bytes()[3].is_ascii_digit()
                }
        })
}
pub fn sha_valid(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|c| c.is_ascii_hexdigit())
}
impl WorkerJob {
    pub fn validate(&self) -> Result<()> {
        if self.operation.len() != 36
            || !self
                .operation
                .bytes()
                .all(|c| c.is_ascii_hexdigit() || c == b'-')
            || self.nonce.len() != 36
            || !self
                .nonce
                .bytes()
                .all(|c| c.is_ascii_hexdigit() || c == b'-')
            || !sha_valid(&self.profile_digest)
            || !sha_valid(&self.runner_digest)
            || self.files.is_empty()
            || self.files.len() > 5000
            || self.wall_seconds == 0
            || self.wall_seconds > 3600
            || !(1024..=65536).contains(&self.output_bytes)
            || !(64 * 1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
            || !(8..=256).contains(&self.pids)
            || !(10000..=400000).contains(&self.cpu_micros)
            || self.work_bytes < 4 * 1024 * 1024
            || self.work_bytes > self.memory_bytes / 2
            || self.program.len() > 512
            || !self.program.strip_prefix('/').is_some_and(portable)
            || ["/work", "/input", "/policy", "/tmp"]
                .iter()
                .any(|p| self.program == *p || self.program.starts_with(&format!("{p}/")))
            || self.program.contains('\0')
            || self.args.len() > 32
            || self.args.iter().any(|s| s.len() > 4096 || s.contains('\0'))
        {
            return Err(Error::Invalid(
                "worker envelope exceeds trusted protocol bounds".into(),
            ));
        }
        if serde_json::to_vec(self)?.len() > 1024 * 1024 {
            return Err(Error::Invalid("worker manifest exceeds 1MiB".into()));
        }
        let mut total = 0u64;
        let mut names = std::collections::HashSet::new();
        for f in &self.files {
            if !portable(&f.path)
                || !sha_valid(&f.digest)
                || !names.insert(f.path.to_ascii_lowercase())
                || f.bytes > 8 * 1024 * 1024
            {
                return Err(Error::Invalid("invalid worker input manifest".into()));
            }
            total = total
                .checked_add(f.bytes)
                .ok_or_else(|| Error::Invalid("input size overflow".into()))?;
        }
        if total > self.work_bytes / 2 {
            return Err(Error::Invalid(
                "inputs exceed half of bounded work storage".into(),
            ));
        }
        Ok(())
    }
    pub fn input_digest(&self) -> Result<String> {
        Ok(store::digest(&serde_json::to_string(&self.files)?))
    }
}

pub fn bytes_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{:x}", Sha256::digest(bytes))
}
