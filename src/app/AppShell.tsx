import { useEffect, useState } from "react";
import Sidebar from "./Sidebar";
import SessionView from "./SessionView";
import Settings, { type SettingsTab } from "./Settings";
import WorkspacePanel from "./WorkspacePanel";
import SkillsPanel from "./SkillsPanel";
import About from "./About";
import ProvidersPanel from "./ProvidersPanel";
import AmbientMotes from "./AmbientMotes";
import { setActiveSession } from "./agentStore";

type View =
  | { kind: "session"; id: string }
  | { kind: "settings"; tab?: SettingsTab }
  | { kind: "workspace" }
  | { kind: "skills" }
  | { kind: "about" }
  | { kind: "providers" }
  | { kind: "empty" };

export default function AppShell() {
  const [view, setView] = useState<View>({ kind: "empty" });
  const [refreshKey, setRefreshKey] = useState(0);

  // The store needs to know which session (if any) is on screen purely to
  // decide whether a background turn finishing counts as "unseen" — this
  // is the only thing that ties AppShell to agentStore.
  useEffect(() => {
    setActiveSession(view.kind === "session" ? view.id : null);
  }, [view]);

  // Keying the transition wrapper on the view identity (not just "session"
  // vs "settings") is what makes switching between two different sessions
  // also get a page-turn, not just switching view *kinds* — every
  // navigation in the sidebar reads as turning to a new page.
  const viewKey = view.kind === "session" ? `session:${view.id}` : view.kind;

  return (
    <div className="shell">
      <AmbientMotes />
      <Sidebar
        activeSessionId={view.kind === "session" ? view.id : null}
        onSelectSession={(id) => setView({ kind: "session", id })}
        onOpenSettings={() => setView({ kind: "settings" })}
        onOpenWorkspace={() => setView({ kind: "workspace" })}
        onOpenSkills={() => setView({ kind: "skills" })}
        onOpenAbout={() => setView({ kind: "about" })}
        onOpenProviders={() => setView({ kind: "providers" })}
        onSessionCreated={(id) => {
          setRefreshKey((k) => k + 1);
          setView({ kind: "session", id });
        }}
        refreshKey={refreshKey}
      />
      <div className="shell-content">
        <div className="view-transition" key={viewKey}>
          {view.kind === "session" && <SessionView sessionId={view.id} />}
          {view.kind === "settings" && <Settings initialTab={view.tab} />}
          {view.kind === "workspace" && <WorkspacePanel />}
          {view.kind === "skills" && <SkillsPanel />}
          {view.kind === "about" && <About />}
          {view.kind === "providers" && <ProvidersPanel />}
          {view.kind === "empty" && (
            <div className="empty-state" style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", position: "relative", overflow: "hidden" }}>
              <div style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, zIndex: -1, opacity: 0.5 }}>
                <AmbientMotes count={24} />
              </div>
              <p>Pick a session on the left, or start a new one.</p>
              <button className="primary" style={{ marginTop: "16px", zIndex: 1 }} onClick={() => document.querySelector<HTMLButtonElement>(".sidebar > button.primary")?.click()}>New Session</button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}