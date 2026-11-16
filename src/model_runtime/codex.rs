//! Codex machine protocol parser. The model emits proposal/review data, not workflow commands.
use crate::{
    contracts::V1,
    error::{Error, Result},
    real_execution::pipeline::TextChange,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub schema_version: V1,
    pub summary: String,
    pub changes: Vec<TextChange>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub schema_version: V1,
    pub approve: bool,
    pub findings: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Qualification,
    Engineer,
    Reviewer,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", content = "output", rename_all = "snake_case")]
pub enum RoleOutput {
    Qualification(Probe),
    Engineer(Proposal),
    Reviewer(Assessment),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub schema_version: V1,
    pub nonce: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parsed {
    pub output: RoleOutput,
    pub session: String,
    pub actual_model: Option<String>,
    pub usage: Usage,
    pub events: Vec<String>,
}
fn invalid(s: &str) -> Error {
    Error::Invalid(s.into())
}
/// Only machine envelopes are parsed here. Private reasoning/error prose is neither interpreted
/// as authority nor retained. Question/tool/block/error events cannot approve a candidate.
pub fn parse(lines: &str, role: Role) -> Result<Parsed> {
    if lines.len() > 262144 {
        return Err(invalid("harness protocol output exceeds bound"));
    }
    let mut session = None;
    let mut message = None;
    let mut finished = false;
    let mut started = false;
    let mut events = vec![];
    let mut usage = Usage {
        input_tokens: None,
        output_tokens: None,
        source: "unknown".into(),
    };
    for line in lines.lines().filter(|s| !s.trim().is_empty()) {
        if events.len() >= 1000 {
            return Err(invalid("harness event budget exhausted"));
        }
        let event: Value = serde_json::from_str(line)?;
        let kind = event["type"]
            .as_str()
            .ok_or_else(|| invalid("harness event type missing"))?;
        if finished {
            return Err(invalid("events after terminal harness output"));
        }
        match kind {
            "thread.started" => {
                if session.is_some() {
                    return Err(invalid("duplicate harness session identity"));
                }
                let id = event["thread_id"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .ok_or_else(|| invalid("harness session identity missing"))?;
                session = Some(id.into());
            }
            "turn.started" => {
                if session.is_none() || started {
                    return Err(invalid("invalid harness turn ordering"));
                }
                started = true;
            }
            "item.started" | "item.updated" | "item.completed" => {
                let item = &event["item"];
                match item["type"].as_str() {
                    Some("agent_message") => {
                        if !started {
                            return Err(invalid("role message before turn start"));
                        }
                        if kind == "item.completed" {
                            let text = item["text"]
                                .as_str()
                                .ok_or_else(|| invalid("structured role text missing"))?;
                            if message.is_some() {
                                return Err(invalid("multiple completed role outputs"));
                            }
                            message = Some(text.to_owned());
                        }
                    }
                    Some("reasoning") => {}
                    Some("error") => {}
                    _ => {
                        return Err(invalid(
                            "harness requested a tool, plan or interaction outside the bounded text role",
                        ));
                    }
                }
            }
            "turn.completed" => {
                if !started {
                    return Err(invalid("completion before turn start"));
                }
                finished = true;
                usage.input_tokens = event["usage"]["input_tokens"].as_u64();
                usage.output_tokens = event["usage"]["output_tokens"].as_u64();
                if usage.input_tokens.is_some() || usage.output_tokens.is_some() {
                    usage.source = "harness-reported".into();
                }
            }
            "turn.failed" | "error" => {
                return Err(invalid(
                    "harness failed; structured task output is not accepted",
                ));
            }
            _ => {
                return Err(invalid(
                    "unsupported harness event; runtime contract changed",
                ));
            }
        }
        events.push(kind.to_owned());
    }
    if !finished {
        return Err(invalid("harness stream ended without terminal completion"));
    }
    let text = message.ok_or_else(|| invalid("harness returned no structured role output"))?;
    let output = match role {
        Role::Qualification => RoleOutput::Qualification(serde_json::from_str(&text)?),
        Role::Engineer => {
            let p: Proposal = serde_json::from_str(&text)?;
            if p.summary.trim().is_empty()
                || p.summary.len() > 4000
                || p.changes.is_empty()
                || p.changes.len() > 20
                || serde_json::to_vec(&p.changes)?.len() > 1024 * 1024
            {
                return Err(invalid("proposal exceeds bounded role schema"));
            }
            let mut seen = std::collections::HashSet::new();
            for change in &p.changes {
                if !crate::real_execution::protocol::portable(&change.path)
                    || !seen.insert(change.path.to_ascii_lowercase())
                    || change
                        .before_digest
                        .as_ref()
                        .is_some_and(|s| !crate::real_execution::protocol::sha_valid(s))
                {
                    return Err(invalid(
                        "proposal contains unsafe/colliding paths or preconditions",
                    ));
                }
            }
            RoleOutput::Engineer(p)
        }
        Role::Reviewer => {
            let p: Assessment = serde_json::from_str(&text)?;
            if p.findings.trim().is_empty() || p.findings.len() > 8000 {
                return Err(invalid("review exceeds bounded role schema"));
            }
            RoleOutput::Reviewer(p)
        }
    };
    Ok(Parsed {
        output,
        session: session.ok_or_else(|| invalid("harness session not observed"))?,
        actual_model: None,
        usage,
        events,
    })
}
