import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/api/shell";
import type { OmniRouteConfigPayload } from "../types";

type EngineStatus = "stopped" | "starting" | "running" | "error";
type TabId = "overview" | "models" | "usage" | "info";

interface ModelItem {
  id: string;
  object?: string;
  owned_by?: string;
  created?: number;
}

function formatTokens(count: number | string | undefined | null): string {
  if (count == null) return "0";
  const num = typeof count === "number" ? count : parseFloat(String(count));
  if (isNaN(num) || num <= 0) return "0";

  if (num < 1000) {
    return num.toString();
  }
  if (num < 1_000_000) {
    const val = num / 1000;
    return val % 1 === 0 ? `${val}k` : `${val.toFixed(1)}k`;
  }
  if (num < 1_000_000_000) {
    const val = num / 1_000_000;
    return val % 1 === 0 ? `${val}m` : `${val.toFixed(1)}m`;
  }
  const val = num / 1_000_000_000;
  return val % 1 === 0 ? `${val}b` : `${val.toFixed(1)}b`;
}

function extractMetrics(data: any) {
  if (!data)
    return {
      totalTokens: 0,
      promptTokens: 0,
      completionTokens: 0,
      requests: 0,
      cost: 0,
      savedTokens: 0,
    };

  const root = data.summary || data.data || data.analytics || data;

  const totalTokens =
    root.totalTokens ??
    root.total_tokens ??
    root.tokens ??
    root.total_tokens_used ??
    0;

  const promptTokens =
    root.promptTokens ??
    root.prompt_tokens ??
    root.inputTokens ??
    root.input_tokens ??
    0;

  const completionTokens =
    root.completionTokens ??
    root.completion_tokens ??
    root.outputTokens ??
    root.output_tokens ??
    0;

  const requests =
    root.totalRequests ??
    root.total_requests ??
    root.requests ??
    root.count ??
    0;

  const cost =
    root.totalCostUsd ?? root.totalCost ?? root.total_cost ?? root.cost ?? 0;

  const savedTokens =
    root.savedTokens ?? root.saved_tokens ?? root.tokensSaved ?? 0;

  return {
    totalTokens: Number(totalTokens) || 0,
    promptTokens: Number(promptTokens) || 0,
    completionTokens: Number(completionTokens) || 0,
    requests: Number(requests) || 0,
    cost: Number(cost) || 0,
    savedTokens: Number(savedTokens) || 0,
  };
}

function extractBreakdownLists(data: any) {
  if (!data) return { providers: [], models: [] };

  const provList: any[] = [];
  const rawProv = data.byProvider || data.by_provider || data.providers;
  if (Array.isArray(rawProv)) {
    rawProv.forEach((p) => {
      provList.push({
        name: p.provider || p.name || p.id || "unknown",
        requests: p.requests ?? p.count ?? 0,
        tokens: p.tokens ?? p.totalTokens ?? p.total_tokens ?? 0,
        promptTokens:
          p.promptTokens ?? p.prompt_tokens ?? p.inputTokens ?? p.input_tokens ?? 0,
        completionTokens:
          p.completionTokens ??
          p.completion_tokens ??
          p.outputTokens ??
          p.output_tokens ??
          0,
        cost: p.costUsd ?? p.cost ?? p.totalCost ?? 0,
        p50: p.p50,
        p95: p.p95,
        p99: p.p99,
      });
    });
  } else if (rawProv && typeof rawProv === "object") {
    Object.entries<any>(rawProv).forEach(([name, p]) => {
      if (typeof p === "number") {
        provList.push({ name, requests: 0, tokens: p });
      } else {
        provList.push({
          name: p.provider || name,
          requests: p.requests ?? p.count ?? 0,
          tokens: p.tokens ?? p.totalTokens ?? p.total_tokens ?? 0,
          promptTokens:
            p.promptTokens ?? p.prompt_tokens ?? p.inputTokens ?? p.input_tokens ?? 0,
          completionTokens:
            p.completionTokens ??
            p.completion_tokens ??
            p.outputTokens ??
            p.output_tokens ??
            0,
          cost: p.costUsd ?? p.cost ?? p.totalCost ?? 0,
          p50: p.p50,
          p95: p.p95,
          p99: p.p99,
        });
      }
    });
  }

  const modelList: any[] = [];
  const rawModel = data.byModel || data.by_model || data.models;
  if (Array.isArray(rawModel)) {
    rawModel.forEach((m) => {
      modelList.push({
        name: m.model || m.name || m.id || "unknown",
        requests: m.requests ?? m.count ?? 0,
        tokens: m.tokens ?? m.totalTokens ?? m.total_tokens ?? 0,
        promptTokens:
          m.promptTokens ?? m.prompt_tokens ?? m.inputTokens ?? m.input_tokens ?? 0,
        completionTokens:
          m.completionTokens ??
          m.completion_tokens ??
          m.outputTokens ??
          m.output_tokens ??
          0,
      });
    });
  } else if (rawModel && typeof rawModel === "object") {
    Object.entries<any>(rawModel).forEach(([name, m]) => {
      if (typeof m === "number") {
        modelList.push({ name, requests: 0, tokens: m });
      } else {
        modelList.push({
          name: m.model || name,
          requests: m.requests ?? m.count ?? 0,
          tokens: m.tokens ?? m.totalTokens ?? m.total_tokens ?? 0,
          promptTokens:
            m.promptTokens ?? m.prompt_tokens ?? m.inputTokens ?? m.input_tokens ?? 0,
          completionTokens:
            m.completionTokens ??
            m.completion_tokens ??
            m.outputTokens ??
            m.output_tokens ??
            0,
        });
      }
    });
  }

  return { providers: provList, models: modelList };
}

export default function ProvidersPanel() {
  const [mode, setMode] = useState<"local" | "remote" | null>(null);
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  const [apiKey, setApiKey] = useState<string | null>(null);
  const [engineStatus, setEngineStatus] = useState<EngineStatus>("stopped");
  const [activeTab, setActiveTab] = useState<TabId>("overview");

  // Data states
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedText, setCopiedText] = useState<string | null>(null);

  // Endpoints data
  const [healthData, setHealthData] = useState<any>(null);
  const [modelsData, setModelsData] = useState<ModelItem[]>([]);
  const [telemetryData, setTelemetryData] = useState<any>(null);
  const [cacheData, setCacheData] = useState<any>(null);

  // Search & Filter for models
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedProvider, setSelectedProvider] = useState<string>("all");

  useEffect(() => {
    invoke<OmniRouteConfigPayload | null>("get_omniroute_config").then((cfg) => {
      if (!cfg) return;
      setMode(cfg.mode);
      setApiKey(cfg.api_key);
      if (cfg.mode === "remote") {
        setBaseUrl(cfg.remote_url ? cfg.remote_url.replace(/\/$/, "") : null);
      } else {
        setBaseUrl("http://127.0.0.1:20128");
      }
    });
    invoke<{ status: EngineStatus }>("get_engine_status").then((s) =>
      setEngineStatus(s.status)
    );
    const unlisten = listen<{ status: EngineStatus }>(
      "engine://status",
      (evt) => setEngineStatus(evt.payload.status)
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const safeFetch = async (endpoint: string) => {
    try {
      const res = await invoke<any>("fetch_omniroute_endpoint", {
        endpoint,
        method: "GET",
      });
      return res;
    } catch {
      return null;
    }
  };

  const fetchDashboardData = async (isManualRefresh = false) => {
    if (isManualRefresh) {
      setRefreshing(true);
    } else {
      setLoading(true);
    }
    setError(null);

    // Fetch Health with fallbacks
    const loadHealth = async () => {
      const h1 = await safeFetch("/api/health");
      if (h1 && (h1.status || h1.health || h1.ok || typeof h1 === "object")) return h1;
      const h2 = await safeFetch("/api/monitoring/health");
      if (h2) return h2;
      const h3 = await safeFetch("/health");
      if (h3) return h3;
      return null;
    };

    // Fetch Models with fallbacks
    const loadModels = async () => {
      const m1 = await safeFetch("/v1/models?prefix=alias");
      if (m1) return m1;
      const m2 = await safeFetch("/v1/models");
      if (m2) return m2;
      const m3 = await safeFetch("/api/models/catalog");
      if (m3) return m3;
      return null;
    };

    // Fetch Usage / Telemetry Analytics with fallbacks
    const loadUsage = async () => {
      const u1 = await safeFetch("/api/usage/analytics");
      if (u1) return u1;
      const u2 = await safeFetch("/api/usage/om-usage?format=json");
      if (u2) return u2;
      const u3 = await safeFetch("/api/usage/history");
      if (u3) return u3;
      const u4 = await safeFetch("/api/telemetry/summary");
      if (u4) return u4;
      const u5 = await safeFetch("/api/usage/model-latency-stats");
      if (u5) return u5;
      return null;
    };

    const [healthRes, modelsRes, usageRes, cacheRes] = await Promise.all([
      loadHealth(),
      loadModels(),
      loadUsage(),
      safeFetch("/api/cache/stats"),
    ]);

    let loadedSomething = false;

    if (healthRes) {
      setHealthData(healthRes);
      loadedSomething = true;
    }
    if (modelsRes) {
      loadedSomething = true;
      let rawList: any[] = [];
      if (Array.isArray(modelsRes)) {
        rawList = modelsRes;
      } else if (modelsRes?.data && Array.isArray(modelsRes.data)) {
        rawList = modelsRes.data;
      } else if (modelsRes?.models && Array.isArray(modelsRes.models)) {
        rawList = modelsRes.models;
      }
      const parsedModels: ModelItem[] = rawList.map((m) => {
        if (typeof m === "string") {
          return { id: m };
        }
        return {
          id: m.id || m.name || "unknown",
          object: m.object,
          owned_by: m.owned_by || m.provider || (m.id && m.id.includes("/") ? m.id.split("/")[0] : undefined),
          created: m.created,
        };
      });
      setModelsData(parsedModels);
    }

    if (usageRes) {
      setTelemetryData(usageRes);
      loadedSomething = true;
    }
    if (cacheRes) {
      setCacheData(cacheRes);
      loadedSomething = true;
    }

    if (!loadedSomething && isAvailable) {
      setError("OmniRoute engine is running, but responses from telemetry endpoints were restricted or pending.");
    }

    setLoading(false);
    setRefreshing(false);
  };

  useEffect(() => {
    if (mode !== null && (engineStatus === "running" || mode === "remote")) {
      fetchDashboardData();
    }
  }, [mode, engineStatus]);

  const copyToClipboard = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    setCopiedText(label);
    setTimeout(() => setCopiedText(null), 2000);
  };

  if (mode === null) {
    return (
      <div className="settings-view">
        <h2>OmniRoute Dashboard</h2>
        <p className="hint">Finish setting up OmniRoute in Settings first.</p>
      </div>
    );
  }

  const targetUrl = baseUrl || "http://127.0.0.1:20128";
  const isAvailable = mode === "remote" || engineStatus === "running";

  const providersSet = new Set<string>();
  modelsData.forEach((m) => {
    const owner = m.owned_by || m.id.split("/")[0] || "unknown";
    if (owner && owner !== m.id) providersSet.add(owner);
  });
  const providersList = Array.from(providersSet).sort();

  const filteredModels = modelsData.filter((m) => {
    const owner = m.owned_by || m.id.split("/")[0] || "unknown";
    const matchesSearch =
      m.id.toLowerCase().includes(searchQuery.toLowerCase()) ||
      owner.toLowerCase().includes(searchQuery.toLowerCase());
    const matchesProvider =
      selectedProvider === "all" || owner.toLowerCase() === selectedProvider.toLowerCase();
    return matchesSearch && matchesProvider;
  });

  return (
    <div className="providers-view">
      {/* Top Header */}
      <div className="providers-header">
        <div className="providers-title-group">
          <h2>OmniRoute Dashboard</h2>
          <div className={`status-pill ${mode === "remote" ? "running" : engineStatus}`}>
            <span className="status-orb" />
            <span className="status-text">
              {mode === "remote"
                ? "Remote Connected"
                : engineStatus === "running"
                ? "Engine Running"
                : engineStatus === "starting"
                ? "Starting Engine..."
                : engineStatus === "error"
                ? "Engine Error"
                : "Engine Stopped"}
            </span>
          </div>
        </div>

        {/* Target URL Quick bar */}
        <div className="url-bar">
          <span className="url-label">Endpoint:</span>
          <code className="url-value">{targetUrl}</code>
          <button
            className="icon-btn-compact"
            onClick={() => copyToClipboard(targetUrl, "url")}
            title="Copy Endpoint URL"
          >
            {copiedText === "url" ? "✓ Copied" : "📋 Copy"}
          </button>
        </div>

        {/* Header Actions */}
        <div className="providers-actions">
          {isAvailable && (
            <button
              className={`refresh-btn ${refreshing ? "spinning" : ""}`}
              onClick={() => fetchDashboardData(true)}
              disabled={refreshing || loading}
              title="Refresh telemetry & catalog"
            >
              <svg className="spin-icon" viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0118.8-4.3M22 12.5a10 10 0 01-18.8 4.3" />
              </svg>
              Refresh
            </button>
          )}

          <button className="primary open-browser-cta" onClick={() => open(targetUrl)}>
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6M15 3h6v6M10 14L21 3" />
            </svg>
            Open in Browser
          </button>
        </div>
      </div>

      {!isAvailable ? (
        <div className="empty-state engine-stopped-card">
          <div className="stopped-icon">⚡</div>
          <h3>OmniRoute Engine is Stopped</h3>
          <p>Start the local engine to route requests across models, view analytics, and manage providers.</p>
          <button
            className="primary cta-lg"
            onClick={() => invoke("start_engine")}
            disabled={engineStatus === "starting"}
          >
            {engineStatus === "starting" ? "Starting Engine..." : "Start OmniRoute Engine"}
          </button>
        </div>
      ) : (
        <>
          {/* Tab Navigation */}
          <div className="omni-tabs-container">
            <nav className="omni-tabs">
              <button
                className={`tab-item ${activeTab === "overview" ? "active" : ""}`}
                onClick={() => setActiveTab("overview")}
              >
                Overview & Health
              </button>
              <button
                className={`tab-item ${activeTab === "models" ? "active" : ""}`}
                onClick={() => setActiveTab("models")}
              >
                Model Catalog ({modelsData.length})
              </button>
              <button
                className={`tab-item ${activeTab === "usage" ? "active" : ""}`}
                onClick={() => setActiveTab("usage")}
              >
                Usage & Analytics
              </button>
              <button
                className={`tab-item ${activeTab === "info" ? "active" : ""}`}
                onClick={() => setActiveTab("info")}
              >
                Gateway Config & Info
              </button>
            </nav>
          </div>

          {/* Main Dashboard Body */}
          <div className="omni-tab-content">
            {error && (
              <div className="omni-alert danger">
                <span>⚠️ {error}</span>
                <button onClick={() => fetchDashboardData(true)}>Retry</button>
              </div>
            )}

            {/* TAB 1: OVERVIEW */}
            {activeTab === "overview" && (
              <div className="tab-pane animated-fade-in">
                <div className="metrics-grid">
                  <div className="stat-card">
                    <div className="stat-label">Engine Status</div>
                    <div className="stat-value highlight-violet">
                      {healthData?.status || healthData?.health || (engineStatus === "running" ? "Healthy" : "Active")}
                    </div>
                    <div className="stat-sub">
                      Mode: <span className="text-caps">{mode}</span>
                    </div>
                  </div>

                  <div className="stat-card">
                    <div className="stat-label">Available Models</div>
                    <div className="stat-value highlight-gold">{modelsData.length}</div>
                    <div className="stat-sub">Ready for completion & chat</div>
                  </div>

                  <div className="stat-card">
                    <div className="stat-label">Cache Hit Rate</div>
                    <div className="stat-value">
                      {cacheData?.semanticCache?.hitRate != null
                        ? `${(cacheData.semanticCache.hitRate * 100).toFixed(1)}%`
                        : cacheData?.hit_rate != null
                        ? `${(cacheData.hit_rate * 100).toFixed(1)}%`
                        : "N/A"}
                    </div>
                    <div className="stat-sub">
                      {cacheData?.semanticCache?.memorySize != null
                        ? `${cacheData.semanticCache.memorySize} items in RAM`
                        : "Prompt cache active"}
                    </div>
                  </div>

                  <div className="stat-card">
                    <div className="stat-label">Avg Latency (p50)</div>
                    <div className="stat-value">
                      {telemetryData?.latency_p50 != null
                        ? `${telemetryData.latency_p50}ms`
                        : telemetryData?.avg_latency_ms != null
                        ? `${telemetryData.avg_latency_ms}ms`
                        : "N/A"}
                    </div>
                    <div className="stat-sub">
                      p95: {telemetryData?.latency_p95 != null ? `${telemetryData.latency_p95}ms` : "N/A"}
                    </div>
                  </div>
                </div>

                <div className="overview-sections">
                  <div className="dashboard-card">
                    <h4>System Health Summary</h4>
                    {loading ? (
                      <div className="skeleton-loader">Loading health status...</div>
                    ) : (
                      <div className="health-details">
                        <div className="health-row">
                          <span className="key">Gateway Reachability:</span>
                          <span className="value text-success">✓ Online ({targetUrl})</span>
                        </div>
                        <div className="health-row">
                          <span className="key">Uptime / Active Session:</span>
                          <span className="value">
                            {healthData?.uptime ? `${healthData.uptime}s` : "Running"}
                          </span>
                        </div>
                        <div className="health-row">
                          <span className="key">Active Providers:</span>
                          <span className="value">
                            {providersList.length > 0 ? providersList.join(", ") : "All system default routes"}
                          </span>
                        </div>
                        {healthData && (
                          <pre className="json-snippet">
                            {JSON.stringify(healthData, null, 2)}
                          </pre>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* TAB 2: MODEL CATALOG */}
            {activeTab === "models" && (
              <div className="tab-pane animated-fade-in">
                <div className="catalog-toolbar">
                  <div className="search-box">
                    <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="11" cy="11" r="8" />
                      <path d="M21 21l-4.35-4.35" />
                    </svg>
                    <input
                      type="text"
                      placeholder="Search models by ID or provider..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                    />
                    {searchQuery && (
                      <button className="clear-search" onClick={() => setSearchQuery("")}>
                        ✕
                      </button>
                    )}
                  </div>

                  {providersList.length > 0 && (
                    <div className="provider-filter">
                      <select
                        value={selectedProvider}
                        onChange={(e) => setSelectedProvider(e.target.value)}
                      >
                        <option value="all">All Providers ({modelsData.length})</option>
                        {providersList.map((p) => (
                          <option key={p} value={p}>
                            {p}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}
                </div>

                {loading ? (
                  <div className="skeleton-loader">Loading model catalog...</div>
                ) : filteredModels.length === 0 ? (
                  <div className="empty-catalog">
                    <p>No models match your query standard.</p>
                    {searchQuery && (
                      <button onClick={() => { setSearchQuery(""); setSelectedProvider("all"); }}>
                        Clear Filters
                      </button>
                    )}
                  </div>
                ) : (
                  <div className="models-grid">
                    {filteredModels.map((model) => {
                      const providerTag = model.owned_by || model.id.split("/")[0] || "omniroute";
                      const isCopied = copiedText === model.id;
                      return (
                        <div key={model.id} className="model-card">
                          <div className="model-card-header">
                            <span className="provider-badge">{providerTag}</span>
                            <button
                              className="copy-model-btn"
                              onClick={() => copyToClipboard(model.id, model.id)}
                              title="Copy Model ID"
                            >
                              {isCopied ? "✓ Copied" : "📋 Copy ID"}
                            </button>
                          </div>
                          <div className="model-id" title={model.id}>
                            {model.id}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            )}

            {/* TAB 3: USAGE & ANALYTICS */}
            {activeTab === "usage" && (
              <div className="tab-pane animated-fade-in">
                {(() => {
                  const metrics = extractMetrics(telemetryData);
                  const breakdown = extractBreakdownLists(telemetryData);

                  return (
                    <>
                      <div className="metrics-grid">
                        <div className="stat-card">
                          <div className="stat-label">Total Tokens</div>
                          <div className="stat-value highlight-gold">
                            {formatTokens(metrics.totalTokens)}
                          </div>
                          <div className="stat-sub">
                            {metrics.totalTokens.toLocaleString()} total tokens processed
                          </div>
                        </div>

                        <div className="stat-card">
                          <div className="stat-label">Prompt Tokens (Input)</div>
                          <div className="stat-value highlight-violet">
                            {formatTokens(metrics.promptTokens)}
                          </div>
                          <div className="stat-sub">
                            {metrics.promptTokens > 0
                              ? `${metrics.promptTokens.toLocaleString()} input tokens`
                              : "Input token stream"}
                          </div>
                        </div>

                        <div className="stat-card">
                          <div className="stat-label">Completion Tokens (Output)</div>
                          <div className="stat-value">
                            {formatTokens(metrics.completionTokens)}
                          </div>
                          <div className="stat-sub">
                            {metrics.completionTokens > 0
                              ? `${metrics.completionTokens.toLocaleString()} output tokens`
                              : "Generated tokens"}
                          </div>
                        </div>

                        <div className="stat-card">
                          <div className="stat-label">Total Requests</div>
                          <div className="stat-value">
                            {metrics.requests > 0
                              ? metrics.requests.toLocaleString()
                              : telemetryData?.total_requests ?? telemetryData?.requests ?? 0}
                          </div>
                          <div className="stat-sub">
                            {metrics.cost > 0
                              ? `Est. Cost: $${metrics.cost.toFixed(4)}`
                              : metrics.savedTokens > 0
                              ? `Saved: ${formatTokens(metrics.savedTokens)} tokens`
                              : "Across all gateway model routes"}
                          </div>
                        </div>
                      </div>

                      {/* Provider Breakdown Table */}
                      {breakdown.providers.length > 0 && (
                        <div className="dashboard-card" style={{ marginTop: 14 }}>
                          <h4>Provider Usage Breakdown</h4>
                          <table className="telemetry-table">
                            <thead>
                              <tr>
                                <th>Provider</th>
                                <th>Requests</th>
                                <th>Input Tokens</th>
                                <th>Output Tokens</th>
                                <th>Total Tokens</th>
                                {breakdown.providers.some((p) => p.cost > 0) && (
                                  <th>Cost (Est.)</th>
                                )}
                              </tr>
                            </thead>
                            <tbody>
                              {breakdown.providers.map((p) => (
                                <tr key={p.name}>
                                  <td>
                                    <strong style={{ color: "var(--accent-soft)" }}>
                                      {p.name}
                                    </strong>
                                  </td>
                                  <td>{p.requests ? p.requests.toLocaleString() : "—"}</td>
                                  <td>{formatTokens(p.promptTokens)}</td>
                                  <td>{formatTokens(p.completionTokens)}</td>
                                  <td>
                                    <strong style={{ color: "var(--gold)" }}>
                                      {formatTokens(p.tokens)}
                                    </strong>
                                  </td>
                                  {breakdown.providers.some((pr) => pr.cost > 0) && (
                                    <td>{p.cost > 0 ? `$${p.cost.toFixed(4)}` : "—"}</td>
                                  )}
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}

                      {/* Model Breakdown Table */}
                      {breakdown.models.length > 0 && (
                        <div className="dashboard-card" style={{ marginTop: 14 }}>
                          <h4>Model Usage Breakdown</h4>
                          <table className="telemetry-table">
                            <thead>
                              <tr>
                                <th>Model</th>
                                <th>Requests</th>
                                <th>Input Tokens</th>
                                <th>Output Tokens</th>
                                <th>Total Tokens</th>
                              </tr>
                            </thead>
                            <tbody>
                              {breakdown.models.map((m) => (
                                <tr key={m.name}>
                                  <td>
                                    <code style={{ color: "var(--text)" }}>{m.name}</code>
                                  </td>
                                  <td>{m.requests ? m.requests.toLocaleString() : "—"}</td>
                                  <td>{formatTokens(m.promptTokens)}</td>
                                  <td>{formatTokens(m.completionTokens)}</td>
                                  <td>
                                    <strong style={{ color: "var(--gold)" }}>
                                      {formatTokens(m.tokens)}
                                    </strong>
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}

                      {/* Provider Latency Table if telemetry summary was returned */}
                      {telemetryData?.providers && !breakdown.providers.length && (
                        <div className="dashboard-card" style={{ marginTop: 14 }}>
                          <h4>Provider Latency & Request Breakdown</h4>
                          <table className="telemetry-table">
                            <thead>
                              <tr>
                                <th>Provider</th>
                                <th>Requests</th>
                                <th>p50 Latency</th>
                                <th>p95 Latency</th>
                                <th>p99 Latency</th>
                              </tr>
                            </thead>
                            <tbody>
                              {Object.entries<any>(telemetryData.providers).map(
                                ([name, stats]) => (
                                  <tr key={name}>
                                    <td>
                                      <strong style={{ color: "var(--accent-soft)" }}>
                                        {name}
                                      </strong>
                                    </td>
                                    <td>{stats.count ?? 0}</td>
                                    <td>{stats.p50 != null ? `${stats.p50}ms` : "—"}</td>
                                    <td>{stats.p95 != null ? `${stats.p95}ms` : "—"}</td>
                                    <td>{stats.p99 != null ? `${stats.p99}ms` : "—"}</td>
                                  </tr>
                                )
                              )}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </>
                  );
                })()}

                <div className="analytics-details" style={{ marginTop: 14 }}>
                  <div className="dashboard-card">
                    <h4>Cache & Memory Stats</h4>
                    {cacheData?.semanticCache ? (
                      <div className="health-details">
                        <div className="health-row">
                          <span className="key">Memory Cache Size:</span>
                          <span className="value">
                            {cacheData.semanticCache.memorySize ?? 0} / {cacheData.semanticCache.memoryMaxSize ?? 500}
                          </span>
                        </div>
                        <div className="health-row">
                          <span className="key">DB Cache Size:</span>
                          <span className="value">{cacheData.semanticCache.dbSize ?? 0}</span>
                        </div>
                        <div className="health-row">
                          <span className="key">Semantic Hit Rate:</span>
                          <span className="value highlight-gold">
                            {cacheData.semanticCache.hitRate != null
                              ? `${(cacheData.semanticCache.hitRate * 100).toFixed(1)}%`
                              : "N/A"}
                          </span>
                        </div>
                        {cacheData.idempotency && (
                          <div className="health-row">
                            <span className="key">Idempotency Window:</span>
                            <span className="value">
                              {cacheData.idempotency.activeKeys ?? 0} active keys ({cacheData.idempotency.windowMs / 1000}s)
                            </span>
                          </div>
                        )}
                      </div>
                    ) : (
                      <p className="hint">
                        Prompt cache & idempotency stats will populate in real time as model calls run.
                      </p>
                    )}
                  </div>

                  {telemetryData && (
                    <div className="dashboard-card">
                      <h4>Raw Telemetry Payload</h4>
                      <pre className="json-snippet">
                        {JSON.stringify(telemetryData, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* TAB 4: GATEWAY CONFIG & INFO */}
            {activeTab === "info" && (
              <div className="tab-pane animated-fade-in">
                <div className="config-grid">
                  <div className="dashboard-card">
                    <h4>Connection Mode</h4>
                    <div className="info-row">
                      <span className="info-label">Mode:</span>
                      <span className="info-val badge-mode">{mode?.toUpperCase()}</span>
                    </div>
                    <div className="info-row">
                      <span className="info-label">Base URL:</span>
                      <code className="info-code">{targetUrl}</code>
                    </div>
                    <div className="info-row">
                      <span className="info-label">Auth Token:</span>
                      <span className="info-val">
                        {apiKey ? "•••••••••••• (Configured)" : "None (Local Anonymous / Public)"}
                      </span>
                    </div>
                  </div>

                  <div className="dashboard-card">
                    <h4>CLI & SDK Quick Access</h4>
                    <p className="hint small">Use OmniRoute as a drop-in replacement for OpenAI endpoints:</p>

                    <div className="code-snippet-box">
                      <div className="snippet-header">
                        <span>OpenAI SDK Base URL</span>
                        <button
                          onClick={() =>
                            copyToClipboard(`${targetUrl}/v1`, "snippet-url")
                          }
                        >
                          {copiedText === "snippet-url" ? "✓ Copied" : "📋 Copy"}
                        </button>
                      </div>
                      <code>{`${targetUrl}/v1`}</code>
                    </div>

                    <div className="code-snippet-box" style={{ marginTop: 10 }}>
                      <div className="snippet-header">
                        <span>cURL Test Command</span>
                        <button
                          onClick={() =>
                            copyToClipboard(
                              `curl ${targetUrl}/v1/models${apiKey ? ` -H "Authorization: Bearer ${apiKey}"` : ""}`,
                              "snippet-curl"
                            )
                          }
                        >
                          {copiedText === "snippet-curl" ? "✓ Copied" : "📋 Copy"}
                        </button>
                      </div>
                      <code>{`curl ${targetUrl}/v1/models${apiKey ? ` -H "Authorization: Bearer ${apiKey}"` : ""}`}</code>
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
