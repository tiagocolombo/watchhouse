//! Fixed local Docker executor. No candidate credentials, host-writable mounts or arbitrary flags.
use super::protocol::*;
use crate::{
    error::{Error, Result},
    repair::snapshot::file_digest,
    store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::{Duration, timeout},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerPolicy {
    pub schema_version: crate::contracts::V1,
    pub revision: i64,
    pub docker_path: PathBuf,
    pub docker_digest: String,
    pub socket: PathBuf,
    pub image: String,
    pub runner_digest: String,
    pub staging_root: PathBuf,
    pub program: String,
    pub args: Vec<String>,
    pub memory_bytes: u64,
    pub pids: u32,
    pub cpu_micros: u32,
    pub work_bytes: u64,
    pub wall_seconds: u32,
    pub output_bytes: u32,
    pub proof_seconds: u32,
    pub dispatch_grace_seconds: u32,
    pub cleanup_attempts: u32,
    pub cleanup_retry_seconds: u32,
}
impl ContainerPolicy {
    pub fn validate(&self) -> Result<()> {
        if !(15..=300).contains(&self.dispatch_grace_seconds)
            || !(1..=10).contains(&self.cleanup_attempts)
            || !(1..=3600).contains(&self.cleanup_retry_seconds)
            || self.proof_seconds == 0
            || self.proof_seconds > 3600
            || self.revision < 1
            || !self.docker_path.is_absolute()
            || !self.socket.is_absolute()
            || !self.staging_root.is_absolute()
            || !sha_valid(&self.docker_digest)
            || !sha_valid(&self.image)
            || !sha_valid(&self.runner_digest)
            || !self.staging_root.is_dir()
        {
            return Err(Error::Invalid(
                "invalid immutable container policy/pins/roots".into(),
            ));
        }
        let root = self.staging_root.to_string_lossy();
        if root.contains([',', '\n', '\r'])
            || self.docker_path.starts_with(&self.staging_root)
            || self.socket.starts_with(&self.staging_root)
        {
            return Err(Error::Invalid(
                "container control plane must be outside staging".into(),
            ));
        }
        let sample = self.job(
            "00000000-0000-0000-0000-000000000000".into(),
            "00000000-0000-0000-0000-000000000001".into(),
            vec![InputFile {
                executable: false,
                path: "value".into(),
                digest: store::digest("x"),
                bytes: 1,
            }],
        );
        sample.validate()
    }
    pub fn digest(&self) -> Result<String> {
        Ok(store::digest(&serde_json::to_string(self)?))
    }
    pub fn job(&self, operation: String, nonce: String, files: Vec<InputFile>) -> WorkerJob {
        WorkerJob {
            schema_version: crate::contracts::V1,
            operation,
            nonce,
            profile_digest: self.digest().unwrap_or_default(),
            runner_digest: self.runner_digest.clone(),
            files,
            program: self.program.clone(),
            args: self.args.clone(),
            wall_seconds: self.wall_seconds,
            output_bytes: self.output_bytes,
            memory_bytes: self.memory_bytes,
            pids: self.pids,
            cpu_micros: self.cpu_micros,
            work_bytes: self.work_bytes,
            host_net_namespace: String::new(),
            host_pid_namespace: String::new(),
            canary_port: 0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub status: String,
    pub running: bool,
    pub exit_code: i32,
    pub oom_killed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineIdentity {
    pub id: String,
    pub version: String,
    pub architecture: String,
    pub cgroup_version: String,
    pub kernel_version: String,
    pub security_digest: String,
    pub host_boot_id: String,
}
#[derive(Clone)]
pub struct Docker {
    pub policy: ContainerPolicy,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn exact(v: &Value, key: &str, expected: Value) -> Result<()> {
    if v.get(key) != Some(&expected) {
        return Err(invalid(&format!("container inspection differs at {key}")));
    }
    Ok(())
}
fn id_valid(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Docker {
    pub fn new(policy: ContainerPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self { policy })
    }
    pub fn name(&self, job: &WorkerJob) -> String {
        format!("watchhouse-{}", job.operation)
    }
    async fn rpc(&self, args: &[String], seconds: u64, limit: usize) -> Result<Vec<u8>> {
        if !cfg!(target_os = "linux") {
            return Err(invalid(
                "this container worker requires a native Linux host",
            ));
        }
        if file_digest(&self.policy.docker_path)? != self.policy.docker_digest {
            return Err(invalid("pinned Docker client changed"));
        }
        let home = tempfile::tempdir().map_err(|_| invalid("client scratch unavailable"))?;
        let mut c = Command::new(&self.policy.docker_path);
        c.arg("--host")
            .arg(format!("unix://{}", self.policy.socket.display()))
            .arg("--config")
            .arg(home.path())
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(path) = std::env::var_os("PATH") {
            c.env("PATH", path);
        }
        let mut child = c
            .spawn()
            .map_err(|_| invalid("container client unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| invalid("client stdout unavailable"))?;
        let work = async {
            let read = async {
                let mut bytes = vec![];
                stdout
                    .take(limit as u64 + 1)
                    .read_to_end(&mut bytes)
                    .await
                    .map_err(|_| invalid("container client read failed"))?;
                if bytes.len() > limit {
                    return Err(invalid("container client output exceeds bound"));
                }
                Ok(bytes)
            };
            let (bytes, status) = tokio::try_join!(read, async {
                child
                    .wait()
                    .await
                    .map_err(|_| invalid("container client wait failed"))
            })?;
            if !status.success() {
                return Err(invalid(
                    "container operation failed or outcome unknown; reconcile the reserved identity",
                ));
            }
            Ok(bytes)
        };
        let bytes = timeout(Duration::from_secs(seconds), work)
            .await
            .map_err(|_| invalid("container operation timed out; outcome unknown"))??;
        if file_digest(&self.policy.docker_path)? != self.policy.docker_digest {
            return Err(invalid("container client changed during operation"));
        }
        Ok(bytes)
    }
    pub async fn engine(&self) -> Result<EngineIdentity> {
        let bytes = self
            .rpc(
                &["info".into(), "--format".into(), "{{json .}}".into()],
                10,
                2 * 1024 * 1024,
            )
            .await?;
        let v: Value = serde_json::from_slice(&bytes)?;
        exact(&v, "OSType", json!("linux"))?;
        exact(&v, "CgroupVersion", json!("2"))?;
        if !v["SecurityOptions"].as_array().is_some_and(|a| {
            a.iter()
                .any(|s| s.as_str().is_some_and(|s| s.starts_with("name=seccomp,")))
        }) {
            return Err(invalid("worker requires enforced default seccomp"));
        }
        let field = |name: &str| -> Result<String> {
            let s = v[name]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() < 256)
                .ok_or_else(|| invalid("engine identity missing"))?;
            Ok(s.into())
        };
        let image = self
            .rpc(
                &["image".into(), "inspect".into(), self.policy.image.clone()],
                10,
                2 * 1024 * 1024,
            )
            .await?;
        let image: Vec<Value> = serde_json::from_slice(&image)?;
        if image.len() != 1 {
            return Err(invalid("pinned image is not uniquely installed"));
        }
        exact(&image[0], "Id", json!(self.policy.image))?;
        exact(&image[0], "Os", json!("linux"))?;
        let arch = field("Architecture")?;
        let native = match arch.as_str() {
            "x86_64" | "amd64" => "amd64",
            "aarch64" | "arm64" => "arm64",
            _ => return Err(invalid("unsupported native worker architecture")),
        };
        exact(&image[0], "Architecture", json!(native))?;
        Ok(EngineIdentity {
            id: field("ID")?,
            version: field("ServerVersion")?,
            architecture: arch,
            cgroup_version: "2".into(),
            kernel_version: field("KernelVersion")?,
            security_digest: store::digest(&serde_json::to_string(&v["SecurityOptions"])?),
            host_boot_id: std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
                .map_err(|_| invalid("host boot identity unavailable"))?
                .trim()
                .to_owned(),
        })
    }
    fn job_matches(&self, job: &WorkerJob) -> Result<()> {
        job.validate()?;
        let p = &self.policy;
        if job.profile_digest != p.digest()?
            || job.program != p.program
            || job.args != p.args
            || job.memory_bytes != p.memory_bytes
            || job.pids != p.pids
            || job.cpu_micros != p.cpu_micros
            || job.work_bytes != p.work_bytes
            || job.wall_seconds != p.wall_seconds
            || job.output_bytes != p.output_bytes
            || job.runner_digest != p.runner_digest
        {
            return Err(invalid("job does not match immutable worker policy"));
        }
        Ok(())
    }
    pub fn create_args(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<Vec<String>> {
        self.job_matches(job)?;
        for path in [input, policy] {
            if !path.is_absolute()
                || !path.starts_with(&self.policy.staging_root)
                || path.to_string_lossy().contains([',', '\n', '\r'])
                || !path.is_dir()
            {
                return Err(invalid(
                    "worker mounts must be private canonical staged directories",
                ));
            }
            if std::fs::canonicalize(path).map_err(|_| invalid("staging unavailable"))? != path {
                return Err(invalid("staging path must be canonical and not a symlink"));
            }
        }
        if input.starts_with(policy) || policy.starts_with(input) {
            return Err(invalid("worker policy/input roots must not overlap"));
        }
        Ok(vec![
            "container".into(),
            "create".into(),
            "--pull=never".into(),
            "--name".into(),
            self.name(job),
            "--label".into(),
            format!("watchhouse.operation={}", job.operation),
            "--label".into(),
            format!("watchhouse.profile={}", job.profile_digest),
            "--read-only".into(),
            "--network=none".into(),
            "--ipc=private".into(),
            "--cgroupns=private".into(),
            "--cap-drop=ALL".into(),
            "--security-opt=no-new-privileges:true".into(),
            "--cap-add=SETUID".into(),
            "--cap-add=SETGID".into(),
            "--user=0:0".into(),
            "--workdir=/work".into(),
            "--pids-limit".into(),
            job.pids.to_string(),
            "--memory".into(),
            job.memory_bytes.to_string(),
            "--memory-swap".into(),
            job.memory_bytes.to_string(),
            "--cpu-period=100000".into(),
            "--cpu-quota".into(),
            job.cpu_micros.to_string(),
            "--ulimit".into(),
            "nofile=128:128".into(),
            "--ulimit".into(),
            "core=0:0".into(),
            "--tmpfs".into(),
            format!("/work:rw,nosuid,nodev,size={},mode=1777", job.work_bytes),
            "--tmpfs".into(),
            "/tmp:rw,nosuid,nodev,noexec,size=4194304,mode=1777".into(),
            "--env=HOME=/tmp".into(),
            "--env=TMPDIR=/tmp".into(),
            "--env=PATH=/usr/local/bin:/usr/bin:/bin".into(),
            "--mount".into(),
            format!("type=bind,src={},dst=/input,readonly", input.display()),
            "--mount".into(),
            format!("type=bind,src={},dst=/policy,readonly", policy.display()),
            "--entrypoint=/watchhouse-worker-init".into(),
            self.policy.image.clone(),
            if probe { "probe" } else { "run" }.into(),
        ])
    }
    /// Caller must persist reservation/create-dispatched before this external effect.
    pub async fn create(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<String> {
        let bytes = self
            .rpc(&self.create_args(job, input, policy, probe)?, 15, 1024)
            .await?;
        let id = String::from_utf8(bytes)
            .map_err(|_| invalid("invalid container identity"))?
            .trim()
            .to_ascii_lowercase();
        if !id_valid(&id) {
            return Err(invalid(
                "invalid container identity; reconcile by reserved name",
            ));
        }
        Ok(id)
    }
    /// Inspect only by the reserved name, then validate its full boundary before start/removal.
    pub async fn inspect(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<Observation> {
        self.job_matches(job)?;
        let bytes = self
            .rpc(
                &["container".into(), "inspect".into(), self.name(job)],
                10,
                2 * 1024 * 1024,
            )
            .await?;
        let rows: Vec<Value> = serde_json::from_slice(&bytes)?;
        if rows.len() != 1 {
            return Err(invalid("container identity ambiguous"));
        }
        validate_inspect(&rows[0], job, input, policy, probe, &self.policy.image)
    }
    /// Caller persists start-dispatched first. This method never repeats creation/start after uncertainty.
    pub async fn start(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<WorkerOutput> {
        if probe {
            return self.qualify_boundary(job, input, policy).await;
        }
        let observed = self.inspect(job, input, policy, probe).await?;
        if observed.status != "created" || observed.running {
            return Err(invalid(
                "container start requires positively observed created state",
            ));
        }
        let bytes = self
            .rpc(
                &[
                    "container".into(),
                    "start".into(),
                    "--attach".into(),
                    observed.id.clone(),
                ],
                u64::from(job.wall_seconds) + 15,
                (job.output_bytes as usize) * 16 + 65536,
            )
            .await?;
        let result: WorkerOutput = serde_json::from_slice(&bytes)?;
        validate_output(job, &result)?;
        let terminal = self.inspect(job, input, policy, probe).await?;
        if terminal.running
            || terminal.status != "exited"
            || terminal.oom_killed
            || terminal.exit_code != 0
        {
            return Err(invalid(
                "worker outcome was not positively observed terminal; reconcile resources",
            ));
        }
        self.cancel(job, input, policy, false).await?;
        Ok(result)
    }
    async fn qualify_boundary(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
    ) -> Result<WorkerOutput> {
        let work = async {
            let created = self.inspect(job, input, policy, true).await?;
            if created.status != "created" || created.running {
                return Err(invalid("qualification requires created owned container"));
            }
            let bytes = self
                .rpc(
                    &["container".into(), "start".into(), created.id.clone()],
                    10,
                    1024,
                )
                .await?;
            if String::from_utf8_lossy(&bytes).trim() != created.id {
                return Err(invalid("qualification start acknowledgment missing"));
            }
            loop {
                let running = self.inspect(job, input, policy, true).await?;
                if !running.running {
                    return Err(invalid("qualification exited before cancellation proof"));
                }
                let bytes = self
                    .rpc(
                        &["container".into(), "logs".into(), created.id.clone()],
                        5,
                        (job.output_bytes as usize) * 16 + 65536,
                    )
                    .await?;
                if !bytes.is_empty() {
                    let mut output: WorkerOutput = serde_json::from_slice(&bytes)?;
                    validate_output(job, &output)?;
                    if output.checks.get("descendant_started") != Some(&true) {
                        return Err(invalid(
                            "qualification descendant never demonstrated progress",
                        ));
                    }
                    self.cancel(job, input, policy, true).await?;
                    output.checks.insert("removal_acknowledged".into(), true);
                    return Ok(output);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        timeout(Duration::from_secs(u64::from(job.wall_seconds) + 15), work)
            .await
            .map_err(|_| invalid("qualification timed out; exact container cleanup required"))?
    }
    /// Fenced removal of our exact positively inspected identity handles start/cancel races.
    pub async fn cancel(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<()> {
        let observed = self.inspect(job, input, policy, probe).await?;
        let bytes = self
            .rpc(
                &[
                    "container".into(),
                    "rm".into(),
                    "--force".into(),
                    observed.id.clone(),
                ],
                15,
                1024,
            )
            .await?;
        if String::from_utf8_lossy(&bytes).trim() != observed.id {
            return Err(invalid(
                "removal acknowledgment missing; cancellation remains unknown",
            ));
        }
        Ok(())
    }
    pub async fn logs(
        &self,
        job: &WorkerJob,
        input: &Path,
        policy: &Path,
        probe: bool,
    ) -> Result<WorkerOutput> {
        let o = self.inspect(job, input, policy, probe).await?;
        if o.running || o.status != "exited" || o.exit_code != 0 || o.oom_killed {
            return Err(invalid("no accepted terminal worker output"));
        }
        let bytes = self
            .rpc(
                &["container".into(), "logs".into(), o.id],
                10,
                (job.output_bytes as usize) * 16 + 65536,
            )
            .await?;
        let result: WorkerOutput = serde_json::from_slice(&bytes)?;
        validate_output(job, &result)?;
        // Recover terminal output only with positively acknowledged scoped removal.
        self.cancel(job, input, policy, probe).await?;
        Ok(result)
    }
}
pub fn validate_output(job: &WorkerJob, result: &WorkerOutput) -> Result<()> {
    if result.operation != job.operation
        || result.nonce != job.nonce
        || result.profile_digest != job.profile_digest
        || result.input_digest != job.input_digest()?
        || result.stdout.len() > job.output_bytes as usize * 3
        || result.stderr.len() > job.output_bytes as usize * 3
        || result.checks.len() > 32
    {
        return Err(invalid("worker result/fence/manifest/output mismatch"));
    }
    Ok(())
}
pub(super) fn validate_inspect(
    v: &Value,
    job: &WorkerJob,
    input: &Path,
    policy: &Path,
    probe: bool,
    image: &str,
) -> Result<Observation> {
    let id = v["Id"]
        .as_str()
        .filter(|s| id_valid(s))
        .ok_or_else(|| invalid("invalid inspected container identity"))?;
    exact(v, "Name", json!(format!("/watchhouse-{}", job.operation)))?;
    exact(v, "Image", json!(image))?;
    let c = &v["Config"];
    exact(c, "Image", json!(image))?;
    exact(c, "User", json!("0:0"))?;
    exact(c, "WorkingDir", json!("/work"))?;
    exact(c, "Entrypoint", json!(["/watchhouse-worker-init"]))?;
    exact(c, "Cmd", json!([if probe { "probe" } else { "run" }]))?;
    exact(&c["Labels"], "watchhouse.operation", json!(job.operation))?;
    exact(
        &c["Labels"],
        "watchhouse.profile",
        json!(job.profile_digest),
    )?;
    let env = c["Env"]
        .as_array()
        .ok_or_else(|| invalid("container environment missing"))?;
    let expected = [
        "HOME=/tmp",
        "TMPDIR=/tmp",
        "PATH=/usr/local/bin:/usr/bin:/bin",
    ];
    if env.len() != expected.len() || !expected.iter().all(|s| env.contains(&json!(s))) {
        return Err(invalid("unexpected container environment"));
    }
    let h = &v["HostConfig"];
    for (key, val) in [
        ("Privileged", json!(false)),
        ("ReadonlyRootfs", json!(true)),
        ("NetworkMode", json!("none")),
        ("IpcMode", json!("private")),
        ("CgroupnsMode", json!("private")),
        ("PidMode", json!("")),
        ("Memory", json!(job.memory_bytes)),
        ("MemorySwap", json!(job.memory_bytes)),
        ("PidsLimit", json!(job.pids)),
        ("CpuPeriod", json!(100000)),
        ("CpuQuota", json!(job.cpu_micros)),
    ] {
        exact(h, key, val)?;
    }
    if !h["CapAdd"].as_array().is_some_and(|v| {
        v.len() == 2 && v.contains(&json!("SETUID")) && v.contains(&json!("SETGID"))
    }) || !h["CapDrop"]
        .as_array()
        .is_some_and(|v| v == &vec![json!("ALL")])
        || !h["SecurityOpt"]
            .as_array()
            .is_some_and(|v| v == &vec![json!("no-new-privileges:true")])
        || h["Devices"].as_array().is_some_and(|v| !v.is_empty())
    {
        return Err(invalid(&format!(
            "container privilege profile differs: add={} drop={} security={}",
            h["CapAdd"], h["CapDrop"], h["SecurityOpt"]
        )));
    }
    let tmpfs = h["Tmpfs"]
        .as_object()
        .ok_or_else(|| invalid("bounded tmpfs missing"))?;
    if tmpfs.len() != 2
        || tmpfs.get("/work")
            != Some(&json!(format!(
                "rw,nosuid,nodev,size={},mode=1777",
                job.work_bytes
            )))
        || tmpfs.get("/tmp") != Some(&json!("rw,nosuid,nodev,noexec,size=4194304,mode=1777"))
    {
        return Err(invalid("container writable storage differs"));
    }
    let mounts = v["Mounts"]
        .as_array()
        .ok_or_else(|| invalid("container mounts missing"))?;
    let mut binds = BTreeMap::new();
    for m in mounts {
        let dest = m["Destination"].as_str().unwrap_or("");
        if m["Type"] == "bind" {
            if m["RW"] != false || !["/input", "/policy"].contains(&dest) {
                return Err(invalid("unexpected writable/host mount"));
            }
            binds.insert(dest, m["Source"].as_str().unwrap_or(""));
        } else if m["Type"] != "tmpfs" || !["/work", "/tmp"].contains(&dest) {
            return Err(invalid("unexpected container mount"));
        }
    }
    if binds.len() != 2
        || binds.get("/input") != Some(&input.to_str().unwrap_or(""))
        || binds.get("/policy") != Some(&policy.to_str().unwrap_or(""))
    {
        return Err(invalid("container mount identity differs"));
    }
    let state = &v["State"];
    Ok(Observation {
        id: id.into(),
        status: state["Status"]
            .as_str()
            .ok_or_else(|| invalid("container status missing"))?
            .into(),
        running: state["Running"]
            .as_bool()
            .ok_or_else(|| invalid("container running state missing"))?,
        exit_code: state["ExitCode"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| invalid("container exit state missing"))?,
        oom_killed: state["OOMKilled"]
            .as_bool()
            .ok_or_else(|| invalid("container OOM state missing"))?,
    })
}
