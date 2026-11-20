//! Claude Code 2.1.329 bare, restricted, tool-free text role adapter.
use super::{
    codex::{Parsed, Role, Usage},
    policy::ModelPolicy,
};
use crate::error::{Error, Result};
use serde_json::Value;
fn invalid() -> Error {
    Error::Invalid("Claude text-role protocol or capability boundary changed".into())
}
pub fn args(p: &ModelPolicy) -> Vec<String> {
    [
        "--bare",
        "--restricted",
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--disallowedTools",
        "*",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--setting-sources",
        "",
        "--permission-mode",
        "dontAsk",
        "--max-turns",
        "1",
        "--model",
        &p.requested_model,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub fn parse(lines: &str, role: Role) -> Result<Parsed> {
    if lines.len() > 262144 {
        return Err(invalid());
    }
    let mut session: Option<String> = None;
    let mut early_session: Option<String> = None;
    let mut text = None;
    let mut model = None;
    let mut result = None;
    let mut events = vec![];
    let mut usage = Usage {
        input_tokens: None,
        output_tokens: None,
        source: "unknown".into(),
    };
    for line in lines
        .split('\n')
        .map(|s| s.trim_end_matches('\r'))
        .filter(|s| !s.is_empty())
    {
        if result.is_some() {
            return Err(invalid());
        }
        let v: Value = serde_json::from_str(line)?;
        let sid = v["session_id"]
            .as_str()
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .ok_or_else(invalid)?;
        if session
            .as_ref()
            .or(early_session.as_ref())
            .is_some_and(|old| old != sid)
        {
            return Err(invalid());
        }
        match v["type"].as_str() {
            Some("system") if v["subtype"] == "ui_invalidate" && v["event"] == "ui.render" => {
                early_session = Some(sid.into());
                events.push("system:ui_invalidate".into());
            }
            Some("system") if v["subtype"] == "init" => {
                if session.is_some()
                    || v["claude_code_version"] != "2.1.329"
                    || v["permissionMode"] != "dontAsk"
                    || v["apiKeySource"] != "ANTHROPIC_API_KEY"
                {
                    return Err(invalid());
                }
                for key in [
                    "tools",
                    "mcp_servers",
                    "slash_commands",
                    "skills",
                    "additional_directories",
                ] {
                    if !v[key].as_array().is_some_and(Vec::is_empty) {
                        return Err(invalid());
                    }
                }
                if !v["plugins"].as_array().is_some_and(|p| {
                    p.iter().all(|p| {
                        p["path"] == "builtin"
                            && p["name"].as_str().is_some_and(|n| {
                                [
                                    "cc-plugin-agents-md",
                                    "cc-plugin-privacy",
                                    "cc-plugin-plugin-authoring",
                                ]
                                .contains(&n)
                            })
                            && p["source"]
                                == format!("{}@builtin", p["name"].as_str().unwrap_or(""))
                    })
                }) {
                    return Err(invalid());
                }
                if !v["agents"].as_array().is_some_and(|a| {
                    a.iter().all(|v| {
                        v.as_str().is_some_and(|s| {
                            [
                                "claude",
                                "Explore",
                                "general-purpose",
                                "Plan",
                                "statusline-setup",
                            ]
                            .contains(&s)
                        })
                    })
                }) {
                    return Err(invalid());
                }
                session = Some(sid.into());
                events.push("system:init".into());
            }
            Some("assistant") => {
                if session.is_none() || text.is_some() || !v["parent_tool_use_id"].is_null() {
                    return Err(invalid());
                }
                let content = v["message"]["content"].as_array().ok_or_else(invalid)?;
                if content
                    .iter()
                    .any(|b| !matches!(b["type"].as_str(), Some("text" | "thinking")))
                {
                    return Err(invalid());
                }
                let texts = content
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .collect::<Vec<_>>();
                if texts.len() != 1 {
                    return Err(invalid());
                }
                text = Some(texts[0]["text"].as_str().ok_or_else(invalid)?.to_owned());
                model = Some(
                    v["message"]["model"]
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 128)
                        .ok_or_else(invalid)?
                        .to_owned(),
                );
                events.push("assistant".into());
            }
            Some("result") => {
                if session.is_none()
                    || text.is_none()
                    || v["subtype"] != "success"
                    || v["is_error"] != false
                    || v["num_turns"] != 1
                    || v["stop_reason"] != "end_turn"
                    || v["terminal_reason"] != "completed"
                    || v["subagent_stats"]["spawned"] != 0
                    || !v["permission_denials"]
                        .as_array()
                        .is_some_and(Vec::is_empty)
                {
                    return Err(invalid());
                }
                let final_text = v["result"].as_str().ok_or_else(invalid)?;
                if Some(final_text) != text.as_deref() {
                    return Err(invalid());
                }
                result = Some(super::codex::parse_role(final_text, role.clone())?);
                usage.input_tokens = v["usage"]["input_tokens"].as_u64();
                usage.output_tokens = v["usage"]["output_tokens"].as_u64();
                if usage.input_tokens.is_some() || usage.output_tokens.is_some() {
                    usage.source = "harness-reported".into();
                }
                events.push("result:success".into());
            }
            _ => return Err(invalid()),
        }
    }
    Ok(Parsed {
        output: result.ok_or_else(invalid)?,
        session: format!("claude:{}", session.ok_or_else(invalid)?),
        actual_model: model,
        usage,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../../tests/fixtures/model-runtime/claude.jsonl");
    fn messages() -> Vec<Value> {
        FIXTURE
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect()
    }
    fn stream(m: &[Value]) -> String {
        m.iter().map(|v| format!("{v}\n")).collect()
    }
    #[test]
    fn native_observed_text_protocol_retains_identity_and_only_valid_role_data() {
        let p = parse(FIXTURE, Role::Qualification).unwrap();
        assert_eq!(p.session, "claude:00000000-0000-4000-8000-000000000001");
        assert_eq!(p.actual_model.as_deref(), Some("watchhouse-fixture"));
        assert_eq!(p.usage.input_tokens, Some(10));
        assert_eq!(p.usage.source, "harness-reported");
        for (role, body) in [
            (
                Role::Engineer,
                serde_json::json!({"schema_version":1,"summary":"Exact fixture repair","changes":[{"path":"src/fix.rs","before_digest":null,"after":"fixed"}]}),
            ),
            (
                Role::Reviewer,
                serde_json::json!({"schema_version":1,"approve":true,"findings":"Independent fixture review"}),
            ),
        ] {
            let mut m = messages();
            let text = serde_json::to_string(&body).unwrap();
            m[2]["message"]["content"][0]["text"] = Value::String(text.clone());
            m[3]["result"] = Value::String(text);
            assert!(parse(&stream(&m), role).is_ok());
        }
    }
    #[test]
    fn altered_capabilities_sessions_results_and_forged_schema_are_rejected() {
        let variants: Vec<fn(&mut Vec<Value>)> = vec![
            |m| m[1]["tools"] = serde_json::json!(["Bash"]),
            |m| m[1]["mcp_servers"] = serde_json::json!([{"name":"foreign"}]),
            |m| m[1]["skills"] = serde_json::json!(["foreign"]),
            |m| m[1]["plugins"][0]["path"] = serde_json::json!("/foreign"),
            |m| m[1]["agents"] = serde_json::json!(["custom"]),
            |m| m[1]["claude_code_version"] = serde_json::json!("unqualified"),
            |m| m[2]["message"]["content"][0]["type"] = serde_json::json!("tool_use"),
            |m| m[2]["session_id"] = serde_json::json!("00000000-0000-4000-8000-000000000002"),
            |m| m[3]["permission_denials"] = serde_json::json!([{"tool":"Bash"}]),
            |m| m[3]["subagent_stats"]["spawned"] = serde_json::json!(1),
            |m| {
                m[3]["result"] = serde_json::json!("{\"schema_version\":1,\"nonce\":\"rewritten\"}")
            },
            |m| {
                m.pop();
            },
            |m| m.push(m[3].clone()),
        ];
        for alter in variants {
            let mut m = messages();
            alter(&mut m);
            assert!(parse(&stream(&m), Role::Qualification).is_err());
        }
        let mut m = messages();
        let text=serde_json::json!({"schema_version":1,"summary":"escape","changes":[{"path":"../escape","before_digest":null,"after":"no"}]}).to_string();
        m[2]["message"]["content"][0]["text"] = Value::String(text.clone());
        m[3]["result"] = Value::String(text);
        assert!(parse(&stream(&m), Role::Engineer).is_err());
    }
}
