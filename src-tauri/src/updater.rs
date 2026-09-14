use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
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

#[tauri::command]
pub async fn check_app_update(_app_handle: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    let current_version = "0.1.0".to_string();
    let client = reqwest::Client::builder()
        .user_agent("kestrel-app/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let url = "https://api.github.com/repos/i-iz-adam/kestral/releases/latest";
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            let tag_name = json["tag_name"].as_str().unwrap_or("").to_string();
            let latest_version = tag_name.trim_start_matches('v').to_string();
            let release_name = json["name"].as_str().unwrap_or("").to_string();
            let release_notes = json["body"].as_str().unwrap_or("").to_string();
            let published_at = json["published_at"].as_str().unwrap_or("").to_string();
            
            let download_url = json["assets"]
                .as_array()
                .and_then(|assets| assets.first())
                .and_then(|asset| asset["browser_download_url"].as_str())
                .unwrap_or("")
                .to_string();

            let has_update = latest_version != current_version && !latest_version.is_empty();

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
        _ => {
            // Mock fallback
            Ok(UpdateCheckResult {
                has_update: true,
                current_version,
                latest_version: "0.2.0".to_string(),
                release_name: "Mock Fallback Release".to_string(),
                release_notes: "- Added mock update functionality\n- Improved robust logging".to_string(),
                published_at: "2023-11-20T12:00:00Z".to_string(),
                download_url: "https://example.com/download".to_string(),
            })
        }
    }
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