import { useEffect, useState, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openShell } from "@tauri-apps/plugin-shell";
import type { OmniRouteConfigPayload, SessionDefaults, ModelInfo, ModelsCache, ModelTestResult } from "../types";
import GithubPanel from "./GithubPanel";
import WorkspacePanel from "./WorkspacePanel";
import UpdatesSection from "./UpdatesSection";

/// Mirrors `agent::MODEL` in the Rust backend — the model a turn falls
/// back to when no default has been chosen (or it's been reset). Kept in
/// sync by hand since the two sides don't share a build step; if that
/// constant ever changes, update this alongside it.
const BUILTIN_DEFAULT_MODEL = "auto/coding";

/// How long a cached model list is treated as "fresh enough" that the
/// picker doesn't feel a need to silently re-fetch on every Settings
/// visit. The manual Refresh button always bypasses this.
const MODELS_STALE_MS = 5 * 60 * 1000;

function formatRelativeTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 10_000) return "just now";
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

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

  // ---- Default model picker ----
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [modelsFetchedAt, setModelsFetchedAt] = useState<number | null>(null);
  const [modelsLoading, setModelsLoading] = useState(true); // no cache yet — first paint
  const [modelsSyncing, setModelsSyncing] = useState(false); // quiet background refresh
  const [modelsRefreshing, setModelsRefreshing] = useState(false); // explicit Refresh click
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [modelSearch, setModelSearch] = useState("");
  const [defaultModel, setDefaultModel] = useState<string | null>(null);
  const [selectingModel, setSelectingModel] = useState<string | null>(null);
  const [justSavedModel, setJustSavedModel] = useState<string | null>(null);
  const [testStatus, setTestStatus] = useState<"idle" | "testing" | "ok" | "fail">("idle");
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const hasLoadedModelsOnce = useRef(false);

  useEffect(() => {
    invoke<OmniRouteConfigPayload | null>("get_omniroute_config").then((cfg) => {
      if (cfg) {
        setMode(cfg.mode);
        setRemoteUrl(cfg.remote_url ?? "");
        setApiKey(cfg.api_key ?? "");
        setDefaultModel(cfg.default_model ?? null);
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
    loadModelsOnOpen();
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

  // Stale-while-revalidate: paint instantly from whatever's cached on
  // disk (near-instant, no network round trip), then always kick off a
  // background refresh so the list stays current. The refresh itself is
  // a plain async `invoke` handled entirely on the Rust side, so it
  // never blocks typing or navigation while it's in flight — only the
  // very first launch, with nothing cached yet, shows a loading state.
  const loadModelsOnOpen = async () => {
    if (hasLoadedModelsOnce.current) return;
    hasLoadedModelsOnce.current = true;
    try {
      const cache = await invoke<ModelsCache | null>("get_cached_models");
      if (cache && cache.models?.length) {
        setModels(cache.models);
        setModelsFetchedAt(cache.fetched_at);
        setModelsLoading(false);
        if (Date.now() - cache.fetched_at < MODELS_STALE_MS) {
          return; // fresh enough — skip the extra background round trip
        }
      }
    } catch {
      // no cache yet, fall through to a foreground fetch below
    }
    refreshModels({ quiet: true });
  };

  const refreshModels = async (opts: { quiet?: boolean } = {}) => {
    if (opts.quiet) {
      setModelsSyncing(true);
    } else {
      setModelsRefreshing(true);
    }
    setModelsError(null);
    try {
      const res = await invoke<ModelsCache>("fetch_omniroute_models");
      setModels(res.models);
      setModelsFetchedAt(res.fetched_at);
    } catch (e) {
      setModelsError(typeof e === "string" ? e : "Couldn't load the model list.");
    } finally {
      setModelsLoading(false);
      setModelsSyncing(false);
      setModelsRefreshing(false);
    }
  };

  const activeModelId = defaultModel && defaultModel.trim() ? defaultModel : BUILTIN_DEFAULT_MODEL;

  const filteredModels = useMemo(() => {
    const q = modelSearch.trim().toLowerCase();
    if (!q) return models;
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        (m.owned_by ?? "").toLowerCase().includes(q)
    );
  }, [models, modelSearch]);

  const selectModel = async (id: string) => {
    if (id === activeModelId || selectingModel) return;
    setSelectingModel(id);
    setTestStatus("idle");
    setTestMessage(null);
    try {
      await invoke("set_default_model", { model: id });
      setDefaultModel(id);
      setJustSavedModel(id);
      setTimeout(() => setJustSavedModel((cur) => (cur === id ? null : cur)), 1100);
    } catch (e) {
      setModelsError(typeof e === "string" ? e : "Couldn't set that as the default model.");
    } finally {
      setSelectingModel(null);
    }
  };

  const resetDefaultModel = async () => {
    if (activeModelId === BUILTIN_DEFAULT_MODEL || selectingModel) return;
    setSelectingModel("__reset__");
    setTestStatus("idle");
    setTestMessage(null);
    try {
      await invoke("set_default_model", { model: null });
      setDefaultModel(null);
      setJustSavedModel(BUILTIN_DEFAULT_MODEL);
      setTimeout(() => setJustSavedModel((cur) => (cur === BUILTIN_DEFAULT_MODEL ? null : cur)), 1100);
    } catch (e) {
      setModelsError(typeof e === "string" ? e : "Couldn't reset the default model.");
    } finally {
      setSelectingModel(null);
    }
  };

  const testActiveModel = async () => {
    if (testStatus === "testing") return;
    setTestStatus("testing");
    setTestMessage(null);
    try {
      const res = await invoke<ModelTestResult>("test_model", { model: activeModelId });
      setTestStatus(res.ok ? "ok" : "fail");
      setTestMessage(res.message);
    } catch (e) {
      setTestStatus("fail");
      setTestMessage(typeof e === "string" ? e : "Couldn't reach the model.");
    }
  };

  const buildConfig = (): OmniRouteConfigPayload => ({
    mode,
    remote_url: mode === "remote" ? remoteUrl : null,
    api_key: mode === "remote" ? apiKey : null,
    // Carried through so saving the connection settings never wipes out
    // a default model chosen via the picker below — this button only
    // ever touches mode/URL/key, but the backend struct is one record.
    default_model: defaultModel,
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
    { id: "omniroute" as const, label: "OmniRoute & Engine", icon: "🔌", keywords: "omniroute connection engine process mode local remote api key command args default model models picker search refresh test active reset" },
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

      <section className="model-picker-section">
        <div className="model-picker-header">
          <h3>Default model</h3>
          <div className="active-model-badge">
            <span className="active-model-dot" />
            <span className="active-model-label">Active:</span>
            <strong>{activeModelId}</strong>
          </div>
        </div>
        <p className="hint small">
          Pick which model new turns and sub-agents are sent to. Selecting a
          model saves it right away — leave it on the built-in default if
          you're not sure.
        </p>

        <div className="model-picker-toolbar">
          <div className="model-search-bar">
            <svg className="search-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder="Search models..."
              value={modelSearch}
              onChange={(e) => setModelSearch(e.target.value)}
            />
            {modelSearch && (
              <button className="search-clear-btn" onClick={() => setModelSearch("")}>
                ✕
              </button>
            )}
          </div>
          <div className="model-toolbar-actions">
            <button
              className={`model-refresh-btn ${modelsRefreshing ? "spinning" : ""}`}
              onClick={() => refreshModels()}
              disabled={modelsRefreshing}
              title="Refresh the model list"
            >
              <svg className="refresh-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4">
                <path d="M21 2v6h-6" />
                <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
                <path d="M3 22v-6h6" />
                <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
              </svg>
              {modelsRefreshing ? "Refreshing…" : "Refresh"}
            </button>
            <button
              className="model-test-btn"
              onClick={testActiveModel}
              disabled={testStatus === "testing"}
              title="Send a live test request to the active model"
            >
              {testStatus === "testing" && <span className="mini-spinner" />}
              {testStatus === "testing" ? "Testing…" : "Test model"}
            </button>
            <button
              className="small"
              onClick={resetDefaultModel}
              disabled={activeModelId === BUILTIN_DEFAULT_MODEL || selectingModel === "__reset__"}
              title="Revert to the built-in default model"
            >
              Reset to default
            </button>
          </div>
        </div>

        {testStatus === "ok" && <div className="ok model-test-result">{testMessage}</div>}
        {testStatus === "fail" && <div className="fail model-test-result">{testMessage}</div>}

        <div className="model-list-meta">
          {modelsSyncing && !modelsRefreshing && (
            <span className="model-syncing-hint">
              <span className="model-syncing-dot" /> Syncing latest list…
            </span>
          )}
          {!modelsSyncing && modelsFetchedAt && (
            <span className="hint small">Updated {formatRelativeTime(modelsFetchedAt)}</span>
          )}
        </div>

        {modelsError && <p className="fail model-error-msg">{modelsError}</p>}

        <div className="model-list">
          {modelsLoading ? (
            Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="model-row-skeleton" style={{ animationDelay: `${i * 0.06}s` }} />
            ))
          ) : filteredModels.length === 0 ? (
            <div className="model-list-empty">
              {modelSearch
                ? `No models matching "${modelSearch}"`
                : "No models found yet — try refreshing."}
            </div>
          ) : (
            filteredModels.map((m, i) => {
              const isActive = m.id === activeModelId;
              const isSelecting = selectingModel === m.id;
              const justSaved = justSavedModel === m.id;
              return (
                <button
                  key={m.id}
                  className={`model-row ${isActive ? "active" : ""} ${justSaved ? "just-saved" : ""}`}
                  onClick={() => selectModel(m.id)}
                  disabled={isSelecting}
                  style={{ animationDelay: `${Math.min(i, 20) * 0.02}s` }}
                >
                  <span className="model-row-radio" />
                  <span className="model-row-id">{m.id}</span>
                  {m.owned_by && <span className="model-row-owner">{m.owned_by}</span>}
                  {isSelecting && <span className="mini-spinner" />}
                  {isActive && !isSelecting && <span className="model-row-active-tag">Active</span>}
                </button>
              );
            })
          )}
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
    <UpdatesSection />
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
    </div>
  );
}
