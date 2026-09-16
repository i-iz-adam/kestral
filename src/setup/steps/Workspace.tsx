import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import type { StepProps } from "../types";

export default function Workspace({ onComplete }: StepProps) {
  const [path, setPath] = useState<string | null>(null);

  const pick = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") setPath(selected);
  };

  const save = async () => {
    if (!path) return;
    await invoke("add_workspace", { name: null, path });
    onComplete();
  };

  return (
    <div className="step-card">
      <h2>Choose a workspace</h2>
      <p>
        Pick the folder the agent reads from and writes to by default. You
        can add more projects later.
      </p>
      <button onClick={pick}>Choose folder</button>
      {path && <p className="hint">{path}</p>}
      <button className="primary" onClick={save} disabled={!path}>
        Continue
      </button>
    </div>
  );
}
