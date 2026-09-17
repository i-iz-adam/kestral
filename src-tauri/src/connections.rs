use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use uuid::Uuid;

use crate::github;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub r#type: String, // "github", "discord", "slack", "telegram", "notion", "linear", "webhook", "postgres"
    pub name: String,   // e.g. "The Magician", "Work GitHub", "Ops Bot"
    pub created_at: u64,
    pub updated_at: u64,
    pub status: String, // "connected", "error", "untested"
    pub account_name: Option<String>,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionsStore {
    pub connections: Vec<Connection>,
}

fn config_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .app_config_dir()
        .expect("could not resolve app config dir");
    fs::create_dir_all(&dir).ok();
    dir.join("connections.json")
}

pub fn load_connections(app_handle: &tauri::AppHandle) -> Vec<Connection> {
    let path = config_path(app_handle);
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(store) = serde_json::from_str::<ConnectionsStore>(&data) {
                return store.connections;
            }
        }
    }

    // Check backward compatibility with legacy github_config.json
    let mut connections = Vec::new();
    if let Some(gh_token) = github::load_token(app_handle) {
        if !gh_token.trim().is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            connections.push(Connection {
                id: Uuid::new_v4().to_string(),
                r#type: "github".to_string(),
                name: "GitHub".to_string(),
                created_at: now,
                updated_at: now,
                status: "connected".to_string(),
                account_name: None,
                config: {
                    let mut map = HashMap::new();
                    map.insert("token".to_string(), gh_token);
                    map
                },
            });
        }
    }

    connections
}

pub fn save_connections(app_handle: &tauri::AppHandle, connections: &[Connection]) -> Result<(), String> {
    let path = config_path(app_handle);
    let store = ConnectionsStore {
        connections: connections.to_vec(),
    };
    let json_str = serde_json::to_string_pretty(&store).map_err(|e| e.to_string())?;
    fs::write(path, json_str).map_err(|e| e.to_string())?;

    // Keep primary github token in sync with github_config.json
    if let Some(gh_conn) = connections.iter().find(|c| c.r#type == "github") {
        if let Some(token) = gh_conn.config.get("token") {
            github::save_token(app_handle, token).ok();
        }
    } else {
        // If no github connection exists, clear legacy token
        github::save_token(app_handle, "").ok();
    }

    Ok(())
}

pub async fn test_connection(mut connection: Connection) -> Connection {
    let client = reqwest::Client::new();
    let conn_type = connection.r#type.to_lowercase();
    let token = connection.config.get("token").cloned().unwrap_or_default();

    match conn_type.as_str() {
        "github" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing token".to_string());
                return connection;
            }
            match github::test_token(&token).await {
                Ok(login) => {
                    connection.status = "connected".to_string();
                    connection.account_name = Some(format!("@{}", login));
                }
                Err(err) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(err);
                }
            }
        }
        "discord" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing bot token".to_string());
                return connection;
            }
            let resp = client
                .get("https://discord.com/api/v10/users/@me")
                .header("Authorization", format!("Bot {}", token.trim()))
                .header("User-Agent", "kestrel-agent")
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json_val) = r.json::<Value>().await {
                        let username = json_val.get("username").and_then(|v| v.as_str()).unwrap_or("Discord Bot");
                        let discriminator = json_val.get("discriminator").and_then(|v| v.as_str()).unwrap_or("0");
                        let account = if discriminator == "0" || discriminator.is_empty() {
                            username.to_string()
                        } else {
                            format!("{}#{}", username, discriminator)
                        };
                        connection.status = "connected".to_string();
                        connection.account_name = Some(account);
                    } else {
                        connection.status = "connected".to_string();
                        connection.account_name = Some("Discord Bot".to_string());
                    }
                }
                Ok(r) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(format!("Discord API HTTP {}", r.status()));
                }
                Err(e) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(e.to_string());
                }
            }
        }
        "slack" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing token".to_string());
                return connection;
            }
            let resp = client
                .post("https://slack.com/api/auth.test")
                .header("Authorization", format!("Bearer {}", token.trim()))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json_val) = r.json::<Value>().await {
                        let ok = json_val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                        if ok {
                            let bot = json_val.get("user").and_then(|v| v.as_str()).unwrap_or("Slack Bot");
                            let team = json_val.get("team").and_then(|v| v.as_str()).unwrap_or("");
                            connection.status = "connected".to_string();
                            connection.account_name = Some(format!("{} ({})", bot, team));
                        } else {
                            let err = json_val.get("error").and_then(|v| v.as_str()).unwrap_or("Auth failed");
                            connection.status = "error".to_string();
                            connection.account_name = Some(err.to_string());
                        }
                    } else {
                        connection.status = "connected".to_string();
                    }
                }
                Ok(r) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(format!("Slack API {}", r.status()));
                }
                Err(e) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(e.to_string());
                }
            }
        }
        "telegram" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing bot token".to_string());
                return connection;
            }
            let url = format!("https://api.telegram.org/bot{}/getMe", token.trim());
            let resp = client.get(&url).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json_val) = r.json::<Value>().await {
                        let username = json_val
                            .get("result")
                            .and_then(|res| res.get("username"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Telegram Bot");
                        connection.status = "connected".to_string();
                        connection.account_name = Some(format!("@{}", username));
                    } else {
                        connection.status = "connected".to_string();
                    }
                }
                Ok(r) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(format!("Telegram API {}", r.status()));
                }
                Err(e) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(e.to_string());
                }
            }
        }
        "notion" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing integration token".to_string());
                return connection;
            }
            let resp = client
                .get("https://api.notion.com/v1/users/me")
                .header("Authorization", format!("Bearer {}", token.trim()))
                .header("Notion-Version", "2022-06-28")
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json_val) = r.json::<Value>().await {
                        let name = json_val.get("name").and_then(|v| v.as_str()).unwrap_or("Notion Bot");
                        connection.status = "connected".to_string();
                        connection.account_name = Some(name.to_string());
                    } else {
                        connection.status = "connected".to_string();
                    }
                }
                Ok(r) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(format!("Notion API {}", r.status()));
                }
                Err(e) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(e.to_string());
                }
            }
        }
        "linear" => {
            if token.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing API key".to_string());
                return connection;
            }
            let resp = client
                .post("https://api.linear.app/graphql")
                .header("Authorization", token.trim())
                .json(&json!({ "query": "{ viewer { id name email } }" }))
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(json_val) = r.json::<Value>().await {
                        let name = json_val
                            .get("data")
                            .and_then(|d| d.get("viewer"))
                            .and_then(|v| v.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("Linear User");
                        connection.status = "connected".to_string();
                        connection.account_name = Some(name.to_string());
                    } else {
                        connection.status = "connected".to_string();
                    }
                }
                Ok(r) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(format!("Linear API {}", r.status()));
                }
                Err(e) => {
                    connection.status = "error".to_string();
                    connection.account_name = Some(e.to_string());
                }
            }
        }
        "webhook" => {
            let url = connection.config.get("endpoint_url").cloned().unwrap_or_default();
            if url.trim().is_empty() {
                connection.status = "error".to_string();
                connection.account_name = Some("Missing endpoint URL".to_string());
                return connection;
            }
            connection.status = "connected".to_string();
            connection.account_name = Some(url);
        }
        _ => {
            connection.status = "connected".to_string();
        }
    }

    connection
}

pub async fn execute_connection_action(
    app_handle: &tauri::AppHandle,
    connection_name: &str,
    action: &str,
    args: Value,
) -> Result<Value, String> {
    let connections = load_connections(app_handle);
    let target = connections
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(connection_name))
        .ok_or_else(|| format!("Connection with name '{}' not found", connection_name))?;

    let client = reqwest::Client::new();
    let conn_type = target.r#type.to_lowercase();
    let token = target.config.get("token").cloned().unwrap_or_default();

    match conn_type.as_str() {
        "discord" => {
            let guild_id = target.config.get("guild_id").cloned().or_else(|| {
                args.get("guild_id").and_then(|v| v.as_str()).map(|s| s.to_string())
            });
            let channel_id = target.config.get("channel_id").cloned().or_else(|| {
                args.get("channel_id").and_then(|v| v.as_str()).map(|s| s.to_string())
            });

            match action {
                "create_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required for creating a Discord role")?;
                    let role_name = args.get("name").or_else(|| args.get("role_name")).and_then(|v| v.as_str()).unwrap_or("New Role");
                    let color = args.get("color").and_then(|v| v.as_u64()).unwrap_or(0);
                    let permissions = args.get("permissions").and_then(|v| v.as_str()).unwrap_or("0");

                    let url = format!("https://discord.com/api/v10/guilds/{}/roles", g_id);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({
                            "name": role_name,
                            "color": color,
                            "permissions": permissions,
                            "hoist": true
                        }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    if resp.status().is_success() {
                        let res_json: Value = resp.json().await.map_err(|e| e.to_string())?;
                        Ok(json!({ "success": true, "role": res_json }))
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        Err(format!("Discord API error: {}", text))
                    }
                }
                "send_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required for sending a Discord message")?;
                    let content = args.get("content").or_else(|| args.get("message")).and_then(|v| v.as_str()).unwrap_or("");
                    let url = format!("https://discord.com/api/v10/channels/{}/messages", c_id);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({ "content": content }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    if resp.status().is_success() {
                        let res_json: Value = resp.json().await.map_err(|e| e.to_string())?;
                        Ok(json!({ "success": true, "message": res_json }))
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        Err(format!("Discord API error: {}", text))
                    }
                }
                "list_roles" => {
                    let g_id = guild_id.ok_or("Guild ID is required for listing Discord roles")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/roles", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    if resp.status().is_success() {
                        let roles: Value = resp.json().await.map_err(|e| e.to_string())?;
                        Ok(json!({ "roles": roles }))
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        Err(format!("Discord API error: {}", text))
                    }
                }
                "list_channels" => {
                    let g_id = guild_id.ok_or("Guild ID is required for listing Discord channels")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/channels", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    if resp.status().is_success() {
                        let channels: Value = resp.json().await.map_err(|e| e.to_string())?;
                        Ok(json!({ "channels": channels }))
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        Err(format!("Discord API error: {}", text))
                    }
                }
                _ => Err(format!("Unsupported Discord action: {}", action)),
            }
        }
        "github" => {
            let workspace = args.get("workspace").and_then(|v| v.as_str()).unwrap_or(".");
            let raw = github::execute(&token, workspace, action, &args).await?;
            serde_json::from_str(&raw).map_err(|e| e.to_string())
        }
        "webhook" => {
            let endpoint = target.config.get("endpoint_url").ok_or("Missing endpoint_url")?;
            let resp = client
                .post(endpoint)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token))
                .json(&args)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let text = resp.text().await.unwrap_or_default();
            Ok(json!({ "response": text }))
        }
        _ => Err(format!("Execution for connection type '{}' is not implemented", conn_type)),
    }
}

pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "integration_action",
                "description": "Execute an action using a named integration connection (e.g., Discord bot, GitHub connection, Webhook, etc.). For example, use this when the user says 'Using The Magician, create a role for moderators'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "connection_name": {
                            "type": "string",
                            "description": "The exact name of the configured connection (e.g. 'The Magician', 'Work GitHub', 'Ops Bot')"
                        },
                        "action": {
                            "type": "string",
                            "description": "Action to perform (e.g., 'create_role', 'send_message', 'list_roles', 'list_channels', 'list_issues', 'create_issue')"
                        },
                        "args": {
                            "type": "object",
                            "description": "Arguments for the action (e.g., {'name': 'moderators', 'color': 16711680} or {'content': 'Hello!'})"
                        }
                    },
                    "required": ["connection_name", "action"]
                }
            }
        }
    ])
}
