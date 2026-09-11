import { useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { StepProps } from "../types";

type Mode = "local" | "remote";

interface OmniRouteConfigPayload {
  mode: Mode;
  remote_url: string | null;
  api_key: string | null;
}

export default function OmniRouteConfig({ onComplete }: StepProps) {
  const [mode, setMode] = useState<Mode>("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "fail">("idle");
  const [error, setError] = useState<string | null>(null);

  const buildConfig = (): OmniRouteConfigPayload => ({
    mode,
    remote_url: mode === "remote" ? remoteUrl.trim() : null,
    api_key: mode === "remote" ? apiKey : null,
  });

  const test = async () => {
    setStatus("testing");
    setError(null);
    try {
      const ok = await invoke<boolean>("test_omniroute_connection", {
        config: buildConfig(),
      });
      setStatus(ok ? "ok" : "fail");
    } catch (e) {
      setStatus("fail");
      setError(String(e));
    }
  };

  const save = async () => {
    if (mode === "remote" && !remoteUrl.trim()) {
      setError("Enter a URL for the remote OmniRoute instance first.");
      return;
    }
    setError(null);
    await invoke("save_omniroute_config", { config: buildConfig() });
    onComplete();
  };

  return (
    <div className="step-card">
      <h2>Connect your LLM backend</h2>
      <p>
        Model calls route through OmniRoute. Run it on this machine, or
        point at one already running elsewhere.
      </p>

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
            placeholder="https://your-server:20128"
          />
          <label>API key</label>
          <input
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            type="password"
            placeholder="optional"
          />
        </div>
      )}

      {mode === "local" && (
        <p className="hint">
          Local mode expects OmniRoute at 127.0.0.1:20128. A bundled,
          auto-managed copy is planned — for now, start it yourself before
          testing the connection.
        </p>
      )}

      <div className="row">
        <button onClick={test} disabled={status === "testing"}>
          Test connection
        </button>
        {status === "ok" && <span className="ok">Connected</span>}
        {status === "fail" && <span className="fail">Couldn't connect</span>}
      </div>
      {error && <p className="fail">{error}</p>}

      <button className="primary" onClick={save}>
        Continue
      </button>
    </div>
  );
}
