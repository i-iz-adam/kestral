import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import type { StepProps } from "../types";

export interface PythonStatusPayload {
  installed: boolean;
  version?: string | null;
  binary?: string | null;
}

export default function PythonSetup({ onComplete }: StepProps) {
  const [status, setStatus] = useState<PythonStatusPayload | null>(null);
  const [checking, setChecking] = useState(true);

  const checkStatus = () => {
    setChecking(true);
    invoke<PythonStatusPayload>("check_python_installed")
      .then((res) => {
        setStatus(res);
        setChecking(false);
      })
      .catch(() => {
        setStatus({ installed: false });
        setChecking(false);
      });
  };

  useEffect(() => {
    checkStatus();
  }, []);

  const handleDownload = () => {
    open("https://www.python.org/downloads/");
  };

  const handleContinue = () => {
    onComplete();
  };

  return (
    <div className="step-card">
      <h2>Python Sandbox Environment</h2>
      <p>
        Kestrel uses Python to run a sandboxed execution tool for agent reasoning, math calculations, data crunching, and chart generation.
      </p>

      {checking ? (
        <div className="hint" style={{ margin: "16px 0" }}>
          Checking Python installation...
        </div>
      ) : status?.installed ? (
        <div style={{ margin: "16px 0" }}>
          <div className="ok" style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 500 }}>
            <span>✓</span> Python is installed ({status.version || status.binary})
          </div>
          <p className="hint small" style={{ marginTop: 6 }}>
            The <code>run_python</code> execution tool is verified and ready for agent use.
          </p>
        </div>
      ) : (
        <div style={{ margin: "16px 0" }}>
          <div className="fail" style={{ display: "flex", alignItems: "center", gap: 6, fontWeight: 500 }}>
            <span>⚠️</span> Python was not found on your system PATH.
          </div>
          <p className="hint small" style={{ marginTop: 6 }}>
            Without Python, the <code>run_python</code> tool will remain disabled for the agent. You can download and install Python now, or decline and install it later from Settings.
          </p>
          <div className="row" style={{ marginTop: 12, gap: 8, flexWrap: "wrap" }}>
            <button type="button" onClick={handleDownload} style={{ background: "var(--accent)", color: "var(--bg)", borderColor: "var(--accent)" }}>
              Download Python
            </button>
            <button type="button" onClick={checkStatus}>
              Re-check Installation
            </button>
          </div>
        </div>
      )}

      <div className="row" style={{ marginTop: 20, justifyContent: "space-between" }}>
        {!status?.installed && (
          <button
            type="button"
            onClick={handleContinue}
            style={{ color: "var(--text-dim)", background: "transparent", border: "none", cursor: "pointer", fontSize: 13 }}
          >
            Decline / Skip for now
          </button>
        )}
        <button className="primary" onClick={handleContinue} style={{ marginLeft: "auto" }}>
          {status?.installed ? "Continue" : "Continue without Python"}
        </button>
      </div>
    </div>
  );
}
