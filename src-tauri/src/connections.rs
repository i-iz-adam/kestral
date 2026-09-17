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

fn encode_emoji(emoji: &str) -> String {
    emoji
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => (b as char).to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

async fn handle_discord_response(resp: reqwest::Response) -> Result<Value, String> {
    let status = resp.status();
    if status.is_success() {
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(json!({ "success": true }));
        }
        let text = resp.text().await.unwrap_or_default();
        if text.trim().is_empty() {
            return Ok(json!({ "success": true }));
        }
        match serde_json::from_str::<Value>(&text) {
            Ok(val) => Ok(json!({ "success": true, "data": val })),
            Err(_) => Ok(json!({ "success": true, "raw": text })),
        }
    } else {
        let text = resp.text().await.unwrap_or_default();
        Err(format!("Discord API error ({}): {}", status, text))
    }
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
                    handle_discord_response(resp).await
                }
                "delete_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let role_id = args.get("role_id").and_then(|v| v.as_str()).ok_or("role_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/roles/{}", g_id, role_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
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
                    handle_discord_response(resp).await
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
                    handle_discord_response(resp).await
                }
                "get_channel" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_channel" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let name = args.get("name").or_else(|| args.get("channel_name")).and_then(|v| v.as_str()).ok_or("channel name is required")?;
                    let c_type = args.get("type").and_then(|v| v.as_u64()).unwrap_or(0);
                    let topic = args.get("topic").and_then(|v| v.as_str());
                    let parent_id = args.get("parent_id").and_then(|v| v.as_str());
                    let mut body = json!({ "name": name, "type": c_type });
                    if let Some(t) = topic { body["topic"] = json!(t); }
                    if let Some(p) = parent_id { body["parent_id"] = json!(p); }
                    let url = format!("https://discord.com/api/v10/guilds/{}/channels", g_id);
                    let resp = client.post(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&body).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_channel" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_channel" | "set_channel_topic" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let mut body = json!({});
                    if let Some(name) = args.get("name").and_then(|v| v.as_str()) { body["name"] = json!(name); }
                    if let Some(topic) = args.get("topic").and_then(|v| v.as_str()) { body["topic"] = json!(topic); }
                    if let Some(nsfw) = args.get("nsfw").and_then(|v| v.as_bool()) { body["nsfw"] = json!(nsfw); }
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client.patch(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&body).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
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
                    handle_discord_response(resp).await
                }
                "delete_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}/messages/{}", c_id, m_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let content = args.get("content").or_else(|| args.get("message")).and_then(|v| v.as_str()).unwrap_or("");
                    let url = format!("https://discord.com/api/v10/channels/{}/messages/{}", c_id, m_id);
                    let resp = client.patch(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&json!({ "content": content })).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_messages" | "list_messages" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50);
                    let url = format!("https://discord.com/api/v10/channels/{}/messages?limit={}", c_id, limit);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "pin_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}/pins/{}", c_id, m_id);
                    let resp = client.put(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "unpin_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}/pins/{}", c_id, m_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_pins" | "get_pinned_messages" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}/pins", c_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "add_reaction" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let emoji = args.get("emoji").and_then(|v| v.as_str()).ok_or("emoji is required")?;
                    let encoded_emoji = encode_emoji(emoji);
                    let url = format!("https://discord.com/api/v10/channels/{}/messages/{}/reactions/{}/@me", c_id, m_id, encoded_emoji);
                    let resp = client.put(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_reaction" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args.get("message_id").and_then(|v| v.as_str()).ok_or("message_id is required")?;
                    let emoji = args.get("emoji").and_then(|v| v.as_str()).ok_or("emoji is required")?;
                    let target_user = args.get("user_id").and_then(|v| v.as_str()).unwrap_or("@me");
                    let encoded_emoji = encode_emoji(emoji);
                    let url = format!("https://discord.com/api/v10/channels/{}/messages/{}/reactions/{}/{}", c_id, m_id, encoded_emoji, target_user);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_members" | "list_guild_members" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(1000);
                    let url = format!("https://discord.com/api/v10/guilds/{}/members?limit={}", g_id, limit);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/members/{}", g_id, u_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "kick_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/members/{}", g_id, u_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "ban_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let delete_secs = args.get("delete_message_seconds").and_then(|v| v.as_u64()).unwrap_or(0);
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans/{}", g_id, u_id);
                    let resp = client.put(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&json!({ "delete_message_seconds": delete_secs })).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "unban_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans/{}", g_id, u_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_bans" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans", g_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "assign_role" | "add_member_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let r_id = args.get("role_id").and_then(|v| v.as_str()).ok_or("role_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/members/{}/roles/{}", g_id, u_id, r_id);
                    let resp = client.put(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "remove_member_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args.get("user_id").and_then(|v| v.as_str()).ok_or("user_id is required")?;
                    let r_id = args.get("role_id").and_then(|v| v.as_str()).ok_or("role_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/members/{}/roles/{}", g_id, u_id, r_id);
                    let resp = client.delete(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_guild" | "get_server" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}?with_counts=true", g_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_thread" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let name = args.get("name").and_then(|v| v.as_str()).ok_or("thread name is required")?;
                    if let Some(m_id) = args.get("message_id").and_then(|v| v.as_str()) {
                        let url = format!("https://discord.com/api/v10/channels/{}/messages/{}/threads", c_id, m_id);
                        let resp = client.post(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&json!({ "name": name })).send().await.map_err(|e| e.to_string())?;
                        handle_discord_response(resp).await
                    } else {
                        let url = format!("https://discord.com/api/v10/channels/{}/threads", c_id);
                        let resp = client.post(&url).header("Authorization", format!("Bot {}", token.trim())).header("Content-Type", "application/json").json(&json!({ "name": name, "type": 11 })).send().await.map_err(|e| e.to_string())?;
                        handle_discord_response(resp).await
                    }
                }
                "list_threads" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/threads/active", g_id);
                    let resp = client.get(&url).header("Authorization", format!("Bot {}", token.trim())).send().await.map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
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
                            "description": "Action to perform (e.g., 'send_message', 'list_channels', 'create_channel', 'delete_channel', 'edit_channel', 'get_messages', 'delete_message', 'edit_message', 'pin_message', 'unpin_message', 'list_pins', 'add_reaction', 'delete_reaction', 'list_roles', 'create_role', 'delete_role', 'assign_role', 'remove_member_role', 'list_members', 'get_member', 'kick_member', 'ban_member', 'unban_member', 'list_bans', 'get_guild', 'create_thread', 'list_threads')"
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
