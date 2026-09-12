import { useState } from "react";
import Sidebar from "./Sidebar";
import SessionView from "./SessionView";
import Settings from "./Settings";
import GithubPanel from "./GithubPanel";
import SkillsPanel from "./SkillsPanel";
import About from "./About";
import ProvidersPanel from "./ProvidersPanel";
import AmbientMotes from "./AmbientMotes";

type View =
  | { kind: "session"; id: string }
  | { kind: "settings" }
  | { kind: "github" }
  | { kind: "skills" }
  | { kind: "about" }
  | { kind: "providers" }
  | { kind: "empty" };

export default function AppShell() {
  const [view, setView] = useState<View>({ kind: "empty" });
  const [refreshKey, setRefreshKey] = useState(0);

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
        onOpenGithub={() => setView({ kind: "github" })}
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
          {view.kind === "settings" && <Settings />}
          {view.kind === "github" && <GithubPanel />}
          {view.kind === "skills" && <SkillsPanel />}
          {view.kind === "about" && <About />}
          {view.kind === "providers" && <ProvidersPanel />}
          {view.kind === "empty" && (
            <div className="empty-state">
              <p>Pick a session on the left, or start a new one.</p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
