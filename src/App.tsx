import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import Wizard from "./setup/Wizard";
import AppShell from "./app/AppShell";
import type { SetupStepDef } from "./setup/types";

export default function App() {
  const [checking, setChecking] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);

  useEffect(() => {
    invoke<SetupStepDef[]>("get_pending_setup_steps")
      .then((steps) => {
        setNeedsSetup(steps.length > 0);
      })
      .finally(() => setChecking(false));
  }, []);

  if (checking) {
    return <div className="loading-screen">Loading...</div>;
  }

  if (needsSetup) {
    return <Wizard onFinished={() => setNeedsSetup(false)} />;
  }

  return <AppShell />;
}
