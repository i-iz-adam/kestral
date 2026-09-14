import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type { Skill, SkillProposal } from "../types";
import Modal from "./Modal";
import WorkspacePicker from "./WorkspacePicker";
import { getCurrentWorkspaceFilter, subscribeWorkspaceFilter } from "./agentStore";

type Scope = "global" | "project";

interface NewSkillDraft {
  name: string;
  description: string;
  content: string;
  triggers: string;
}

const emptyNewSkill = (): NewSkillDraft => ({ name: "", description: "", content: "", triggers: "" });

export default function SkillsPanel() {
  const [scope, setScope] = useState<Scope>("global");
  // Defaults to whatever workspace the sidebar is currently filtered to
  // (if any) — a reasonable starting guess for "which project am I
  // probably asking about" — but is independently changeable here via
  // the picker, since browsing another project's skills shouldn't
  // require switching the whole sidebar's filter first.
  const [workspacePath, setWorkspacePath] = useState<string | null>(() => getCurrentWorkspaceFilter());
  const [skills, setSkills] = useState<Skill[]>([]);
  const [proposals, setProposals] = useState<SkillProposal[]>([]);
  const [agentsMd, setAgentsMd] = useState<string | null>(null);
  const [agentsMdLoaded, setAgentsMdLoaded] = useState(false);
  const [viewingAgentsMd, setViewingAgentsMd] = useState(false);
  const [installUrl, setInstallUrl] = useState("");
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewing, setViewing] = useState<Skill | null>(null);
  const [content, setContent] = useState<string>("");
  const [reviewing, setReviewing] = useState<SkillProposal | null>(null);
  const [draft, setDraft] = useState<{ name: string; description: string; content: string }>({
    name: "",
    description: "",
    content: "",
  });
  const [newSkill, setNewSkill] = useState<NewSkillDraft | null>(null);
  const [savingNewSkill, setSavingNewSkill] = useState(false);

  const load = () => {
    invoke<Skill[]>("list_skills", { workspace: workspacePath ?? undefined }).then(setSkills);
    invoke<SkillProposal[]>("list_skill_proposals").then(setProposals);
  };

  useEffect(load, [workspacePath]);

  // Stay in sync if the sidebar's workspace filter changes while this
  // panel is open, same as everything else that reads it.
  useEffect(() => subscribeWorkspaceFilter(() => setWorkspacePath(getCurrentWorkspaceFilter())), []);

  useEffect(() => {
    if (!workspacePath) {
      setAgentsMd(null);
      setAgentsMdLoaded(true);
      return;
    }
    setAgentsMdLoaded(false);
    invoke<string | null>("get_agents_md", { workspace: workspacePath })
      .then(setAgentsMd)
      .catch(() => setAgentsMd(null))
      .finally(() => setAgentsMdLoaded(true));
  }, [workspacePath]);

  // The reflection pass (see reflect.rs) runs in the background, well
  // after any tool-call UI has settled — a live listener is how a newly
  // proposed skill actually shows up without the user having to reopen
  // this panel to notice it.
  useEffect(() => {
    const unlisten = listen("agent://skill-proposed", () => load());
    return () => {
      unlisten.then((f) => f());
    };
  }, [workspacePath]);

  const toggle = async (skill: Skill) => {
    await invoke("toggle_skill", { id: skill.id, enabled: !skill.enabled });
    load();
  };

  const view = async (skill: Skill) => {
    const c = await invoke<string>("get_skill_content", { id: skill.id, workspace: workspacePath ?? undefined });
    setContent(c);
    setViewing(skill);
  };

  const remove = async (skill: Skill) => {
    try {
      await invoke("delete_skill", { id: skill.id, workspace: workspacePath ?? undefined });
      load();
    } catch (e) {
      setError(String(e));
    }
  };

  const install = async () => {
    if (!installUrl.trim()) return;
    setInstalling(true);
    setError(null);
    try {
      await invoke("install_skill_from_url", { url: installUrl.trim() });
      setInstallUrl("");
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstalling(false);
    }
  };

  const openReview = (p: SkillProposal) => {
    setDraft({ name: p.name, description: p.description, content: p.content });
    setReviewing(p);
  };

  const saveDraft = async () => {
    if (!reviewing) return;
    await invoke("update_skill_proposal", {
      id: reviewing.id,
      name: draft.name,
      description: draft.description,
      content: draft.content,
      triggers: reviewing.triggers,
    });
  };

  const accept = async () => {
    if (!reviewing) return;
    setError(null);
    try {
      await saveDraft();
      await invoke("accept_skill_proposal", { id: reviewing.id });
      setReviewing(null);
      load();
    } catch (e) {
      setError(String(e));
    }
  };

  const reject = async (p: SkillProposal) => {
    await invoke("reject_skill_proposal", { id: p.id });
    if (reviewing?.id === p.id) setReviewing(null);
    load();
  };

  const saveNewSkill = async () => {
    if (!newSkill) return;
    setSavingNewSkill(true);
    setError(null);
    try {
      await invoke("create_skill", {
        id: undefined,
        name: newSkill.name,
        description: newSkill.description,
        content: newSkill.content,
        triggers: newSkill.triggers
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
        workspace: workspacePath ?? undefined,
        project: scope === "project",
      });
      setNewSkill(null);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setSavingNewSkill(false);
    }
  };

  const bySource = (source: Skill["source"]) => skills.filter((s) => s.source === source);
  const builtin = bySource("builtin");
  const learned = bySource("learned");
  const installed = bySource("installed");
  const project = bySource("project");
  const globalCount = builtin.length + learned.length + installed.length;

  const renderRow = (s: Skill, deletable: boolean, i: number) => (
    <div key={s.id} className="skill-row" style={{ ["--i" as never]: i }}>
      <div className="skill-info">
        <div className="skill-name-row">
          <span className={`skill-source-dot ${s.source}`} />
          <span className="skill-name">{s.name}</span>
          {s.overridden && <span className="skill-badge modified">modified</span>}
          {s.source === "project" && <span className="skill-badge project">project</span>}
        </div>
        <span className="skill-desc">{s.description}</span>
      </div>
      <div className="skill-actions">
        <button onClick={() => view(s)}>View</button>
        <label className="skill-toggle">
          <input type="checkbox" checked={s.enabled} onChange={() => toggle(s)} />
          <span className="track" />
          <span className="thumb" />
        </label>
        {(deletable || s.overridden) && (
          <button onClick={() => remove(s)}>{s.overridden && !deletable ? "Revert" : "Delete"}</button>
        )}
      </div>
    </div>
  );

  return (
    <div className="settings-view">
      <div className="skills-header">
        <h2>Skills</h2>
        <span className="life-orb" title="Quietly checking every message against what's available" />
      </div>
      <p className="hint">
        Skills are short, focused instructions the agent pulls in when relevant — matched by quick
        keyword triggers, and, for anything those miss, by asking a fast model whether one would
        actually help. The agent can also write and refine skills itself: directly when you ask it
        to, or — for something it noticed on its own — as a proposal below that only takes effect
        once you approve it.
      </p>

      <div className="skills-scope-tabs">
        <button className={`tab-item ${scope === "global" ? "active" : ""}`} onClick={() => setScope("global")}>
          Global <span className="count">{globalCount}</span>
        </button>
        <button className={`tab-item ${scope === "project" ? "active" : ""}`} onClick={() => setScope("project")}>
          This project <span className="count">{project.length}</span>
        </button>
      </div>

      {scope === "project" ? (
        <div className="skills-scope-pane">
          <div className="workspace-scope-row">
            <span>Viewing</span>
            <WorkspacePicker value={workspacePath} onChange={setWorkspacePath} className="workspace-picker" />
          </div>

          {!workspacePath ? (
            <div className="skill-empty">
              <span className="skill-empty-icon">◇</span>
              Pick a workspace above to see or add its project-scoped skills.
            </div>
          ) : (
            <>
              {agentsMdLoaded && (
                <div className={`agents-md-card ${agentsMd ? "" : "missing"}`}>
                  <div style={{ display: "flex", alignItems: "center", gap: 12, minWidth: 0 }}>
                    <span className="agents-md-icon">AG</span>
                    <div className="agents-md-info">
                      <span className="agents-md-title">AGENTS.md</span>
                      <span className="agents-md-status">
                        {agentsMd
                          ? "Found — read into context automatically on every turn in this project."
                          : 'None yet. Ask the agent to create one — the built-in "AGENTS.md authoring" skill covers what belongs in it.'}
                      </span>
                    </div>
                  </div>
                  {agentsMd && <button onClick={() => setViewingAgentsMd(true)}>View</button>}
                </div>
              )}

              <div className="skills-section-head">
                <h3 style={{ margin: 0 }}>Project skills</h3>
                <button className="new-skill-btn" onClick={() => setNewSkill(emptyNewSkill())}>
                  + New project skill
                </button>
              </div>
              <p className="hint small">
                Lives in <code>.kestrel/skills/</code> at this project's root — commit it to share
                with anyone else working on the repo. A project skill takes precedence over a global
                one of the same id.
              </p>
              <div className="skill-list">
                {project.map((s, i) => renderRow(s, true, i))}
                {project.length === 0 && (
                  <div className="skill-empty">
                    <span className="skill-empty-icon">✦</span>
                    No project-specific skills yet — e.g. this project's own deploy process or
                    conventions that wouldn't make sense anywhere else.
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      ) : (
        <div className="skills-scope-pane">
          {proposals.length > 0 && (
            <section className="skill-proposals-section">
              <h3>
                <span className="skill-proposal-sparkle">✦</span> Proposed by Kestrel ({proposals.length})
              </h3>
              <p className="hint small">
                Noticed on its own during a session — nothing here has any effect until you accept it.
              </p>
              <div className="skill-list" style={{ marginBottom: 6 }}>
                {proposals.map((p, i) => (
                  <div key={p.id} className="skill-row" style={{ ["--i" as never]: i }}>
                    <div className="skill-info">
                      <span className="skill-name">
                        {p.kind === "update" ? `Update: ${p.target_id}` : p.name}
                      </span>
                      <span className="skill-desc">{p.rationale}</span>
                    </div>
                    <div className="skill-actions">
                      <button onClick={() => openReview(p)}>Review</button>
                      <button onClick={() => reject(p)}>Dismiss</button>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}

          <section>
            <div className="skills-section-head">
              <h3 style={{ margin: 0 }}>Built in</h3>
            </div>
            <div className="skill-list">{builtin.map((s, i) => renderRow(s, false, i))}</div>
          </section>

          <section>
            <div className="skills-section-head">
              <h3 style={{ margin: 0 }}>Learned</h3>
            </div>
            <p className="hint small">Written by the agent itself, either directly or via an accepted proposal.</p>
            <div className="skill-list">
              {learned.map((s, i) => renderRow(s, true, i))}
              {learned.length === 0 && (
                <div className="skill-empty">
                  <span className="skill-empty-icon">✦</span>None yet.
                </div>
              )}
            </div>
          </section>

          <section>
            <div className="skills-section-head">
              <h3 style={{ margin: 0 }}>Installed</h3>
              <button className="new-skill-btn" onClick={() => setNewSkill(emptyNewSkill())}>
                + New skill
              </button>
            </div>
            <div className="skill-list">
              {installed.map((s, i) => renderRow(s, true, i))}
              {installed.length === 0 && (
                <div className="skill-empty">
                  <span className="skill-empty-icon">✦</span>None installed yet.
                </div>
              )}
            </div>
          </section>

          <section>
            <h3>Install from URL</h3>
            <p className="hint small">
              Point at a raw markdown file with a <code>name</code>/<code>description</code>{" "}
              frontmatter block, e.g. a raw GitHub URL.
            </p>
            <div className="row">
              <input
                value={installUrl}
                onChange={(e) => setInstallUrl(e.target.value)}
                placeholder="https://raw.githubusercontent.com/.../skill.md"
                style={{ flex: 1 }}
              />
              <button className="primary" onClick={install} disabled={installing}>
                {installing ? "Installing..." : "Install"}
              </button>
            </div>
          </section>
        </div>
      )}

      {error && <p className="fail">{error}</p>}

      {viewing && (
        <Modal title={viewing.name} onClose={() => setViewing(null)}>
          <pre className="skill-content">{content}</pre>
        </Modal>
      )}

      {viewingAgentsMd && agentsMd && (
        <Modal title="AGENTS.md" onClose={() => setViewingAgentsMd(false)}>
          <pre className="skill-content">{agentsMd}</pre>
        </Modal>
      )}

      {newSkill && (
        <Modal
          title={scope === "project" ? "New project skill" : "New skill"}
          onClose={() => setNewSkill(null)}
        >
          <input
            value={newSkill.name}
            onChange={(e) => setNewSkill({ ...newSkill, name: e.target.value })}
            placeholder="Name"
            style={{ width: "100%", marginBottom: 6 }}
          />
          <input
            value={newSkill.description}
            onChange={(e) => setNewSkill({ ...newSkill, description: e.target.value })}
            placeholder="One-line description — what it covers and when it's relevant"
            style={{ width: "100%", marginBottom: 6 }}
          />
          <input
            value={newSkill.triggers}
            onChange={(e) => setNewSkill({ ...newSkill, triggers: e.target.value })}
            placeholder="Optional trigger words, comma-separated"
            style={{ width: "100%", marginBottom: 6 }}
          />
          <textarea
            value={newSkill.content}
            onChange={(e) => setNewSkill({ ...newSkill, content: e.target.value })}
            placeholder="Full skill content, in markdown"
            rows={12}
            style={{ width: "100%", fontFamily: "monospace" }}
          />
          <div className="row" style={{ marginTop: 8 }}>
            <button
              className="primary"
              onClick={saveNewSkill}
              disabled={savingNewSkill || !newSkill.name.trim() || !newSkill.content.trim()}
            >
              {savingNewSkill ? "Saving..." : "Save"}
            </button>
            <button onClick={() => setNewSkill(null)}>Cancel</button>
          </div>
        </Modal>
      )}

      {reviewing && (
        <Modal title={`Review proposal: ${draft.name || reviewing.target_id}`} onClose={() => setReviewing(null)}>
          <p className="hint small">
            <strong>Why Kestrel proposed this:</strong> {reviewing.rationale}
          </p>
          {reviewing.previous_content && (
            <>
              <p className="hint small">
                <strong>Current content:</strong>
              </p>
              <pre className="skill-content">{reviewing.previous_content}</pre>
            </>
          )}
          <p className="hint small">
            <strong>Proposed content</strong> (editable before accepting):
          </p>
          <input
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            placeholder="Name"
            style={{ width: "100%", marginBottom: 4 }}
          />
          <input
            value={draft.description}
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            placeholder="Description"
            style={{ width: "100%", marginBottom: 4 }}
          />
          <textarea
            value={draft.content}
            onChange={(e) => setDraft({ ...draft, content: e.target.value })}
            rows={14}
            style={{ width: "100%", fontFamily: "monospace" }}
          />
          <div className="row" style={{ marginTop: 8 }}>
            <button className="primary" onClick={accept}>
              Accept
            </button>
            <button onClick={() => reject(reviewing)}>Dismiss</button>
          </div>
        </Modal>
      )}
    </div>
  );
}
