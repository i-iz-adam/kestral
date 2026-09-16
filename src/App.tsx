import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Wizard from "./setup/Wizard";
import AppShell from "./app/AppShell";
import Titlebar from "./app/Titlebar";
import type { SetupStepDef } from "./setup/types";
import { ensureAgentEventsStarted } from "./app/agentStore";
import { checkAppUpdates } from "./app/updaterStore";

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
    // Trigger background check for updates on app launch
    checkAppUpdates().catch((err) => console.warn("Background update check error:", err));

    invoke<SetupStepDef[]>("get_pending_setup_steps")
      .then((steps) => {
        setNeedsSetup(steps.length > 0);
      })
      .finally(() => setChecking(false));
  }, []);

  return (
    <div className="app-container">
      <Titlebar />
      <div className="app-body">
        {checking ? (
          <div className="loading-screen">Loading...</div>
        ) : needsSetup ? (
          <Wizard onFinished={() => setNeedsSetup(false)} />
        ) : (
          <AppShell />
        )}
      </div>
    </div>
  );
}
