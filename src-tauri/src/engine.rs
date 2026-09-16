use serde::Serialize;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineStatus {
    Stopped,
    /// Process launched, not yet confirmed reachable — see confirm_running.
    Starting,
    Running,
    Error,
}

pub struct EngineHandle {
    child: Option<Child>,
    pub status: EngineStatus,
    pub last_error: Option<String>,
}

impl Default for EngineHandle {
    fn default() -> Self {
        EngineHandle { child: None, status: EngineStatus::Stopped, last_error: None }
    }
}

pub struct EngineState(pub Mutex<EngineHandle>);

impl Default for EngineState {
    fn default() -> Self {
        EngineState(Mutex::new(EngineHandle::default()))
    }
}

#[derive(Clone, Serialize)]
struct EngineEvent {
    status: EngineStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn emit(app_handle: &tauri::AppHandle, status: EngineStatus, error: Option<String>) {
    let _ = app_handle.emit("engine://status", EngineEvent { status, error });
}

fn engine_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    app_handle
        .path()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("engine")
}

/// Where a local, pinned install of OmniRoute lives once installed — see
/// `install()`. This is NOT a compiled sidecar binary: OmniRoute runs its
/// TypeScript source directly via `tsx` at startup and depends on native
/// per-platform binaries (sharp, for image handling), so this app runs it
/// through Node the normal way rather than trying to package it into a
/// single portable executable. See OMNIROUTE_INTEGRATION.md for the full
/// reasoning — that was the actual plan for this pass before a closer look
/// at the real package made it clear it wasn't a good fit.
fn local_bin_path(app_handle: &tauri::AppHandle) -> PathBuf {
    engine_dir(app_handle)
        .join("node_modules")
        .join("omniroute")
        .join("bin")
        .join("omniroute.mjs")
}

pub fn is_installed(app_handle: &tauri::AppHandle) -> bool {
    local_bin_path(app_handle).exists()
}

#[derive(Clone, Serialize)]
struct InstallLogEvent {
    line: String,
}

#[derive(Clone, Serialize)]
struct InstallDoneEvent {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Runs `npm install omniroute --prefix <engine_dir>` — a real, one-time
/// ~450MB download the package itself pulls in (Next.js, native image
/// libs, and friends). Streams npm's own output back to the UI as it runs
/// rather than leaving the person staring at a spinner with no idea
/// whether anything is happening, since this can take a few minutes.
pub async fn install(app_handle: tauri::AppHandle) {
    let dir = engine_dir(&app_handle);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        let _ = app_handle.emit(
            "engine://install-done",
            InstallDoneEvent { success: false, error: Some(e.to_string()) },
        );
        return;
    }

    let mut cmd = tokio::process::Command::new("npm");
    cmd.args([
        "install",
        "omniroute",
        "--prefix",
        &dir.to_string_lossy(),
        "--no-audit",
        "--no-fund",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = app_handle.emit(
                "engine://install-done",
                InstallDoneEvent { success: false, error: Some(e.to_string()) },
            );
            return;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = handle.emit("engine://install-log", InstallLogEvent { line });
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let handle = app_handle.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = handle.emit("engine://install-log", InstallLogEvent { line });
            }
        });
    }

    match child.wait().await {
        Ok(status) if status.success() => {
            let mut cfg = crate::config::load_engine_config(&app_handle);
            cfg.use_local_install = true;
            let _ = crate::config::save_engine_config(&app_handle, &cfg);
            let _ = app_handle
                .emit("engine://install-done", InstallDoneEvent { success: true, error: None });
        }
        Ok(status) => {
            let _ = app_handle.emit(
                "engine://install-done",
                InstallDoneEvent {
                    success: false,
                    error: Some(format!("npm install exited with {}", status)),
                },
            );
        }
        Err(e) => {
            let _ = app_handle.emit(
                "engine://install-done",
                InstallDoneEvent { success: false, error: Some(e.to_string()) },
            );
        }
    }
}

/// Launches OmniRoute as a managed child process — the pinned local install
/// if one exists and is preferred (fast, deterministic version), otherwise
/// the configured fallback command (`npx -y omniroute ...` by default,
/// which re-resolves the package each launch). Either way this runs
/// through a shell so `npx`/`node` resolve the same way they would from a
/// terminal, including on Windows where the executable is really a `.cmd`
/// shim. OmniRoute's own data directory is pointed inside this app's
/// config folder via DATA_DIR, so its storage lives alongside ours instead
/// of in `~/.omniroute`.
pub fn start(app_handle: tauri::AppHandle) {
    {
        let state = app_handle.state::<EngineState>();
        let mut h = state.0.lock().unwrap();
        if matches!(h.status, EngineStatus::Running | EngineStatus::Starting) {
            return;
        }
        h.status = EngineStatus::Starting;
        h.last_error = None;
    }
    emit(&app_handle, EngineStatus::Starting, None);

    let cfg = crate::config::load_engine_config(&app_handle);
    let full_command = if cfg.use_local_install && is_installed(&app_handle) {
        let bin = local_bin_path(&app_handle);
        format!("node \"{}\" --no-open --no-tray", bin.to_string_lossy())
    } else {
        format!("{} {}", cfg.command, cfg.args.join(" "))
    };

    let data_dir = engine_dir(&app_handle).join("data");
    let _ = std::fs::create_dir_all(&data_dir);

    let (shell, shell_flag) = if cfg!(target_os = "windows") {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_flag)
        .arg(&full_command)
        .env("DATA_DIR", &data_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let spawn_result = cmd.spawn();

    let state = app_handle.state::<EngineState>();
    match spawn_result {
        Ok(child) => {
            let mut h = state.0.lock().unwrap();
            h.child = Some(child);
            // Process launched; actual readiness is confirmed by the
            // frontend polling test_omniroute_connection while status is
            // Starting, then calling confirm_running.
        }
        Err(e) => {
            let mut h = state.0.lock().unwrap();
            h.status = EngineStatus::Error;
            h.last_error = Some(e.to_string());
            drop(h);
            emit(&app_handle, EngineStatus::Error, Some(e.to_string()));
        }
    }
}

pub fn confirm_running(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();
    let mut h = state.0.lock().unwrap();
    if h.status != EngineStatus::Running {
        h.status = EngineStatus::Running;
        h.last_error = None;
        drop(h);
        emit(app_handle, EngineStatus::Running, None);
    }
}

/// Kills the process tree, not just the immediate child. On Windows,
/// `Child::kill()` alone only signals the `cmd.exe` shell we spawned
/// through — the actual `npx`/`node` process underneath survives it. This
/// uses `taskkill /T` to bring down the whole tree instead.
pub fn stop(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();
    let mut h = state.0.lock().unwrap();
    if let Some(mut child) = h.child.take() {
        let pid = child.id();
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new("taskkill");
            cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
            cmd.creation_flags(0x0800_0000);
            let _ = cmd.status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    h.status = EngineStatus::Stopped;
    h.last_error = None;
    drop(h);
    emit(app_handle, EngineStatus::Stopped, None);
}

pub fn status(app_handle: &tauri::AppHandle) -> (EngineStatus, Option<String>) {
    let state = app_handle.state::<EngineState>();
    let h = state.0.lock().unwrap();
    (h.status, h.last_error.clone())
}
