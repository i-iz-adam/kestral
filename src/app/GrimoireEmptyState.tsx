import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Session, Workspace } from "../types";
import AmbientMotes from "./AmbientMotes";
import { getActiveWorkspace } from "./agentStore";

interface Props {
  onSelectSession: (id: string) => void;
  onSessionCreated: (id: string) => void;
  onOpenWorkspace: () => void;
}

interface StarterSpell {
  icon: string;
  title: string;
  description: string;
  promptTitle: string;
  tag: string;
}

const STARTER_SPELLS: StarterSpell[] = [
  {
    icon: "🔮",
    title: "Feature Forge",
    description: "Inscribe a new UI component, backend route, or application logic.",
    promptTitle: "Feature: Craft new capability",
    tag: "Craft",
  },
  {
    icon: "🛡️",
    title: "Bug Remedy",
    description: "Diagnose failing tests, build errors, or unexpected runtime behavior.",
    promptTitle: "Debug: Investigate & fix error",
    tag: "Repair",
  },
  {
    icon: "📜",
    title: "Architect Review",
    description: "Audit repository structure, explain complex code, or review diffs.",
    promptTitle: "Review: Code & architecture audit",
    tag: "Audit",
  },
  {
    icon: "⚡",
    title: "Quick Incantation",
    description: "Ask an open question or perform a quick targeted code tweak.",
    promptTitle: "Quick Session",
    tag: "Fast",
  },
];

function formatTimeAgo(timestamp?: number): string {
  if (!timestamp) return "Recently";
  const now = Date.now();
  const diffSec = Math.floor((now - timestamp) / 1000);
  if (diffSec < 60) return "Just now";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 30) return `${diffDays}d ago`;
  return new Date(timestamp).toLocaleDateString();
}

export default function GrimoireEmptyState({
  onSelectSession,
  onSessionCreated,
  onOpenWorkspace,
}: Props) {
  const [creating, setCreating] = useState(false);
  const [activeSpellTitle, setActiveSpellTitle] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const activeWorkspacePath = getActiveWorkspace();

  const fetchSessions = useCallback(() => {
    invoke<Session[]>("list_sessions")
      .then((list) => {
        const active = list.filter((s) => !s.archived);
        active.sort((a, b) => (b.updated_at ?? b.created_at) - (a.updated_at ?? a.created_at));
        setSessions(active.slice(0, 4));
      })
      .catch(() => {});
  }, []);

  const fetchWorkspaces = useCallback(() => {
    invoke<Workspace[]>("list_workspaces")
      .then(setWorkspaces)
      .catch(() => {});
  }, []);

  useEffect(() => {
    fetchSessions();
    fetchWorkspaces();
  }, [fetchSessions, fetchWorkspaces]);

  const createNewSession = async (title = "New session") => {
    if (creating) return;
    setCreating(true);
    try {
      const session = await invoke<Session>("create_session", {
        title,
        mode: "coding",
        workspace: getActiveWorkspace(),
      });
      onSessionCreated(session.id);
    } catch (e) {
      console.error("Failed to summon session:", e);
    } finally {
      setCreating(false);
      setActiveSpellTitle(null);
    }
  };

  const handleSpellClick = (spell: StarterSpell) => {
    setActiveSpellTitle(spell.title);
    createNewSession(spell.promptTitle);
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        createNewSession("New session");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const activeWorkspaceName = activeWorkspacePath
    ? workspaces.find((w) => w.path === activeWorkspacePath)?.name ??
      activeWorkspacePath.split("/").pop() ??
      activeWorkspacePath
    : "No active workspace";

  return (
    <div className="grimoire-empty-container">
      {/* Background Ember Particles & Radial Spell Glow */}
      <div className="grimoire-bg-glow" aria-hidden="true" />
      <div className="grimoire-motes-wrap" aria-hidden="true">
        <AmbientMotes count={32} />
      </div>

      <div className="grimoire-content">
        {/* Hero Section */}
        <div className="grimoire-hero">
          <div className="grimoire-emblem-wrap">
            <div className="grimoire-aura-ring" />
            <div className="grimoire-orbit-rune" />
            <svg
              className="grimoire-emblem-svg"
              viewBox="0 0 120 120"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <defs>
                <linearGradient id="grimoireCover" x1="0" y1="0" x2="120" y2="120">
                  <stop offset="0%" stopColor="#2e224d" />
                  <stop offset="50%" stopColor="#181329" />
                  <stop offset="100%" stopColor="#0b0814" />
                </linearGradient>
                <linearGradient id="grimoireGold" x1="0" y1="0" x2="120" y2="120">
                  <stop offset="0%" stopColor="#f3e0aa" />
                  <stop offset="50%" stopColor="#e0b45c" />
                  <stop offset="100%" stopColor="#9a7125" />
                </linearGradient>
                <linearGradient id="manaGlow" x1="20" y1="20" x2="100" y2="100">
                  <stop offset="0%" stopColor="#b794ff" stopOpacity="0.8" />
                  <stop offset="50%" stopColor="#9b6bff" stopOpacity="0.5" />
                  <stop offset="100%" stopColor="#e0b45c" stopOpacity="0" />
                </linearGradient>
                <filter id="glowFilter" x="-20%" y="-20%" width="140%" height="140%">
                  <feGaussianBlur stdDeviation="4" result="blur" />
                  <feComposite in="SourceGraphic" in2="blur" operator="over" />
                </filter>
              </defs>

              <circle cx="60" cy="60" r="50" fill="url(#manaGlow)" />
              <circle cx="60" cy="60" r="48" stroke="url(#grimoireGold)" strokeWidth="1" strokeDasharray="4 6" opacity="0.4" />

              <path
                d="M 24,78 C 36,72 52,73 60,78 C 68,73 84,72 96,78 L 96,38 C 84,32 68,33 60,38 C 52,33 36,32 24,38 Z"
                fill="url(#grimoireCover)"
                stroke="url(#grimoireGold)"
                strokeWidth="2"
              />

              <path
                d="M 28,41 C 38,36 50,37 57,41 L 57,75 C 50,71 38,70 28,75 Z"
                fill="#1f1836"
                stroke="rgba(183, 148, 255, 0.3)"
                strokeWidth="1"
              />
              <path
                d="M 63,41 C 70,37 82,36 92,41 L 92,75 C 82,70 70,71 63,75 Z"
                fill="#1f1836"
                stroke="rgba(183, 148, 255, 0.3)"
                strokeWidth="1"
              />

              <path d="M 60,35 L 60,82" stroke="url(#grimoireGold)" strokeWidth="2.5" strokeLinecap="round" />
              <path d="M 60,82 Q 64,92 60,98 Q 57,93 60,82" fill="url(#grimoireGold)" opacity="0.85" />

              <line x1="33" y1="48" x2="52" y2="48" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />
              <line x1="33" y1="54" x2="48" y2="54" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />
              <line x1="33" y1="60" x2="51" y2="60" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />

              <line x1="68" y1="48" x2="87" y2="48" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />
              <line x1="72" y1="54" x2="87" y2="54" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />
              <line x1="68" y1="60" x2="84" y2="60" stroke="var(--text-dim)" strokeWidth="1" opacity="0.5" />

              <g filter="url(#glowFilter)">
                <polygon points="60,18 66,27 60,34 54,27" fill="#e0b45c" />
                <polygon points="60,20 64,27 60,32 56,27" fill="#ffffff" opacity="0.8" />
              </g>

              <circle cx="36" cy="24" r="1.5" fill="#b794ff" opacity="0.8" />
              <circle cx="84" cy="24" r="1.5" fill="#e0b45c" opacity="0.9" />
              <circle cx="20" cy="55" r="1.2" fill="#e0b45c" opacity="0.7" />
              <circle cx="100" cy="55" r="1.2" fill="#b794ff" opacity="0.7" />
            </svg>
          </div>

          <h1 className="grimoire-title">The Arcane Grimoire</h1>
          <p className="grimoire-subtitle">
            Summon an agent session to craft code, diagnose errors, or explore your repository.
          </p>

          <div className="grimoire-action-row">
            <button
              className="grimoire-primary-btn"
              onClick={() => createNewSession("New session")}
              disabled={creating}
            >
              <span className="btn-spark" aria-hidden="true">✨</span>
              <span>{creating && !activeSpellTitle ? "Summoning Session..." : "Summon New Session"}</span>
              <kbd className="btn-kbd">⌘N</kbd>
            </button>
          </div>
        </div>

        {/* Quick Starter Spells */}
        <div className="grimoire-section">
          <div className="grimoire-section-header">
            <span className="section-rune">✦</span>
            <h2>Inscribe a Starter Spell</h2>
            <span className="section-rune">✦</span>
          </div>

          <div className="grimoire-spells-grid">
            {STARTER_SPELLS.map((spell) => {
              const isSelected = creating && activeSpellTitle === spell.title;
              return (
                <button
                  key={spell.title}
                  className={`grimoire-spell-card ${isSelected ? "summoning" : ""}`}
                  onClick={() => handleSpellClick(spell)}
                  disabled={creating}
                >
                  <div className="spell-card-header">
                    <span className="spell-icon">{spell.icon}</span>
                    <span className="spell-tag">{spell.tag}</span>
                  </div>
                  <h3 className="spell-title">{spell.title}</h3>
                  <p className="spell-desc">{spell.description}</p>
                  <div className="spell-card-footer">
                    <span>{isSelected ? "Summoning..." : "Inscribe Spell →"}</span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Active Workspace & Recent Sessions Grid */}
        <div className="grimoire-bottom-grid">
          <div className="grimoire-card workspace-portal-card">
            <div className="card-header">
              <span className="card-icon">📂</span>
              <h3>Active Realm Workspace</h3>
            </div>
            <p className="workspace-path-display" title={activeWorkspacePath ?? ""}>
              {activeWorkspaceName}
            </p>
            <p className="workspace-hint">
              {activeWorkspacePath
                ? activeWorkspacePath
                : "No workspace selected yet for session tools."}
            </p>
            <button className="grimoire-secondary-btn" onClick={onOpenWorkspace}>
              <span>Switch Workspace Realm</span>
            </button>
          </div>

          <div className="grimoire-card recent-sessions-card">
            <div className="card-header">
              <span className="card-icon">📖</span>
              <h3>Recent Tome Chapters</h3>
            </div>
            {sessions.length > 0 ? (
              <div className="recent-sessions-list">
                {sessions.map((s) => (
                  <button
                    key={s.id}
                    className="recent-session-item"
                    onClick={() => onSelectSession(s.id)}
                  >
                    <div className="recent-item-info">
                      <span className="recent-item-title">{s.title || "Untitled Session"}</span>
                      <span className="recent-item-meta">
                        {formatTimeAgo(s.updated_at ?? s.created_at)}
                      </span>
                    </div>
                    <span className="recent-item-arrow">→</span>
                  </button>
                ))}
              </div>
            ) : (
              <div className="empty-recents-hint">
                <p>Your spellbook is currently empty.</p>
                <p className="hint-sub">Summon a new session above to begin your first chapter.</p>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="grimoire-footer">
          <span>✦</span>
          <span>Press <kbd>⌘ N</kbd> or <kbd>Ctrl N</kbd> to summon instantly</span>
          <span>•</span>
          <span>Kestrel Arcane Engine</span>
          <span>✦</span>
        </div>
      </div>
    </div>
  );
}
