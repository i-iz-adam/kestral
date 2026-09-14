import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open } from "@tauri-apps/api/dialog";
import type { OmniRouteConfigPayload, SessionDefaults, Workspace } from "../types";
import UpdaterModal from "./UpdaterModal";
import CustomInstallerModal from "./CustomInstallerModal";

export default function Settings() {
  const [mode, setMode] = useState<"local" | "remote">("local");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [status, setStatus] = useState<"idle" | "testing" | "ok" | "fail">(
    "idle"
  );
  const [saved, setSaved] = useState(false);
  const [engineCommand, setEngineCommand] = useState("npx");
  const [engineArgs, setEngineArgs] = useState("-y omniroute");
  const [engineAutoStart, setEngineAutoStart] = useState(true);
  const [engineSaved, setEngineSaved] = useState(false);

  const [defaultPlanning, setDefaultPlanning] = useState(true);
  const [defaultSubagents, setDefaultSubagents] = useState(true);
  const [defaultGracefulStop, setDefaultGracefulStop] = useState(true);
  const [defaultsSaved, setDefaultsSaved] = useState(false);

  const [updaterOpen, setUpdaterOpen] = useState(false);
  const [installerOpen, setInstallerOpen] = useState(false);

  useEffect(() => {
    invoke<OmniRouteConfigPayload | null>("get_omniroute_config").then(
      (cfg) => {
        if (cfg) {
          setMode(cfg.mode);
          setRemoteUrl(cfg.remote_url ?? "");
          setApiKey(cfg.api_key ?? "");
        }
      }
    );
    invoke<Workspace[]>("list_workspaces").then(setWorkspaces);
    invoke<{ command: string; args: string[]; auto_start: boolean }>(
      "get_engine_config"
    ).then((cfg) => {
      setEngineCommand(cfg.command);
      setEngineArgs(cfg.args.join(" "));
      setEngineAutoStart(cfg.auto_start);
    });
    invoke<SessionDefaults>("get_session_defaults").then((defs) => {
      if (defs) {
        setDefaultPlanning(defs.planning_enabled ?? true);
        setDefaultSubagents(defs.subagents_enabled ?? true);
        setDefaultGracefulStop(defs.graceful_stop ?? true);
      }
    });
  }, []);

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

  const addWorkspace = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    const workspace = await invoke<Workspace>("add_workspace", {
      name: null,
      path: selected,
    });
    setWorkspaces((list) =>
      list.some((w) => w.id === workspace.id) ? list : [...list, workspace]
    );
  };

  const removeWorkspace = async (id: string) => {
    await invoke("remove_workspace", { id });
    setWorkspaces((list) => list.filter((w) => w.id !== id));
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

  return (
    <div className="settings-view">
      <h2>Settings</h2>

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
          <button className="primary" onClick={save}>
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
            className="primary"
            onClick={saveEngine}
            style={{ marginTop: 10 }}
          >
            {engineSaved ? "Saved" : "Save"}
          </button>
        </section>
      )}

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
          className="primary"
          onClick={saveDefaults}
          style={{ marginTop: 10 }}
        >
          {defaultsSaved ? "Saved" : "Save"}
        </button>
      </section>

      <section>
        <h3>Application Updates & Installer</h3>
        <p className="hint small">
          Check for software updates or rerun the custom installation setup.
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

      <section>
        <h3>Workspaces</h3>
        <p className="hint small">
          Folders the agent can work from. Each session picks one when it's
          created and can switch later — add as many projects here as you
          like.
        </p>
        <div className="workspace-list">
          {workspaces.map((w) => (
            <div className="workspace-list-item" key={w.id}>
              <div>
                <span className="workspace-list-name">{w.name}</span>
                <span className="hint small">{w.path}</span>
              </div>
              <button onClick={() => removeWorkspace(w.id)} title="Remove">
                Remove
              </button>
            </div>
          ))}
          {workspaces.length === 0 && (
            <p className="hint small">No workspaces yet.</p>
          )}
        </div>
        <button onClick={addWorkspace} style={{ marginTop: 10 }}>
          Add folder
        </button>
      </section>

      <UpdaterModal isOpen={updaterOpen} onClose={() => setUpdaterOpen(false)} />
      <CustomInstallerModal isOpen={installerOpen} onClose={() => setInstallerOpen(false)} />
    </div>
  );
}
