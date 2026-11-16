//! Fixed Linux Bubblewrap qualification adapter. Never accepts an arbitrary executable/task.
use super::*;
use crate::repair::snapshot::{file_digest, output};
use std::{fs, net::TcpListener, path::Path, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    time::{Duration, Instant, sleep, timeout_at},
};
pub(super) const REQUIRED: &[&str] = &[
    "tool_identity",
    "input_read",
    "base_read_only",
    "policy_read_only",
    "outside_hidden",
    "environment_clean",
    "work_read_write",
    "network_denied",
    "private_net_namespace",
    "private_pid_namespace",
    "resource_limits",
    "host_files_unchanged",
    "descendant_termination",
];
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_version: V1,
    nonce: String,
    input_read: bool,
    base_read_only: bool,
    policy_read_only: bool,
    outside_hidden: bool,
    environment_clean: bool,
    work_read_write: bool,
    network_denied: bool,
    net_namespace: String,
    pid_namespace: String,
    cpu_seconds: u32,
    memory_bytes: u64,
    file_bytes: u64,
    descriptors: u32,
    core_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Started {
    schema_version: V1,
    nonce: String,
    started: bool,
}
fn namespace_valid(value: &str, kind: &str) -> bool {
    value.len() <= 40
        && value
            .strip_prefix(&format!("{kind}:["))
            .and_then(|s| s.strip_suffix(']'))
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn fail(reason: &str, quarantined: bool) -> Evidence {
    Evidence {
        checks: BTreeMap::new(),
        details: None,
        output_digest: None,
        reason: reason.into(),
        quarantined,
        origin: "native".into(),
    }
}
fn static_elf(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if meta.len() > 32 * 1024 * 1024 {
        return false;
    }
    let Ok(b) = fs::read(path) else { return false };
    if b.len() < 64 || &b[..4] != b"\x7fELF" || b[4] != 2 || b[5] != 1 {
        return false;
    }
    let machine = u16::from_le_bytes([b[18], b[19]]);
    if !matches!(
        (std::env::consts::ARCH, machine),
        ("x86_64", 62) | ("aarch64", 183)
    ) {
        return false;
    }
    let offset = u64::from_le_bytes(b[32..40].try_into().unwrap());
    let size = u16::from_le_bytes([b[54], b[55]]) as u64;
    let count = u16::from_le_bytes([b[56], b[57]]) as u64;
    if !(56..=256).contains(&size) || !(1..=128).contains(&count) {
        return false;
    }
    for n in 0..count {
        let Some(i) = offset
            .checked_add(n * size)
            .and_then(|i| usize::try_from(i).ok())
        else {
            return false;
        };
        let Some(header) = b.get(i..i.saturating_add(4)) else {
            return false;
        };
        if u32::from_le_bytes(header.try_into().unwrap()) == 3 {
            return false;
        }
    }
    true
}
fn command(p: &WorkerPolicy, root: &Path, mode: &str, nonce: &str, port: u16) -> Command {
    let mut c = Command::new(&p.limiter.path);
    c.args([
        format!("--as={0}:{0}", p.memory_bytes),
        format!("--cpu={0}:{0}", p.cpu_seconds),
        format!("--fsize={0}:{0}", p.file_bytes),
        format!("--nofile={0}:{0}", p.descriptors),
        "--core=0:0".into(),
        "--".into(),
    ])
    .arg(&p.boundary.path);
    c.args([
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--assert-userns-disabled",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--dir",
        "/home",
        "--dir",
        "/home/worker",
        "--ro-bind",
    ])
    .arg(root.join("input"))
    .arg("/input")
    .arg("--bind")
    .arg(root.join("work"))
    .arg("/work")
    .arg("--ro-bind")
    .arg(&p.probe.path)
    .arg("/probe")
    .arg("--ro-bind")
    .arg(root.join("policy"))
    .arg("/policy");
    c.args([
        "--size",
        "1048576",
        "--tmpfs",
        "/tmp",
        "--remount-ro",
        "/",
        "--chdir",
        "/work",
        "--setenv",
        "HOME",
        "/home/worker",
        "--setenv",
        "TMPDIR",
        "/tmp",
        "--",
        "/probe",
        mode,
        nonce,
    ]);
    if mode == "boundaries" {
        c.arg(root.join("host-only")).arg(port.to_string());
    }
    c
}
async fn cancellation(
    p: &WorkerPolicy,
    root: &Path,
    nonce: &str,
    port: u16,
    deadline: Instant,
) -> Result<bool> {
    let mut c = command(p, root, "descendants", nonce, port);
    c.env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = c
        .spawn()
        .map_err(|_| Error::Invalid("boundary launcher unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("probe stream unavailable".into()))?;
    let mut reader = BufReader::new(stdout).take(2049);
    let mut line = String::new();
    let ready = timeout_at(deadline, reader.read_line(&mut line))
        .await
        .map_err(|_| Error::Invalid("descendant start timed out".into()))?
        .map_err(|_| Error::Invalid("probe stream interrupted".into()))?;
    if ready == 0 || ready > 2048 {
        return Ok(false);
    }
    let started: Started = serde_json::from_str(&line)?;
    let _schema = started.schema_version;
    if started.nonce != nonce || !started.started || !root.join("work/pulse").exists() {
        return Ok(false);
    }
    let before = fs::read(root.join("work/pulse"))
        .map_err(|_| Error::Invalid("descendant pulse unavailable".into()))?;
    sleep(Duration::from_millis(60)).await;
    let after = fs::read(root.join("work/pulse"))
        .map_err(|_| Error::Invalid("descendant pulse unavailable".into()))?;
    if before == after {
        return Ok(false);
    }
    child
        .start_kill()
        .map_err(|_| Error::Invalid("probe cancellation failed".into()))?;
    timeout_at(deadline, child.wait())
        .await
        .map_err(|_| Error::Invalid("probe cancellation not acknowledged".into()))?
        .map_err(|_| Error::Invalid("probe wait failed".into()))?;
    sleep(Duration::from_millis(100)).await;
    let first = fs::read(root.join("work/pulse"))
        .map_err(|_| Error::Invalid("probe pulse unavailable".into()))?;
    sleep(Duration::from_millis(300)).await;
    let second = fs::read(root.join("work/pulse"))
        .map_err(|_| Error::Invalid("probe pulse unavailable".into()))?;
    Ok(first == second)
}
pub(super) async fn run(p: &WorkerPolicy, nonce: &str) -> Evidence {
    if !cfg!(target_os = "linux") {
        return fail(
            "Unsupported platform: this qualification adapter requires Linux. No capabilities verified.",
            false,
        );
    }
    for tool in [&p.boundary, &p.limiter, &p.probe] {
        if file_digest(&tool.path).ok().as_ref() != Some(&tool.digest) {
            return fail(
                "Pinned worker tool unavailable or changed; no tool was invoked.",
                false,
            );
        }
    }
    if !static_elf(&p.probe.path) {
        return fail(
            "Qualification payload must be a pinned static ELF for this architecture; no payload invoked.",
            false,
        );
    }
    let root = match tempfile::Builder::new()
        .prefix(&format!("qualification-{nonce}-"))
        .tempdir_in(&p.scratch_root)
    {
        Ok(d) => d,
        Err(_) => return fail("Private qualification scratch unavailable.", false),
    };
    let prepare = || -> std::io::Result<()> {
        fs::create_dir(root.path().join("input"))?;
        fs::create_dir(root.path().join("work"))?;
        for name in ["input/value", "policy", "host-only"] {
            fs::write(root.path().join(name), nonce)?;
        }
        Ok(())
    };
    if prepare().is_err() {
        return fail(
            "Synthetic qualification fixtures could not be prepared.",
            false,
        );
    }
    let listener = match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)) {
        Ok(l) => l,
        Err(_) => return fail("Host network canary unavailable.", false),
    };
    if listener.set_nonblocking(true).is_err() {
        return fail("Host canary could not be made nonblocking.", false);
    }
    let port = listener.local_addr().unwrap().port();
    let deadline = Instant::now() + Duration::from_secs(p.wall_seconds.into());
    let bytes = match output(
        command(p, root.path(), "boundaries", nonce, port),
        8192,
        deadline,
    )
    .await
    {
        Ok(b) => b,
        Err(_) => {
            let _ = root.keep();
            return fail(
                "Boundary setup/probe failed or exceeded bounds. No capabilities verified; scratch quarantined for operator reconciliation.",
                true,
            );
        }
    };
    let observation = match serde_json::from_slice::<Observation>(&bytes) {
        Ok(o) if o.nonce == nonce => o,
        _ => {
            let _ = root.keep();
            return fail(
                "Probe protocol/nonce invalid; no capabilities verified.",
                true,
            );
        }
    };
    let _schema = observation.schema_version;
    let observation_record = serde_json::to_value(&observation).ok();
    let host_net = fs::read_link("/proc/self/ns/net")
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let host_pid = fs::read_link("/proc/self/ns/pid")
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let mut checks = BTreeMap::from([
        ("tool_identity".into(), true),
        ("input_read".into(), observation.input_read),
        ("base_read_only".into(), observation.base_read_only),
        ("policy_read_only".into(), observation.policy_read_only),
        ("outside_hidden".into(), observation.outside_hidden),
        ("environment_clean".into(), observation.environment_clean),
        (
            "work_read_write".into(),
            observation.work_read_write
                && fs::read_to_string(root.path().join("work/result"))
                    .ok()
                    .as_deref()
                    == Some(nonce),
        ),
        (
            "network_denied".into(),
            observation.network_denied && listener.accept().is_err(),
        ),
        (
            "private_net_namespace".into(),
            namespace_valid(&observation.net_namespace, "net")
                && fs::read_link("/proc/self/ns/net")
                    .is_ok_and(|v| v.to_string_lossy() != observation.net_namespace),
        ),
        (
            "private_pid_namespace".into(),
            namespace_valid(&observation.pid_namespace, "pid")
                && fs::read_link("/proc/self/ns/pid")
                    .is_ok_and(|v| v.to_string_lossy() != observation.pid_namespace),
        ),
        (
            "resource_limits".into(),
            observation.cpu_seconds == p.cpu_seconds
                && observation.memory_bytes == p.memory_bytes
                && observation.file_bytes == p.file_bytes
                && observation.descriptors == p.descriptors
                && observation.core_bytes == 0,
        ),
        (
            "host_files_unchanged".into(),
            ["input/value", "policy", "host-only"].iter().all(|name| {
                fs::read_to_string(root.path().join(name)).ok().as_deref() == Some(nonce)
            }),
        ),
    ]);
    let stopped = if checks.values().all(|v| *v) {
        cancellation(p, root.path(), nonce, port, deadline)
            .await
            .unwrap_or(false)
    } else {
        false
    };
    checks.insert("descendant_termination".into(), stopped);
    // Recheck pins after observation; a changed installed tool invalidates this report.
    checks.insert(
        "tool_identity".into(),
        [&p.boundary, &p.limiter, &p.probe]
            .iter()
            .all(|t| file_digest(&t.path).ok().as_ref() == Some(&t.digest)),
    );
    let passed = checks.values().all(|v| *v);
    let quarantined = !stopped;
    if quarantined {
        let _ = root.keep();
    }
    Evidence{details:Some(serde_json::json!({"observation":observation_record,"host_net_namespace":host_net,"host_pid_namespace":host_pid,"descendant_termination_observed":stopped})),checks,output_digest:Some(format!("sha256:{:x}",sha2::Sha256::digest(&bytes))),reason:if passed{"Controlled probe passed measured filesystem/network/rlimit/descendant checks. Scope is probe-only; no real harness admission."}else{"One or more qualification checks failed; real execution remains denied."}.into(),quarantined,origin:"native".into()}
}
use sha2::Digest;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_boundary_command_has_no_host_root_network_or_arbitrary_task_mount() {
        let root = std::env::temp_dir().join("qualification-fixture");
        let tool = PinnedTool {
            path: std::env::temp_dir().join("trusted-tool"),
            digest: format!("sha256:{}", "0".repeat(64)),
        };
        let p = WorkerPolicy {
            schema_version: V1,
            revision: 1,
            boundary: tool.clone(),
            limiter: tool.clone(),
            probe: tool,
            scratch_root: root.clone(),
            wall_seconds: 2,
            cpu_seconds: 1,
            memory_bytes: 134217728,
            file_bytes: 8192,
            descriptors: 64,
            valid_seconds: 10,
        };
        let c = command(
            &p,
            &root,
            "boundaries",
            "00000000-0000-0000-0000-000000000000",
            12345,
        );
        let args = c
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for arg in [
            "--unshare-all",
            "--unshare-user",
            "--disable-userns",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--cap-drop",
            "--remount-ro",
        ] {
            assert!(args.iter().any(|s| s == arg));
        }
        for arg in [
            "--share-net",
            "--dev-bind",
            "--unshare-user-try",
            "--not-a-security-boundary",
            "/usr",
            "/bin/sh",
        ] {
            assert!(!args.iter().any(|s| s == arg));
        }
        assert!(args.iter().any(|s| s == "--as=134217728:134217728"));
    }
    #[test]
    fn probe_protocol_and_namespace_tokens_reject_forged_fields() {
        assert!(namespace_valid("net:[4026531992]", "net"));
        assert!(!namespace_valid("", "net"));
        assert!(!namespace_valid("net:[x]", "net"));
        assert!(
            serde_json::from_str::<Started>(
                r#"{"schema_version":1,"nonce":"n","started":true,"real_execution_admitted":true}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Started>(r#"{"schema_version":2,"nonce":"n","started":true}"#)
                .is_err()
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn trusted_payload_compiles_statically_and_malformed_or_dynamic_payloads_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let probe = d.path().join("probe");
        let out = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "target-feature=+crt-static"])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/isolation_probe.rs"))
            .arg("-o")
            .arg(&probe)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(static_elf(&probe));
        let invalid = d.path().join("invalid");
        fs::write(&invalid, b"not an ELF").unwrap();
        assert!(!static_elf(&invalid));
    }
}
