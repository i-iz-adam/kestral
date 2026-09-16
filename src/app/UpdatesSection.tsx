import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
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

    if (!updateResult && !checking && !error) {
      checkAppUpdates();
    }

    listen<InstallProgressPayload>("update-progress", (event) => {
      setUpdateProgress(event.payload);
      if (event.payload.completed) {
        setTimeout(() => {
          setInstallingUpdate(false);
        }, 2500);
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
    setUpdateProgress({ stage: "init", percent: 0, message: "Preparing update download...", completed: false });
    try {
      await invoke("download_and_install_update", { downloadUrl: updateResult?.download_url });
    } catch (err) {
      console.error(err);
      setInstallingUpdate(false);
    }
  };

  const handleRunInstaller = async () => {
    setInstallingCustom(true);
    setCustomDone(false);
    setCustomProgress({ stage: "init", percent: 0, message: "Initializing environment setup...", completed: false });
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
      {/* Hero Banner Header */}
      <div className="updates-hero-banner">
        <div className="updates-hero-info">
          <div className="updates-hero-title-row">
            <h2>Application Updates & Setup</h2>
            <span className="updates-channel-tag">Stable Release Channel</span>
          </div>
          <p className="updates-hero-desc">
            Keep Kestrel updated with the latest release features, review changelogs, or configure your local environment installation.
          </p>
        </div>
        <div className="updates-status-badge-container">
          <div className={`status-orb ${updateResult?.has_update ? "has-update" : "up-to-date"}`} />
          <div className="status-badge-text">
            <span className="current-ver-label">Installed: v{updateResult?.current_version || "1.0.4"}</span>
            {updateResult?.has_update ? (
              <span className="update-available-pill">New v{updateResult.latest_version} Available!</span>
            ) : (
              <span className="up-to-date-pill">System Up to Date</span>
            )}
          </div>
        </div>
      </div>

      {/* Main Grid */}
      <div className="updates-grid-layout">
        {/* Left Column: Software Updates */}
        <div className="update-card glass-panel">
          <div className="card-header">
            <div className="card-icon-wrap violet">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                <polyline points="7 10 12 15 17 10" />
                <line x1="12" y1="15" x2="12" y2="3" />
              </svg>
            </div>
            <div>
              <h3>Software Updates</h3>
              <p className="card-subtitle">GitHub release channel and OTA installation</p>
            </div>
          </div>

          <div className="card-body">
            {checking ? (
              <div className="update-status-box checking">
                <div className="spinner-orb" />
                <p>Checking GitHub release feed...</p>
              </div>
            ) : error ? (
              <div className="update-status-box error">
                <div className="error-icon">⚠️</div>
                <div className="error-details">
                  <p className="error-text">{error}</p>
                  <button onClick={handleCheckUpdates} className="btn-secondary small-btn">
                    Retry Check
                  </button>
                </div>
              </div>
            ) : updateResult?.has_update ? (
              <div className="update-status-box available">
                <div className="release-highlight">
                  <div>
                    <span className="release-version-tag">v{updateResult.latest_version}</span>
                    <h4 className="release-title">{updateResult.release_name}</h4>
                  </div>
                  {updateResult.published_at && (
                    <span className="release-date">
                      Published: {new Date(updateResult.published_at).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' })}
                    </span>
                  )}
                </div>
                {updateResult.release_notes && (
                  <div className="release-notes-preview">
                    <span className="release-notes-heading">Changelog Highlights</span>
                    <ul className="commit-list">
                      {updateResult.release_notes.split("\n").slice(0, 6).map((line, i) => {
                        const trimmed = line.trim();
                        if (!trimmed) return null;
                        const m = trimmed.match(/^\* (.+) \(([a-f0-9]+)\)$/);
                        if (m) {
                          return (
                            <li key={i} className="commit-item">
                              <span className="commit-badge">{m[2].substring(0, 7)}</span>
                              <span className="commit-msg">{m[1]}</span>
                            </li>
                          );
                        }
                        return (
                          <li key={i} className="commit-item generic">
                            <span className="commit-bullet">•</span>
                            <span className="commit-msg">{trimmed.replace(/^[*-]\s*/, '')}</span>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                )}
              </div>
            ) : (
              <div className="update-status-box up-to-date">
                <div className="check-success-circle">
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                    <path d="M20 6L9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                </div>
                <div>
                  <h4 className="up-to-date-title">You're on the latest build</h4>
                  <p className="up-to-date-subtitle">Kestrel v{updateResult?.current_version || "1.0.4"} is active and running cleanly.</p>
                </div>
              </div>
            )}

            {installingUpdate && updateProgress && (
              <div className="live-progress-container">
                <div className="progress-status-row">
                  <span className="progress-msg">{updateProgress.message}</span>
                  <span className="progress-pct">{updateProgress.percent}%</span>
                </div>
                <div className="progress-bar-container">
                  <div
                    className="progress-bar-fill shimmer-effect"
                    style={{ width: `${updateProgress.percent}%` }}
                  />
                </div>
              </div>
            )}
          </div>

          <div className="card-footer">
            <button
              onClick={handleCheckUpdates}
              disabled={checking || installingUpdate}
              className="btn-secondary"
            >
              {checking ? "Checking..." : "Check for Updates"}
            </button>
            {updateResult?.has_update && !installingUpdate && (
              <button onClick={handleInstallUpdate} className="btn-primary pulse-glow">
                Install Update Now
              </button>
            )}
          </div>
        </div>

        {/* Right Column: Custom Installer & Setup Wizard */}
        <div className="update-card glass-panel">
          <div className="card-header">
            <div className="card-icon-wrap gold">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 2L2 7l10 5 10-5-10-5z" />
                <path d="M2 17l10 5 10-5" />
                <path d="M2 12l10 5 10-5" />
              </svg>
            </div>
            <div>
              <h3>Custom Installer & Environment</h3>
              <p className="card-subtitle">Configure installation paths, shortcuts, and protocols</p>
            </div>
          </div>

          <div className="card-body">
            {!installingCustom && !customDone ? (
              <div className="installer-config-form">
                <div className="form-group">
                  <label className="form-label">Target Directory</label>
                  <div className="input-with-icon">
                    <span className="input-icon">📁</span>
                    <input
                      type="text"
                      className="text-input"
                      value={installerConfig.install_dir}
                      onChange={(e) => setInstallerConfig({ ...installerConfig, install_dir: e.target.value })}
                    />
                  </div>
                </div>

                <div className="toggle-grid">
                  <label className="checkbox-card" onClick={() => handleToggleConfig("create_desktop_shortcut")}>
                    <input
                      type="checkbox"
                      checked={installerConfig.create_desktop_shortcut}
                      onChange={() => {}}
                    />
                    <div className="checkbox-info">
                      <span className="checkbox-title">Desktop Shortcut</span>
                      <span className="checkbox-desc">Add Kestrel icon to desktop</span>
                    </div>
                  </label>

                  <label className="checkbox-card" onClick={() => handleToggleConfig("create_start_menu_shortcut")}>
                    <input
                      type="checkbox"
                      checked={installerConfig.create_start_menu_shortcut}
                      onChange={() => {}}
                    />
                    <div className="checkbox-info">
                      <span className="checkbox-title">Start Menu Entry</span>
                      <span className="checkbox-desc">Add entry to application menu</span>
                    </div>
                  </label>

                  <label className="checkbox-card" onClick={() => handleToggleConfig("register_protocol")}>
                    <input
                      type="checkbox"
                      checked={installerConfig.register_protocol}
                      onChange={() => {}}
                    />
                    <div className="checkbox-info">
                      <span className="checkbox-title">Register Protocol</span>
                      <span className="checkbox-desc">Enable kestrel:// deep links</span>
                    </div>
                  </label>

                  <label className="checkbox-card" onClick={() => handleToggleConfig("launch_on_finish")}>
                    <input
                      type="checkbox"
                      checked={installerConfig.launch_on_finish}
                      onChange={() => {}}
                    />
                    <div className="checkbox-info">
                      <span className="checkbox-title">Launch on Finish</span>
                      <span className="checkbox-desc">Automatically launch Kestrel</span>
                    </div>
                  </label>
                </div>
              </div>
            ) : installingCustom ? (
              <div className="live-progress-container">
                <div className="progress-status-row">
                  <span className="progress-msg">{customProgress?.message || "Running setup..."}</span>
                  <span className="progress-pct">{customProgress?.percent || 0}%</span>
                </div>
                <div className="progress-bar-container">
                  <div
                    className="progress-bar-fill gold-gradient shimmer-effect"
                    style={{ width: `${customProgress?.percent || 0}%` }}
                  />
                </div>
              </div>
            ) : (
              <div className="installer-done-box">
                <div className="success-icon-wrap">✨</div>
                <h4>Environment Successfully Configured!</h4>
                <p>Kestrel settings and shortcuts have been saved and applied.</p>
              </div>
            )}
          </div>

          <div className="card-footer">
            {!installingCustom && !customDone && (
              <button onClick={handleRunInstaller} className="btn-primary gold-btn">
                Run Setup Wizard
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
