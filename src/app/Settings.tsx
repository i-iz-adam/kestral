import { useEffect, useState, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { OmniRouteConfigPayload, SessionDefaults, ModelInfo, ModelsCache } from "../types";
import {
  getOmniRouteConfig,
  saveOmniRouteConfig,
  fetchOmniRouteModels,
  listImageModels,
  setDefaultModel as apiSetDefaultModel,
  setDefaultImageModel as apiSetDefaultImageModel,
  testOmniRouteConnection,
  testModel as apiTestModel,
} from "./omnirouteApi";
import IntegrationsPanel from "./IntegrationsPanel";
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
  | "sandbox"
  | "updates";

interface SettingsProps {
  initialTab?: SettingsTab;
}

interface VirtualModelListProps {
  models: ModelInfo[];
  activeModelId: string;
  selectingModel: string | null;
  justSavedModel: string | null;
  modelsLoading: boolean;
  modelSearch: string;
  onSelectModel: (id: string) => void;
}

function VirtualModelList({
  models,
  activeModelId,
  selectingModel,
  justSavedModel,
  modelsLoading,
  modelSearch,
  onSelectModel,
}: VirtualModelListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
    setScrollTop(0);
  }, [modelSearch, modelsLoading]);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
  };

  const ITEM_HEIGHT = 44; // 38px button height + 6px gap
  const CONTAINER_HEIGHT = 340;
  const OVERSCAN = 6;

  const totalHeight = models.length * ITEM_HEIGHT;
  const startIndex = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN);
  const endIndex = Math.min(models.length, Math.ceil((scrollTop + CONTAINER_HEIGHT) / ITEM_HEIGHT) + OVERSCAN);
  const offsetY = startIndex * ITEM_HEIGHT;
  const visibleModels = models.slice(startIndex, endIndex);

  return (
    <div
      ref={containerRef}
      className="model-list virtualized-model-list"
      onScroll={handleScroll}
      style={{ height: `${CONTAINER_HEIGHT}px`, overflowY: "auto", position: "relative" }}
    >
      {modelsLoading ? (
        Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="model-row-skeleton" style={{ animationDelay: `${i * 0.06}s` }} />
        ))
      ) : models.length === 0 ? (
        <div className="model-list-empty">
          {modelSearch
            ? `No models matching "${modelSearch}"`
            : "No models found yet — try refreshing."}
        </div>
      ) : (
        <div style={{ height: `${totalHeight}px`, width: "100%", position: "relative" }}>
          <div
            style={{
              transform: `translateY(${offsetY}px)`,
              display: "flex",
              flexDirection: "column",
              gap: "6px",
            }}
          >
            {visibleModels.map((m) => {
              const isActive = m.id === activeModelId;
              const isSelecting = selectingModel === m.id;
              const justSaved = justSavedModel === m.id;
              return (
                <button
                  key={m.id}
                  className={`model-row ${isActive ? "active" : ""} ${justSaved ? "just-saved" : ""}`}
                  onClick={() => onSelectModel(m.id)}
                  disabled={isSelecting}
                  style={{ height: "38px" }}
                >
                  <span className="model-row-radio" />
                  <span className="model-row-id">{m.id}</span>
                  {m.owned_by && <span className="model-row-owner">{m.owned_by}</span>}
                  {isSelecting && <span className="mini-spinner" />}
                  {isActive && !isSelecting && <span className="model-row-active-tag">Active</span>}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

interface VirtualImageModelListProps {
  models: string[];
  activeModelId: string | null;
  selectingModel: string | null;
  justSavedModel: string | null;
  modelsLoading: boolean;
  modelSearch: string;
  onSelectModel: (id: string) => void;
}

function VirtualImageModelList({
  models,
  activeModelId,
  selectingModel,
  justSavedModel,
  modelsLoading,
  modelSearch,
  onSelectModel,
}: VirtualImageModelListProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
    setScrollTop(0);
  }, [modelSearch, modelsLoading]);

  const handleScroll = (e: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(e.currentTarget.scrollTop);
  };

  const ITEM_HEIGHT = 44;
  const CONTAINER_HEIGHT = 220;
  const OVERSCAN = 5;

  const totalHeight = models.length * ITEM_HEIGHT;
  const startIndex = Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN);
  const endIndex = Math.min(models.length, Math.ceil((scrollTop + CONTAINER_HEIGHT) / ITEM_HEIGHT) + OVERSCAN);
  const offsetY = startIndex * ITEM_HEIGHT;
  const visibleModels = models.slice(startIndex, endIndex);

  return (
    <div
      ref={containerRef}
      className="model-list virtualized-model-list"
      onScroll={handleScroll}
      style={{ height: `${CONTAINER_HEIGHT}px`, overflowY: "auto", position: "relative" }}
    >
      {modelsLoading ? (
        Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="model-row-skeleton" style={{ animationDelay: `${i * 0.06}s` }} />
        ))
      ) : models.length === 0 ? (
        <div className="model-list-empty">
          {modelSearch
            ? `No image models matching "${modelSearch}"`
            : "No image models found — configure an image provider in the Providers dashboard."}
        </div>
      ) : (
        <div style={{ height: `${totalHeight}px`, width: "100%", position: "relative" }}>
          <div
            style={{
              transform: `translateY(${offsetY}px)`,
              display: "flex",
              flexDirection: "column",
              gap: "6px",
            }}
          >
            {visibleModels.map((id) => {
              const isActive = id === activeModelId;
              const isSelecting = selectingModel === id;
              const justSaved = justSavedModel === id;
              return (
                <button
                  key={id}
                  className={`model-row ${isActive ? "active" : ""} ${justSaved ? "just-saved" : ""}`}
                  onClick={() => onSelectModel(id)}
                  disabled={isSelecting}
                  style={{ height: "38px" }}
                >
                  <span className="model-row-radio" />
                  <span className="model-row-id">{id}</span>
                  {isSelecting && <span className="mini-spinner" />}
                  {isActive && !isSelecting && <span className="model-row-active-tag">Active</span>}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

export default function Settings({ initialTab = "omniroute" }: SettingsProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>(
    initialTab === ("workspaces" as any) ? "omniroute" : initialTab
  );
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
  const [defaultShellTimeout, setDefaultShellTimeout] = useState(60);
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

  // ---- Image model picker ----
  const [defaultImageModel, setDefaultImageModel] = useState<string | null>(null);
  const [imageModels, setImageModels] = useState<string[]>([]);
  const [imageModelsLoading, setImageModelsLoading] = useState(true);
  const [imageModelsError, setImageModelsError] = useState<string | null>(null);
  const [imageModelSearch, setImageModelSearch] = useState("");
  const [selectingImageModel, setSelectingImageModel] = useState<string | null>(null);
  const [justSavedImageModel, setJustSavedImageModel] = useState<string | null>(null);

  useEffect(() => {
    getOmniRouteConfig().then((cfg) => {
      if (cfg) {
        setMode(cfg.mode);
        setRemoteUrl(cfg.remote_url ?? "");
        setApiKey(cfg.api_key ?? "");
        setDefaultModel(cfg.default_model ?? null);
        setDefaultImageModel(cfg.default_image_model ?? null);
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
        setDefaultShellTimeout(defs.shell_timeout_seconds ?? 60);
      }
    });
    checkPython();
    loadModelsOnOpen();
    loadImageModels();
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
          return;
        }
      }
    } catch {
      // no cache yet
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
      const res = await fetchOmniRouteModels();
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

  const loadImageModels = async () => {
    setImageModelsLoading(true);
    setImageModelsError(null);
    try {
      const list = await listImageModels();
      setImageModels(list);
    } catch (e) {
      setImageModelsError(typeof e === "string" ? e : "Couldn't load image models.");
    } finally {
      setImageModelsLoading(false);
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

  const filteredImageModels = useMemo(() => {
    const q = imageModelSearch.trim().toLowerCase();
    if (!q) return imageModels;
    return imageModels.filter((m) => m.toLowerCase().includes(q));
  }, [imageModels, imageModelSearch]);

  const selectModel = async (id: string) => {
    if (id === activeModelId || selectingModel) return;
    setSelectingModel(id);
    setTestStatus("idle");
    setTestMessage(null);
    try {
      await apiSetDefaultModel(id);
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
      await apiSetDefaultModel(null);
      setDefaultModel(null);
      setJustSavedModel(BUILTIN_DEFAULT_MODEL);
      setTimeout(() => setJustSavedModel((cur) => (cur === BUILTIN_DEFAULT_MODEL ? null : cur)), 1100);
    } catch (e) {
      setModelsError(typeof e === "string" ? e : "Couldn't reset the default model.");
    } finally {
      setSelectingModel(null);
    }
  };

  const selectImageModel = async (id: string) => {
    if (id === defaultImageModel || selectingImageModel) return;
    setSelectingImageModel(id);
    setImageModelsError(null);
    try {
      await apiSetDefaultImageModel(id);
      setDefaultImageModel(id);
      setJustSavedImageModel(id);
      setTimeout(() => setJustSavedImageModel((cur) => (cur === id ? null : cur)), 1100);
    } catch (e) {
      setImageModelsError(typeof e === "string" ? e : "Couldn't set image model.");
    } finally {
      setSelectingImageModel(null);
    }
  };

  const resetImageModel = async () => {
    if (!defaultImageModel || selectingImageModel === "__reset__") return;
    setSelectingImageModel("__reset__");
    setImageModelsError(null);
    try {
      await apiSetDefaultImageModel(null);
      setDefaultImageModel(null);
      setJustSavedImageModel(null);
    } catch (e) {
      setImageModelsError(typeof e === "string" ? e : "Couldn't reset image model.");
    } finally {
      setSelectingImageModel(null);
    }
  };

  const testActiveModel = async () => {
    if (testStatus === "testing") return;
    setTestStatus("testing");
    setTestMessage(null);
    try {
      const res = await apiTestModel(activeModelId);
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
    default_model: defaultModel,
    default_image_model: defaultImageModel,
  });

  const test = async () => {
    setStatus("testing");
    try {
      const ok = await testOmniRouteConnection(buildConfig());
      setStatus(ok ? "ok" : "fail");
    } catch {
      setStatus("fail");
    }
  };

  const save = async () => {
    await saveOmniRouteConfig(buildConfig());
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
        shell_timeout_seconds: defaultShellTimeout,
      },
    });
    setDefaultsSaved(true);
    setTimeout(() => setDefaultsSaved(false), 1500);
  };

  const tabs = [
    { id: "omniroute" as const, label: "OmniRoute & Engine", icon: "🔌", keywords: "omniroute connection engine process mode local remote api key command args default model models picker search refresh test active reset image generation" },
    { id: "defaults" as const, label: "Session Defaults", icon: "⚙️", keywords: "session defaults planning mode sub-agents subagents graceful stop" },
    { id: "github" as const, label: "Integrations & Connections", icon: "🌐", keywords: "integrations connections github discord bot the magician token slack telegram notion linear webhook postgres" },
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

        <VirtualModelList
          models={filteredModels}
          activeModelId={activeModelId}
          selectingModel={selectingModel}
          justSavedModel={justSavedModel}
          modelsLoading={modelsLoading}
          modelSearch={modelSearch}
          onSelectModel={selectModel}
        />
      </section>

      <section className="model-picker-section image-model-picker-section">
        <div className="model-picker-header">
          <h3>Image generation model</h3>
          <div className="active-model-badge">
            <span className="active-model-dot" />
            <span className="active-model-label">Active:</span>
            <strong>{defaultImageModel || "Auto-discover (First available)"}</strong>
          </div>
        </div>
        <p className="hint small">
          Pick which image generation model tools send image prompts to. Choosing a model saves it right away.
          If set to auto-discover, OmniRoute automatically uses the first working image provider.
        </p>

        <div className="model-picker-toolbar">
          <div className="model-search-bar">
            <svg className="search-icon" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder="Search image models..."
              value={imageModelSearch}
              onChange={(e) => setImageModelSearch(e.target.value)}
            />
            {imageModelSearch && (
              <button className="search-clear-btn" onClick={() => setImageModelSearch("")}>
                ✕
              </button>
            )}
          </div>
          <div className="model-toolbar-actions">
            <button
              className={`model-refresh-btn ${imageModelsLoading ? "spinning" : ""}`}
              onClick={loadImageModels}
              disabled={imageModelsLoading}
              title="Refresh image models"
            >
              <svg className="refresh-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4">
                <path d="M21 2v6h-6" />
                <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
                <path d="M3 22v-6h6" />
                <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
              </svg>
              {imageModelsLoading ? "Refreshing…" : "Refresh"}
            </button>
            <button
              className="small"
              onClick={resetImageModel}
              disabled={!defaultImageModel || selectingImageModel === "__reset__"}
              title="Reset to auto-discovery mode"
            >
              Reset to auto
            </button>
          </div>
        </div>

        {imageModelsError && <p className="fail model-error-msg">{imageModelsError}</p>}

        <VirtualImageModelList
          models={filteredImageModels}
          activeModelId={defaultImageModel}
          selectingModel={selectingImageModel}
          justSavedModel={justSavedImageModel}
          modelsLoading={imageModelsLoading}
          modelSearch={imageModelSearch}
          onSelectModel={selectImageModel}
        />
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
          <div style={{ marginTop: 12, display: "flex", alignItems: "center", gap: 8 }}>
            <label style={{ fontSize: 13, color: "var(--fg)" }}>
              Shell command timeout (seconds):
            </label>
            <input
              type="number"
              min={1}
              max={1800}
              value={defaultShellTimeout}
              onChange={(e) => setDefaultShellTimeout(Math.max(1, parseInt(e.target.value) || 60))}
              style={{ width: 80, padding: "4px 8px" }}
            />
          </div>
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
      <IntegrationsPanel />
    </div>
  );

  const renderSandboxSection = () => (
    <div className="settings-section-block">
      <h2>Python Sandbox</h2>
      <section>
        <h3>Execution environment</h3>
        <p className="hint small">
          The <code>run_python</code> tool runs Python snippets in an isolated
          subprocess.
        </p>
        <div className="field-group">
          <label>Status</label>
          <div className="row" style={{ alignItems: "center" }}>
            {checkingPython ? (
              <span className="hint">Checking...</span>
            ) : pythonStatus?.installed ? (
              <span className="ok">
                Available ({pythonStatus.version || pythonStatus.binary})
              </span>
            ) : (
              <span className="fail">Python not found on PATH</span>
            )}
            <button className="small" onClick={checkPython} disabled={checkingPython}>
              Re-check
            </button>
          </div>
        </div>
      </section>
    </div>
  );

  const renderUpdatesSection = () => (
    <div className="settings-section-block">
      <h2>Updates & System</h2>
      <UpdatesSection />
    </div>
  );

  return (
    <div className="settings-layout">
      <div className="settings-sidebar">
        <div className="settings-sidebar-header">
          <h2 className="settings-title">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
            Settings
          </h2>
          <div className="settings-search">
            <svg className="search-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
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
        <nav className="settings-nav">
          {filteredTabs.map((t) => (
            <button
              key={t.id}
              className={`settings-nav-item ${activeTab === t.id ? "active" : ""}`}
              onClick={() => setActiveTab(t.id)}
            >
              <span className="settings-nav-icon">{t.icon}</span>
              <span className="label">{t.label}</span>
            </button>
          ))}
          {filteredTabs.length === 0 && (
            <div className="settings-no-results">
              No matching settings
            </div>
          )}
        </nav>
      </div>

      <div className="settings-content">
        {activeTab === "omniroute" && renderOmniRouteSection()}
        {activeTab === "defaults" && renderDefaultsSection()}
        {activeTab === "github" && renderGithubSection()}
        {activeTab === "sandbox" && renderSandboxSection()}
        {activeTab === "updates" && renderUpdatesSection()}
      </div>
    </div>
  );
}
