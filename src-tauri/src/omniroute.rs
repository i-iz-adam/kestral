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

/// Makes an HTTP request to any OmniRoute endpoint with authentication and base URL resolution handled.
pub async fn fetch_endpoint(
    cfg: &OmniRouteConfig,
    endpoint: &str,
    method: Option<&str>,
    body: Option<&Value>,
) -> Result<Value, String> {
    let base = base_url(cfg)?;
    let clean_endpoint = if endpoint.starts_with('/') {
        endpoint.to_string()
    } else {
        format!("/{}", endpoint)
    };
    let url = format!("{}{}", base, clean_endpoint);

    let method_str = method.unwrap_or("GET").to_uppercase();

    let build_request = |with_auth: bool| {
        let client = reqwest::Client::new();
        let mut req = match method_str.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "DELETE" => client.delete(&url),
            _ => client.get(&url),
        };

        if with_auth {
            if let Some(key) = &cfg.api_key {
                if !key.is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }
            }
        }

        if let Some(b) = body {
            req = req.json(b);
        }
        req
    };

    let mut resp = build_request(true)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to OmniRoute endpoint {}: {}", clean_endpoint, e))?;

    // If 401 Unauthorized or 403 Forbidden, retry without Auth header
    // (management / public endpoints may reject inference Bearer keys)
    if (resp.status().as_u16() == 401 || resp.status().as_u16() == 403) && cfg.api_key.is_some() {
        if let Ok(retry_resp) = build_request(false).send().await {
            if retry_resp.status().is_success() {
                resp = retry_resp;
            }
        }
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("OmniRoute returned {}: {}", status, text));
    }

    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }

    serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({ "text": text })))
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

pub async fn web_search(
    cfg: &OmniRouteConfig,
    query: &str,
    provider: Option<&str>,
    limit: Option<usize>,
) -> Result<String, String> {
    let base = base_url(cfg)?;
    let url = format!("{}/v1/search", base);

    let mut body = serde_json::json!({
        "query": query,
    });
    if let Some(p) = provider {
        if !p.trim().is_empty() {
            body["provider"] = serde_json::json!(p.trim());
        }
    }
    if let Some(l) = limit {
        body["limit"] = serde_json::json!(l);
    }

    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(key) = &cfg.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return Ok(format!("Failed to connect to OmniRoute search endpoint: {}", e)),
    };

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        if status.as_u16() == 429 {
            return Ok("Web search rate limit / quota exceeded (HTTP 429). All search providers in OmniRoute pool (Tavily, Brave, Exa, Serper, etc.) may be rate limited or out of quota.".to_string());
        }
        if status.as_u16() == 404 {
            return Ok("Web search failed (HTTP 404): The endpoint /v1/search was not found on your OmniRoute server. Ensure OmniRoute is running and up to date.".to_string());
        }
        let hint = if text.contains("provider") || text.contains("configured") || status.as_u16() == 400 || status.as_u16() == 500 {
            "\nNote: Please ensure at least one search provider (such as Tavily, Brave, Exa, Serper, etc.) is configured in your OmniRoute providers dashboard."
        } else {
            ""
        };
        return Ok(format!("Web search error (HTTP {}): {}{}", status, text, hint));
    }

    if text.trim().is_empty() {
        return Ok("Web search returned success but an empty response body.".to_string());
    }

    if let Ok(v) = serde_json::from_str::<Value>(&text) {
        let results = v.get("results")
            .or_else(|| v.get("data"))
            .and_then(|r| r.as_array())
            .or_else(|| v.as_array());

        if let Some(items) = results {
            if items.is_empty() {
                return Ok(format!("No search results found for query: {:?}", query));
            }
            let mut formatted = Vec::new();
            for (idx, item) in items.iter().enumerate() {
                if let Some(obj) = item.as_object() {
                    let title = obj.get("title").and_then(|t| t.as_str()).unwrap_or("Untitled");
                    let link = obj.get("url").or_else(|| obj.get("link")).and_then(|u| u.as_str()).unwrap_or("");
                    let snippet = obj.get("snippet")
                        .or_else(|| obj.get("content"))
                        .or_else(|| obj.get("description"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("");

                    if !link.is_empty() {
                        formatted.push(format!("{}. [{}]({})\n{}", idx + 1, title, link, snippet));
                    } else {
                        formatted.push(format!("{}. {}\n{}", idx + 1, title, snippet));
                    }
                } else if let Some(s) = item.as_str() {
                    formatted.push(format!("{}. {}", idx + 1, s));
                }
            }
            if !formatted.is_empty() {
                return Ok(formatted.join("\n\n"));
            }
        }
        return Ok(text);
    }

    Ok(text)
}

pub async fn web_fetch(
    cfg: &OmniRouteConfig,
    url_str: &str,
    provider: Option<&str>,
) -> Result<String, String> {
    let base = base_url(cfg)?;
    let endpoint = format!("{}/v1/web/fetch", base);

    let mut body = serde_json::json!({
        "url": url_str,
    });
    if let Some(p) = provider {
        if !p.trim().is_empty() {
            body["provider"] = serde_json::json!(p.trim());
        }
    }

    let client = reqwest::Client::new();
    let mut req = client.post(&endpoint).json(&body);
    if let Some(key) = &cfg.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return Ok(format!("Failed to connect to OmniRoute web fetch endpoint: {}", e)),
    };

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        if status.as_u16() == 429 {
            return Ok("Web fetch rate limit / quota exceeded (HTTP 429). All web fetch providers in OmniRoute pool (Firecrawl, Jina Reader, Tavily Extract, TinyFish Fetch) may be rate limited or out of quota.".to_string());
        }
        if status.as_u16() == 404 {
            return Ok("Web fetch failed (HTTP 404): The endpoint /v1/web/fetch was not found on your OmniRoute server. Ensure OmniRoute is running and up to date.".to_string());
        }
        let hint = if text.contains("provider") || text.contains("configured") || status.as_u16() == 400 || status.as_u16() == 500 {
            "\nNote: Please ensure at least one web-fetch provider (such as Firecrawl, Jina Reader, Tavily Extract, TinyFish Fetch) is configured in your OmniRoute providers dashboard."
        } else {
            ""
        };
        return Ok(format!("Web fetch error (HTTP {}): {}{}", status, text, hint));
    }

    if text.trim().is_empty() {
        return Ok("Web fetch returned success but an empty response body.".to_string());
    }

    if let Ok(v) = serde_json::from_str::<Value>(&text) {
        let content = v.get("markdown")
            .or_else(|| v.get("content"))
            .or_else(|| v.get("text"))
            .or_else(|| v.get("data").and_then(|d| d.get("markdown").or_else(|| d.get("content"))))
            .and_then(|c| c.as_str());

        let title = v.get("title")
            .or_else(|| v.get("data").and_then(|d| d.get("title")))
            .and_then(|t| t.as_str());

        let result_str = match (title, content) {
            (Some(t), Some(c)) => format!("# {}\n\n{}", t, c),
            (None, Some(c)) => c.to_string(),
            (Some(t), None) => format!("# {}\n\n{}", t, text),
            (None, None) => text.clone(),
        };

        const MAX_LEN: usize = 50_000;
        if result_str.len() > MAX_LEN {
            let truncated: String = result_str.chars().take(MAX_LEN).collect();
            return Ok(format!("{}\n\n[Content truncated at 50,000 characters]", truncated));
        }
        return Ok(result_str);
    }

    const MAX_LEN: usize = 50_000;
    if text.len() > MAX_LEN {
        let truncated: String = text.chars().take(MAX_LEN).collect();
        return Ok(format!("{}\n\n[Content truncated at 50,000 characters]", truncated));
    }
    Ok(text)
}
