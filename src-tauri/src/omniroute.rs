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

/// Parses an SSE `text/event-stream` body (`data: {...}` lines) into a
/// single assistant message by concatenating `delta.content` chunks and
/// reassembling chunked `delta.tool_calls` (arguments arrive fragmented).
fn parse_sse_response(text: &str) -> Result<ChatMessage, String> {
    let mut content = String::new();
    // (id, type, name, arguments) per tool-call index.
    let mut tool_acc: Vec<(String, String, String, String)> = Vec::new();
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
        }
    }

    if !saw_chunk {
        return Err("OmniRoute returned an SSE body with no data: chunks".to_string());
    }

    let tool_calls = if tool_acc.iter().any(|(_, _, n, _)| !n.is_empty()) {
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
    } else {
        None
    };

    Ok(ChatMessage {
        role: "assistant".into(),
        content: if content.is_empty() { None } else { Some(content) },
        tool_calls,
        ..Default::default()
    })
}

/// Sends one chat completion request (non-streaming — token-level streaming
/// is a good next step but round-level tool progress already streams via
/// Tauri events, see agent.rs). Returns the assistant message, which may
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
