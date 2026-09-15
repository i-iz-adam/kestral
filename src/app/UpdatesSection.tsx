import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useUpdaterStore, checkAppUpdates } from "./updaterStore";

interface InstallProgressPayload {
  stage: string;
  percent: number;
  message: string;
  completed: boolean;
}

export default function UpdatesSection() {
  const { checking, result: updateResult, error } = useUpdaterStore();
  const [installingUpdate, setInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<InstallProgressPayload | null>(null);

  const [installerConfig, setInstallerConfig] = useState({
    install_dir: "%APPDATA%\\Kestrel",
    create_desktop_shortcut: true,
    create_start_menu_shortcut: true,
    register_protocol: true,
    launch_on_finish: true,
  });
  const [installingCustom, setInstallingCustom] = useState(false);
  const [customProgress, setCustomProgress] = useState<InstallProgressPayload | null>(null);
  const [customDone, setCustomDone] = useState(false);

  useEffect(() => {
    let unlistenUpdate: UnlistenFn | undefined;
    let unlistenInstaller: UnlistenFn | undefined;

    // Background check on load if not checked yet
    if (!updateResult && !checking && !error) {
      checkAppUpdates();
    }

    listen<InstallProgressPayload>("update-progress", (event) => {
      setUpdateProgress(event.payload);
      if (event.payload.completed) {
        setTimeout(() => {
          setInstallingUpdate(false);
        }, 1500);
      }
    }).then((un) => (unlistenUpdate = un));

    listen<InstallProgressPayload>("installer-progress", (event) => {
      setCustomProgress(event.payload);
      if (event.payload.completed) {
        setCustomDone(true);
        setInstallingCustom(false);
      }
    }).then((un) => (unlistenInstaller = un));

    return () => {
      if (unlistenUpdate) unlistenUpdate();
      if (unlistenInstaller) unlistenInstaller();
    };
  }, [updateResult, checking, error]);

  const handleCheckUpdates = async () => {
    await checkAppUpdates(true);
  };

  const handleInstallUpdate = async () => {
    setInstallingUpdate(true);
    setUpdateProgress({ stage: "init", percent: 0, message: "Preparing update...", completed: false });
    try {
      await invoke("download_and_install_update");
    } catch (err) {
      console.error(err);
      setInstallingUpdate(false);
    }
  };

  const handleRunInstaller = async () => {
    setInstallingCustom(true);
    setCustomDone(false);
    setCustomProgress({ stage: "init", percent: 0, message: "Initializing package...", completed: false });
    try {
      await invoke("run_custom_installer", { config: installerConfig });
    } catch (err) {
      console.error(err);
      setInstallingCustom(false);
    }
  };

  const handleToggleConfig = (key: keyof typeof installerConfig) => {
    setInstallerConfig((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  return (
    <div className="updates-section-container">
      <div className="updates-hero-banner">
        <div className="updates-hero-info">
          <h2>Application Updates & Setup</h2>
          <p className="hint">
            Keep Kestrel running on the latest cutting-edge release, review changelogs, or customize your local environment installation.
          </p>
        </div>
        <div className="updates-status-badge-container">
          <div className={`status-orb ${updateResult?.has_update ? "has-update" : "up-to-date"}`} />
          <div className="status-badge-text">
            <span className="current-ver-label">Current: v{updateResult?.current_version || "1.0.2"}</span>
            {updateResult?.has_update ? (
              <span className="update-available-pill">New v{updateResult.latest_version} Available!</span>
            ) : (
              <span className="up-to-date-pill">System Up to Date</span>
            )}
          </div>
        </div>
      </div>

      <div className="updates-grid-layout">
        {/* Left Column: Software Updates Card */}
        <div className="update-card glass-panel">
          <div className="card-header">
            <div className="card-icon">🚀</div>
            <div>
              <h3>Software Updates</h3>
              <p className="card-subtitle">Check release feeds and install OTA updates</p>
            </div>
          </div>

          <div className="card-body">
            {checking ? (
              <div className="update-status-box checking">
                <div className="spinner-orb" />
                <p>Checking GitHub release channel...</p>
              </div>
            ) : error ? (
              <div className="update-status-box error">
                <p className="error-text">{error}</p>
                <button onClick={handleCheckUpdates} className="btn-secondary">
                  Retry Check
                </button>
              </div>
            ) : updateResult?.has_update ? (
              <div className="update-status-box available">
                <div className="release-highlight">
                  <h4>{updateResult.release_name}</h4>
                  <span className="release-date">Published: {new Date(updateResult.published_at).toLocaleDateString()}</span>
                </div>
                <div className="release-notes-preview">
                  <ul className="commit-list">
                    {updateResult.release_notes.split("\n").slice(0, 5).map((line, i) => {
                      const m = line.match(/^\* (.+) \(([a-f0-9]+)\)$/);
                      if (m) {
                        return (
                          <li key={i} className="commit-item">
                            <span className="commit-badge">{m[2].substring(0, 7)}</span>
                            <span className="commit-msg">{m[1]}</span>
                          </li>
                        );
                      }
                      return <li key={i}>{line}</li>;
                    })}
                  </ul>
                </div>
              </div>
            ) : (
              <div className="update-status-box up-to-date">
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="#50fa7b" strokeWidth="2.5">
                  <path d="M20 6L9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                <p>You are running the latest stable build (v{updateResult?.current_version || "1.0.2"}).</p>
              </div>
            )}

            {installingUpdate && updateProgress && (
              <div className="live-progress-container animate-fade-in">
                <div className="progress-bar-container">
                  <div
                    className="progress-bar-fill shimmer-effect"
                    style={{ width: `${updateProgress.percent}%` }}
                  />
                </div>
                <div className="progress-status-row">
                  <span>{updateProgress.message}</span>
                  <span className="progress-pct">{updateProgress.percent}%</span>
                </div>
              </div>
            )}
          </div>

          <div className="card-footer">
            <button onClick={handleCheckUpdates} disabled={checking || installingUpdate} className="btn-secondary">
              {checking ? "Checking..." : "Check for Updates"}
            </button>
            {updateResult?.has_update && !installingUpdate && (
              <button onClick={handleInstallUpdate} className="btn-primary pulse-glow">
                Install Update Now
              </button>
            )}
          </div>
        </div>

        {/* Right Column: Custom Installer & Setup Card */}
        <div className="update-card glass-panel">
          <div className="card-header">
            <div className="card-icon">🛠️</div>
            <div>
              <h3>Custom Installer Wizard</h3>
              <p className="card-subtitle">Configure installation paths, shortcuts, and protocols</p>
            </div>
          </div>

          <div className="card-body">
            {!installingCustom && !customDone ? (
              <div className="installer-config-form animate-fade-in">
                <div className="form-group">
                  <label>Target Directory</label>
                  <input
                    type="text"
                    value={installerConfig.install_dir}
                    onChange={(e) => setInstallerConfig({ ...installerConfig, install_dir: e.target.value })}
                  />
                </div>

                <div className="toggle-grid">
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={installerConfig.create_desktop_shortcut}
                      onChange={() => handleToggleConfig("create_desktop_shortcut")}
                    />
                    <span>Desktop Shortcut</span>
                  </label>
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={installerConfig.create_start_menu_shortcut}
                      onChange={() => handleToggleConfig("create_start_menu_shortcut")}
                    />
                    <span>Start Menu Entry</span>
                  </label>
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={installerConfig.register_protocol}
                      onChange={() => handleToggleConfig("register_protocol")}
                    />
                    <span>Register Protocol (kestrel://)</span>
                  </label>
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={installerConfig.launch_on_finish}
                      onChange={() => handleToggleConfig("launch_on_finish")}
                    />
                    <span>Launch on Finish</span>
                  </label>
                </div>
              </div>
            ) : installingCustom ? (
              <div className="live-progress-container animate-fade-in">
                <div className="progress-bar-container">
                  <div
                    className="progress-bar-fill gold-gradient shimmer-effect"
                    style={{ width: `${customProgress?.percent || 0}%` }}
                  />
                </div>
                <div className="progress-status-row">
                  <span>{customProgress?.message}</span>
                  <span className="progress-pct">{customProgress?.percent}%</span>
                </div>
              </div>
            ) : (
              <div className="installer-done-box rune-flare animate-fade-in">
                <div className="success-icon-wrap">✨</div>
                <h4>Setup Successfully Completed!</h4>
                <p>Kestrel environment is configured and ready for action.</p>
              </div>
            )}
          </div>

          <div className="card-footer">
            {!installingCustom && !customDone && (
              <button onClick={handleRunInstaller} className="btn-primary gold-btn">
                Run Custom Installer
              </button>
            )}
            {customDone && (
              <button onClick={() => setCustomDone(false)} className="btn-secondary">
                Configure Again
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
