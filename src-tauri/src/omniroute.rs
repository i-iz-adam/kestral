use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::OmniRouteConfig;

/// OpenAI-compatible chat message. `content` is optional because an
/// assistant message that only carries tool_calls has no text content.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

fn base_url(cfg: &OmniRouteConfig) -> Result<String, String> {
    match cfg.mode.as_str() {
        "local" => Ok("http://127.0.0.1:20128".to_string()),
        _ => cfg
            .remote_url
            .clone()
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_end_matches('/').to_string())
            .ok_or_else(|| "No remote OmniRoute URL configured".to_string()),
    }
}

/// (id, type, name, arguments) per tool-call index — shared accumulator
/// shape between the whole-body SSE parser and the incremental streaming
/// parser, since both reassemble the same fragmented `delta.tool_calls`.
type ToolAcc = Vec<(String, String, String, String)>;

/// Folds one `delta.tool_calls` array (from a single SSE chunk) into the
/// running accumulator. Arguments arrive as string fragments across many
/// chunks, so this only ever appends to `entry.3`, never replaces it.
fn accumulate_tool_call_delta(tool_acc: &mut ToolAcc, calls: &[Value]) {
    for call in calls {
        let index = call.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
        while tool_acc.len() <= index {
            tool_acc.push((String::new(), "function".into(), String::new(), String::new()));
        }
        let entry = &mut tool_acc[index];
        if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
            if !id.is_empty() {
                entry.0 = id.to_string();
            }
        }
        if let Some(t) = call.get("type").and_then(|v| v.as_str()) {
            if !t.is_empty() {
                entry.1 = t.to_string();
            }
        }
        if let Some(name) = call
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
        {
            if !name.is_empty() {
                entry.2 = name.to_string();
            }
        }
        if let Some(args) = call
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|v| v.as_str())
        {
            entry.3.push_str(args);
        }
    }
}

fn finish_tool_acc(tool_acc: ToolAcc) -> Option<Vec<ToolCall>> {
    if !tool_acc.iter().any(|(_, _, n, _)| !n.is_empty()) {
        return None;
    }
    Some(
        tool_acc
            .into_iter()
            .enumerate()
            .filter(|(_, (_, _, name, _))| !name.is_empty())
            .map(|(i, (id, call_type, name, arguments))| ToolCall {
                id: if id.is_empty() { format!("call_{}", i) } else { id },
                call_type,
                function: ToolCallFunction { name, arguments },
            })
            .collect(),
    )
}

/// Parses an SSE `text/event-stream` body (`data: {...}` lines) into a
/// single assistant message by concatenating `delta.content` chunks and
/// reassembling chunked `delta.tool_calls` (arguments arrive fragmented).
fn parse_sse_response(text: &str) -> Result<ChatMessage, String> {
    let mut content = String::new();
    let mut tool_acc: ToolAcc = Vec::new();
    let mut saw_chunk = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with("data:") {
            continue;
        }
        let payload = line["data:".len()..].trim();
        if payload == "[DONE]" {
            break;
        }
        let v: Value = serde_json::from_str(payload)
            .map_err(|e| format!("error decoding SSE chunk: {} (chunk: {:?})", e, payload))?;
        saw_chunk = true;
        let delta = v
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("delta"));
        if let Some(delta) = delta {
            if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
                content.push_str(s);
            }
            if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
                accumulate_tool_call_delta(&mut tool_acc, calls);
            }
        }
    }

    if !saw_chunk {
        return Err("OmniRoute returned an SSE body with no data: chunks".to_string());
    }

    Ok(ChatMessage {
        role: "assistant".into(),
        content: if content.is_empty() { None } else { Some(content) },
        tool_calls: finish_tool_acc(tool_acc),
        ..Default::default()
    })
}

/// Sends one chat completion request, non-streaming. Still used by
/// sub-agents (subagent.rs), whose intermediate text is never shown to the
/// user, so there's nothing to stream. The top-level turn loop
/// (agent.rs::run_turn) uses `chat_completion_stream` instead so assistant
/// text renders token-by-token. Returns the assistant message, which may
/// itself carry tool_calls for the caller to execute.
pub async fn chat_completion(
    cfg: &OmniRouteConfig,
    model: &str,
    messages: &[ChatMessage],
    tools: Option<&Value>,
) -> Result<ChatMessage, String> {
    let base = base_url(cfg)?;
    let url = format!("{}/v1/chat/completions", base);

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false,
    });
    if let Some(t) = tools {
        body["tools"] = t.clone();
    }

    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(key) = &cfg.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OmniRoute returned {}: {}", status, text));
    }

    let text = resp.text().await.map_err(|e| e.to_string())?;
    if text.trim().is_empty() {
        return Err(
            "OmniRoute returned success with an empty response body (expected JSON)".to_string(),
        );
    }
    // OmniRoute may return SSE (`data: {...}` chunks) even when
    // `stream: false` is sent — handle both shapes.
    let trimmed = text.trim_start();
    if trimmed.starts_with("data:") {
        return parse_sse_response(&text);
    }
    let json: Value = serde_json::from_str(&text).map_err(|e| {
        let preview: String = text.chars().take(500).collect();
        format!("error decoding response body: {} (preview: {:?})", e, preview)
    })?;
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or("No choices in OmniRoute response")?;
    let message = choice
        .get("message")
        .ok_or("No message in OmniRoute response choice")?;
    serde_json::from_value(message.clone()).map_err(|e| e.to_string())
}

/// Sends one chat completion request with `stream: true` and invokes
/// `on_delta` with each incremental piece of assistant text as it arrives
/// off the wire, so the caller can forward it to the UI in real time.
/// Still returns the fully assembled `ChatMessage` at the end (content and
/// reassembled tool_calls), same shape as `chat_completion` — callers that
/// don't care about incremental delivery can pass a no-op closure.
///
/// Falls back to treating the body as a single non-streamed JSON response
/// if the server never sends a single parseable `data:` chunk (some
/// OpenAI-compatible backends ignore `stream: true` entirely), so this is
/// safe to use as the default path rather than needing a feature check.
pub async fn chat_completion_stream<F: FnMut(&str)>(
    cfg: &OmniRouteConfig,
    model: &str,
    messages: &[ChatMessage],
    tools: Option<&Value>,
    mut on_delta: F,
) -> Result<ChatMessage, String> {
    let base = base_url(cfg)?;
    let url = format!("{}/v1/chat/completions", base);

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
    });
    if let Some(t) = tools {
        body["tools"] = t.clone();
    }

    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(key) = &cfg.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OmniRoute returned {}: {}", status, text));
    }

    let mut content = String::new();
    let mut tool_acc: ToolAcc = Vec::new();
    let mut saw_chunk = false;
    let mut line_buf = String::new();
    let mut raw_buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(next) = stream.next().await {
        let bytes = next.map_err(|e| e.to_string())?;
        let text_chunk = String::from_utf8_lossy(&bytes);
        line_buf.push_str(&text_chunk);
        raw_buf.push_str(&text_chunk);

        // Process every complete line we have so far; keep any trailing
        // partial line (a chunk boundary can land mid-line) in line_buf.
        while let Some(pos) = line_buf.find('\n') {
            let line = line_buf[..pos].trim_end_matches('\r').trim().to_string();
            line_buf.drain(..=pos);

            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let payload = line["data:".len()..].trim();
            if payload == "[DONE]" {
                continue;
            }
            let v: Value = match serde_json::from_str(payload) {
                Ok(v) => v,
                // Tolerate stray keep-alive or malformed lines rather than
                // failing the whole turn over one bad chunk.
                Err(_) => continue,
            };
            saw_chunk = true;
            let delta = v
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("delta"));
            if let Some(delta) = delta {
                if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
                    if !s.is_empty() {
                        content.push_str(s);
                        on_delta(s);
                    }
                }
                if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
                    accumulate_tool_call_delta(&mut tool_acc, calls);
                }
            }
        }
    }

    if saw_chunk {
        return Ok(ChatMessage {
            role: "assistant".into(),
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls: finish_tool_acc(tool_acc),
            ..Default::default()
        });
    }

    // Nothing parsed as an SSE chunk — this backend likely ignored
    // `stream: true`. Try the accumulated body as a plain JSON response
    // (or, failing that, as a whole-body SSE blob) before giving up, same
    // tolerance chat_completion has for a non-streaming server.
    if raw_buf.trim().is_empty() {
        return Err(
            "OmniRoute returned success with an empty response body (expected JSON or SSE)"
                .to_string(),
        );
    }
    if raw_buf.trim_start().starts_with("data:") {
        let msg = parse_sse_response(&raw_buf)?;
        if let Some(text) = &msg.content {
            if !text.is_empty() {
                on_delta(text);
            }
        }
        return Ok(msg);
    }
    let json: Value = serde_json::from_str(&raw_buf).map_err(|e| {
        let preview: String = raw_buf.chars().take(500).collect();
        format!("error decoding response body: {} (preview: {:?})", e, preview)
    })?;
    let choice = json
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or("No choices in OmniRoute response")?;
    let message = choice
        .get("message")
        .ok_or("No message in OmniRoute response choice")?;
    let msg: ChatMessage = serde_json::from_value(message.clone()).map_err(|e| e.to_string())?;
    if let Some(text) = &msg.content {
        if !text.is_empty() {
            on_delta(text);
        }
    }
    Ok(msg)
}
