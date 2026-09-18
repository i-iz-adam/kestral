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

pub fn save_connections(
    app_handle: &tauri::AppHandle,
    connections: &[Connection],
) -> Result<(), String> {
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
                        let username = json_val
                            .get("username")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Discord Bot");
                        let discriminator = json_val
                            .get("discriminator")
                            .and_then(|v| v.as_str())
                            .unwrap_or("0");
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
                        let ok = json_val
                            .get("ok")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if ok {
                            let bot = json_val
                                .get("user")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Slack Bot");
                            let team = json_val.get("team").and_then(|v| v.as_str()).unwrap_or("");
                            connection.status = "connected".to_string();
                            connection.account_name = Some(format!("{} ({})", bot, team));
                        } else {
                            let err = json_val
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Auth failed");
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
                        let name = json_val
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Notion Bot");
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
            let url = connection
                .config
                .get("endpoint_url")
                .cloned()
                .unwrap_or_default();
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
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

pub fn perm_flag_from_name(name: &str) -> Option<u64> {
    match name.trim().to_uppercase().as_str() {
        "CREATE_INSTANT_INVITE" | "CREATE_INVITE" => Some(1 << 0),
        "KICK_MEMBERS" => Some(1 << 1),
        "BAN_MEMBERS" => Some(1 << 2),
        "ADMINISTRATOR" | "ADMIN" => Some(1 << 3),
        "MANAGE_CHANNELS" => Some(1 << 4),
        "MANAGE_GUILD" | "MANAGE_SERVER" => Some(1 << 5),
        "ADD_REACTIONS" => Some(1 << 6),
        "VIEW_AUDIT_LOG" => Some(1 << 7),
        "PRIORITY_SPEAKER" => Some(1 << 8),
        "STREAM" | "VIDEO" => Some(1 << 9),
        "VIEW_CHANNEL" | "READ_MESSAGES" => Some(1 << 10),
        "SEND_MESSAGES" => Some(1 << 11),
        "SEND_TTS_MESSAGES" => Some(1 << 12),
        "MANAGE_MESSAGES" => Some(1 << 13),
        "EMBED_LINKS" => Some(1 << 14),
        "ATTACH_FILES" => Some(1 << 15),
        "READ_MESSAGE_HISTORY" => Some(1 << 16),
        "MENTION_EVERYONE" => Some(1 << 17),
        "USE_EXTERNAL_EMOJIS" => Some(1 << 18),
        "VIEW_GUILD_INSIGHTS" => Some(1 << 19),
        "CONNECT" => Some(1 << 20),
        "SPEAK" => Some(1 << 21),
        "MUTE_MEMBERS" => Some(1 << 22),
        "DEAFEN_MEMBERS" => Some(1 << 23),
        "MOVE_MEMBERS" => Some(1 << 24),
        "USE_VAD" => Some(1 << 25),
        "CHANGE_NICKNAME" => Some(1 << 26),
        "MANAGE_NICKNAMES" => Some(1 << 27),
        "MANAGE_ROLES" => Some(1 << 28),
        "MANAGE_WEBHOOKS" => Some(1 << 29),
        "MANAGE_GUILD_EXPRESSIONS" | "MANAGE_EMOJIS_AND_STICKERS" => Some(1 << 30),
        "USE_APPLICATION_COMMANDS" | "USE_SLASH_COMMANDS" => Some(1 << 31),
        "REQUEST_TO_SPEAK" => Some(1 << 32),
        "MANAGE_EVENTS" => Some(1 << 33),
        "MANAGE_THREADS" => Some(1 << 34),
        "CREATE_PUBLIC_THREADS" => Some(1 << 35),
        "CREATE_PRIVATE_THREADS" => Some(1 << 36),
        "USE_EXTERNAL_STICKERS" => Some(1 << 37),
        "SEND_MESSAGES_IN_THREADS" => Some(1 << 38),
        "USE_EMBEDDED_ACTIVITIES" => Some(1 << 39),
        "MODERATE_MEMBERS" | "TIMEOUT_MEMBERS" => Some(1 << 40),
        "VIEW_CREATOR_MONETIZATION_ANALYTICS" => Some(1 << 41),
        "USE_SOUNDBOARD" => Some(1 << 42),
        _ => None,
    }
}

pub fn parse_permission_value(val: Option<&Value>) -> String {
    let val = match val {
        Some(v) => v,
        None => return "0".to_string(),
    };

    if let Some(n) = val.as_u64() {
        return n.to_string();
    }

    if let Some(s) = val.as_str() {
        let s_trimmed = s.trim();
        if let Ok(n) = s_trimmed.parse::<u64>() {
            return n.to_string();
        }
        let mut bits: u64 = 0;
        for token in s_trimmed.split(&[',', '|', ' '][..]) {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            if let Ok(n) = token.parse::<u64>() {
                bits |= n;
            } else if let Some(flag) = perm_flag_from_name(token) {
                bits |= flag;
            }
        }
        return bits.to_string();
    }

    if let Some(arr) = val.as_array() {
        let mut bits: u64 = 0;
        for elem in arr {
            if let Some(n) = elem.as_u64() {
                bits |= n;
            } else if let Some(s) = elem.as_str() {
                let s_trimmed = s.trim();
                if let Ok(n) = s_trimmed.parse::<u64>() {
                    bits |= n;
                } else if let Some(flag) = perm_flag_from_name(s_trimmed) {
                    bits |= flag;
                }
            }
        }
        return bits.to_string();
    }

    "0".to_string()
}

pub fn parse_permission_overwrites(val: Option<&Value>) -> Option<Vec<Value>> {
    let arr = val?.as_array()?;
    let mut overwrites = Vec::new();
    for item in arr {
        let id = item
            .get("id")
            .or_else(|| item.get("role_id"))
            .or_else(|| item.get("user_id"))
            .and_then(|v| v.as_str());
        if let Some(id_str) = id {
            let o_type = match item.get("type") {
                Some(v) if v.as_u64().is_some() => v.as_u64().unwrap(),
                Some(v) if v.as_str() == Some("member") || v.as_str() == Some("user") => 1,
                _ => 0, // default role
            };
            let allow = parse_permission_value(item.get("allow"));
            let deny = parse_permission_value(item.get("deny"));
            overwrites.push(json!({
                "id": id_str,
                "type": o_type,
                "allow": allow,
                "deny": deny
            }));
        }
    }
    Some(overwrites)
}

fn parse_color(val: &Value) -> u64 {
    if let Some(n) = val.as_u64() {
        return n;
    }
    if let Some(s) = val.as_str() {
        let s_trimmed = s.trim();
        match s_trimmed.to_uppercase().as_str() {
            "DEFAULT" => return 0,
            "AQUA" | "CYAN" => return 0x1ABC9C,
            "GREEN" => return 0x2ECC71,
            "BLUE" => return 0x3498DB,
            "PURPLE" => return 0x9B59B6,
            "GOLD" | "YELLOW" => return 0xF1C40F,
            "ORANGE" => return 0xE67E22,
            "RED" => return 0xE74C3C,
            "GREY" | "GRAY" => return 0x95A5A6,
            "NAVY" | "DARK_BLUE" => return 0x34495E,
            "BLURPLE" => return 0x5865F2,
            "FUCHSIA" | "PINK" => return 0xEB459E,
            "WHITE" => return 0xFFFFFF,
            "BLACK" => return 0x000001,
            _ => {}
        }
        let clean = s_trimmed.trim_start_matches('#');
        if let Ok(n) = u64::from_str_radix(clean, 16) {
            return n;
        }
        if let Ok(n) = s_trimmed.parse::<u64>() {
            return n;
        }
    }
    0
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
                args.get("guild_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            let channel_id = target.config.get("channel_id").cloned().or_else(|| {
                args.get("channel_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

            match action {
                "create_role" => {
                    let g_id =
                        guild_id.ok_or("Guild ID is required for creating a Discord role")?;
                    let role_name = args
                        .get("name")
                        .or_else(|| args.get("role_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("New Role");
                    let color = args.get("color").map(parse_color).unwrap_or(0);
                    let permissions = parse_permission_value(args.get("permissions"));
                    let hoist = args.get("hoist").and_then(|v| v.as_bool()).unwrap_or(true);
                    let mentionable = args
                        .get("mentionable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let mut body = json!({
                        "name": role_name,
                        "color": color,
                        "permissions": permissions,
                        "hoist": hoist,
                        "mentionable": mentionable
                    });
                    if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
                        body["icon"] = json!(icon);
                    }
                    if let Some(emoji) = args.get("unicode_emoji").and_then(|v| v.as_str()) {
                        body["unicode_emoji"] = json!(emoji);
                    }

                    let url = format!("https://discord.com/api/v10/guilds/{}/roles", g_id);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_role" | "update_role" | "set_role_permissions" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let role_id = args
                        .get("role_id")
                        .and_then(|v| v.as_str())
                        .ok_or("role_id is required")?;
                    let mut body = json!({});
                    if let Some(name) = args
                        .get("name")
                        .or_else(|| args.get("role_name"))
                        .and_then(|v| v.as_str())
                    {
                        body["name"] = json!(name);
                    }
                    if let Some(c) = args.get("color") {
                        body["color"] = json!(parse_color(c));
                    }
                    if let Some(p) = args.get("permissions") {
                        body["permissions"] = json!(parse_permission_value(Some(p)));
                    }
                    if let Some(hoist) = args.get("hoist").and_then(|v| v.as_bool()) {
                        body["hoist"] = json!(hoist);
                    }
                    if let Some(mentionable) = args.get("mentionable").and_then(|v| v.as_bool()) {
                        body["mentionable"] = json!(mentionable);
                    }
                    if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
                        body["icon"] = json!(icon);
                    }
                    if let Some(emoji) = args.get("unicode_emoji").and_then(|v| v.as_str()) {
                        body["unicode_emoji"] = json!(emoji);
                    }

                    let url = format!("https://discord.com/api/v10/guilds/{}/roles/{}", g_id, role_id);
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let role_id = args
                        .get("role_id")
                        .and_then(|v| v.as_str())
                        .ok_or("role_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/roles/{}",
                        g_id, role_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_role" | "view_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required for getting Discord role")?;
                    let role_id = args.get("role_id").and_then(|v| v.as_str());
                    let url = format!("https://discord.com/api/v10/guilds/{}/roles", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let res = handle_discord_response(resp).await?;
                    if let Some(r_id) = role_id {
                        if let Some(arr) = res.get("data").and_then(|v| v.as_array()).or_else(|| res.as_array()) {
                            if let Some(found) = arr.iter().find(|r| r.get("id").and_then(|v| v.as_str()) == Some(r_id)) {
                                return Ok(json!({ "success": true, "data": found }));
                            } else {
                                return Err(format!("Role with id {} not found", r_id));
                            }
                        }
                    }
                    Ok(res)
                }
                "list_roles" | "view_roles" => {
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
                "modify_role_positions" | "reorder_roles" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let roles = args
                        .get("roles")
                        .ok_or("roles array is required for reordering roles")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/roles", g_id);
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(roles)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_channels" | "view_channels" | "get_channels" => {
                    let g_id =
                        guild_id.ok_or("Guild ID is required for listing Discord channels")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/channels", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_channel" | "view_channel" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_channel" | "create_category" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let name = args
                        .get("name")
                        .or_else(|| args.get("channel_name"))
                        .and_then(|v| v.as_str())
                        .ok_or("channel name is required")?;
                    let default_type = if action == "create_category" { 4 } else { 0 };
                    let c_type = args.get("type").and_then(|v| v.as_u64()).unwrap_or(default_type);
                    let mut body = json!({ "name": name, "type": c_type });
                    if let Some(topic) = args.get("topic").and_then(|v| v.as_str()) {
                        body["topic"] = json!(topic);
                    }
                    if let Some(parent_id) = args.get("parent_id").and_then(|v| v.as_str()) {
                        body["parent_id"] = json!(parent_id);
                    }
                    if let Some(pos) = args.get("position").and_then(|v| v.as_u64()) {
                        body["position"] = json!(pos);
                    }
                    if let Some(nsfw) = args.get("nsfw").and_then(|v| v.as_bool()) {
                        body["nsfw"] = json!(nsfw);
                    }
                    if let Some(bitrate) = args.get("bitrate").and_then(|v| v.as_u64()) {
                        body["bitrate"] = json!(bitrate);
                    }
                    if let Some(user_limit) = args.get("user_limit").and_then(|v| v.as_u64()) {
                        body["user_limit"] = json!(user_limit);
                    }
                    if let Some(rate_limit) = args.get("rate_limit_per_user").and_then(|v| v.as_u64()) {
                        body["rate_limit_per_user"] = json!(rate_limit);
                    }
                    if let Some(overwrites) = parse_permission_overwrites(args.get("permission_overwrites")) {
                        body["permission_overwrites"] = json!(overwrites);
                    }
                    let url = format!("https://discord.com/api/v10/guilds/{}/channels", g_id);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_channel" | "delete_category" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_channel" | "edit_category" | "set_channel_topic" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let mut body = json!({});
                    if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                        body["name"] = json!(name);
                    }
                    if let Some(c_type) = args.get("type").and_then(|v| v.as_u64()) {
                        body["type"] = json!(c_type);
                    }
                    if let Some(topic) = args.get("topic").and_then(|v| v.as_str()) {
                        body["topic"] = json!(topic);
                    }
                    if let Some(parent_id) = args.get("parent_id").and_then(|v| v.as_str()) {
                        body["parent_id"] = json!(parent_id);
                    }
                    if let Some(pos) = args.get("position").and_then(|v| v.as_u64()) {
                        body["position"] = json!(pos);
                    }
                    if let Some(nsfw) = args.get("nsfw").and_then(|v| v.as_bool()) {
                        body["nsfw"] = json!(nsfw);
                    }
                    if let Some(bitrate) = args.get("bitrate").and_then(|v| v.as_u64()) {
                        body["bitrate"] = json!(bitrate);
                    }
                    if let Some(user_limit) = args.get("user_limit").and_then(|v| v.as_u64()) {
                        body["user_limit"] = json!(user_limit);
                    }
                    if let Some(rate_limit) = args.get("rate_limit_per_user").and_then(|v| v.as_u64()) {
                        body["rate_limit_per_user"] = json!(rate_limit);
                    }
                    if let Some(overwrites) = parse_permission_overwrites(args.get("permission_overwrites")) {
                        body["permission_overwrites"] = json!(overwrites);
                    }
                    let url = format!("https://discord.com/api/v10/channels/{}", c_id);
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "modify_channel_positions" | "reorder_channels" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let channels = args
                        .get("channels")
                        .ok_or("channels array is required for reordering channels")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/channels", g_id);
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(channels)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "set_channel_permissions" | "set_permission_overwrite" | "edit_channel_permissions" | "set_permissions" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let overwrite_id = args
                        .get("overwrite_id")
                        .or_else(|| args.get("target_id"))
                        .or_else(|| args.get("role_id"))
                        .or_else(|| args.get("user_id"))
                        .and_then(|v| v.as_str())
                        .ok_or("overwrite_id (role_id or user_id) is required")?;

                    let o_type = match args.get("type") {
                        Some(v) if v.as_u64().is_some() => v.as_u64().unwrap(),
                        Some(v) if v.as_str() == Some("member") || v.as_str() == Some("user") => 1,
                        _ => 0,
                    };

                    let allow = parse_permission_value(args.get("allow"));
                    let deny = parse_permission_value(args.get("deny"));

                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/permissions/{}",
                        c_id, overwrite_id
                    );
                    let resp = client
                        .put(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({
                            "allow": allow,
                            "deny": deny,
                            "type": o_type
                        }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_channel_permissions" | "delete_permission_overwrite" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let overwrite_id = args
                        .get("overwrite_id")
                        .or_else(|| args.get("target_id"))
                        .or_else(|| args.get("role_id"))
                        .or_else(|| args.get("user_id"))
                        .and_then(|v| v.as_str())
                        .ok_or("overwrite_id (role_id or user_id) is required")?;

                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/permissions/{}",
                        c_id, overwrite_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_guild" | "edit_server" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let mut body = json!({});
                    if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                        body["name"] = json!(name);
                    }
                    if let Some(desc) = args.get("description").and_then(|v| v.as_str()) {
                        body["description"] = json!(desc);
                    }
                    if let Some(icon) = args.get("icon").and_then(|v| v.as_str()) {
                        body["icon"] = json!(icon);
                    }
                    if let Some(banner) = args.get("banner").and_then(|v| v.as_str()) {
                        body["banner"] = json!(banner);
                    }
                    if let Some(splash) = args.get("splash").and_then(|v| v.as_str()) {
                        body["splash"] = json!(splash);
                    }
                    if let Some(afk_id) = args.get("afk_channel_id").and_then(|v| v.as_str()) {
                        body["afk_channel_id"] = json!(afk_id);
                    }
                    if let Some(afk_t) = args.get("afk_timeout").and_then(|v| v.as_u64()) {
                        body["afk_timeout"] = json!(afk_t);
                    }
                    if let Some(sys_id) = args.get("system_channel_id").and_then(|v| v.as_str()) {
                        body["system_channel_id"] = json!(sys_id);
                    }
                    if let Some(verif) = args.get("verification_level").and_then(|v| v.as_u64()) {
                        body["verification_level"] = json!(verif);
                    }

                    let url = format!("https://discord.com/api/v10/guilds/{}", g_id);
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_emojis" | "list_guild_emojis" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/emojis", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_emoji" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let name = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or("emoji name is required")?;
                    let image = args
                        .get("image")
                        .and_then(|v| v.as_str())
                        .ok_or("image data string (data:image/jpeg;base64,...) is required")?;
                    let mut body = json!({ "name": name, "image": image });
                    if let Some(r) = args.get("roles") {
                        body["roles"] = r.clone();
                    }
                    let url = format!("https://discord.com/api/v10/guilds/{}/emojis", g_id);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&body)
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_emoji" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let emoji_id = args
                        .get("emoji_id")
                        .and_then(|v| v.as_str())
                        .ok_or("emoji_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/emojis/{}", g_id, emoji_id);
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_dm" | "open_dm" | "get_dm_channel" => {
                    let u_id = args
                        .get("user_id")
                        .or_else(|| args.get("recipient_id"))
                        .and_then(|v| v.as_str())
                        .ok_or("user_id (or recipient_id) is required to open a DM channel")?;
                    let url = "https://discord.com/api/v10/users/@me/channels";
                    let resp = client
                        .post(url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({ "recipient_id": u_id }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "send_dm" | "dm_user" | "send_direct_message" => {
                    let content = args
                        .get("content")
                        .or_else(|| args.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let target_dm_channel_id = if let Some(u_id) = args
                        .get("user_id")
                        .or_else(|| args.get("recipient_id"))
                        .and_then(|v| v.as_str())
                    {
                        let dm_url = "https://discord.com/api/v10/users/@me/channels";
                        let dm_resp = client
                            .post(dm_url)
                            .header("Authorization", format!("Bot {}", token.trim()))
                            .header("Content-Type", "application/json")
                            .json(&json!({ "recipient_id": u_id }))
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        let dm_data = handle_discord_response(dm_resp).await?;
                        let dm_id = dm_data
                            .get("data")
                            .and_then(|d| d.get("id"))
                            .or_else(|| dm_data.get("id"))
                            .and_then(|v| v.as_str())
                            .ok_or("Failed to obtain DM channel ID from Discord")?
                            .to_string();
                        dm_id
                    } else if let Some(ref c_id) = channel_id {
                        c_id.clone()
                    } else {
                        return Err("user_id (or recipient_id) or channel_id is required for sending a DM".to_string());
                    };

                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages",
                        target_dm_channel_id
                    );
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
                "list_dms" | "list_dm_channels" => {
                    let url = "https://discord.com/api/v10/users/@me/channels";
                    let resp = client
                        .get(url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "send_message" => {
                    let target_c_id = if let Some(ref c_id) = channel_id {
                        c_id.clone()
                    } else if let Some(u_id) = args
                        .get("user_id")
                        .or_else(|| args.get("recipient_id"))
                        .and_then(|v| v.as_str())
                    {
                        let dm_url = "https://discord.com/api/v10/users/@me/channels";
                        let dm_resp = client
                            .post(dm_url)
                            .header("Authorization", format!("Bot {}", token.trim()))
                            .header("Content-Type", "application/json")
                            .json(&json!({ "recipient_id": u_id }))
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        let dm_data = handle_discord_response(dm_resp).await?;
                        dm_data
                            .get("data")
                            .and_then(|d| d.get("id"))
                            .or_else(|| dm_data.get("id"))
                            .and_then(|v| v.as_str())
                            .ok_or("Failed to obtain DM channel ID from Discord")?
                            .to_string()
                    } else {
                        return Err("Channel ID or user_id is required for sending a Discord message".to_string());
                    };

                    let content = args
                        .get("content")
                        .or_else(|| args.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let url = format!("https://discord.com/api/v10/channels/{}/messages", target_c_id);
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
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages/{}",
                        c_id, m_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "edit_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let content = args
                        .get("content")
                        .or_else(|| args.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages/{}",
                        c_id, m_id
                    );
                    let resp = client
                        .patch(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({ "content": content }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_messages" | "list_messages" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50);
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages?limit={}",
                        c_id, limit
                    );
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "pin_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/pins/{}",
                        c_id, m_id
                    );
                    let resp = client
                        .put(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "unpin_message" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/pins/{}",
                        c_id, m_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_pins" | "get_pinned_messages" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let url = format!("https://discord.com/api/v10/channels/{}/pins", c_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "add_reaction" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let emoji = args
                        .get("emoji")
                        .and_then(|v| v.as_str())
                        .ok_or("emoji is required")?;
                    let encoded_emoji = encode_emoji(emoji);
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages/{}/reactions/{}/@me",
                        c_id, m_id, encoded_emoji
                    );
                    let resp = client
                        .put(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "delete_reaction" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let m_id = args
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .ok_or("message_id is required")?;
                    let emoji = args
                        .get("emoji")
                        .and_then(|v| v.as_str())
                        .ok_or("emoji is required")?;
                    let target_user = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("@me");
                    let encoded_emoji = encode_emoji(emoji);
                    let url = format!(
                        "https://discord.com/api/v10/channels/{}/messages/{}/reactions/{}/{}",
                        c_id, m_id, encoded_emoji, target_user
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_members" | "list_guild_members" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(1000);
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/members?limit={}",
                        g_id, limit
                    );
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/members/{}",
                        g_id, u_id
                    );
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "kick_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/members/{}",
                        g_id, u_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "ban_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let delete_secs = args
                        .get("delete_message_seconds")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans/{}", g_id, u_id);
                    let resp = client
                        .put(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .header("Content-Type", "application/json")
                        .json(&json!({ "delete_message_seconds": delete_secs }))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "unban_member" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans/{}", g_id, u_id);
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "list_bans" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/bans", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "assign_role" | "add_member_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let r_id = args
                        .get("role_id")
                        .and_then(|v| v.as_str())
                        .ok_or("role_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/members/{}/roles/{}",
                        g_id, u_id, r_id
                    );
                    let resp = client
                        .put(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "remove_member_role" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let u_id = args
                        .get("user_id")
                        .and_then(|v| v.as_str())
                        .ok_or("user_id is required")?;
                    let r_id = args
                        .get("role_id")
                        .and_then(|v| v.as_str())
                        .ok_or("role_id is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}/members/{}/roles/{}",
                        g_id, u_id, r_id
                    );
                    let resp = client
                        .delete(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "get_guild" | "get_server" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!(
                        "https://discord.com/api/v10/guilds/{}?with_counts=true",
                        g_id
                    );
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                "create_thread" => {
                    let c_id = channel_id.ok_or("Channel ID is required")?;
                    let name = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or("thread name is required")?;
                    if let Some(m_id) = args.get("message_id").and_then(|v| v.as_str()) {
                        let url = format!(
                            "https://discord.com/api/v10/channels/{}/messages/{}/threads",
                            c_id, m_id
                        );
                        let resp = client
                            .post(&url)
                            .header("Authorization", format!("Bot {}", token.trim()))
                            .header("Content-Type", "application/json")
                            .json(&json!({ "name": name }))
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        handle_discord_response(resp).await
                    } else {
                        let url = format!("https://discord.com/api/v10/channels/{}/threads", c_id);
                        let resp = client
                            .post(&url)
                            .header("Authorization", format!("Bot {}", token.trim()))
                            .header("Content-Type", "application/json")
                            .json(&json!({ "name": name, "type": 11 }))
                            .send()
                            .await
                            .map_err(|e| e.to_string())?;
                        handle_discord_response(resp).await
                    }
                }
                "list_threads" => {
                    let g_id = guild_id.ok_or("Guild ID is required")?;
                    let url = format!("https://discord.com/api/v10/guilds/{}/threads/active", g_id);
                    let resp = client
                        .get(&url)
                        .header("Authorization", format!("Bot {}", token.trim()))
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    handle_discord_response(resp).await
                }
                _ => Err(format!("Unsupported Discord action: {}", action)),
            }
        }
        "github" => {
            let workspace = args
                .get("workspace")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let raw = github::execute(&token, workspace, action, &args).await?;
            serde_json::from_str(&raw).map_err(|e| e.to_string())
        }
        "webhook" => {
            let endpoint = target
                .config
                .get("endpoint_url")
                .ok_or("Missing endpoint_url")?;
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
        _ => Err(format!(
            "Execution for connection type '{}' is not implemented",
            conn_type
        )),
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
                            "description": "Action to perform (e.g., 'send_message', 'send_dm', 'create_dm', 'list_dms', 'list_channels', 'create_channel', 'create_category', 'delete_channel', 'edit_channel', 'get_channel', 'reorder_channels', 'list_roles', 'get_role', 'create_role', 'edit_role', 'delete_role', 'reorder_roles', 'set_channel_permissions', 'delete_channel_permissions', 'assign_role', 'remove_member_role', 'list_members', 'get_member', 'kick_member', 'ban_member', 'unban_member', 'list_bans', 'get_guild', 'edit_guild', 'create_thread', 'list_threads', 'list_emojis', 'create_emoji', 'delete_emoji')"
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_permission_value() {
        assert_eq!(parse_permission_value(None), "0");
        assert_eq!(parse_permission_value(Some(&json!("8"))), "8");
        assert_eq!(parse_permission_value(Some(&json!(8))), "8");
        assert_eq!(parse_permission_value(Some(&json!("ADMINISTRATOR"))), "8");

        // VIEW_CHANNEL (1024) | SEND_MESSAGES (2048) = 3072
        assert_eq!(
            parse_permission_value(Some(&json!(["VIEW_CHANNEL", "SEND_MESSAGES"]))),
            "3072"
        );
        assert_eq!(
            parse_permission_value(Some(&json!("VIEW_CHANNEL, SEND_MESSAGES"))),
            "3072"
        );
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color(&json!(0)), 0);
        assert_eq!(parse_color(&json!("#FF0000")), 0xFF0000);
        assert_eq!(parse_color(&json!("RED")), 0xE74C3C);
        assert_eq!(parse_color(&json!("BLURPLE")), 0x5865F2);
        assert_eq!(parse_color(&json!(16711680)), 16711680);
    }

    #[test]
    fn test_parse_permission_overwrites() {
        let input = json!([
            {
                "id": "123456",
                "type": "role",
                "allow": ["VIEW_CHANNEL", "SEND_MESSAGES"],
                "deny": ["MANAGE_MESSAGES"]
            }
        ]);
        let parsed = parse_permission_overwrites(Some(&input)).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["id"], "123456");
        assert_eq!(parsed[0]["type"], 0);
        assert_eq!(parsed[0]["allow"], "3072");
        assert_eq!(parsed[0]["deny"], "8192");
    }
}
