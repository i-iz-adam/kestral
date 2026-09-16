use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::OmniRouteConfig;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn get_http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Retry policy for the model call itself. There's already 429-handling
/// for web_search/web_fetch and a 401/403-retry-without-auth for the main
/// request (see fetch_endpoint), but nothing covered a plain transient
/// failure — a network blip, a 5xx, a rate limit — on the chat completion
/// call, which for a long-running multi-hour session is long enough to hit
/// eventually and, uncovered, kills the whole turn over what's usually a
/// momentary problem. Retries only ever happen before anything from the
/// response has been used (no success status yet for chat_completion; no
/// delta emitted yet for chat_completion_stream) — once real content is in
/// play, retrying would mean silently resending the request and risking
/// duplicated/confusing output, so at that point an error is just an error.
const MAX_RETRIES: u32 = 3;

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_millis(500 * 2u64.saturating_pow(attempt))
}

/// Returns true if the role is a valid OpenAI/OmniRoute LLM completion role.
/// Used to filter out internal UI/session metadata roles (like `skill-loaded`)
/// before building HTTP payloads.
pub fn is_valid_llm_role(role: &str) -> bool {
    matches!(role, "system" | "user" | "assistant" | "tool" | "function")
}

/// Formats `ChatMessage` structs into OpenAI-compatible JSON message objects.
/// Filters out non-standard roles and constructs vision `image_url` parts
/// when vision is supported, or appends fallback text when vision is disabled.
pub fn format_messages_for_llm(messages: &[ChatMessage], has_vision: bool) -> Vec<Value> {
    messages
        .iter()
        .filter(|m| is_valid_llm_role(&m.role))
        .map(|m| {
            if has_vision && m.images.as_ref().map_or(false, |imgs| !imgs.is_empty()) {
                let mut content_parts: Vec<Value> = Vec::new();
                if let Some(text) = &m.content {
                    if !text.is_empty() {
                        content_parts.push(serde_json::json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }
                if let Some(imgs) = &m.images {
                    for img in imgs {
                        content_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {
                                "url": img
                            }
                        }));
                    }
                }
                let mut obj = serde_json::to_value(m).unwrap_or_default();
                if let Some(map) = obj.as_object_mut() {
                    map.insert("content".to_string(), Value::Array(content_parts));
                    map.remove("images");
                }
                obj
            } else {
                let mut obj = serde_json::to_value(m).unwrap_or_default();
                if let Some(map) = obj.as_object_mut() {
                    if !has_vision && m.images.as_ref().map_or(false, |imgs| !imgs.is_empty()) {
                        let text = m.content.as_deref().unwrap_or("");
                        let fallback = if text.is_empty() {
                            "[Attached image(s) - note: current model does not support vision]".to_string()
                        } else {
                            format!("{}\n\n[Attached image(s) - note: current model does not support vision]", text)
                        };
                        map.insert("content".to_string(), Value::String(fallback));
                    }
                    map.remove("images");
                }
                obj
            }
        })
        .collect()
}

/// OpenAI-compatible chat message. `content` is optional because an
/// assistant message that only carries tool_calls has no text content.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
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
        let client = get_http_client();
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

pub async fn supports_vision(cfg: &OmniRouteConfig, model: &str) -> bool {
    if let Ok(base) = base_url(cfg) {
        let url = format!("{}/v1/models", base);
        let client = get_http_client();
        let mut req = client.get(&url);
        if let Some(key) = &cfg.api_key {
            if !key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<Value>().await {
                    let models_array = json
                        .get("data")
                        .or_else(|| json.get("models"))
                        .and_then(|m| m.as_array());

                    if let Some(models) = models_array {
                        for m in models {
                            let id = m.get("id").and_then(|s| s.as_str()).unwrap_or_default();
                            if id == model {
                                if let Some(caps) = m.get("capabilities").or_else(|| m.get("supports")) {
                                    if let Some(v) = caps.get("vision").or_else(|| caps.get("multimodal")) {
                                        if let Some(b) = v.as_bool() {
                                            return b;
                                        }
                                    }
                                }
                                if let Some(multimodal) = m.get("multimodal").and_then(|v| v.as_bool()) {
                                    return multimodal;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let lower = model.to_lowercase();
    if lower == "auto" || lower == "auto/vision" || lower == "auto/multimodal" {
        return true;
    }
    lower.contains("vision")
        || lower.contains("vl")
        || lower.contains("gpt-4o")
        || lower.contains("claude-3")
        || lower.contains("gemini")
        || lower.contains("llava")
        || lower.contains("qwen-vl")
        || lower.contains("pixtral")
}

/// Fetches the model catalog for the picker in Settings. Tries the
/// alias-expanded listing first (so OmniRoute's curated `auto/*` routes
/// show up alongside concrete provider models, same as the Providers
/// dashboard does), and falls back to the plain listing for older
/// OmniRoute builds that don't recognize the `prefix` query param.
pub async fn list_models(cfg: &OmniRouteConfig) -> Result<Vec<crate::config::ModelInfo>, String> {
    let with_aliases = fetch_endpoint(cfg, "/v1/models?prefix=alias", None, None).await;
    let raw = match with_aliases {
        Ok(v) => v,
        Err(_) => fetch_endpoint(cfg, "/v1/models", None, None).await?,
    };

    let list = raw
        .get("data")
        .or_else(|| raw.get("models"))
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| raw.as_array().cloned())
        .ok_or_else(|| "OmniRoute /v1/models returned an unexpected shape".to_string())?;

    let mut models: Vec<crate::config::ModelInfo> = list
        .iter()
        .filter_map(|m| {
            if let Some(id) = m.as_str() {
                return Some(crate::config::ModelInfo {
                    id: id.to_string(),
                    owned_by: None,
                    context_length: None,
                });
            }
            let obj = m.as_object()?;
            let id = obj
                .get("id")
                .or_else(|| obj.get("name"))
                .and_then(|v| v.as_str())?
                .to_string();
            let owned_by = obj
                .get("owned_by")
                .or_else(|| obj.get("provider"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| id.split('/').next().map(|s| s.to_string()).filter(|_| id.contains('/')));
            let context_length = obj
                .get("context_length")
                .or_else(|| obj.get("context_window"))
                .and_then(|v| v.as_u64());
            Some(crate::config::ModelInfo { id, owned_by, context_length })
        })
        .collect();

    models.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    models.dedup_by(|a, b| a.id == b.id);

    if models.is_empty() {
        return Err("OmniRoute returned no models".to_string());
    }

    Ok(models)
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
            .and_then(|c| c.get("delta").or_else(|| c.get("message")));
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

    let has_vision = supports_vision(cfg, model).await;
    let formatted_messages = format_messages_for_llm(messages, has_vision);

    let mut body = serde_json::json!({
        "model": model,
        "messages": formatted_messages,
        "stream": false,
    });
    if let Some(t) = tools {
        body["tools"] = t.clone();
    }

    let mut attempt = 0u32;
    loop {
        let client = get_http_client();
        let mut req = client.post(&url).json(&body);
        if let Some(key) = &cfg.api_key {
            if !key.is_empty() {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(backoff_delay(attempt)).await;
                    attempt += 1;
                    continue;
                }
                return Err(format!("Failed to connect to OmniRoute: {}", e));
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            if is_retryable_status(status.as_u16()) && attempt < MAX_RETRIES {
                tokio::time::sleep(backoff_delay(attempt)).await;
                attempt += 1;
                continue;
            }
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
        return serde_json::from_value(message.clone()).map_err(|e| e.to_string());
    }
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

    let has_vision = supports_vision(cfg, model).await;
    let formatted_messages = format_messages_for_llm(messages, has_vision);

    let mut body = serde_json::json!({
        "model": model,
        "messages": formatted_messages,
        "stream": true,
    });
    if let Some(t) = tools {
        body["tools"] = t.clone();
    }

    let resp = {
        let mut attempt = 0u32;
        loop {
            let client = get_http_client();
            let mut req = client.post(&url).json(&body);
            if let Some(key) = &cfg.api_key {
                if !key.is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }
            }

            match req.send().await {
                Ok(r) if r.status().is_success() => break r,
                Ok(r) => {
                    let status = r.status();
                    if is_retryable_status(status.as_u16()) && attempt < MAX_RETRIES {
                        tokio::time::sleep(backoff_delay(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    let text = r.text().await.unwrap_or_default();
                    return Err(format!("OmniRoute returned {}: {}", status, text));
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(backoff_delay(attempt)).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(format!("Failed to connect to OmniRoute: {}", e));
                }
            }
        }
    };

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
                .and_then(|c| c.get("delta").or_else(|| c.get("message")));
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
        let tool_calls = finish_tool_acc(tool_acc);
        if content.is_empty() && tool_calls.is_none() {
            if let Ok(msg) = parse_sse_response(&raw_buf) {
                if msg.content.is_some() || msg.tool_calls.is_some() {
                    if let Some(text) = &msg.content {
                        if !text.is_empty() {
                            on_delta(text);
                        }
                    }
                    return Ok(msg);
                }
            }
            if let Ok(json) = serde_json::from_str::<Value>(&raw_buf) {
                if let Some(choice) = json.get("choices").and_then(|c| c.get(0)) {
                    if let Some(message) = choice.get("message").or_else(|| choice.get("delta")) {
                        if let Ok(msg) = serde_json::from_value::<ChatMessage>(message.clone()) {
                            if let Some(text) = &msg.content {
                                if !text.is_empty() {
                                    on_delta(text);
                                }
                            }
                            return Ok(msg);
                        }
                    }
                }
            }
        }
        return Ok(ChatMessage {
            role: "assistant".into(),
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls,
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

/// One image that came back from `/v1/images/generations`, already
/// materialized as raw bytes regardless of whether the provider answered
/// with inline base64 (`b64_json`) or a URL we had to go and fetch —
/// callers shouldn't have to care which, and several of the providers
/// OmniRoute fronts (OpenAI, xAI, Together/FLUX, Nebius, NanoBanana,
/// local SD WebUI/ComfyUI) disagree about it.
#[derive(Debug, Clone)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    pub mime: String,
    /// Some providers rewrite the prompt before rendering and hand the
    /// rewritten version back; worth surfacing, since it explains why the
    /// result may not match what was asked for word for word.
    pub revised_prompt: Option<String>,
}

/// Sniffs the container from the file's magic bytes rather than trusting
/// a `Content-Type` (URL responses) or guessing PNG (base64 payloads,
/// which carry no type at all) — this is what the saved file extension
/// and the data: URL the UI renders are both derived from, so getting it
/// wrong shows up as a broken image in the chat.
fn sniff_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "image/svg+xml"
    } else {
        "image/png"
    }
}

pub fn mime_extension(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        _ => "png",
    }
}

/// Image models advertised by this OmniRoute install, newest listing
/// first. Used to pick a sensible default when neither the caller nor the
/// saved config names one, so image generation works on a fresh install
/// without a settings trip. `GET /v1/images/generations` is the documented
/// listing endpoint; `/v1/models` is the fallback for older builds that
/// only expose the combined catalog.
pub async fn list_image_models(cfg: &OmniRouteConfig) -> Result<Vec<String>, String> {
    let raw = match fetch_endpoint(cfg, "/v1/images/generations", None, None).await {
        Ok(v) => v,
        Err(_) => fetch_endpoint(cfg, "/v1/models", None, None).await?,
    };

    let list = raw
        .get("data")
        .or_else(|| raw.get("models"))
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| raw.as_array().cloned())
        .unwrap_or_default();

    let mut ids: Vec<String> = list
        .iter()
        .filter_map(|m| {
            if let Some(id) = m.as_str() {
                return Some(id.to_string());
            }
            let obj = m.as_object()?;
            // When this came from the combined /v1/models catalog, keep
            // only the image entries — a chat model id sent to
            // /v1/images/generations is just a 400 later on.
            if let Some(kind) = obj
                .get("type")
                .or_else(|| obj.get("modality"))
                .or_else(|| obj.get("model_type"))
                .and_then(|v| v.as_str())
            {
                if !kind.to_lowercase().contains("image") {
                    return None;
                }
            }
            obj.get("id")
                .or_else(|| obj.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    ids.dedup();
    if ids.is_empty() {
        return Err("OmniRoute returned no image models — add an image provider (OpenAI, xAI, Together, Nebius, NanoBanana, SD WebUI, ComfyUI, …) in the Providers dashboard".to_string());
    }
    Ok(ids)
}

/// Picks the model to render with: an explicit request wins, then the
/// saved default, then whatever the install actually has, then the
/// documented example id as a last resort so the call produces a real
/// provider error message rather than a local "nothing configured".
pub async fn resolve_image_model(cfg: &OmniRouteConfig, requested: Option<&str>) -> String {
    if let Some(m) = requested.map(str::trim).filter(|s| !s.is_empty()) {
        return m.to_string();
    }
    if let Some(m) = cfg
        .default_image_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return m.to_string();
    }
    if let Ok(models) = list_image_models(cfg).await {
        if let Some(first) = models.into_iter().next() {
            return first;
        }
    }
    "openai/gpt-image-2".to_string()
}

async fn fetch_image_url(url: &str) -> Result<Vec<u8>, String> {
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("failed to download generated image: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!(
            "failed to download generated image: provider returned {}",
            resp.status()
        ));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("failed to read generated image body: {}", e))
}

fn decode_b64_image(raw: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    // Providers occasionally hand back a full data: URL in the b64_json
    // slot; strip the prefix rather than failing to decode it.
    let payload = raw
        .split_once(";base64,")
        .map(|(_, tail)| tail)
        .unwrap_or(raw)
        .trim();
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("provider returned base64 that wouldn't decode: {}", e))
}

/// Renders `prompt` via `POST /v1/images/generations`. `size` is passed
/// through untouched (providers accept different sets — 1024x1024 is the
/// safe common denominator), and `n` is how many variations to ask for.
///
/// Deliberately not retried the way chat completions are: image calls are
/// slow and metered per image, so a silent retry risks paying twice for a
/// request that may well have succeeded upstream. A failure comes straight
/// back with the provider's own message so the model can adjust the prompt
/// or the caller can pick a different provider.
pub async fn generate_image(
    cfg: &OmniRouteConfig,
    model: &str,
    prompt: &str,
    size: Option<&str>,
    n: Option<u64>,
    quality: Option<&str>,
    style: Option<&str>,
) -> Result<Vec<GeneratedImage>, String> {
    let base = base_url(cfg)?;
    let url = format!("{}/v1/images/generations", base);

    let mut body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "size": size.unwrap_or("1024x1024"),
        // Ask for inline base64 where the provider honors it — it saves a
        // second round trip, and several providers' hosted URLs expire
        // quickly enough that a slow save could miss them.
        "response_format": "b64_json",
    });
    if let Some(count) = n.filter(|c| *c > 1) {
        body["n"] = serde_json::json!(count);
    }
    if let Some(q) = quality.map(str::trim).filter(|s| !s.is_empty()) {
        body["quality"] = serde_json::json!(q);
    }
    if let Some(s) = style.map(str::trim).filter(|s| !s.is_empty()) {
        body["style"] = serde_json::json!(s);
    }

    let client = reqwest::Client::builder()
        // A cold local SD WebUI/ComfyUI render can genuinely take minutes;
        // reqwest's default has no timeout at all, which is worse — this
        // bounds it without cutting off a legitimately slow provider.
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(&body);
    if let Some(key) = &cfg.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("failed to connect to OmniRoute image endpoint: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        let hint = match status.as_u16() {
            404 => "\nNote: /v1/images/generations wasn't found — make sure OmniRoute is running and up to date.",
            400 | 422 => "\nNote: the model id may not be an image model, or the requested size isn't one this provider accepts.",
            401 | 403 => "\nNote: check the OmniRoute API key saved in Settings.",
            429 => "\nNote: the image provider is rate limited or out of quota — try another provider in the Providers dashboard.",
            _ => "",
        };
        return Err(format!(
            "image generation failed (HTTP {}) on model {}: {}{}",
            status, model, text, hint
        ));
    }

    let json: Value = serde_json::from_str(&text).map_err(|e| {
        let preview: String = text.chars().take(300).collect();
        format!("could not decode image response: {} (preview: {:?})", e, preview)
    })?;

    let entries = json
        .get("data")
        .or_else(|| json.get("images"))
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| json.as_array().cloned())
        .ok_or_else(|| {
            let preview: String = text.chars().take(300).collect();
            format!("image response had no data array (preview: {:?})", preview)
        })?;

    let mut out: Vec<GeneratedImage> = Vec::new();
    for entry in &entries {
        let revised_prompt = entry
            .get("revised_prompt")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // An entry can be a bare base64/URL string, or an object keyed by
        // any of several names depending on provider.
        let inline = entry
            .get("b64_json")
            .or_else(|| entry.get("b64"))
            .or_else(|| entry.get("image_base64"))
            .or_else(|| entry.get("base64"))
            .and_then(|v| v.as_str());
        let link = entry
            .get("url")
            .or_else(|| entry.get("image_url"))
            .and_then(|v| v.as_str())
            .or_else(|| entry.as_str().filter(|s| s.starts_with("http")));

        let bytes = if let Some(b64) = inline.or_else(|| entry.as_str().filter(|s| !s.starts_with("http"))) {
            decode_b64_image(b64)?
        } else if let Some(link) = link {
            fetch_image_url(link).await?
        } else {
            continue;
        };

        if bytes.is_empty() {
            continue;
        }
        let mime = sniff_image_mime(&bytes).to_string();
        out.push(GeneratedImage { bytes, mime, revised_prompt });
    }

    if out.is_empty() {
        return Err(format!(
            "model {} returned a response with no usable image data",
            model
        ));
    }
    Ok(out)
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

    let client = get_http_client();
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

    let client = get_http_client();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_llm_role() {
        assert!(is_valid_llm_role("system"));
        assert!(is_valid_llm_role("user"));
        assert!(is_valid_llm_role("assistant"));
        assert!(is_valid_llm_role("tool"));
        assert!(is_valid_llm_role("function"));

        assert!(!is_valid_llm_role("skill-loaded"));
        assert!(!is_valid_llm_role("custom-role"));
    }

    #[test]
    fn test_format_messages_for_llm_filters_custom_roles() {
        let msgs = vec![
            ChatMessage {
                role: "system".into(),
                content: Some("System prompt".into()),
                ..Default::default()
            },
            ChatMessage {
                role: "user".into(),
                content: Some("Help me with image".into()),
                images: Some(vec!["data:image/png;base64,abc".into()]),
                ..Default::default()
            },
            ChatMessage {
                role: "skill-loaded".into(),
                content: Some(r#"{"call_id":"123","name":"__skill_loaded__"}"#.into()),
                ..Default::default()
            },
            ChatMessage {
                role: "assistant".into(),
                content: Some("Here is the answer".into()),
                ..Default::default()
            },
        ];

        let formatted = format_messages_for_llm(&msgs, true);
        assert_eq!(formatted.len(), 3);
        assert_eq!(formatted[0]["role"], "system");
        assert_eq!(formatted[1]["role"], "user");
        assert_eq!(formatted[2]["role"], "assistant");

        // Verify image conversion for vision model
        let user_content = &formatted[1]["content"];
        assert!(user_content.is_array());
        let parts = user_content.as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "Help me with image");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,abc");
    }

    #[test]
    fn test_format_messages_for_llm_non_vision_fallback() {
        let msgs = vec![
            ChatMessage {
                role: "user".into(),
                content: Some("Look at this".into()),
                images: Some(vec!["data:image/png;base64,abc".into()]),
                ..Default::default()
            },
        ];

        let formatted = format_messages_for_llm(&msgs, false);
        assert_eq!(formatted.len(), 1);
        assert_eq!(formatted[0]["role"], "user");
        assert!(formatted[0]["content"].as_str().unwrap().contains("does not support vision"));
    }

    #[tokio::test]
    async fn test_supports_vision() {
        let cfg = OmniRouteConfig::default();
        assert!(supports_vision(&cfg, "auto").await);
        assert!(supports_vision(&cfg, "auto/vision").await);
        assert!(supports_vision(&cfg, "auto/multimodal").await);
        assert!(supports_vision(&cfg, "claude-3-5-sonnet").await);
        assert!(supports_vision(&cfg, "gpt-4o").await);

        assert!(!supports_vision(&cfg, "auto/coding").await);
        assert!(!supports_vision(&cfg, "auto/fast").await);
        assert!(!supports_vision(&cfg, "deepseek-coder").await);
    }
}