//! Credential-free synthetic Responses endpoint used only for native transport qualification.
use crate::{
    error::{Error, Result},
    store,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
#[derive(Debug, Clone, Default)]
pub struct Measurement {
    pub requests: u32,
    pub execution_tools_absent: bool,
    pub tools_digest: Option<String>,
    pub failure: Option<String>,
}
pub async fn serve(
    final_json: Value,
) -> Result<(u16, Arc<Mutex<Measurement>>, tokio::task::JoinHandle<()>)> {
    serve_for(super::policy::Adapter::Codex, final_json).await
}
pub async fn serve_for(
    adapter: super::policy::Adapter,
    final_json: Value,
) -> Result<(u16, Arc<Mutex<Measurement>>, tokio::task::JoinHandle<()>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| Error::Invalid("native fixture endpoint unavailable".into()))?;
    let port = listener
        .local_addr()
        .map_err(|_| Error::Invalid("native fixture address unavailable".into()))?
        .port();
    let m = Arc::new(Mutex::new(Measurement::default()));
    let measured = m.clone();
    let task = tokio::spawn(async move {
        let handler = async {
            for _ in 0..8 {
                let (stream, _) = listener
                    .accept()
                    .await
                    .map_err(|_| Error::Invalid("fixture connection lost".into()))?;
                if response(stream, &final_json, &measured, adapter).await? {
                    return Ok::<_, Error>(());
                }
            }
            Err(Error::Invalid("fixture request budget exceeded".into()))
        };
        match timeout(Duration::from_secs(600), handler).await {
            Ok(Ok(())) => {}
            _ => measured.lock().unwrap().failure = Some("fixture_protocol_failed".into()),
        }
    });
    Ok((port, m, task))
}
async fn response(
    mut stream: TcpStream,
    final_json: &Value,
    m: &Arc<Mutex<Measurement>>,
    adapter: super::policy::Adapter,
) -> Result<bool> {
    let work = async {
        let mut bytes = vec![];
        let boundary = loop {
            if bytes.len() > 16384 {
                return Err(Error::Invalid("fixture HTTP headers exceed bound".into()));
            }
            let mut buf = [0; 4096];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|_| Error::Invalid("fixture HTTP input lost".into()))?;
            if n == 0 {
                return Err(Error::Invalid("fixture HTTP headers incomplete".into()));
            }
            bytes.extend_from_slice(&buf[..n]);
            if let Some(n) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break n;
            }
        };
        let header = String::from_utf8(bytes[..boundary].to_vec())
            .map_err(|_| Error::Invalid("invalid fixture HTTP encoding".into()))?;
        let mut fields = header.lines();
        let first = fields.next().unwrap_or("");
        let fields: Vec<_> = fields.collect();
        let claude = adapter == super::policy::Adapter::ClaudeCode;
        let expected = if claude {
            first.starts_with("POST /v1/messages ")
                || first.starts_with("POST /v1/messages?beta=true ")
        } else {
            first.starts_with("POST /v1/responses ")
        };
        if !expected {
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .map_err(|_| Error::Invalid("fixture HTTP close failed".into()))?;
            return Ok(false);
        }
        if fields
            .iter()
            .any(|s| s.to_ascii_lowercase().starts_with("authorization:"))
        {
            return Err(Error::Invalid(
                "native fixture received credentials; profile denied".into(),
            ));
        }
        if claude
            && !fields.iter().any(|s| {
                s.split_once(':').is_some_and(|(k, v)| {
                    k.eq_ignore_ascii_case("x-api-key")
                        && v.trim() == "watchhouse-synthetic-claude-not-a-credential"
                })
            })
        {
            return Err(Error::Invalid(
                "fixture must use only the synthetic API marker".into(),
            ));
        }
        let count = fields
            .iter()
            .find_map(|s| {
                s.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.trim().parse::<usize>().ok())
            })
            .filter(|n| *n <= 2 * 1024 * 1024)
            .ok_or_else(|| Error::Invalid("fixture body length missing/exceeds bound".into()))?;
        while bytes.len() < boundary + 4 + count {
            let mut buf = [0; 4096];
            let n = stream
                .read(&mut buf)
                .await
                .map_err(|_| Error::Invalid("fixture body lost".into()))?;
            if n == 0 {
                return Err(Error::Invalid("fixture body incomplete".into()));
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let body: Value = serde_json::from_slice(&bytes[boundary + 4..boundary + 4 + count])?;
        if claude
            && (!body["messages"]
                .as_array()
                .is_some_and(|m| m.iter().any(|m| m["role"] == "user"))
                || final_json["nonce"]
                    .as_str()
                    .is_some_and(|n| !body["messages"].to_string().contains(n)))
        {
            return Err(Error::Invalid(
                "native client did not forward the sealed qualification briefing".into(),
            ));
        }
        let tools_value = if claude && body.get("tools").is_none() {
            json!([])
        } else {
            body["tools"].clone()
        };
        let tools = tools_value
            .as_array()
            .ok_or_else(|| Error::Invalid("native tool advertisement missing".into()))?;
        // The one inert question tool is not execution authority. Any new/file/shell/network/
        // provider-hosted tool fails qualification. Actual tool/interaction events also fail role parsing.
        let allowed = if claude {
            tools.is_empty()
        } else {
            tools.len() <= 1
                && tools
                    .iter()
                    .all(|t| t["type"] == "function" && t["name"] == "request_user_input")
        };
        {
            let mut m = m.lock().unwrap();
            m.requests += 1;
            m.execution_tools_absent = allowed;
            m.tools_digest = Some(store::digest(&serde_json::to_string(tools)?));
        }
        if !allowed {
            return Err(Error::Invalid(
                "native client advertises an unapproved execution tool".into(),
            ));
        }
        let item = json!({"id":"msg_watchhouse_fixture","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":serde_json::to_string(final_json)?}]});
        let events: Vec<Value> = if claude {
            vec![
                json!({"type":"message_start","message":{"id":"msg_watchhouse_fixture","type":"message","role":"assistant","model":body["model"],"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
                json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":serde_json::to_string(final_json)?}}),
                json!({"type":"content_block_stop","index":0}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":10}}),
                json!({"type":"message_stop"}),
            ]
        } else {
            vec![
                json!({"type":"response.created","response":{"id":"resp_watchhouse_fixture","model":body["model"]}}),
                json!({"type":"response.output_item.added","output_index":0,"item":item}),
                json!({"type":"response.output_item.done","output_index":0,"item":item}),
                json!({"type":"response.completed","response":{"id":"resp_watchhouse_fixture","model":body["model"],"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":10,"total_tokens":20}}}),
            ]
        };
        let response = events
            .iter()
            .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().unwrap(), e))
            .collect::<String>();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.map_err(|_|Error::Invalid("fixture SSE output lost".into()))?;
        Ok(true)
    };
    timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| Error::Invalid("fixture HTTP connection timed out".into()))?
}
