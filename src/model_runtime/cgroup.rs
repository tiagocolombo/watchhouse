//! Scoped cgroup-v2 resource identity for trusted harness clients. Candidate execution is separate.
use crate::{
    error::{Error, Result},
    real_execution::protocol::sha_valid,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memory_bytes: u64,
    pub pids: u32,
    pub cpu_micros: u32,
    pub wall_seconds: u32,
    pub output_bytes: u32,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if !(128 * 1024 * 1024..=8 * 1024 * 1024 * 1024).contains(&self.memory_bytes)
            || !(8..=256).contains(&self.pids)
            || !(10000..=400000).contains(&self.cpu_micros)
            || !(1..=600).contains(&self.wall_seconds)
            || !(4096..=262144).contains(&self.output_bytes)
        {
            return Err(invalid("model-client limits exceed trusted bounds"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    pub operation: String,
    pub directory: PathBuf,
    pub inode: u64,
    pub boot_id: String,
    pub limits: Limits,
    pub profile_digest: String,
}
fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
fn read(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|_| invalid("cgroup control metadata unavailable"))?;
    if bytes.len() > 65536 {
        return Err(invalid("cgroup control metadata exceeds bound"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("invalid cgroup metadata encoding"))
}
fn operation(value: &str) -> Result<()> {
    if uuid::Uuid::parse_str(value).is_err() {
        return Err(invalid("cgroup operation must be a reserved UUID"));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn inode(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::metadata(path)
        .map_err(|_| invalid("cgroup identity unavailable"))?
        .ino())
}
#[cfg(not(target_os = "linux"))]
fn inode(_: &Path) -> Result<u64> {
    Err(invalid("native model cgroups require Linux"))
}
pub fn boot() -> Result<String> {
    let value = read(Path::new("/proc/sys/kernel/random/boot_id"))?;
    let value = value.trim();
    operation(value)?;
    Ok(value.into())
}
pub fn validate_root(root: &Path) -> Result<()> {
    if !cfg!(target_os = "linux")
        || !root.is_absolute()
        || root == Path::new("/sys/fs/cgroup")
        || !root.starts_with("/sys/fs/cgroup")
        || fs::canonicalize(root)
            .map_err(|_| invalid("operator-delegated cgroup root unavailable"))?
            != root
    {
        return Err(invalid(
            "an explicit canonical delegated cgroup-v2 subtree is required",
        ));
    }
    let mounts = read(Path::new("/proc/self/mountinfo"))?;
    if !mounts.lines().any(|line| {
        line.split_whitespace().nth(4) == Some("/sys/fs/cgroup") && line.contains(" - cgroup2 ")
    }) {
        return Err(invalid("cgroup-v2 kernel mount not observed"));
    }
    let membership = read(Path::new("/proc/self/cgroup"))?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .ok_or_else(|| invalid("worker cgroup-v2 membership unavailable"))?;
    let current = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
    if current == root || !current.starts_with(root) {
        return Err(invalid(
            "worker must already run in a child of its delegated cgroup subtree",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let own = fs::metadata("/proc/self")
            .map_err(|_| invalid("worker UID unavailable"))?
            .uid();
        let meta =
            fs::metadata(root).map_err(|_| invalid("delegated cgroup ownership unavailable"))?;
        if own == 0 || meta.uid() != own || meta.mode() & 0o022 != 0 {
            return Err(invalid(
                "model-client cgroups require a private subtree delegated to the unprivileged worker",
            ));
        }
    }
    if read(&root.join("cgroup.type"))?.trim() != "domain" {
        return Err(invalid("delegated cgroup root must be a domain"));
    }
    let controllers = read(&root.join("cgroup.subtree_control"))?;
    if !["cpu", "memory", "pids"]
        .iter()
        .all(|s| controllers.split_whitespace().any(|c| c == *s))
    {
        return Err(invalid(
            "operator must delegate cpu, memory and pids controllers",
        ));
    }
    Ok(())
}
pub fn create(
    root: &Path,
    operation_id: &str,
    profile_digest: &str,
    limits: Limits,
) -> Result<Handle> {
    validate_root(root)?;
    operation(operation_id)?;
    limits.validate()?;
    if !sha_valid(profile_digest) {
        return Err(invalid("model profile digest missing"));
    }
    let directory = root.join(format!("watchhouse-{operation_id}"));
    fs::create_dir(&directory).map_err(|_|invalid("cgroup create uncertain or already exists; inspect the reserved identity without recreating"))?;
    // A crash between creation and configuration leaves an unstarted reserved resource, never permission to dispatch.
    for (file, value) in [
        ("memory.max", limits.memory_bytes.to_string()),
        ("memory.swap.max", "0".into()),
        ("memory.oom.group", "1".into()),
        ("pids.max", limits.pids.to_string()),
        ("cpu.max", format!("{} 100000", limits.cpu_micros)),
    ] {
        fs::write(directory.join(file), value).map_err(|_| {
            invalid("cgroup configuration failed; inspect reserved unstarted resource")
        })?;
    }
    let handle = Handle {
        operation: operation_id.into(),
        inode: inode(&directory)?,
        directory,
        boot_id: boot()?,
        limits,
        profile_digest: profile_digest.into(),
    };
    verify(root, &handle)?;
    Ok(handle)
}
pub fn verify(root: &Path, handle: &Handle) -> Result<()> {
    validate_root(root)?;
    operation(&handle.operation)?;
    handle.limits.validate()?;
    if !sha_valid(&handle.profile_digest)
        || handle.directory != root.join(format!("watchhouse-{}", handle.operation))
        || fs::canonicalize(&handle.directory)
            .map_err(|_| invalid("owned cgroup is missing; absence is not cancellation"))?
            != handle.directory
        || inode(&handle.directory)? != handle.inode
        || boot()? != handle.boot_id
    {
        return Err(invalid(
            "cgroup identity/host generation changed; quarantine requires reconciliation",
        ));
    }
    for (file, value) in [
        ("memory.max", handle.limits.memory_bytes.to_string()),
        ("memory.swap.max", "0".into()),
        ("memory.oom.group", "1".into()),
        ("pids.max", handle.limits.pids.to_string()),
        ("cpu.max", format!("{} 100000", handle.limits.cpu_micros)),
    ] {
        if read(&handle.directory.join(file))?.trim() != value {
            return Err(invalid("cgroup resource profile changed"));
        }
    }
    if !handle.directory.join("cgroup.kill").is_file()
        || fs::OpenOptions::new()
            .write(true)
            .open(handle.directory.join("cgroup.kill"))
            .is_err()
    {
        return Err(invalid("kernel cgroup-wide cancellation unavailable"));
    }
    Ok(())
}
pub fn populated(root: &Path, handle: &Handle) -> Result<bool> {
    verify(root, handle)?;
    let events = read(&handle.directory.join("cgroup.events"))?;
    match events.lines().find_map(|s| s.strip_prefix("populated ")) {
        Some("0") => Ok(false),
        Some("1") => Ok(true),
        _ => Err(invalid("cgroup populated status unavailable")),
    }
}
/// Called by the fixed initializer before spawning any client or initializing asynchronous workers.
pub fn attach_self(root: &Path, handle: &Handle) -> Result<()> {
    verify(root, handle)?;
    fs::write(
        handle.directory.join("cgroup.procs"),
        std::process::id().to_string(),
    )
    .map_err(|_| invalid("could not attach trusted initializer before harness launch"))?;
    assert_self(root, handle)?;
    Ok(())
}
/// An existing matching kernel identity plus populated=0 acknowledges the entire subtree,
/// including detached descendants. Never kill arbitrary PIDs or infer success from missing paths.
pub async fn cancel(root: &Path, handle: &Handle) -> Result<()> {
    verify(root, handle)?;
    fs::write(handle.directory.join("cgroup.kill"), "1")
        .map_err(|_| invalid("cgroup kill acknowledgment unavailable"))?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if !populated(root, handle)? {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(invalid("cgroup descendants remain; retain quarantine"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

pub fn assert_self(root: &Path, handle: &Handle) -> Result<()> {
    verify(root, handle)?;
    let membership = read(Path::new("/proc/self/cgroup"))?;
    let relative = handle
        .directory
        .strip_prefix("/sys/fs/cgroup")
        .map_err(|_| invalid("cgroup membership escaped root"))?;
    if !membership.lines().any(|l| {
        l.strip_prefix("0::")
            .is_some_and(|s| Path::new(s) == Path::new("/").join(relative))
    }) {
        return Err(invalid(
            "kernel did not confirm initializer cgroup membership",
        ));
    }
    Ok(())
}

pub fn recover_unstarted(
    root: &Path,
    operation_id: &str,
    profile_digest: &str,
    limits: Limits,
) -> Result<Handle> {
    validate_root(root)?;
    operation(operation_id)?;
    let directory = root.join(format!("watchhouse-{operation_id}"));
    let handle = Handle {
        operation: operation_id.into(),
        directory: directory.clone(),
        inode: inode(&directory)?,
        boot_id: boot()?,
        limits,
        profile_digest: profile_digest.into(),
    };
    verify(root, &handle)?;
    if populated(root, &handle)? {
        return Err(invalid(
            "unstarted reserved cgroup unexpectedly contains processes; inspect without replaying start",
        ));
    }
    Ok(handle)
}
