import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/api/shell";
import type { OmniRouteConfigPayload } from "../types";

type EngineStatus = "stopped" | "starting" | "running" | "error";

export default function ProvidersPanel() {
  const [mode, setMode] = useState<"local" | "remote" | null>(null);
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  const [engineStatus, setEngineStatus] = useState<EngineStatus>("stopped");
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    invoke<OmniRouteConfigPayload | null>("get_omniroute_config").then((cfg) => {
      if (!cfg) return;
      setMode(cfg.mode);
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

  if (mode === null) {
    return (
      <div className="settings-view">
        <h2>Providers</h2>
        <p className="hint">Finish setting up OmniRoute in Settings first.</p>
      </div>
    );
  }

  const showFrame = mode === "remote" || engineStatus === "running";

  return (
    <div className="providers-view">
      <div className="providers-header">
        <h2>Providers</h2>
        <span className="hint small">
          OmniRoute's own dashboard, embedded — connect providers, watch
          usage, and tune routing here directly.
        </span>
        <div className="providers-actions">
          {showFrame && (
            <button onClick={() => setReloadKey((k) => k + 1)}>Reload</button>
          )}
          {baseUrl && (
            <button onClick={() => open(baseUrl)}>Open in browser</button>
          )}
        </div>
      </div>

      {!showFrame ? (
        <div className="empty-state">
          <p>OmniRoute isn't running yet.</p>
          <button
            className="primary"
            onClick={() => invoke("start_engine")}
            disabled={engineStatus === "starting"}
          >
            {engineStatus === "starting" ? "Starting..." : "Start OmniRoute"}
          </button>
        </div>
      ) : (
        baseUrl && (
          <>
            <iframe
              key={baseUrl + reloadKey}
              src={baseUrl}
              className="providers-frame"
              title="OmniRoute dashboard"
            />
            <p className="hint small providers-note">
              If this looks blank, OmniRoute's server may be blocking being
              embedded — use "Open in browser" above instead.
            </p>
          </>
        )
      )}
    </div>
  );
}
