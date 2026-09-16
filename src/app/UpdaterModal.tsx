import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useUpdaterStore, checkAppUpdates } from "./updaterStore";

interface UpdaterModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface InstallProgressPayload {
  stage: string;
  percent: number;
  message: string;
  completed: boolean;
}

export default function UpdaterModal({ isOpen, onClose }: UpdaterModalProps) {
  const { checking, result: updateResult, error } = useUpdaterStore();
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgressPayload | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    if (isOpen) {
      if (!updateResult && !checking && !error) {
        checkAppUpdates();
      }
      listen<InstallProgressPayload>("update-progress", (event) => {
        setProgress(event.payload);
        if (event.payload.completed) {
          setTimeout(() => {
            setInstalling(false);
            onClose();
          }, 1500);
        }
      }).then((un) => (unlisten = un));
    }
    return () => {
      if (unlisten) unlisten();
      setProgress(null);
      setInstalling(false);
    };
  }, [isOpen, onClose, updateResult, checking, error]);

  const handleCheck = async () => {
    await checkAppUpdates(true);
  };

  const handleInstall = async () => {
    setInstalling(true);
    setProgress({ stage: "init", percent: 0, message: "Initializing...", completed: false });
    try {
      await invoke("download_and_install_update", { downloadUrl: updateResult?.download_url });
    } catch (err) {
      console.error(err);
      setInstalling(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay">
      <div className="modal-content updater-modal">
        <div className="updater-header">
          <h2>Kestrel Updater</h2>
          <div className="orb-indicator" />
          {updateResult && (
            <span className="version-badge">
              v{updateResult.current_version} &rarr; v{updateResult.latest_version}
            </span>
          )}
        </div>

        <div className="updater-body">
          {checking ? (
            <div className="updater-init">
              <p>Checking for the latest release...</p>
              <button disabled className="btn-primary">
                Checking...
              </button>
            </div>
          ) : error ? (
            <div className="updater-init">
              <p className="error-text" style={{ color: "var(--red, #ff5555)", marginBottom: 12 }}>
                {error}
              </p>
              <button onClick={handleCheck} className="btn-primary">
                Retry Check
              </button>
            </div>
          ) : !updateResult ? (
            <div className="updater-init">
              <p>Check for the latest features and fixes.</p>
              <button onClick={handleCheck} className="btn-primary">
                Check for Updates
              </button>
            </div>
          ) : updateResult.has_update ? (
            <div className="updater-release-notes">
              <h3>{updateResult.release_name}</h3>
              <ul className="commit-list">
                {updateResult.release_notes.split("\n").map((line, i) => {
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
          ) : (
            <div className="updater-init">
              <p>You are on the latest version (v{updateResult.current_version}).</p>
              <button onClick={handleCheck} className="btn-secondary" style={{ marginTop: 12 }}>
                Check Again
              </button>
            </div>
          )}

          {installing && progress && (
            <div className="updater-progress">
              <div className="progress-bar-container">
                <div
                  className="progress-bar-fill"
                  style={{ width: `${progress.percent}%` }}
                />
              </div>
              <div className="progress-status">
                <span>{progress.message}</span>
                <span>{progress.percent}%</span>
              </div>
            </div>
          )}
        </div>

        <div className="updater-actions">
          <button onClick={onClose} disabled={installing} className="btn-secondary">
            Close
          </button>
          {updateResult?.has_update && !installing && (
            <button onClick={handleInstall} className="btn-primary">
              Install Update
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
