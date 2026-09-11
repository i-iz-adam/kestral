use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GithubConfig {
    pub token: Option<String>,
}

fn config_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir");
    fs::create_dir_all(&dir).ok();
    dir.join("github_config.json")
}

pub fn save_token(app_handle: &tauri::AppHandle, token: &str) -> std::io::Result<()> {
    let cfg = GithubConfig { token: Some(token.to_string()) };
    fs::write(config_path(app_handle), serde_json::to_string_pretty(&cfg)?)
}

pub fn load_token(app_handle: &tauri::AppHandle) -> Option<String> {
    let data = fs::read_to_string(config_path(app_handle)).ok()?;
    let cfg: GithubConfig = serde_json::from_str(&data).ok()?;
    cfg.token
}

/// Verifies a token against GitHub's /user endpoint and returns the login
/// on success, so the UI can show "Connected as X" rather than just "ok".
pub async fn test_token(token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "kestrel")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GitHub returned {}", resp.status()));
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(json.get("login").and_then(|v| v.as_str()).unwrap_or("unknown").to_string())
}

/// The tool schema exposed to the model. Every tool takes optional
/// owner/repo — if omitted, `execute` falls back to the session's linked
/// repo (see resolve_repo below).
pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "github_list_issues",
                "description": "List open issues for a repo. Omit owner/repo to use the repo linked to this session.",
                "parameters": {
                    "type": "object",
                    "properties": { "owner": {"type": "string"}, "repo": {"type": "string"} }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_get_issue",
                "description": "Get full details of one issue, including its body.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}
                    },
                    "required": ["number"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_list_issue_comments",
                "description": "List comments on an issue or pull request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}
                    },
                    "required": ["number"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_comment_issue",
                "description": "Post a comment on an issue or pull request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}, "body": {"type": "string"}
                    },
                    "required": ["number", "body"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_close_issue",
                "description": "Close an issue.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}
                    },
                    "required": ["number"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_list_open_prs",
                "description": "List open pull requests for a repo. Omit owner/repo to use the repo linked to this session.",
                "parameters": {
                    "type": "object",
                    "properties": { "owner": {"type": "string"}, "repo": {"type": "string"} }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_get_pr",
                "description": "Get full details of one pull request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}
                    },
                    "required": ["number"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "github_merge_pr",
                "description": "Merge a pull request.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "owner": {"type": "string"}, "repo": {"type": "string"},
                        "number": {"type": "integer"}
                    },
                    "required": ["number"]
                }
            }
        }
    ])
}

/// Tools that change something on GitHub — gated behind planning-mode
/// approval the same way write_file/run_shell are.
pub fn is_mutating(name: &str) -> bool {
    matches!(name, "github_comment_issue" | "github_close_issue" | "github_merge_pr")
}

fn resolve_repo(args: &Value, linked: Option<&str>) -> Result<(String, String), String> {
    let owner = args.get("owner").and_then(|v| v.as_str()).map(str::to_string);
    let repo = args.get("repo").and_then(|v| v.as_str()).map(str::to_string);
    if let (Some(o), Some(r)) = (owner, repo) {
        return Ok((o, r));
    }
    if let Some(l) = linked {
        let parts: Vec<&str> = l.splitn(2, '/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }
    Err("No owner/repo given, and no repo linked to this session".to_string())
}

async fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    token: &str,
    url: &str,
    body: Option<Value>,
) -> Result<String, String> {
    let mut req = client
        .request(method, url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "kestrel");
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("GitHub returned {}: {}", status, text));
    }
    Ok(text)
}

/// Dispatches one github_* tool call. Used both by the agent loop and
/// directly by the UI (tool cards call the same underlying actions the
/// agent would, via the github_action command in main.rs).
pub async fn execute(
    token: &str,
    linked_repo: Option<&str>,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    match name {
        "github_list_issues" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let url = format!("https://api.github.com/repos/{}/{}/issues?state=open", owner, repo);
            request(&client, reqwest::Method::GET, token, &url, None).await
        }
        "github_get_issue" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let url = format!("https://api.github.com/repos/{}/{}/issues/{}", owner, repo, number);
            request(&client, reqwest::Method::GET, token, &url, None).await
        }
        "github_list_issue_comments" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let url = format!("https://api.github.com/repos/{}/{}/issues/{}/comments", owner, repo, number);
            request(&client, reqwest::Method::GET, token, &url, None).await
        }
        "github_comment_issue" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let body = args.get("body").and_then(|v| v.as_str()).ok_or("missing body")?;
            let url = format!("https://api.github.com/repos/{}/{}/issues/{}/comments", owner, repo, number);
            request(&client, reqwest::Method::POST, token, &url, Some(json!({ "body": body }))).await
        }
        "github_close_issue" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let url = format!("https://api.github.com/repos/{}/{}/issues/{}", owner, repo, number);
            request(&client, reqwest::Method::PATCH, token, &url, Some(json!({ "state": "closed" }))).await
        }
        "github_list_open_prs" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let url = format!("https://api.github.com/repos/{}/{}/pulls?state=open", owner, repo);
            request(&client, reqwest::Method::GET, token, &url, None).await
        }
        "github_get_pr" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let url = format!("https://api.github.com/repos/{}/{}/pulls/{}", owner, repo, number);
            request(&client, reqwest::Method::GET, token, &url, None).await
        }
        "github_merge_pr" => {
            let (owner, repo) = resolve_repo(args, linked_repo)?;
            let number = args.get("number").and_then(|v| v.as_i64()).ok_or("missing number")?;
            let url = format!("https://api.github.com/repos/{}/{}/pulls/{}/merge", owner, repo, number);
            request(&client, reqwest::Method::PUT, token, &url, Some(json!({}))).await
        }
        _ => Err(format!("unknown github tool: {}", name)),
    }
}
