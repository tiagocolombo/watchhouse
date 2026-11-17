//! GitHub Cloud exact-head merge transport. No force, branch update, bypass or auto-merge API.
use super::{Check, Job, Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, time::Duration};
#[derive(Debug, Clone, Copy)]
pub enum Failure {
    Denied,
    Unavailable,
    InvalidResponse,
    MissingCredentials,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub repository: String,
    pub branch: String,
    pub base_branch: String,
    pub head: String,
    pub base: String,
    pub tree: String,
    pub open: bool,
    pub draft: bool,
    pub mergeable: bool,
    pub clean: bool,
    pub strict_protection: bool,
    pub required_checks: Vec<Check>,
    pub successful_checks: Vec<Check>,
    pub all_checks_pass: bool,
    pub current_approvals: u32,
    pub changes_requested: bool,
    pub required_approvals: u32,
    pub paths: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Merged {
    pub commit: String,
    pub head: String,
    pub base: String,
    pub tree: String,
}
pub trait Backend {
    fn native(&self) -> bool {
        false
    }
    fn inspect(&self, p: &Policy, j: &Job) -> std::result::Result<Observation, Failure>;
    fn merge(&self, p: &Policy, j: &Job) -> std::result::Result<Merged, Failure>;
    fn reconcile(&self, p: &Policy, j: &Job) -> std::result::Result<Merged, Failure>;
}
pub fn eligible(p: &Policy, j: &Job, o: &Observation) -> std::result::Result<(), Failure> {
    let mut paths = o.paths.clone();
    paths.sort();
    paths.dedup();
    if o.repository != j.repository
        || o.branch != j.branch
        || o.base_branch != p.base_branch
        || o.head != j.head
        || o.base != j.base
        || o.tree != j.tree
        || !o.open
        || o.draft
        || !o.mergeable
        || !o.clean
        || !o.strict_protection
        || !o.all_checks_pass
        || o.changes_requested
        || o.current_approvals < p.required_approvals.max(o.required_approvals)
        || paths != j.paths
        || paths.iter().any(|s| !p.permits(s))
        || p.required_checks
            .iter()
            .chain(&o.required_checks)
            .any(|c| !o.successful_checks.contains(c))
    {
        return Err(Failure::Denied);
    }
    Ok(())
}
pub struct Native;
struct Api {
    p: Policy,
    client: reqwest::blocking::Client,
    token: String,
}
impl Api {
    fn new(p: &Policy) -> std::result::Result<Self, Failure> {
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
        Ok(Self {
            p: p.clone(),
            client,
            token,
        })
    }
    fn request(&self, path: &str, body: Option<Value>) -> std::result::Result<Value, Failure> {
        // Only coordinator-generated fixed repository suffixes are accepted. Never follow URLs.
        if !(path.is_empty() || path.starts_with('/'))
            || path.contains("..")
            || !path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.:?=&".contains(&b))
        {
            return Err(Failure::Denied);
        }
        let url = format!("https://api.github.com/repos/{}{}", self.p.repository, path);
        let r = if let Some(body) = body {
            if self.p.paused
                || !path
                    .strip_prefix("/pulls/")
                    .and_then(|s| s.strip_suffix("/merge"))
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                || body["merge_method"] != "merge"
                || !body["sha"]
                    .as_str()
                    .is_some_and(crate::delivery::objects::oid)
                || body.as_object().is_none_or(|o| o.len() != 2)
            {
                return Err(Failure::Denied);
            }
            self.client.put(url).json(&body)
        } else {
            self.client.get(url)
        };
        let response = r
            .bearer_auth(&self.token)
            .header("User-Agent", "Watchhouse-merge-v1")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .map_err(|_| Failure::Unavailable)?;
        if !response.status().is_success() {
            return Err(
                if matches!(
                    response.status().as_u16(),
                    401 | 403 | 404 | 405 | 409 | 422
                ) {
                    Failure::Denied
                } else {
                    Failure::Unavailable
                },
            );
        }
        if response
            .headers()
            .get("link")
            .is_some_and(|v| v.to_str().map_or(true, |s| s.contains("rel=\"next\"")))
        {
            return Err(Failure::InvalidResponse);
        }
        let mut bytes = vec![];
        response
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Unavailable)?;
        if bytes.len() > 1024 * 1024 {
            return Err(Failure::InvalidResponse);
        }
        serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidResponse)
    }
    fn get(&self, path: &str) -> std::result::Result<Value, Failure> {
        self.request(path, None)
    }
}
fn string<'a>(v: &'a Value, key: &str) -> std::result::Result<&'a str, Failure> {
    v[key].as_str().ok_or(Failure::InvalidResponse)
}
fn array(v: &Value) -> std::result::Result<&Vec<Value>, Failure> {
    v.as_array().ok_or(Failure::InvalidResponse)
}
fn exact_pr(p: &Policy, j: &Job, pr: &Value) -> bool {
    pr["number"].as_u64() == Some(j.pr)
        && pr["head"]["repo"]["full_name"] == p.repository
        && pr["base"]["repo"]["full_name"] == p.repository
        && pr["head"]["ref"] == j.branch
        && pr["head"]["sha"] == j.head
        && pr["base"]["ref"] == p.base_branch
}
fn protected(v: &Value) -> bool {
    v["required_status_checks"]["strict"] == true
        && v["enforce_admins"]["enabled"] == true
        && v["allow_force_pushes"]["enabled"] == false
        && v["allow_deletions"]["enabled"] == false
        && v["required_pull_request_reviews"].is_object()
        && v["required_pull_request_reviews"]["dismiss_stale_reviews"] == true
        && v["required_conversation_resolution"]["enabled"] == true
        && v["required_pull_request_reviews"]["bypass_pull_request_allowances"]
            .as_object()
            .is_none_or(|o| o.values().all(|a| a.as_array().is_some_and(Vec::is_empty)))
}
impl Backend for Native {
    fn native(&self) -> bool {
        true
    }
    fn inspect(&self, p: &Policy, j: &Job) -> std::result::Result<Observation, Failure> {
        let api = Api::new(p)?;
        let repository = api.get("")?;
        if repository["permissions"]["admin"] != false
            || repository["allow_merge_commit"] != true
            || repository["archived"] != false
        {
            return Err(Failure::Denied);
        }
        let pr = api.get(&format!("/pulls/{}", j.pr))?;
        if !exact_pr(p, j, &pr) {
            return Err(Failure::Denied);
        }
        let branch = api.get(&format!("/branches/{}", p.base_branch))?;
        let protection = api.get(&format!("/branches/{}/protection", p.base_branch))?;
        let commit = api.get(&format!("/git/commits/{}", j.head))?;
        let parents = array(&commit["parents"])?;
        // Published candidates have exactly the frozen base parent. Strict server protection
        // then rejects a concurrently advanced base rather than testing against an unseen base.
        if parents.len() != 1 || parents[0]["sha"] != j.base || commit["sha"] != j.head {
            return Err(Failure::Denied);
        }
        let files = api.get(&format!("/pulls/{}/files?per_page=100", j.pr))?;
        let files = array(&files)?;
        if pr["changed_files"].as_u64() != Some(files.len() as u64)
            || files.is_empty()
            || files.len() > 100
            || files.iter().any(|f| f["status"] == "renamed")
        {
            return Err(Failure::Denied);
        }
        let paths = files
            .iter()
            .map(|f| Ok(string(f, "filename")?.to_owned()))
            .collect::<std::result::Result<Vec<_>, Failure>>()?;
        let checks = api.get(&format!("/commits/{}/check-runs?per_page=100", j.head))?;
        let rows = array(&checks["check_runs"])?;
        if checks["total_count"].as_u64() != Some(rows.len() as u64) {
            return Err(Failure::InvalidResponse);
        }
        let mut latest: BTreeMap<(String, u64), (u64, &Value)> = BTreeMap::new();
        for r in rows {
            if r["head_sha"] != j.head {
                return Err(Failure::InvalidResponse);
            }
            let key = (
                string(r, "name")?.to_owned(),
                r["app"]["id"].as_u64().ok_or(Failure::InvalidResponse)?,
            );
            let id = r["id"].as_u64().ok_or(Failure::InvalidResponse)?;
            if latest.get(&key).is_none_or(|(old, _)| id > *old) {
                latest.insert(key, (id, r));
            }
        }
        let successful_checks = latest
            .iter()
            .filter(|(_, (_, r))| r["status"] == "completed" && r["conclusion"] == "success")
            .map(|((name, app_id), _)| Check {
                name: name.clone(),
                app_id: *app_id,
            })
            .collect();
        let all_checks_pass = latest.values().all(|(_, r)| {
            r["status"] == "completed"
                && ["success", "neutral", "skipped"]
                    .contains(&r["conclusion"].as_str().unwrap_or(""))
        });
        let status = api.get(&format!("/commits/{}/status?per_page=100", j.head))?;
        let total = status["total_count"]
            .as_u64()
            .ok_or(Failure::InvalidResponse)?;
        if total != array(&status["statuses"])?.len() as u64 {
            return Err(Failure::InvalidResponse);
        }
        let all_checks_pass = all_checks_pass && (total == 0 || status["state"] == "success");
        let required_checks = array(&protection["required_status_checks"]["checks"])?
            .iter()
            .map(|r| {
                Ok(Check {
                    name: string(r, "context")?.to_owned(),
                    app_id: r["app_id"]
                        .as_u64()
                        .filter(|i| *i > 0)
                        .ok_or(Failure::Denied)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, Failure>>()?;
        if required_checks.is_empty() {
            return Err(Failure::Denied);
        }
        let reviews = api.get(&format!("/pulls/{}/reviews?per_page=100", j.pr))?;
        let mut latest_reviews = BTreeMap::new();
        for r in array(&reviews)? {
            let state = string(r, "state")?;
            if state == "COMMENTED" || state == "PENDING" {
                continue;
            }
            latest_reviews.insert(string(&r["user"], "login")?.to_owned(), r);
        }
        let current_approvals = latest_reviews
            .iter()
            .filter(|(who, r)| {
                r["state"] == "APPROVED"
                    && r["commit_id"] == j.head
                    && **who != pr["user"]["login"].as_str().unwrap_or("")
            })
            .count() as u32;
        let changes_requested = latest_reviews
            .values()
            .any(|r| r["state"] == "CHANGES_REQUESTED");
        Ok(Observation {
            repository: p.repository.clone(),
            branch: j.branch.clone(),
            base_branch: p.base_branch.clone(),
            head: string(&pr["head"], "sha")?.into(),
            base: string(&branch["commit"], "sha")?.into(),
            tree: string(&commit["tree"], "sha")?.into(),
            open: pr["state"] == "open",
            draft: pr["draft"] != false,
            mergeable: pr["mergeable"] == true,
            clean: pr["mergeable_state"] == "clean",
            strict_protection: branch["protected"] == true && protected(&protection),
            required_checks,
            successful_checks,
            all_checks_pass,
            current_approvals,
            changes_requested,
            required_approvals:
                protection["required_pull_request_reviews"]["required_approving_review_count"]
                    .as_u64()
                    .filter(|n| *n <= 10)
                    .ok_or(Failure::Denied)? as u32,
            paths,
        })
    }
    fn merge(&self, p: &Policy, j: &Job) -> std::result::Result<Merged, Failure> {
        eligible(p, j, &self.inspect(p, j)?)?;
        if chrono::DateTime::parse_from_rfc3339(&j.deadline)
            .map_or(true, |t| t <= chrono::Utc::now())
        {
            return Err(Failure::Denied);
        }
        let result = Api::new(p)?.request(
            &format!("/pulls/{}/merge", j.pr),
            Some(json!({"sha":j.head,"merge_method":"merge"})),
        )?;
        match result["merged"].as_bool() {
            Some(true) => {}
            Some(false) => return Err(Failure::Denied),
            None => return Err(Failure::InvalidResponse),
        }
        // Once GitHub acknowledged an effect, any failed observation stays uncertain.
        let merged = self.reconcile(p, j).map_err(|_| Failure::Unavailable)?;
        if result["sha"] != merged.commit {
            return Err(Failure::InvalidResponse);
        }
        Ok(merged)
    }
    fn reconcile(&self, p: &Policy, j: &Job) -> std::result::Result<Merged, Failure> {
        let api = Api::new(p)?;
        let pr = api.get(&format!("/pulls/{}", j.pr))?;
        if !exact_pr(p, j, &pr) || pr["merged"] != true {
            return Err(Failure::Unavailable);
        }
        let sha = string(&pr, "merge_commit_sha")?;
        if !crate::delivery::objects::oid(sha) {
            return Err(Failure::InvalidResponse);
        }
        let commit = api.get(&format!("/git/commits/{sha}"))?;
        let parents = array(&commit["parents"])?;
        if commit["sha"] != sha
            || parents.len() != 2
            || parents[0]["sha"] != j.base
            || parents[1]["sha"] != j.head
            || commit["tree"]["sha"] != j.tree
        {
            return Err(Failure::InvalidResponse);
        }
        Ok(Merged {
            commit: sha.into(),
            head: j.head.clone(),
            base: j.base.clone(),
            tree: j.tree.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_server_guard_cannot_be_replaced_by_head_ci_only() {
        let mut p = json!({"required_status_checks":{"strict":true},"enforce_admins":{"enabled":true},"allow_force_pushes":{"enabled":false},"allow_deletions":{"enabled":false},"required_pull_request_reviews":{"dismiss_stale_reviews":true,"bypass_pull_request_allowances":{"users":[],"teams":[],"apps":[]}},"required_conversation_resolution":{"enabled":true}});
        assert!(protected(&p));
        p["required_status_checks"]["strict"] = json!(false);
        assert!(!protected(&p));
        p["required_status_checks"]["strict"] = json!(true);
        p["required_pull_request_reviews"]["bypass_pull_request_allowances"]["apps"] =
            json!([{"id":1}]);
        assert!(!protected(&p));
    }
}
