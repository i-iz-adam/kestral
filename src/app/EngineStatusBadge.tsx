import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";

type Status = "stopped" | "starting" | "running" | "error";

interface EnginePayload {
  status: Status;
  error?: string;
}

export default function EngineStatusBadge() {
  const [omniMode, setOmniMode] = useState<string | null>(null);
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installLine, setInstallLine] = useState("");
  const [installError, setInstallError] = useState<string | null>(null);

  const [status, setStatus] = useState<Status>("stopped");
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    invoke<{ mode: string } | null>("get_omniroute_config").then((cfg) => {
      setOmniMode(cfg?.mode ?? null);
    });
    invoke<boolean>("is_engine_installed").then(setInstalled);
    invoke<{ status: Status; error?: string }>("get_engine_status").then((s) => {
      setStatus(s.status);
      setError(s.error ?? null);
    });

    const unlistenStatus = listen<EnginePayload>("engine://status", (evt) => {
      setStatus(evt.payload.status);
      setError(evt.payload.error ?? null);
    });
    const unlistenLog = listen<{ line: string }>("engine://install-log", (evt) => {
      setInstallLine(evt.payload.line);
    });
    const unlistenDone = listen<{ success: boolean; error?: string }>(
      "engine://install-done",
      (evt) => {
        setInstalling(false);
        if (evt.payload.success) {
          setInstalled(true);
          setInstallError(null);
        } else {
          setInstallError(evt.payload.error ?? "Install failed");
        }
      }
    );
    return () => {
      unlistenStatus.then((f) => f());
      unlistenLog.then((f) => f());
      unlistenDone.then((f) => f());
    };
  }, []);

  // While "starting", a launched process isn't necessarily a ready server
  // yet — poll the real endpoint and only call it Running once it actually
  // answers.
  useEffect(() => {
    if (status === "starting") {
      pollRef.current = setInterval(async () => {
        try {
          const ok = await invoke<boolean>("test_omniroute_connection", {
            config: { mode: "local", remote_url: null, api_key: null },
          });
          if (ok) {
            await invoke("confirm_engine_running");
          }
        } catch {
          // not up yet — keep polling
        }
      }, 2000);
    } else if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [status]);

  // Nothing for this app to manage when pointed at a remote instance.
  if (omniMode !== "local") return null;

  const install = async () => {
    setInstalling(true);
    setInstallError(null);
    setInstallLine("");
    await invoke("install_engine");
  };

  // Not installed yet — offer the one-time local install instead of the
  // normal status row. This is a real ~450MB download OmniRoute itself
  // pulls in, so say so rather than leaving a bare spinner.
  if (installed === false) {
    return (
      <div className={"engine-badge " + (installing ? "starting" : "")}>
        <span className="engine-dot" />
        <span className="engine-label" title={installLine || undefined}>
          {installing
            ? installLine || "Installing OmniRoute..."
            : installError || "OmniRoute not installed (~450MB, one-time)"}
        </span>
        {!installing && <button onClick={install}>Install</button>}
      </div>
    );
  }

  const label =
    status === "running"
      ? "OmniRoute running"
      : status === "starting"
      ? "Starting OmniRoute..."
      : status === "error"
      ? "OmniRoute error"
      : "OmniRoute stopped";

  return (
    <div className={"engine-badge " + status} title={error ?? undefined}>
      <span className="engine-dot" />
      <span className="engine-label">{label}</span>
      {status === "stopped" || status === "error" ? (
        <button onClick={() => invoke("start_engine")}>Start</button>
      ) : (
        <button onClick={() => invoke("stop_engine")}>Stop</button>
      )}
    </div>
  );
}
