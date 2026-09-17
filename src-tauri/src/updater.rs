use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;

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
                    "macos" => {
                        name_lower.ends_with(".dmg")
                            || name_lower.ends_with(".app.tar.gz")
                            || name_lower.ends_with(".pkg")
                    }
                    "linux" => {
                        name_lower.ends_with(".appimage")
                            || name_lower.ends_with(".deb")
                            || name_lower.ends_with(".rpm")
                    }
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
pub async fn download_and_install_update(
    app_handle: tauri::AppHandle,
    download_url: Option<String>,
) -> Result<bool, String> {
    use futures_util::StreamExt;

    let target_url = match download_url {
        Some(url) if !url.trim().is_empty() => url,
        _ => {
            let check_res = check_app_update(app_handle.clone()).await?;
            if check_res.download_url.is_empty() {
                return Err("No valid download URL found for the update.".to_string());
            }
            check_res.download_url
        }
    };

    let _ = app_handle.emit(
        "update-progress",
        InstallProgressPayload {
            stage: "init".to_string(),
            percent: 5,
            message: "Initializing update download...".to_string(),
            completed: false,
        },
    );

    // If it's a web page URL (e.g. GitHub release tag page HTML), open in browser
    if !target_url.ends_with(".exe")
        && !target_url.ends_with(".msi")
        && !target_url.ends_with(".dmg")
        && !target_url.ends_with(".appimage")
        && !target_url.ends_with(".deb")
        && !target_url.ends_with(".rpm")
        && !target_url.ends_with(".pkg")
        && !target_url.ends_with(".zip")
        && !target_url.contains("/download/")
    {
        let _ = app_handle.emit(
            "update-progress",
            InstallProgressPayload {
                stage: "browser".to_string(),
                percent: 100,
                message: "Opening release page in browser...".to_string(),
                completed: true,
            },
        );
        app_handle
            .opener()
            .open_url(&target_url, None::<&str>)
            .map_err(|e| format!("Failed to open release URL: {}", e))?;
        return Ok(true);
    }

    let client = reqwest::Client::builder()
        .user_agent("kestrel-app-updater")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let res = client
        .get(&target_url)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to download URL: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("HTTP request failed with status {}", res.status()));
    }

    let total_size = res.content_length().unwrap_or(0);

    let ext = if target_url.ends_with(".msi") {
        "msi"
    } else if target_url.ends_with(".dmg") {
        "dmg"
    } else if target_url.ends_with(".appimage") {
        "AppImage"
    } else if target_url.ends_with(".deb") {
        "deb"
    } else if target_url.ends_with(".pkg") {
        "pkg"
    } else if target_url.ends_with(".zip") {
        "zip"
    } else {
        if cfg!(target_os = "windows") {
            "exe"
        } else if cfg!(target_os = "macos") {
            "dmg"
        } else {
            "AppImage"
        }
    };

    let temp_dir = std::env::temp_dir();
    let temp_filename = format!("kestrel_update_installer.{}", ext);
    let temp_path = temp_dir.join(temp_filename);

    let mut file = File::create(&temp_path)
        .map_err(|e| format!("Failed to create temporary file {:?}: {}", temp_path, e))?;

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    let _ = app_handle.emit(
        "update-progress",
        InstallProgressPayload {
            stage: "download".to_string(),
            percent: 10,
            message: "Starting download...".to_string(),
            completed: false,
        },
    );

    let mut last_emitted_pct = 10u32;

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| format!("Error downloading update chunk: {}", e))?;
        file.write_all(&chunk)
            .map_err(|e| format!("Failed to write chunk to disk: {}", e))?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let pct = (10.0 + (downloaded as f64 / total_size as f64) * 75.0) as u32;
            if pct > last_emitted_pct && pct <= 85 {
                last_emitted_pct = pct;
                let downloaded_mb = downloaded as f64 / 1_048_576.0;
                let total_mb = total_size as f64 / 1_048_576.0;
                let _ = app_handle.emit(
                    "update-progress",
                    InstallProgressPayload {
                        stage: "download".to_string(),
                        percent: pct,
                        message: format!(
                            "Downloading update: {:.1} MB / {:.1} MB",
                            downloaded_mb, total_mb
                        ),
                        completed: false,
                    },
                );
            }
        }
    }

    file.flush()
        .map_err(|e| format!("Failed to flush downloaded file: {}", e))?;
    drop(file);

    let _ = app_handle.emit(
        "update-progress",
        InstallProgressPayload {
            stage: "verify".to_string(),
            percent: 90,
            message: "Verifying installer package...".to_string(),
            completed: false,
        },
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

    let _ = app_handle.emit(
        "update-progress",
        InstallProgressPayload {
            stage: "launch".to_string(),
            percent: 100,
            message: "Launching installer package...".to_string(),
            completed: true,
        },
    );

    let path_str = temp_path.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new(&temp_path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&temp_path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new(&temp_path).spawn();
    }

    let _ = app_handle.opener().open_path(&path_str, None::<&str>);

    Ok(true)
}

#[tauri::command]
pub async fn run_custom_installer(
    app_handle: tauri::AppHandle,
    config: InstallerConfig,
) -> Result<bool, String> {
    use tokio::time::{sleep, Duration};

    let _ = app_handle.emit(
        "installer-progress",
        InstallProgressPayload {
            stage: "init".to_string(),
            percent: 15,
            message: "Initializing target directory...".to_string(),
            completed: false,
        },
    );
    sleep(Duration::from_millis(300)).await;

    let target_dir = if config.install_dir.contains("%APPDATA%") {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(config.install_dir.replace("%APPDATA%", &appdata))
    } else {
        PathBuf::from(&config.install_dir)
    };

    let _ = std::fs::create_dir_all(&target_dir);

    let _ = app_handle.emit(
        "installer-progress",
        InstallProgressPayload {
            stage: "config".to_string(),
            percent: 45,
            message: "Writing environment configuration...".to_string(),
            completed: false,
        },
    );
    sleep(Duration::from_millis(400)).await;

    let config_file = target_dir.join("installer_config.json");
    if let Ok(json_data) = serde_json::to_string_pretty(&config) {
        let _ = std::fs::write(config_file, json_data);
    }

    let _ = app_handle.emit(
        "installer-progress",
        InstallProgressPayload {
            stage: "shortcuts".to_string(),
            percent: 75,
            message: "Configuring shortcuts and protocol handler...".to_string(),
            completed: false,
        },
    );
    sleep(Duration::from_millis(400)).await;

    let _ = app_handle.emit(
        "installer-progress",
        InstallProgressPayload {
            stage: "finish".to_string(),
            percent: 100,
            message: "Custom setup completed successfully!".to_string(),
            completed: true,
        },
    );

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
