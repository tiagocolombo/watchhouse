//! Trusted detached model-client journaler. No candidate execution or publication credentials.
use std::{fs, path::PathBuf, process::Stdio, time::Duration};
use watchhouse::{
    contracts::V1,
    error::{Error, Result},
    model_runtime::{
        self, cgroup, client,
        codex::RoleOutput,
        policy::{Authority, private_dir},
        protocol::*,
    },
};
fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("fixture-pulse") {
        let path = std::env::args()
            .nth(2)
            .ok_or_else(|| Error::Invalid("pulse path missing".into()))?;
        let mut n = 0u64;
        loop {
            fs::write(&path, n.to_string())
                .map_err(|_| Error::Invalid("pulse write failed".into()))?;
            n += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| Error::Invalid("sealed job directory required".into()))?,
    );
    private_dir(&root)?;
    let job: Envelope = read_json(&root.join("job.json"), 1024 * 1024)?;
    job.validate()?;
    check_start(&root, &job)?;
    job.profile.native_preflight()?;
    if !root.starts_with(&job.profile.jobs_root)
        || std::fs::canonicalize(
            std::env::current_exe()
                .map_err(|_| Error::Invalid("initializer identity unavailable".into()))?,
        )
        .ok()
        .as_ref()
            != Some(&job.profile.initializer.path)
    {
        return Err(Error::Invalid("initializer/job identity mismatch".into()));
    }
    write_once(
        &root.join("started.json"),
        &serde_json::json!({"schema_version":1,"operation":job.operation,"nonce":job.nonce,"profile_digest":job.profile_digest,"job_digest":digest(&job)?}),
    )?;
    cgroup::attach_self(&job.profile.cgroup_root, &job.resource)?;
    check_start(&root, &job)?;
    let watchdog_root = root.clone();
    let watchdog_job = job.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(
            watchdog_job.profile.limits.wall_seconds.into(),
        ));
        let _ = revoke(
            &watchdog_root,
            &watchdog_job.operation,
            &watchdog_job.nonce,
            &watchdog_job.profile_digest,
        );
        if cgroup::verify(&watchdog_job.profile.cgroup_root, &watchdog_job.resource).is_ok() {
            let _ = fs::write(watchdog_job.resource.directory.join("cgroup.kill"), "1");
        }
        std::process::exit(124);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Invalid("client event loop unavailable".into()))?;
    runtime.block_on(run(root, job))
}
async fn run(root: PathBuf, job: Envelope) -> Result<()> {
    let cwd = root.join("cwd");
    fs::create_dir(&cwd)
        .map_err(|_| Error::Invalid("private client working directory unavailable".into()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cwd, fs::Permissions::from_mode(0o700))
            .map_err(|_| Error::Invalid("client cwd permissions unavailable".into()))?;
    }
    write_once(&root.join("schema.json"), &job.output_schema)?;
    let fixture = job.qualification || matches!(job.profile.authority, Authority::Fixture);
    let mut auth_temp = None;
    let auth_home = if fixture {
        let dir = tempfile::Builder::new()
            .prefix("client-auth-fixture-")
            .tempdir_in(&root)
            .map_err(|_| Error::Invalid("fixture auth home unavailable".into()))?;
        let path = dir.path().to_owned();
        auth_temp = Some(dir);
        path
    } else {
        match &job.profile.authority {
            Authority::OpenaiAuthHome { path } => path.clone(),
            Authority::Fixture => unreachable!(),
        }
    };
    let mut server = None;
    let mut measurement = None;
    let port = if fixture {
        let response = if job.qualification {
            serde_json::json!({"schema_version":1,"nonce":job.nonce})
        } else {
            job.fixture_response
                .clone()
                .ok_or_else(|| Error::Invalid("fixture role response missing".into()))?
        };
        let (p, m, t) = model_runtime::fixture::serve(response).await?;
        server = Some(t);
        measurement = Some(m);
        Some(p)
    } else {
        None
    };
    // The barrier after attachment closes the cancel-before-delayed-start race. A concurrent
    // cancellation after this check kills this initializer and all future descendants in-kernel.
    check_start(&root, &job)?;
    let result = client::run(client::Invocation {
        policy: &job.profile,
        cwd: &cwd,
        auth_home: &auth_home,
        schema: &root.join("schema.json"),
        prompt: &job.prompt,
        role: job.role.clone(),
        fixture_port: port,
        resource: &job.resource,
    })
    .await?;
    if let Some(t) = server {
        t.abort();
    }
    let observed = measurement.as_ref().map(|m| m.lock().unwrap().clone());
    let mut output = Output {
        schema_version: V1,
        operation: job.operation.clone(),
        nonce: job.nonce.clone(),
        profile_digest: job.profile_digest.clone(),
        client: result,
        synthetic: fixture,
        execution_tools_absent: observed
            .as_ref()
            .map(|m| m.execution_tools_absent && m.requests == 1 && m.failure.is_none()),
        advertised_tools_digest: observed.and_then(|m| m.tools_digest),
        descendant_started: false,
    };
    let _auth = auth_temp;
    let mut pulse = None;
    if job.qualification {
        let valid = output.client.parsed.as_ref().is_some_and(
            |p| matches!(&p.output,RoleOutput::Qualification(v) if v.nonce==job.nonce),
        );
        if valid && output.execution_tools_absent == Some(true) {
            let path = root.join("descendant-pulse");
            let mut child = tokio::process::Command::new(&job.profile.initializer.path);
            child
                .arg("fixture-pulse")
                .arg(&path)
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            pulse =
                Some(child.spawn().map_err(|_| {
                    Error::Invalid("qualification descendant could not start".into())
                })?);
            let mut first = None;
            let observe = async {
                loop {
                    if let Ok(value) = fs::read_to_string(&path) {
                        if first.as_ref().is_some_and(|s| s != &value) {
                            break;
                        }
                        first = Some(value);
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            output.descendant_started = tokio::time::timeout(Duration::from_secs(2), observe)
                .await
                .is_ok();
        }
    }
    validate_output(&job, &output)?;
    write_once(&root.join("result.json"), &output)?;
    if job.qualification {
        let _pulse = pulse;
        std::future::pending::<()>().await;
    }
    Ok(())
}
