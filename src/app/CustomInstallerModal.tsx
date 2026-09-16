import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

interface CustomInstallerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onComplete?: () => void;
}

interface InstallProgressPayload {
  stage: string;
  percent: number;
  message: string;
  completed: boolean;
}

export default function CustomInstallerModal({ isOpen, onClose, onComplete }: CustomInstallerModalProps) {
  const [config, setConfig] = useState({
    install_dir: "%APPDATA%\\Kestrel",
    create_desktop_shortcut: true,
    create_start_menu_shortcut: true,
    register_protocol: true,
    launch_on_finish: true,
  });

  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgressPayload | null>(null);
  const [done, setDone] = useState(false);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    if (isOpen) {
      listen<InstallProgressPayload>("installer-progress", (event) => {
        setProgress(event.payload);
        if (event.payload.completed) {
          setDone(true);
          setInstalling(false);
          if (onComplete) onComplete();
        }
      }).then((un) => (unlisten = un));
    }
    return () => {
      if (unlisten) unlisten();
      setInstalling(false);
      setDone(false);
      setProgress(null);
    };
  }, [isOpen, onComplete]);

  const handleInstall = async () => {
    setInstalling(true);
    setProgress({ stage: "init", percent: 0, message: "Preparing runes...", completed: false });
    try {
      await invoke("run_custom_installer", { config });
    } catch (err) {
      console.error(err);
      setInstalling(false);
    }
  };

  const handleToggle = (key: keyof typeof config) => {
    setConfig((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay installer-modal-overlay">
      <div className="modal-content installer-modal grimoire-theme">
        <div className="installer-header">
          <h2>Kestrel Setup Explorer</h2>
          <div className="ambient-motes"></div>
        </div>

        <div className="installer-body">
          {!installing && !done ? (
            <div className="installer-config">
              <div className="form-group">
                <label>Target Directory</label>
                <input 
                  type="text" 
                  value={config.install_dir} 
                  onChange={(e) => setConfig({ ...config, install_dir: e.target.value })} 
                />
              </div>
              <div className="toggle-group">
                <label>
                  <input 
                    type="checkbox" 
                    checked={config.create_desktop_shortcut} 
                    onChange={() => handleToggle("create_desktop_shortcut")} 
                  />
                  Create Desktop Shortcut
                </label>
                <label>
                  <input 
                    type="checkbox" 
                    checked={config.create_start_menu_shortcut} 
                    onChange={() => handleToggle("create_start_menu_shortcut")} 
                  />
                  Create Start Menu Entry
                </label>
                <label>
                  <input 
                    type="checkbox" 
                    checked={config.register_protocol} 
                    onChange={() => handleToggle("register_protocol")} 
                  />
                  Register Protocol (kestrel://)
                </label>
                <label>
                  <input 
                    type="checkbox" 
                    checked={config.launch_on_finish} 
                    onChange={() => handleToggle("launch_on_finish")} 
                  />
                  Launch on finish
                </label>
              </div>
            </div>
          ) : !done ? (
            <div className="installer-progress-view rune-cast">
              <div className="progress-bar-container">
                <div 
                  className="progress-bar-fill gold-gradient" 
                  style={{ width: `${progress?.percent || 0}%` }}
                />
              </div>
              <div className="progress-status">
                <span>{progress?.message}</span>
                <span>{progress?.percent}%</span>
              </div>
            </div>
          ) : (
            <div className="installer-done rune-flare">
              <h3>Installation Complete!</h3>
              <p>Kestrel is ready to take flight.</p>
            </div>
          )}
        </div>

        <div className="installer-actions">
          {!installing && !done && (
            <>
              <button onClick={onClose} className="btn-secondary">Cancel</button>
              <button onClick={handleInstall} className="btn-primary">Install</button>
            </>
          )}
          {done && (
            <button onClick={onClose} className="btn-primary">Launch Kestrel</button>
          )}
        </div>
      </div>
    </div>
  );
}
