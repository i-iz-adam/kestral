import { useEffect, useState, useMemo } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open as openShell } from "@tauri-apps/api/shell";
import type { OmniRouteConfigPayload, SessionDefaults } from "../types";
import UpdaterModal from "./UpdaterModal";
import CustomInstallerModal from "./CustomInstallerModal";
import GithubPanel from "./GithubPanel";
import WorkspacePanel from "./WorkspacePanel";
import { useUpdaterStore } from "./updaterStore";

export interface PythonStatusPayload {
  installed: boolean;
  version?: string | null;
  binary?: string | null;
}

export type SettingsTab =
  | "omniroute"
  | "defaults"
  | "github"
  | "workspaces"
  | "sandbox"
  | "updates";

interface SettingsProps {
  initialTab?: SettingsTab;
}

export default function Settings({ initialTab = "omniroute" }: SettingsProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [searchQuery, setSearchQuery] = useState("");

  const [mode, setMode] = useState<"local" | "remote">("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "fail">("idle");
  const [saved, setSaved] = useState(false);

  const [engineCommand, setEngineCommand] = useState("npx");
  const [engineArgs, setEngineArgs] = useState("-y omniroute");
  const [engineAutoStart, setEngineAutoStart] = useState(true);
  const [engineSaved, setEngineSaved] = useState(false);

  const [defaultPlanning, setDefaultPlanning] = useState(true);
  const [defaultSubagents, setDefaultSubagents] = useState(true);
  const [defaultGracefulStop, setDefaultGracefulStop] = useState(true);
  const [defaultsSaved, setDefaultsSaved] = useState(false);

  const [pythonStatus, setPythonStatus] = useState<PythonStatusPayload | null>(null);
  const [checkingPython, setCheckingPython] = useState(false);

  const [updaterOpen, setUpdaterOpen] = useState(false);
  const [installerOpen, setInstallerOpen] = useState(false);
  const { result: updateResult } = useUpdaterStore();

  useEffect(() => {
    invoke<OmniRouteConfigPayload | null>("get_omniroute_config").then((cfg) => {
      if (cfg) {
        setMode(cfg.mode);
        setRemoteUrl(cfg.remote_url ?? "");
        setApiKey(cfg.api_key ?? "");
      }
    });
    invoke<{ command: string; args: string[]; auto_start: boolean }>("get_engine_config").then(
      (cfg) => {
        setEngineCommand(cfg.command);
        setEngineArgs(cfg.args.join(" "));
        setEngineAutoStart(cfg.auto_start);
      }
    );
    invoke<SessionDefaults>("get_session_defaults").then((defs) => {
      if (defs) {
        setDefaultPlanning(defs.planning_enabled ?? true);
        setDefaultSubagents(defs.subagents_enabled ?? true);
        setDefaultGracefulStop(defs.graceful_stop ?? true);
      }
    });
    checkPython();
  }, []);

  const checkPython = () => {
    setCheckingPython(true);
    invoke<PythonStatusPayload>("check_python_installed")
      .then((res) => {
        setPythonStatus(res);
        setCheckingPython(false);
      })
      .catch(() => {
        setPythonStatus({ installed: false });
        setCheckingPython(false);
      });
  };

  const buildConfig = (): OmniRouteConfigPayload => ({
    mode,
    remote_url: mode === "remote" ? remoteUrl : null,
    api_key: mode === "remote" ? apiKey : null,
  });

  const test = async () => {
    setStatus("testing");
    try {
      const ok = await invoke<boolean>("test_omniroute_connection", {
        config: buildConfig(),
      });
      setStatus(ok ? "ok" : "fail");
    } catch {
      setStatus("fail");
    }
  };

  const save = async () => {
    await invoke("save_omniroute_config", { config: buildConfig() });
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  const saveEngine = async () => {
    await invoke("save_engine_config", {
      command: engineCommand,
      args: engineArgs.split(" ").filter(Boolean),
      autoStart: engineAutoStart,
    });
    setEngineSaved(true);
    setTimeout(() => setEngineSaved(false), 1500);
  };

  const saveDefaults = async () => {
    await invoke("save_session_defaults", {
      defaults: {
        planning_enabled: defaultPlanning,
        subagents_enabled: defaultSubagents,
        graceful_stop: defaultGracefulStop,
      },
    });
    setDefaultsSaved(true);
    setTimeout(() => setDefaultsSaved(false), 1500);
  };

  const tabs = [
    { id: "omniroute" as const, label: "OmniRoute & Engine", icon: "🔌", keywords: "omniroute connection engine process mode local remote api key command args" },
    { id: "defaults" as const, label: "Session Defaults", icon: "⚙️", keywords: "session defaults planning mode sub-agents subagents graceful stop" },
    { id: "github" as const, label: "GitHub Integration", icon: "🐙", keywords: "github token personal access token connect disconnect repo issues pull requests" },
    { id: "workspaces" as const, label: "Workspaces", icon: "📁", keywords: "workspaces folder directory project active workspace path add folder" },
    { id: "sandbox" as const, label: "Python Sandbox", icon: "🐍", keywords: "python sandbox execution environment run_python binary path" },
    { id: "updates" as const, label: "Updates & System", icon: "🚀", keywords: "updates installer application version check for updates rerun installer" },
  ];

  const filteredTabs = useMemo(() => {
    if (!searchQuery.trim()) return tabs;
    const q = searchQuery.toLowerCase().trim();
    return tabs.filter(
      (t) =>
        t.label.toLowerCase().includes(q) ||
        t.keywords.toLowerCase().includes(q)
    );
  }, [searchQuery]);

  const renderOmniRouteSection = () => (
    <div className="settings-section-block">
      <h2>OmniRoute & Engine</h2>
      <section>
        <h3>OmniRoute connection</h3>
        <div className="mode-toggle">
          <button
            className={mode === "local" ? "active" : ""}
            onClick={() => setMode("local")}
          >
            Local
          </button>
          <button
            className={mode === "remote" ? "active" : ""}
            onClick={() => setMode("remote")}
          >
            Remote
          </button>
        </div>
        {mode === "remote" && (
          <div className="field-group">
            <label>OmniRoute URL</label>
            <input
              value={remoteUrl}
              onChange={(e) => setRemoteUrl(e.target.value)}
            />
            <label>API key</label>
            <input
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              type="password"
            />
          </div>
        )}
        <div className="row">
          <button onClick={test}>Test connection</button>
          {status === "ok" && <span className="ok">Connected</span>}
          {status === "fail" && <span className="fail">Couldn't connect</span>}
          <button className={`primary ${saved ? "saved" : ""}`} onClick={save}>
            {saved ? "Saved" : "Save"}
          </button>
        </div>
      </section>

      {mode === "local" && (
        <section>
          <h3>Managed OmniRoute process</h3>
          <p className="hint small">
            In local mode this app launches and supervises OmniRoute
            itself — nothing to run separately. Live status and Start/Stop
            controls are in the sidebar; this is just how it's launched.
          </p>
          <div className="field-group">
            <label>Command</label>
            <input
              value={engineCommand}
              onChange={(e) => setEngineCommand(e.target.value)}
            />
            <label>Arguments</label>
            <input
              value={engineArgs}
              onChange={(e) => setEngineArgs(e.target.value)}
            />
          </div>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={engineAutoStart}
              onChange={(e) => setEngineAutoStart(e.target.checked)}
            />
            Start automatically when the app opens
          </label>
          <button
            className={`primary ${engineSaved ? "saved" : ""}`}
            onClick={saveEngine}
            style={{ marginTop: 10 }}
          >
            {engineSaved ? "Saved" : "Save"}
          </button>
        </section>
      )}
    </div>
  );

  const renderDefaultsSection = () => (
    <div className="settings-section-block">
      <h2>Session Defaults</h2>
      <section>
        <h3>Session defaults</h3>
        <p className="hint small">
          Default settings applied to newly created sessions.
        </p>
        <div className="field-group">
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={defaultPlanning}
              onChange={(e) => setDefaultPlanning(e.target.checked)}
            />
            Planning mode by default (approve writes/commands)
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={defaultSubagents}
              onChange={(e) => setDefaultSubagents(e.target.checked)}
            />
            Use sub-agents by default to keep context clean
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={defaultGracefulStop}
              onChange={(e) => setDefaultGracefulStop(e.target.checked)}
            />
            Graceful stop (sub-agents return an overview when you stop)
          </label>
        </div>
        <button
          className={`primary ${defaultsSaved ? "saved" : ""}`}
          onClick={saveDefaults}
          style={{ marginTop: 10 }}
        >
          {defaultsSaved ? "Saved" : "Save"}
        </button>
      </section>
    </div>
  );

  const renderGithubSection = () => (
    <div className="settings-section-block">
      <h2>GitHub Integration</h2>
      <GithubPanel />
    </div>
  );

  const renderWorkspacesSection = () => (
    <div className="settings-section-block">
      <WorkspacePanel />
    </div>
  );

  const renderSandboxSection = () => (
    <div className="settings-section-block">
      <h2>Python Execution Sandbox</h2>
      <section>
        <h3>Python Status</h3>
        <p className="hint small">
          Status of Python installation for the <code>run_python</code> execution tool.
        </p>
        <div style={{ marginTop: 10 }}>
          {checkingPython ? (
            <span className="hint small">Checking Python status...</span>
          ) : pythonStatus?.installed ? (
            <div>
              <span className="ok">Installed ({pythonStatus.version || pythonStatus.binary})</span>
              <p className="hint small" style={{ marginTop: 4 }}>
                The <code>run_python</code> tool is verified and enabled for agent use.
              </p>
            </div>
          ) : (
            <div>
              <span className="fail">Not Installed</span>
              <p className="hint small" style={{ marginTop: 4 }}>
                The <code>run_python</code> tool is disabled until Python is verified on your system PATH.
              </p>
              <div className="row" style={{ marginTop: 8 }}>
                <button onClick={() => openShell("https://www.python.org/downloads/")}>
                  Download Python
                </button>
                <button onClick={checkPython}>Re-check Installation</button>
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );

  const renderUpdatesSection = () => (
    <div className="settings-section-block">
      <h2>Application Updates & Installer</h2>
      <section>
        <h3>Software Updates</h3>
        <p className="hint small">
          Check for software updates or rerun the custom installation setup.
          {updateResult?.has_update && (
            <span style={{ display: "block", marginTop: 6, color: "#50fa7b", fontWeight: 500 }}>
              Update Available: v{updateResult.current_version} &rarr; v{updateResult.latest_version}
            </span>
          )}
        </p>
        <div className="row" style={{ marginTop: 10 }}>
          <button className="primary" onClick={() => setUpdaterOpen(true)}>
            Check for Updates
          </button>
          <button onClick={() => setInstallerOpen(true)}>
            Launch Installer
          </button>
        </div>
      </section>
    </div>
  );

  const renderContentForTab = (tabId: SettingsTab) => {
    switch (tabId) {
      case "omniroute":
        return renderOmniRouteSection();
      case "defaults":
        return renderDefaultsSection();
      case "github":
        return renderGithubSection();
      case "workspaces":
        return renderWorkspacesSection();
      case "sandbox":
        return renderSandboxSection();
      case "updates":
        return renderUpdatesSection();
    }
  };

  return (
    <div className="settings-layout">
      <div className="settings-header">
        <div className="settings-search-bar">
          <svg className="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="Search settings..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button className="search-clear-btn" onClick={() => setSearchQuery("")}>
              ✕
            </button>
          )}
        </div>
      </div>

      <div className="settings-body">
        <div className="settings-nav">
          {tabs.map((tab) => {
            const isVisible = filteredTabs.some((t) => t.id === tab.id);
            if (!isVisible && searchQuery.trim()) return null;
            return (
              <button
                key={tab.id}
                className={`settings-nav-item ${activeTab === tab.id && !searchQuery.trim() ? "active" : ""}`}
                onClick={() => {
                  setActiveTab(tab.id);
                  if (searchQuery) setSearchQuery("");
                }}
              >
                <span className="settings-nav-icon">{tab.icon}</span>
                <span className="settings-nav-label">{tab.label}</span>
              </button>
            );
          })}
        </div>

        <div className="settings-content">
          {searchQuery.trim() ? (
            filteredTabs.length > 0 ? (
              filteredTabs.map((tab) => (
                <div key={tab.id} className="search-result-group">
                  <div className="search-result-category-badge">{tab.label}</div>
                  {renderContentForTab(tab.id)}
                </div>
              ))
            ) : (
              <div className="settings-no-results">
                <p className="hint">No settings found matching "{searchQuery}"</p>
              </div>
            )
          ) : (
            renderContentForTab(activeTab)
          )}
        </div>
      </div>

      <UpdaterModal isOpen={updaterOpen} onClose={() => setUpdaterOpen(false)} />
      <CustomInstallerModal isOpen={installerOpen} onClose={() => setInstallerOpen(false)} />
    </div>
  );
}
