//! GitHub Cloud wire boundary. Credentials never enter proposal/test workspaces or error text.
use super::policy::PublisherPolicy;
use reqwest::{
    blocking::{Client, Response},
    redirect::Policy,
};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    MissingCredentials,
    InvalidConfiguration,
    Denied,
    Unavailable,
    InvalidResponse,
}
pub type Result<T> = std::result::Result<T, Failure>;
pub struct Github {
    policy: PublisherPolicy,
    client: Client,
    token: String,
}
impl Github {
    pub fn new(policy: PublisherPolicy) -> Result<Self> {
        policy
            .validate()
            .map_err(|_| Failure::InvalidConfiguration)?;
        let token = std::env::var(&policy.token_env).map_err(|_| Failure::MissingCredentials)?;
        Self::with_token(policy, token)
    }
    fn with_token(policy: PublisherPolicy, token: String) -> Result<Self> {
        policy
            .validate()
            .map_err(|_| Failure::InvalidConfiguration)?;
        if token.is_empty() || token.len() > 4096 || token.bytes().any(|b| !b.is_ascii_graphic()) {
            return Err(Failure::MissingCredentials);
        }
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(policy.request_seconds.into()))
            .build()
            .map_err(|_| Failure::InvalidConfiguration)?;
        Ok(Self {
            policy,
            client,
            token,
        })
    }
    fn request(&self, method: reqwest::Method, suffix: &str) -> reqwest::blocking::RequestBuilder {
        self.client
            .request(
                method,
                format!(
                    "https://api.github.com/repos/{}{}",
                    self.policy.repository, suffix
                ),
            )
            .bearer_auth(&self.token)
            .header("User-Agent", "Watchhouse-delivery-v1")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }
    fn read(response: Response, missing: bool) -> Result<Option<Value>> {
        let status = response.status();
        if missing && status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(
                if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    Failure::Unavailable
                } else {
                    Failure::Denied
                },
            );
        }
        let mut bytes = vec![];
        response
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Unavailable)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(Failure::InvalidResponse);
        }
        Ok(Some(
            serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidResponse)?,
        ))
    }
    pub fn get(&self, suffix: &str) -> Result<Option<Value>> {
        if !Self::read_path(suffix) {
            return Err(Failure::InvalidConfiguration);
        }
        Self::read(
            self.request(reqwest::Method::GET, suffix)
                .send()
                .map_err(|_| Failure::Unavailable)?,
            true,
        )
    }
    fn read_path(suffix: &str) -> bool {
        // The coordinator supplies fixed endpoint forms; no provider-returned URL is followed.
        suffix.starts_with('/')
            && !suffix.contains("..")
            && !suffix.contains('#')
            && !suffix.contains('\\')
            && suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.:?=&".contains(&b))
    }
    pub fn write(&self, suffix: &str, body: &Value, update: bool) -> Result<Value> {
        if self.policy.paused
            || !matches!(
                suffix,
                "/git/trees" | "/git/commits" | "/git/refs" | "/pulls"
            ) && !suffix.starts_with("/git/refs/heads/watchhouse/")
                && !suffix
                    .strip_prefix("/pulls/")
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(Failure::Denied);
        }
        if update && ["/git/trees", "/git/commits"].contains(&suffix) {
            return Err(Failure::Denied);
        }
        if suffix == "/git/refs" {
            let branch = body["ref"]
                .as_str()
                .and_then(|s| s.strip_prefix("refs/heads/"));
            let sha = body["sha"].as_str();
            if update
                || !branch.zip(sha).is_some_and(|(branch, sha)| {
                    Self::branch_body(branch, sha).is_ok_and(|expected| expected == *body)
                })
            {
                return Err(Failure::Denied);
            }
        } else if suffix.starts_with("/git/refs/heads/") {
            if !update
                || !body["sha"].as_str().is_some_and(|sha| {
                    Self::update_body(sha).is_ok_and(|expected| expected == *body)
                })
            {
                return Err(Failure::Denied);
            }
        } else if suffix == "/pulls" {
            if update
                || body["base"] != self.policy.base_branch
                || body["draft"] != self.policy.draft
                || !body["head"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("watchhouse/") && super::policy::branch(s))
                || !body.as_object().is_some_and(|o| {
                    o.keys()
                        .all(|k| ["head", "base", "title", "body", "draft"].contains(&k.as_str()))
                })
            {
                return Err(Failure::Denied);
            }
        } else if suffix.starts_with("/pulls/")
            && (!update
                || !body
                    .as_object()
                    .is_some_and(|o| o.keys().all(|k| ["title", "body"].contains(&k.as_str()))))
        {
            return Err(Failure::Denied);
        }
        if !Self::read_path(suffix)
            || serde_json::to_vec(body)
                .map_err(|_| Failure::InvalidConfiguration)?
                .len()
                > 1024 * 1024
        {
            return Err(Failure::InvalidConfiguration);
        }
        let method = if update {
            reqwest::Method::PATCH
        } else {
            reqwest::Method::POST
        };
        Self::read(
            self.request(method, suffix)
                .json(body)
                .send()
                .map_err(|_| Failure::Unavailable)?,
            false,
        )?
        .ok_or(Failure::InvalidResponse)
    }
    pub fn branch_body(branch: &str, sha: &str) -> Result<Value> {
        if !branch.starts_with("watchhouse/")
            || !super::policy::branch(branch)
            || !super::objects::oid(sha)
        {
            return Err(Failure::InvalidConfiguration);
        }
        Ok(json!({"ref":format!("refs/heads/{branch}"),"sha":sha}))
    }
    pub fn update_body(sha: &str) -> Result<Value> {
        if !super::objects::oid(sha) {
            return Err(Failure::InvalidConfiguration);
        }
        Ok(json!({"sha":sha,"force":false}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branch_requests_are_scoped_and_never_force() {
        assert!(Github::branch_body("main", &"a".repeat(40)).is_err());
        assert!(Github::branch_body("watchhouse/../main", &"a".repeat(40)).is_err());
        assert!(Github::branch_body("watchhouse/job", "bad").is_err());
        assert_eq!(
            Github::update_body(&"a".repeat(40)).unwrap()["force"],
            false
        );
        assert!(!Github::read_path("https://attacker.invalid/token"));
        assert!(!Github::read_path("/git/../secrets"));
    }
}
