//! Trusted container entry point. This binary has no coordinator/database/credential interface.
use std::{collections::BTreeMap, fs, io::Write, path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};
use watchhouse::{contracts::V1, real_execution::protocol::*};
fn fixture() -> bool {
    match std::env::args().nth(1).as_deref() {
        Some("fixture-regression") => {
            std::process::exit(
                if fs::read_to_string("answer.txt").ok().as_deref() == Some("4\n") {
                    0
                } else {
                    1
                },
            );
        }
        Some("fixture-identity") => {
            let uid = proc_field("Uid:").unwrap_or_default();
            let caps = proc_field("CapEff:").unwrap_or_default();
            let denied = fs::OpenOptions::new()
                .write(true)
                .open("/proc/1/fd/1")
                .is_err();
            println!(
                "{}",
                serde_json::json!({"non_root":uid.split_whitespace().next()==Some("10001"),"no_capabilities":caps.bytes().all(|b|b==b'0'),"protocol_protected":denied})
            );
            true
        }
        Some("fixture-pulse") => {
            let mut n = 0u64;
            loop {
                let _ = fs::write("/work/descendant-pulse", n.to_string());
                n += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        Some("fixture-stall") => {
            std::thread::sleep(Duration::from_secs(3600));
            true
        }
        _ => false,
    }
}
fn copy_inputs(job: &WorkerJob) -> Result<(), Box<dyn std::error::Error>> {
    let input = Path::new("/input");
    let work = Path::new("/work");
    if fs::read_dir(work)?.next().is_some() {
        return Err("worker scratch must start empty".into());
    }
    for f in &job.files {
        let path = input.join(&f.path);
        let mut parent = path.as_path();
        while parent != input {
            if fs::symlink_metadata(parent)?.file_type().is_symlink() {
                return Err("symlink input rejected".into());
            }
            parent = parent.parent().ok_or("input escaped")?;
        }
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.len() != f.bytes {
            return Err("input manifest changed".into());
        }
        let bytes = fs::read(&path)?;
        if bytes_digest(&bytes) != f.digest {
            return Err("input digest changed".into());
        }
        let target = work.join(&f.path);
        fs::create_dir_all(target.parent().ok_or("missing parent")?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut dir = target.parent().unwrap();
            while dir != work {
                fs::set_permissions(dir, fs::Permissions::from_mode(0o777))?;
                dir = dir.parent().unwrap();
            }
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        file.write_all(&bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(if f.executable {
                0o777
            } else {
                0o666
            }))?;
        }
    }
    Ok(())
}
fn proc_field(name: &str) -> Option<String> {
    fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix(name).map(str::trim).map(str::to_owned))
}
fn probe(job: &WorkerJob) -> BTreeMap<String, bool> {
    let expected = job
        .files
        .first()
        .map(|f| f.path.clone())
        .unwrap_or_default();
    let memory = fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    let pids = fs::read_to_string("/sys/fs/cgroup/pids.max")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());
    let cpu = fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|s| {
            let p: Vec<_> = s.split_whitespace().collect();
            if p.len() != 2 {
                return None;
            }
            Some((p[0].parse::<u64>().ok()?, p[1].parse::<u64>().ok()?))
        });
    let net = fs::read_link("/proc/self/ns/net")
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let pid = fs::read_link("/proc/self/ns/pid")
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], job.canary_port));
    let work_ok = fs::write("/work/probe-write", "synthetic").is_ok();
    let _ = fs::remove_file("/work/probe-write");
    let mut checks = BTreeMap::from([
        ("input_verified".into(), true),
        (
            "runner_identity".into(),
            fs::read("/watchhouse-worker-init")
                .is_ok_and(|b| bytes_digest(&b) == job.runner_digest),
        ),
        (
            "trusted_init_uid".into(),
            proc_field("Uid:")
                .and_then(|s| s.split_whitespace().next().map(str::to_owned))
                .as_deref()
                == Some("0"),
        ),
        (
            "minimal_init_capabilities".into(),
            proc_field("CapEff:").and_then(|s| u64::from_str_radix(&s, 16).ok()) == Some(0xc0),
        ),
        (
            "no_new_privileges".into(),
            proc_field("NoNewPrivs:").as_deref() == Some("1"),
        ),
        (
            "seccomp_enforced".into(),
            proc_field("Seccomp:").as_deref() == Some("2"),
        ),
        (
            "private_network".into(),
            net.is_some_and(|v| v != job.host_net_namespace)
                && std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200))
                    .is_err(),
        ),
        (
            "private_pid".into(),
            pid.is_some_and(|v| v != job.host_pid_namespace),
        ),
        ("memory_limit".into(), memory == Some(job.memory_bytes)),
        ("pids_limit".into(), pids == Some(job.pids)),
        (
            "cpu_limit".into(),
            cpu.is_some_and(|(q, p)| {
                p > 0 && u128::from(q) * 100000 == u128::from(job.cpu_micros) * u128::from(p)
            }),
        ),
        (
            "input_read_only".into(),
            fs::OpenOptions::new()
                .append(true)
                .open(Path::new("/input").join(expected))
                .is_err(),
        ),
        (
            "policy_read_only".into(),
            fs::OpenOptions::new()
                .append(true)
                .open("/policy/job.json")
                .is_err(),
        ),
        (
            "root_read_only".into(),
            fs::write("/escape", "denied").is_err(),
        ),
        ("scratch_writable".into(), work_ok),
        (
            "no_credentials".into(),
            std::env::vars()
                .all(|(k, _)| ["HOME", "TMPDIR", "PATH", "HOSTNAME"].contains(&k.as_str())),
        ),
    ]);
    // The only writable work filesystem is tmpfs; prove ENOSPC rather than trusting an advertised flag.
    let disk = (|| -> std::io::Result<bool> {
        let mut f = fs::File::create("/work/quota-canary")?;
        let chunk = [0u8; 65536];
        let mut written = 0u64;
        while written <= job.work_bytes {
            match f.write_all(&chunk) {
                Ok(()) => written += chunk.len() as u64,
                Err(e) => return Ok(e.raw_os_error() == Some(28) && written <= job.work_bytes),
            }
        }
        Ok(false)
    })();
    let _ = fs::remove_file("/work/quota-canary");
    checks.insert("scratch_quota".into(), disk.unwrap_or(false));
    checks
}
async fn bounded(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    limit: u32,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut bytes = vec![];
    (&mut stream)
        .take(u64::from(limit) + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit as usize {
        return Err("worker output limit exceeded".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if fixture() {
        return Ok(());
    }
    let mode = std::env::args().nth(1).ok_or("run/probe mode required")?;
    if !["run", "probe"].contains(&mode.as_str()) {
        return Err("unknown worker mode".into());
    }
    let bytes = fs::read("/policy/job.json")?;
    if bytes.len() > 1024 * 1024 {
        return Err("worker manifest too large".into());
    }
    let job: WorkerJob = serde_json::from_slice(&bytes)?;
    job.validate()?;
    // Enforce the entire worker lifetime independently of the coordinator, including input
    // preparation/probe loops. PID 1 exit causes the kernel to terminate namespace descendants.
    let hard_wall = job.wall_seconds;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(hard_wall.into()));
        std::process::exit(124);
    });
    copy_inputs(&job)?;
    let mut out = WorkerOutput {
        schema_version: V1,
        operation: job.operation.clone(),
        nonce: job.nonce.clone(),
        profile_digest: job.profile_digest.clone(),
        exit_code: None,
        failure_code: None,
        stdout: String::new(),
        stderr: String::new(),
        input_digest: job.input_digest()?,
        checks: BTreeMap::new(),
    };
    let mut pulse = None;
    if mode == "probe" {
        out.checks = probe(&job);
        let mut identity = Command::new("/watchhouse-worker-init");
        #[cfg(unix)]
        identity.uid(10001).gid(10001);
        identity
            .arg("fixture-identity")
            .env_clear()
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let identity_result = async {
            let mut child = identity.spawn()?;
            let bytes =
                bounded(child.stdout.take().ok_or("identity stdout missing")?, 4096).await?;
            let status = child.wait().await?;
            if !status.success() {
                return Err("identity child failed".into());
            }
            Ok::<BTreeMap<String, bool>, Box<dyn std::error::Error>>(serde_json::from_str(&bytes)?)
        };
        if let Ok(Ok(checks)) = timeout(Duration::from_secs(2), identity_result).await {
            for key in ["non_root", "no_capabilities", "protocol_protected"] {
                out.checks
                    .insert(key.into(), checks.get(key) == Some(&true));
            }
        }
        let mut command = Command::new("/watchhouse-worker-init");
        #[cfg(unix)]
        command.uid(10001).gid(10001);
        command
            .arg("fixture-pulse")
            .env_clear()
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .kill_on_drop(true);
        if let Ok(child) = command.spawn() {
            pulse = Some(child);
            let mut first = None;
            let progress = async {
                loop {
                    if let Ok(value) = fs::read_to_string("/work/descendant-pulse") {
                        if first.as_ref().is_some_and(|old| old != &value) {
                            break;
                        }
                        first = Some(value);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            out.checks.insert(
                "descendant_started".into(),
                timeout(Duration::from_secs(2), progress).await.is_ok(),
            );
        }
        out.exit_code = Some(
            if out.checks.values().all(|v| *v)
                && out.checks.get("descendant_started") == Some(&true)
            {
                0
            } else {
                1
            },
        );
    } else {
        let program = fs::canonicalize(&job.program)?;
        if ["/work", "/input", "/policy", "/tmp"]
            .iter()
            .any(|p| program.starts_with(p))
        {
            return Err("test program resolved into mutable/input scope".into());
        }
        let mut command = Command::new(&job.program);
        #[cfg(unix)]
        {
            command.uid(10001).gid(10001);
        }
        command
            .args(&job.args)
            .current_dir("/work")
            .env_clear()
            .env("HOME", "/tmp")
            .env("TMPDIR", "/tmp")
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match command.spawn() {
            Err(_) => out.failure_code = Some("startup".into()),
            Ok(mut child) => {
                let stdout = child.stdout.take().ok_or("stdout missing")?;
                let stderr = child.stderr.take().ok_or("stderr missing")?;
                let work = async {
                    let (o, e, status) = tokio::try_join!(
                        bounded(stdout, job.output_bytes),
                        bounded(stderr, job.output_bytes),
                        async { Ok::<_, Box<dyn std::error::Error>>(child.wait().await?) }
                    )?;
                    Ok::<_, Box<dyn std::error::Error>>((o, e, status.code()))
                };
                match timeout(Duration::from_secs(job.wall_seconds.into()), work).await {
                    Ok(Ok((o, e, code))) => {
                        out.stdout = o;
                        out.stderr = e;
                        out.exit_code = code;
                    }
                    Ok(Err(_)) => {
                        let _ = child.start_kill();
                        out.failure_code = Some("invalid_or_excess_output".into());
                    }
                    Err(_) => {
                        let _ = child.start_kill();
                        out.failure_code = Some("timeout".into());
                    }
                }
            }
        }
    }
    println!("{}", serde_json::to_string(&out)?);
    std::io::stdout().flush()?;
    if mode == "probe" {
        // Keep a progressing descendant alive. Only positively acknowledged namespace removal
        // by the trusted host controller completes this qualification.
        let _pulse = pulse;
        std::future::pending::<()>().await;
    }
    Ok(())
}
