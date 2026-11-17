//! Versioned HTTPS protocol for an operator-controlled, scoped operations gateway.
use super::{Job, Policy};
use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};
#[derive(Debug, Clone, Copy)]
pub enum Failure {
    MissingCredentials,
    Denied,
    Unavailable,
    InvalidResponse,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionResult {
    pub operation_id: String,
    pub runbook_id: String,
    pub target: String,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation {
    Deployment {
        service: String,
        environment: String,
        repository: String,
        revision: String,
        artifact: String,
        observed_at: String,
        included_commits: Vec<String>,
    },
    Telemetry {
        service: String,
        environment: String,
        window: crate::recovery::Window,
    },
}
impl Observation {
    pub fn validate(&self, p: &Policy, now: DateTime<Utc>) -> Result<()> {
        let historical = |s: &str| DateTime::parse_from_rfc3339(s).is_ok_and(|t| t <= now);
        match self {
            Self::Deployment {
                service,
                environment,
                repository,
                revision,
                artifact,
                observed_at,
                included_commits,
            } => {
                if service != &p.service
                    || environment != &p.environment
                    || repository != &p.repository
                    || !crate::delivery::objects::oid(revision)
                    || !historical(observed_at)
                    || included_commits.len() > 100
                    || included_commits
                        .iter()
                        .any(|s| !crate::delivery::objects::oid(s))
                {
                    return Err(Error::Invalid(
                        "invalid deployment scope/revision/window".into(),
                    ));
                }
                crate::application::text(artifact, "deployed artifact", 512)?;
            }
            Self::Telemetry {
                service,
                environment,
                window: w,
            } => {
                let start = DateTime::parse_from_rfc3339(&w.started_at)
                    .map_err(|_| Error::Invalid("invalid telemetry start".into()))?;
                let end = DateTime::parse_from_rfc3339(&w.ended_at)
                    .map_err(|_| Error::Invalid("invalid telemetry end".into()))?;
                if service != &p.service
                    || environment != &p.environment
                    || start >= end
                    || end > now
                    || end - start > chrono::Duration::days(30)
                    || w.errors > w.requests
                    || w.requests > 1_000_000_000_000
                    || w.p95_ms.is_some_and(|n| n > 86_400_000)
                    || !w.evidence_refs.is_empty()
                {
                    return Err(Error::Invalid("invalid telemetry scope/count/window; gateway may not supply local evidence authority".into()));
                }
                crate::application::text(&w.collection_method, "instrumentation", 2000)?;
                crate::application::text(&w.uncertainty, "coverage uncertainty", 2000)?;
            }
        }
        Ok(())
    }
}
pub trait Backend {
    fn native(&self) -> bool {
        false
    }
    fn execute(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure>;
    fn lookup(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure>;
    fn observe(&self, p: &Policy, id: &str) -> std::result::Result<Observation, Failure>;
}
pub struct Native;
impl Native {
    fn request<T: serde::de::DeserializeOwned>(
        p: &Policy,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> std::result::Result<T, Failure> {
        p.validate().map_err(|_| Failure::Denied)?;
        let token = std::env::var(&p.token_env).map_err(|_| Failure::MissingCredentials)?;
        if token.is_empty() || token.len() > 4096 || token.bytes().any(|b| !b.is_ascii_graphic()) {
            return Err(Failure::MissingCredentials);
        }
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(p.request_seconds.into()))
            .build()
            .map_err(|_| Failure::Denied)?;
        let url = format!("{}{}", p.gateway.trim_end_matches('/'), path);
        let request = if let Some(body) = body {
            client.post(url).json(&body)
        } else {
            client.get(url)
        };
        let response = request
            .bearer_auth(token)
            .header("User-Agent", "Watchhouse-operations-v1")
            .send()
            .map_err(|_| Failure::Unavailable)?;
        if !response.status().is_success() {
            return Err(if matches!(response.status().as_u16(), 401 | 403) {
                Failure::Denied
            } else {
                Failure::Unavailable
            });
        }
        let mut bytes = vec![];
        response
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Unavailable)?;
        if bytes.len() > 65536 {
            return Err(Failure::InvalidResponse);
        }
        serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidResponse)
    }
}
impl Backend for Native {
    fn native(&self) -> bool {
        true
    }
    fn execute(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
        if p.paused {
            return Err(Failure::Denied);
        }
        // No model-provided command, target, URL or shell arguments cross this boundary.
        Self::request(
            p,
            "/v1/actions",
            Some(
                serde_json::json!({"schema_version":1,"operation_id":j.id,"runbook_id":j.runbook.id,"target":j.runbook.target,"policy_digest":j.policy_digest,"deadline":j.deadline}),
            ),
        )
    }
    fn lookup(&self, p: &Policy, j: &Job) -> std::result::Result<ActionResult, Failure> {
        if !super::ident(&j.id) {
            return Err(Failure::Denied);
        }
        Self::request(p, &format!("/v1/actions/{}", j.id), None)
    }
    fn observe(&self, p: &Policy, id: &str) -> std::result::Result<Observation, Failure> {
        if !super::ident(id) || !p.observation_ids.iter().any(|s| s == id) {
            return Err(Failure::Denied);
        }
        Self::request(p, &format!("/v1/observations/{id}"), None)
    }
}
