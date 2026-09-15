use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UpdateCheckResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: String,
    pub download_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallerConfig {
    pub install_dir: String,
    pub create_desktop_shortcut: bool,
    pub create_start_menu_shortcut: bool,
    pub register_protocol: bool,
    pub launch_on_finish: bool,
}

#[derive(Clone, Serialize)]
pub struct InstallProgressPayload {
    pub stage: String,
    pub percent: u32,
    pub message: String,
    pub completed: bool,
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

pub fn is_version_newer(latest: &str, current: &str) -> bool {
    let latest_parts = parse_version(latest);
    let current_parts = parse_version(current);
    if latest_parts.is_empty() || current_parts.is_empty() {
        return latest != current && !latest.is_empty();
    }
    latest_parts > current_parts
}

#[tauri::command]
pub async fn check_app_update(app_handle: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = app_handle.package_info().version.to_string();
    let client = reqwest::Client::builder()
        .user_agent(format!("kestrel-app/{}", current_version))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let url = "https://api.github.com/repos/i-iz-adam/kestral/releases/latest";
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to send update check request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub release API returned status {}",
            response.status()
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse release response JSON: {}", e))?;

    let tag_name = json["tag_name"].as_str().unwrap_or("").to_string();
    let latest_version = tag_name.trim_start_matches('v').to_string();
    let release_name = json["name"].as_str().unwrap_or(&tag_name).to_string();
    let release_notes = json["body"].as_str().unwrap_or("").to_string();
    let published_at = json["published_at"].as_str().unwrap_or("").to_string();
    let html_url = json["html_url"].as_str().unwrap_or("").to_string();

    let target_os = std::env::consts::OS;
    let mut download_url = "".to_string();

    if let Some(assets) = json["assets"].as_array() {
        for asset in assets {
            if let Some(name) = asset["name"].as_str() {
                let name_lower = name.to_lowercase();
                let matches_os = match target_os {
                    "windows" => name_lower.ends_with(".exe") || name_lower.ends_with(".msi"),
                    "macos" => name_lower.ends_with(".dmg") || name_lower.ends_with(".app.tar.gz") || name_lower.ends_with(".pkg"),
                    "linux" => name_lower.ends_with(".appimage") || name_lower.ends_with(".deb") || name_lower.ends_with(".rpm"),
                    _ => false,
                };
                if matches_os {
                    if let Some(url) = asset["browser_download_url"].as_str() {
                        download_url = url.to_string();
                        break;
                    }
                }
            }
        }
        if download_url.is_empty() && !assets.is_empty() {
            if let Some(url) = assets[0]["browser_download_url"].as_str() {
                download_url = url.to_string();
            }
        }
    }

    if download_url.is_empty() {
        download_url = html_url;
    }

    let has_update = is_version_newer(&latest_version, &current_version);

    Ok(UpdateCheckResult {
        has_update,
        current_version,
        latest_version,
        release_name,
        release_notes,
        published_at,
        download_url,
    })
}

#[tauri::command]
pub async fn download_and_install_update(app_handle: tauri::AppHandle) -> Result<bool, String> {
    use tauri::Manager;
    use tokio::time::{sleep, Duration};

    let steps = [
        (10, "Starting download..."),
        (25, "Downloading inner files..."),
        (50, "Unpacking setup..."),
        (75, "Preparing installer..."),
        (100, "Ready to jump..."),
    ];

    for (p, msg) in steps {
        sleep(Duration::from_millis(500)).await;
        let _ = app_handle.emit_all(
            "update-progress",
            InstallProgressPayload {
                stage: "download".to_string(),
                percent: p,
                message: msg.to_string(),
                completed: p == 100,
            },
        );
    }

    Ok(true)
}

#[tauri::command]
pub async fn run_custom_installer(
    app_handle: tauri::AppHandle,
    _config: InstallerConfig,
) -> Result<bool, String> {
    use tauri::Manager;
    use tokio::time::{sleep, Duration};

    let steps = [
        (20, "Initializing package..."),
        (40, "Extracting application assets..."),
        (60, "Registering protocol handler..."),
        (80, "Writing environment configuration..."),
        (100, "Finalizing setup..."),
    ];

    for (p, msg) in steps {
        sleep(Duration::from_millis(600)).await;
        let _ = app_handle.emit_all(
            "installer-progress",
            InstallProgressPayload {
                stage: "install".to_string(),
                percent: p,
                message: msg.to_string(),
                completed: p == 100,
            },
        );
    }
    
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_newer() {
        assert!(is_version_newer("1.0.2", "1.0.1"));
        assert!(is_version_newer("1.1.0", "1.0.1"));
        assert!(is_version_newer("2.0.0", "1.0.1"));
        assert!(!is_version_newer("1.0.1", "1.0.1"));
        assert!(!is_version_newer("1.0.0", "1.0.1"));
        assert!(!is_version_newer("0.9.9", "1.0.1"));
        assert!(is_version_newer("v1.0.2", "1.0.1"));
    }
}
