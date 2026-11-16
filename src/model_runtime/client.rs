//! Fixed Codex text-only adapter. Candidate programs never run in this credential domain.
use super::{
    codex::{self, Parsed, Role},
    policy::{ModelPolicy, Pin},
};
use crate::error::{Error, Result};
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::{Duration, timeout},
};
pub const DISABLED: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "hooks",
    "apps",
    "plugins",
    "remote_plugin",
    "browser_use",
    "browser_use_external",
    "computer_use",
    "view_image",
    "image_generation",
    "in_app_browser",
    "code_mode",
    "code_mode_host",
    "multi_agent",
    "multi_agent_v2",
    "memories",
    "skill_search",
    "skill_mcp_dependency_install",
    "plugin_discovery_prefetch",
    "goals",
    "sleep_tool",
    "tool_suggest",
    "workspace_dependencies",
    "in_app_chat",
    "enable_request_compression",
    "auth_elicitation",
];
/// Only the fixed synthetic endpoint may override the provider. Production uses supported
/// harness-owned OpenAI authentication; user-provided shell flags or endpoint overrides do not exist.
pub fn args(policy: &ModelPolicy, schema: &Path, fixture_port: Option<u16>) -> Vec<String> {
    let mut args = vec![
        "exec".into(),
        "--json".into(),
        "--ephemeral".into(),
        "--ignore-user-config".into(),
        "--ignore-rules".into(),
        "--sandbox".into(),
        "read-only".into(),
        "--skip-git-repo-check".into(),
        "--color".into(),
        "never".into(),
        "--model".into(),
        policy.requested_model.clone(),
        "--output-schema".into(),
        schema.to_string_lossy().into_owned(),
        "-c".into(),
        "web_search=\"disabled\"".into(),
    ];
    if let Some(port) = fixture_port {
        for config in [
            "model_provider=\"watchhouse_fixture\"".into(),
            "model_providers.watchhouse_fixture.name=\"Watchhouse native protocol fixture\"".into(),
            format!("model_providers.watchhouse_fixture.base_url=\"http://127.0.0.1:{port}/v1\""),
            "model_providers.watchhouse_fixture.requires_openai_auth=false".into(),
            "model_providers.watchhouse_fixture.wire_api=\"responses\"".into(),
            "model_providers.watchhouse_fixture.request_max_retries=0".into(),
            "model_providers.watchhouse_fixture.stream_max_retries=0".into(),
        ] {
            args.extend(["-c".into(), config]);
        }
    } else {
        args.extend(["-c".into(), "model_provider=\"openai\"".into()]);
    }
    for feature in DISABLED {
        args.extend(["--disable".into(), (*feature).into()]);
    }
    args.extend([
        "--enable".into(),
        "skip_host_skill_discovery".into(),
        "-".into(),
    ]);
    args
}
async fn bounded(stream: impl tokio::io::AsyncRead + Unpin, limit: u32) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    stream
        .take(u64::from(limit) + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| Error::Invalid("client output stream lost".into()))?;
    if bytes.len() > limit as usize {
        return Err(Error::Invalid("client output budget exhausted".into()));
    }
    Ok(bytes)
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientResult {
    pub parsed: Option<Parsed>,
    pub failure_code: Option<String>,
    pub stderr_digest: String,
}
fn pincheck(pin: &Pin) -> Result<()> {
    pin.verify()
}
pub struct Invocation<'a> {
    pub policy: &'a ModelPolicy,
    pub cwd: &'a Path,
    pub auth_home: &'a Path,
    pub schema: &'a Path,
    pub prompt: &'a str,
    pub role: Role,
    pub fixture_port: Option<u16>,
    pub resource: &'a super::cgroup::Handle,
}
pub async fn run(invocation: Invocation<'_>) -> Result<ClientResult> {
    let Invocation {
        policy,
        cwd,
        auth_home,
        schema,
        prompt,
        role,
        fixture_port,
        resource,
    } = invocation;
    policy.validate()?;
    super::cgroup::assert_self(&policy.cgroup_root, resource)?;
    if resource.profile_digest != policy.digest()? {
        return Err(Error::Conflict(
            "client cgroup/profile fence mismatch".into(),
        ));
    }
    if matches!(policy.authority, super::policy::Authority::Fixture) && fixture_port.is_none() {
        return Err(Error::Invalid(
            "synthetic authority cannot contact a live provider".into(),
        ));
    }
    pincheck(&policy.runtime)?;
    if prompt.len() > 262144
        || prompt.contains('\0')
        || std::fs::canonicalize(cwd).ok().as_deref() != Some(cwd)
        || !cwd.starts_with(&policy.jobs_root)
        || auth_home.starts_with(cwd)
        || cwd.starts_with(auth_home)
    {
        return Err(Error::Invalid(
            "client briefing/cwd/auth scope is invalid".into(),
        ));
    }
    let mut c = Command::new(&policy.runtime.path);
    c.args(args(policy, schema, fixture_port))
        .current_dir(cwd)
        .env_clear()
        .env("HOME", cwd)
        .env("CODEX_HOME", auth_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = std::env::var_os("PATH") {
        c.env("PATH", path);
    }
    let mut child = c
        .spawn()
        .map_err(|_| Error::Invalid("pinned client startup failed".into()))?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| Error::Invalid("client stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("client stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::Invalid("client stderr unavailable".into()))?;
    let work = async {
        let (_, out, err, status) = tokio::try_join!(
            async {
                input
                    .write_all(prompt.as_bytes())
                    .await
                    .map_err(|_| Error::Invalid("client input lost".into()))?;
                input
                    .shutdown()
                    .await
                    .map_err(|_| Error::Invalid("client input close failed".into()))
            },
            bounded(stdout, policy.limits.output_bytes),
            bounded(stderr, 65536),
            async {
                child
                    .wait()
                    .await
                    .map_err(|_| Error::Invalid("client exit acknowledgment lost".into()))
            }
        )?;
        Ok::<_, Error>((out, err, status))
    };
    let mut result = ClientResult {
        parsed: None,
        failure_code: None,
        stderr_digest: crate::store::digest(""),
    };
    match timeout(Duration::from_secs(policy.limits.wall_seconds.into()), work).await {
        Ok(Ok((out, err, status))) => {
            result.stderr_digest = crate::real_execution::protocol::bytes_digest(&err);
            if status.success() {
                match String::from_utf8(out)
                    .ok()
                    .and_then(|s| codex::parse(&s, role).ok())
                {
                    Some(parsed) => result.parsed = Some(parsed),
                    None => result.failure_code = Some("invalid_or_blocked_output".into()),
                }
            } else {
                result.failure_code = Some("client_failed".into());
            }
        }
        Ok(Err(_)) => {
            let _ = child.start_kill();
            result.failure_code = Some("lost_or_excess_output".into());
        }
        Err(_) => {
            let _ = child.start_kill();
            result.failure_code = Some("timeout".into());
        }
    }
    // The owning controller still must positively clear the entire cgroup before accepting output.
    pincheck(&policy.runtime)?;
    Ok(result)
}
