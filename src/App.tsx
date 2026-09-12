import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import Wizard from "./setup/Wizard";
import AppShell from "./app/AppShell";
import type { SetupStepDef } from "./setup/types";
import { ensureAgentEventsStarted } from "./app/agentStore";

export default function App() {
  const [checking, setChecking] = useState(true);
  const [needsSetup, setNeedsSetup] = useState(false);

  useEffect(() => {
    // Registered here (not just from SessionView) so a turn already
    // running in the background is never missed no matter what's on
    // screen when it starts producing events — and calling it again from
    // SessionView's own mount is a no-op, guarded by agentStore's own
    // `started` flag.
    ensureAgentEventsStarted();
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
