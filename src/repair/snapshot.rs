//! Object-only repository copies. No checkout, filters, hooks, shared Git metadata or execution.
use super::*;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Read, process::Stdio};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::{Duration, Instant, timeout_at},
};
fn io(e: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("repair preparation I/O: {e}"))
}
pub(crate) fn file_digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(io)?;
    if file.metadata().map_err(io)?.len() > 512 * 1024 * 1024 {
        return Err(Error::Invalid("runtime executable exceeds 512MiB".into()));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(io)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}
pub(crate) async fn output(
    mut command: Command,
    limit: usize,
    deadline: Instant,
) -> Result<Vec<u8>> {
    let clean_home = tempfile::tempdir().map_err(io)?;
    command
        .env_clear()
        .env("HOME", clean_home.path())
        .env("USERPROFILE", clean_home.path())
        .env("CODEX_HOME", clean_home.path())
        .env("TMP", clean_home.path())
        .env("TEMP", clean_home.path());
    // OS tool lookup only; credentials and caller Git/harness configuration are not inherited.
    for name in ["PATH", "SystemRoot", "WINDIR"] {
        if let Some(v) = std::env::var_os(name) {
            command.env(name, v);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| Error::Invalid("required preparation tool unavailable".into()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("probe stdout unavailable".into()))?;
    let work = async {
        let read = async {
            let mut bytes = vec![];
            (&mut stdout)
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(io)?;
            if bytes.len() > limit {
                return Err(Error::Invalid(
                    "preparation output exceeds trusted bound".into(),
                ));
            }
            Ok(bytes)
        };
        let wait = async {
            let status = child.wait().await.map_err(io)?;
            if !status.success() {
                return Err(Error::Invalid("preparation tool returned failure".into()));
            }
            Ok(())
        };
        let (data, _) = tokio::try_join!(read, wait)?;
        Ok(data)
    };
    timeout_at(deadline, work)
        .await
        .map_err(|_| Error::Invalid("preparation deadline exceeded".into()))?
}
fn git(cfg: &RepairSourceConfig, args: &[&str]) -> Command {
    let mut c = Command::new("git");
    c.arg("--no-replace-objects")
        .arg("--no-lazy-fetch")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-c")
        .arg("core.hooksPath=")
        .arg("-C")
        .arg(&cfg.source_path)
        .args(args);
    c
}
fn portable(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && path.split('/').all(|p| {
            let stem = p.split('.').next().unwrap_or("").to_ascii_uppercase();
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.len() <= 128
                && !p.ends_with('.')
                && !p.eq_ignore_ascii_case(".git")
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit())
        })
}
fn object_identity(kind: &str, bytes: &[u8], oid: &str) -> bool {
    let header = format!("{kind} {}\0", bytes.len());
    let actual = if oid.len() == 40 {
        let mut hash = sha1::Sha1::new();
        hash.update(header.as_bytes());
        hash.update(bytes);
        format!("{:x}", hash.finalize())
    } else {
        let mut hash = Sha256::new();
        hash.update(header.as_bytes());
        hash.update(bytes);
        format!("{:x}", hash.finalize())
    };
    actual == oid
}
pub(crate) async fn prepare(cfg: &RepairSourceConfig) -> Result<(tempfile::TempDir, BaseSnapshot)> {
    let deadline = Instant::now() + Duration::from_secs(cfg.preparation_seconds.into());
    let tree = output(
        git(
            cfg,
            &["rev-parse", &format!("{}^{{tree}}", cfg.base_commit)],
        ),
        128,
        deadline,
    )
    .await?;
    let tree = String::from_utf8(tree).map_err(io)?.trim().to_owned();
    if !oid(&tree) {
        return Err(Error::Invalid("invalid Git tree identity".into()));
    }
    let commit = output(
        git(cfg, &["cat-file", "commit", &cfg.base_commit]),
        65536,
        deadline,
    )
    .await?;
    if !object_identity("commit", &commit, &cfg.base_commit) {
        return Err(Error::Conflict(
            "Git commit identity does not match its bytes".into(),
        ));
    }
    let declared_tree =
        std::str::from_utf8(commit.split(|b| *b == b'\n').next().unwrap_or_default())
            .map_err(io)?;
    if declared_tree != format!("tree {tree}") {
        return Err(Error::Conflict("Git commit/tree linkage changed".into()));
    }
    let mut trees = HashSet::from([tree.clone()]);
    let listing = output(
        git(cfg, &["ls-tree", "-rltz", &cfg.base_commit]),
        cfg.max_files as usize * 1200,
        deadline,
    )
    .await?;
    let mut files = vec![];
    let mut names = HashSet::new();
    let mut total = 0u64;
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry).map_err(io)?;
        let (meta, path) = entry
            .split_once('\t')
            .ok_or_else(|| Error::Invalid("invalid Git entry".into()))?;
        let parts = meta.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 4
            || !oid(parts[2])
            || !portable(path)
            || !names.insert(path.to_ascii_lowercase())
        {
            return Err(Error::Invalid(
                "repository has unsupported paths, symlinks, submodules or case collisions".into(),
            ));
        }
        if parts[0] == "040000" && parts[1] == "tree" && parts[3] == "-" {
            trees.insert(parts[2].to_owned());
            continue;
        }
        if !matches!(parts[0], "100644" | "100755") || parts[1] != "blob" {
            return Err(Error::Invalid(
                "repository has unsupported symlinks or submodules".into(),
            ));
        }
        let size = parts[3].parse::<u64>().map_err(io)?;
        total = total
            .checked_add(size)
            .ok_or_else(|| Error::Invalid("snapshot size overflow".into()))?;
        if size > cfg.max_blob_bytes
            || total > cfg.max_total_bytes
            || files.len() >= cfg.max_files as usize
        {
            return Err(Error::Invalid(
                "repository exceeds trusted snapshot limits".into(),
            ));
        }
        files.push(SnapshotFile {
            path: path.into(),
            git_blob: parts[2].into(),
            git_mode: parts[0].into(),
            bytes: size,
            digest: String::new(),
        });
    }
    if files.is_empty() {
        return Err(Error::Invalid("empty base tree".into()));
    }
    for oid in trees {
        let bytes = output(
            git(cfg, &["cat-file", "tree", &oid]),
            cfg.max_files as usize * 1200,
            deadline,
        )
        .await?;
        if !object_identity("tree", &bytes, &oid) {
            return Err(Error::Conflict(
                "Git tree identity does not match its bytes".into(),
            ));
        }
    }
    let dir = tempfile::Builder::new()
        .prefix("watchhouse-base-")
        .tempdir_in(&cfg.workspace_root)
        .map_err(io)?;
    for f in &mut files {
        let bytes = output(
            git(cfg, &["cat-file", "blob", &f.git_blob]),
            cfg.max_blob_bytes as usize,
            deadline,
        )
        .await?;
        if bytes.len() as u64 != f.bytes {
            return Err(Error::Conflict("Git blob length changed".into()));
        }
        if !object_identity("blob", &bytes, &f.git_blob) {
            return Err(Error::Conflict(
                "Git blob identity does not match its bytes".into(),
            ));
        }
        let path = dir.path().join(&f.path);
        fs::create_dir_all(
            path.parent()
                .ok_or_else(|| Error::Invalid("snapshot parent missing".into()))?,
        )
        .map_err(io)?;
        fs::write(&path, &bytes).map_err(io)?;
        f.digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        // Read-only is an integrity convention, not a security boundary against the host operator.
        let mut permissions = fs::metadata(&path).map_err(io)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).map_err(io)?;
    }
    let manifest = BaseSnapshot {
        base_commit: cfg.base_commit.clone(),
        tree,
        files,
        total_bytes: total,
    };
    Ok((dir, manifest))
}
pub(crate) async fn probe(cfg: &RepairSourceConfig) -> RuntimeProbe {
    let mut probe = RuntimeProbe {
        adapter: "codex-exec-v1-candidate".into(),
        binary_digest: None,
        version: None,
        version_output_digest: None,
        help_output_digest: None,
        advertised: vec![],
        verified: vec![],
        error: None,
    };
    let hash = match file_digest(&cfg.runtime_path) {
        Ok(v) => v,
        Err(_) => {
            probe.error = Some("Configured runtime unavailable".into());
            return probe;
        }
    };
    probe.binary_digest = Some(hash.clone());
    if hash != cfg.runtime_digest {
        probe.error =
            Some("Runtime binary digest does not match trusted configuration; not invoked".into());
        return probe;
    }
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => {
            probe.error = Some("Clean probe directory unavailable".into());
            return probe;
        }
    };
    let mut version = Command::new(&cfg.runtime_path);
    version.current_dir(dir.path()).arg("--version");
    let deadline = Instant::now() + Duration::from_secs(cfg.probe_seconds.into());
    let data = match output(version, 256, deadline).await {
        Ok(d) => d,
        Err(_) => {
            probe.error = Some("Runtime version probe failed or exceeded its bounds".into());
            return probe;
        }
    };
    probe.version_output_digest = Some(format!("sha256:{:x}", Sha256::digest(&data)));
    let v = String::from_utf8_lossy(&data).trim().to_owned();
    if !v.starts_with("codex-cli ")
        || v.len() > 128
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" ._-+".contains(&b))
    {
        probe.error = Some("Configured binary is not a recognized Codex CLI version".into());
        return probe;
    }
    probe.version = Some(v);
    let mut help = Command::new(&cfg.runtime_path);
    help.current_dir(dir.path()).args(["exec", "--help"]);
    match output(help, 32768, deadline).await {
        Ok(bytes) => {
            probe.help_output_digest = Some(format!("sha256:{:x}", Sha256::digest(&bytes)));
            let help = String::from_utf8_lossy(&bytes);
            for flag in [
                "--json",
                "--output-schema",
                "--ignore-user-config",
                "--ignore-rules",
                "--ephemeral",
            ] {
                if help.contains(flag) {
                    probe.advertised.push(flag.into());
                }
            }
        }
        Err(_) => probe.error = Some("Runtime help probe failed or exceeded its bounds".into()),
    }
    // Help flags are advertisements, not evidence of authentication, containment or cancellation.
    probe
}
pub(crate) fn verify(path: &Path, snapshot: &BaseSnapshot, seconds: u32) -> Result<()> {
    if fs::symlink_metadata(path)
        .map_err(io)?
        .file_type()
        .is_symlink()
    {
        return Err(Error::Conflict("snapshot root is a symlink".into()));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds.into());
    let mut entries = 0usize;
    let mut seen = HashSet::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for item in fs::read_dir(&dir).map_err(io)? {
            entries += 1;
            if entries > snapshot.files.len().saturating_mul(1024)
                || std::time::Instant::now() >= deadline
            {
                return Err(Error::Conflict(
                    "snapshot verification exceeds bounds".into(),
                ));
            }
            let item = item.map_err(io)?;
            let kind = item.file_type().map_err(io)?;
            if kind.is_symlink() {
                return Err(Error::Conflict("snapshot contains a symlink".into()));
            }
            if kind.is_dir() {
                pending.push(item.path());
                continue;
            }
            if !kind.is_file() {
                return Err(Error::Conflict("snapshot contains a non-file".into()));
            }
            let file = item.path();
            let relative = file
                .strip_prefix(path)
                .map_err(io)?
                .to_string_lossy()
                .replace('\\', "/");
            let expected = snapshot
                .files
                .iter()
                .find(|f| f.path == relative)
                .ok_or_else(|| Error::Conflict("snapshot contains an unexpected file".into()))?;
            if fs::metadata(&file).map_err(io)?.len() != expected.bytes
                || file_digest(&file)? != expected.digest
            {
                return Err(Error::Conflict("frozen snapshot changed".into()));
            }
            seen.insert(relative);
        }
    }
    if seen.len() != snapshot.files.len() {
        return Err(Error::Conflict("frozen snapshot is incomplete".into()));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probe_child() {
        println!(
            "fixture-secret-cleared:{}",
            std::env::var_os("WATCHHOUSE_PROBE_FIXTURE_SECRET").is_none()
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    fn child() -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "repair::snapshot::tests::probe_child",
                "--nocapture",
            ])
            .env("WATCHHOUSE_PROBE_FIXTURE_SECRET", "synthetic-must-not-pass");
        command
    }
    #[tokio::test]
    async fn probes_clear_caller_secrets_and_reject_excess_output() {
        let bytes = output(child(), 4096, Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains("fixture-secret-cleared:true")
        );
        assert!(
            output(child(), 8, Instant::now() + Duration::from_secs(5))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn probes_fail_closed_on_deadline() {
        let error = output(child(), 4096, Instant::now() + Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("deadline"));
    }
    #[test]
    fn cross_platform_paths_reject_metadata_traversal_and_device_aliases() {
        for path in [
            ".git/config",
            "../escape",
            "/absolute",
            "a/../b",
            "bad:name",
            "a\\b",
            "NUL.txt",
            "src/COM1.txt",
            "a.",
            "a//b",
        ] {
            assert!(!portable(path), "{path}");
        }
        assert!(portable("src/checkout_logic.rs"));
        assert!(portable(".github/workflows/ci.yml"));
    }
}
