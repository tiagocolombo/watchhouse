//! Deterministic Git commit identity permits read-only reconciliation of a lost response.
use super::{objects, policy::PublisherPolicy};
use crate::error::{Error, Result};
use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use serde_json::{Value, json};
pub fn prepare(
    p: &PublisherPolicy,
    tree: &str,
    parents: &[String],
    at: &str,
    message: &str,
) -> Result<(String, Value)> {
    p.validate()?;
    if !objects::oid(tree)
        || parents.is_empty()
        || parents.len() > 2
        || parents.iter().any(|s| !objects::oid(s))
        || message.contains('\0')
        || !message.ends_with('\n')
    {
        return Err(Error::Invalid("invalid publication commit identity".into()));
    }
    let date: DateTime<Utc> = DateTime::parse_from_rfc3339(at)
        .map_err(|_| Error::Invalid("invalid commit date".into()))?
        .with_timezone(&Utc);
    let date = date.with_nanosecond(0).unwrap();
    let identity = format!(
        "{} <{}> {} +0000",
        p.author_name,
        p.author_email,
        date.timestamp()
    );
    let mut raw = format!("tree {tree}\n");
    for parent in parents {
        raw.push_str(&format!("parent {parent}\n"));
    }
    raw.push_str(&format!(
        "author {identity}\ncommitter {identity}\n\n{message}"
    ));
    let actor = json!({"name":p.author_name,"email":p.author_email,"date":date.to_rfc3339_opts(SecondsFormat::Secs,true)});
    Ok((
        objects::object("commit", raw.as_bytes()),
        json!({"tree":tree,"parents":parents,"message":message,"author":actor,"committer":actor}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    #[test]
    fn deterministic_commit_matches_actual_git_with_ordered_parents_and_newlines() {
        let root = tempfile::tempdir().unwrap();
        let p:PublisherPolicy=serde_json::from_value(json!({"schema_version":1,"revision":1,"repository":"fixture/repo","base_branch":"main","token_env":"WATCHHOUSE_FIXTURE_TOKEN","author_name":"Fixture","author_email":"fixture@example.invalid","draft":true,"paused":false,"job_seconds":120,"request_seconds":5,"reconciliation_attempts":3,"max_revisions":3,"retry_seconds":2})).unwrap();
        let git = |args: &[&str], input: &str| {
            let mut child = Command::new("git")
                .arg("-C")
                .arg(root.path())
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    if cfg!(windows) { "NUL" } else { "/dev/null" },
                )
                .env("GIT_AUTHOR_NAME", "Fixture")
                .env("GIT_COMMITTER_NAME", "Fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
                .env("GIT_AUTHOR_DATE", "2026-11-16T12:00:00Z")
                .env("GIT_COMMITTER_DATE", "2026-11-16T12:00:00Z")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_owned()
        };
        git(&["init", "-q"], "");
        let tree = git(&["mktree"], "");
        let parent = git(&["commit-tree", &tree], "base\n");
        let other = git(&["commit-tree", &tree, "-p", &parent], "other\n");
        let parents = vec![other.clone(), parent.clone()];
        let message = "Repair fixture\n\nWatchhouse publication fixture\n";
        let (sha, request) =
            prepare(&p, &tree, &parents, "2026-11-16T12:00:00.999Z", message).unwrap();
        assert_eq!(
            sha,
            git(
                &["commit-tree", &tree, "-p", &other, "-p", &parent],
                message
            )
        );
        assert_eq!(request["author"]["date"], "2026-11-16T12:00:00Z");
        assert!(
            prepare(
                &p,
                &tree,
                &parents,
                "2026-11-16T12:00:00Z",
                "missing newline"
            )
            .is_err()
        );
    }
}
