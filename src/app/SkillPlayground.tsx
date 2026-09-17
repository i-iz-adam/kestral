import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Skill } from "../types";
import WorkspacePicker from "./WorkspacePicker";
import { getCurrentWorkspaceFilter, subscribeWorkspaceFilter } from "./agentStore";

type Scope = "global" | "project";
type Tab = "library" | "playground";

interface MatchPreview {
  skill: Skill;
  content: string;
}

interface Draft {
  id?: string;
  name: string;
  description: string;
  triggers: string;
  content: string;
}

const emptyDraft = (): Draft => ({ name: "", description: "", triggers: "", content: "" });

/**
 * The skill authoring workbench. The library deliberately remains separate
 * from the playground: authors can try prompts without accidentally writing
 * to a skill, and the same deterministic trigger command used by turns is
 * used for the preview results.
 */
export default function SkillPlayground() {
  const [tab, setTab] = useState<Tab>("library");
  const [scope, setScope] = useState<Scope>("global");
  const [workspacePath, setWorkspacePath] = useState<string | null>(() => getCurrentWorkspaceFilter());
  const [skills, setSkills] = useState<Skill[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [prompt, setPrompt] = useState("");
  const [matches, setMatches] = useState<MatchPreview[]>([]);
  const [running, setRunning] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setSkills(await invoke<Skill[]>("list_skills", { workspace: workspacePath ?? undefined }));
    } catch (e) {
      setError(String(e));
    }
  }, [workspacePath]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => subscribeWorkspaceFilter(() => setWorkspacePath(getCurrentWorkspaceFilter())), []);

  const visibleSkills = useMemo(
    () => skills.filter((skill) => scope === "project" ? skill.source === "project" : skill.source !== "project"),
    [scope, skills]
  );

  const selectSkill = async (skill: Skill) => {
    setSelectedId(skill.id);
    setError(null);
    try {
      const content = await invoke<string | null>("get_skill_content", {
        id: skill.id,
        workspace: scope === "project" ? (workspacePath ?? undefined) : undefined,
      });
      setDraft({ id: skill.id, name: skill.name, description: skill.description, triggers: skill.triggers.join(", "), content: content ?? "" });
    } catch (e) { setError(String(e)); }
  };

  const newSkill = () => {
    setSelectedId(null);
    setDraft(emptyDraft());
    setTab("library");
    setError(null);
  };

  const save = async () => {
    if (!draft || !draft.name.trim() || !draft.content.trim()) return;
    setSaving(true); setError(null); setNotice(null);
    try {
      await invoke("create_skill", {
        id: draft.id,
        name: draft.name.trim(),
        description: draft.description.trim(),
        content: draft.content,
        triggers: draft.triggers.split(",").map((trigger) => trigger.trim()).filter(Boolean),
        workspace: workspacePath ?? undefined,
        project: scope === "project",
      });
      await load();
      setNotice("Skill saved");
    } catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };

  const remove = async (skill: Skill) => {
    if (!window.confirm(`Delete “${skill.name}”?`)) return;
    setError(null);
    try {
      await invoke("delete_skill", { id: skill.id, workspace: workspacePath ?? undefined });
      if (selectedId === skill.id) { setSelectedId(null); setDraft(null); }
      await load();
    } catch (e) { setError(String(e)); }
  };

  const toggle = async (skill: Skill) => {
    setError(null);
    try {
      await invoke("toggle_skill", { id: skill.id, enabled: !skill.enabled });
      await load();
    } catch (e) { setError(String(e)); }
  };

  const runPreview = async () => {
    if (!prompt.trim()) return;
    setRunning(true); setError(null);
    try {
      setMatches(await invoke<MatchPreview[]>("preview_skill_matches", {
        prompt,
        workspace: workspacePath ?? undefined,
      }));
    } catch (e) { setError(String(e)); }
    finally { setRunning(false); }
  };

  // This lets an author validate a new/unsaved trigger immediately. Existing
  // skills additionally go through Rust so built-in fallback keywords match
  // exactly as they do during a real turn.
  const draftMatches = useMemo(() => {
    if (!draft?.triggers.trim() || !prompt.trim()) return [];
    const input = prompt.toLowerCase();
    const tokens = new Set(input.split(/[^a-z0-9]+/).filter(Boolean));
    return draft.triggers.split(",").map((trigger) => trigger.trim()).filter(Boolean)
      .filter((trigger) => {
        const normalized = trigger.toLowerCase();
        return normalized.includes(" ") || normalized.includes(".")
          ? input.includes(normalized)
          : tokens.has(normalized);
      });
  }, [draft, prompt]);

  return (
    <div className="settings-view skill-playground">
      <div className="skills-header">
        <div>
          <h2>Skill Playground</h2>
          <p className="hint">Browse, author, and safely test the instructions Kestrel can load.</p>
        </div>
        <span className="life-orb" title="Trigger matching is available offline" />
      </div>

      <div className="skills-scope-tabs">
        <button className={`tab-item ${tab === "library" ? "active" : ""}`} onClick={() => setTab("library")}>Library</button>
        <button className={`tab-item ${tab === "playground" ? "active" : ""}`} onClick={() => setTab("playground")}>Playground</button>
      </div>

      {tab === "library" ? (
        <>
          <div className="skills-scope-tabs">
            <button className={`tab-item ${scope === "global" ? "active" : ""}`} onClick={() => setScope("global")}>Global <span className="count">{skills.filter((s) => s.source !== "project").length}</span></button>
            <button className={`tab-item ${scope === "project" ? "active" : ""}`} onClick={() => setScope("project")}>Project <span className="count">{skills.filter((s) => s.source === "project").length}</span></button>
          </div>
          {scope === "project" && (
            <div className="workspace-scope-row">
              <span>Viewing</span>
              <WorkspacePicker value={workspacePath} onChange={setWorkspacePath} className="workspace-picker" />
            </div>
          )}
          <div className="skill-playground-toolbar">
            <span className="hint small">{scope === "project" ? "Project skills live in .kestrel/skills/." : "Built-in, installed, and learned skills."}</span>
            <button className="new-skill-btn" onClick={newSkill}>+ New {scope === "project" ? "project " : ""}skill</button>
          </div>
          {scope === "project" && !workspacePath ? (
            <div className="skill-empty">Pick a workspace above to browse project skills.</div>
          ) : (
            <div className="skill-playground-library">
              <div className="skill-list">
                {visibleSkills.map((skill, i) => (
                  <button key={skill.id} className={`skill-row skill-row-button ${selectedId === skill.id ? "selected" : ""}`} style={{ ["--i" as never]: i }} onClick={() => void selectSkill(skill)}>
                    <div className="skill-info">
                      <div className="skill-name-row"><span className={`skill-source-dot ${skill.source}`} /><span className="skill-name">{skill.name}</span>{skill.overridden && <span className="skill-badge modified">modified</span>}</div>
                      <span className="skill-desc">{skill.description}</span>
                      {skill.triggers.length > 0 && <span className="skill-trigger-list">{skill.triggers.join(" · ")}</span>}
                    </div>
                    <span className="skill-actions"><span onClick={(event) => { event.stopPropagation(); void toggle(skill); }}>{skill.enabled ? "Enabled" : "Disabled"}</span>{(skill.source !== "builtin" || skill.overridden) && <span onClick={(event) => { event.stopPropagation(); void remove(skill); }}>{skill.overridden && skill.source === "builtin" ? "Revert" : "Delete"}</span>}</span>
                  </button>
                ))}
                {visibleSkills.length === 0 && <div className="skill-empty">No skills in this scope yet.</div>}
              </div>
              {draft && (
                <SkillEditor draft={draft} setDraft={setDraft} save={save} saving={saving} onCancel={() => { setDraft(null); setSelectedId(null); }} />
              )}
            </div>
          )}
        </>
      ) : (
        <div className="skill-playground-pane">
          <div className="skills-scope-tabs">
            <button className={`tab-item ${scope === "global" ? "active" : ""}`} onClick={() => setScope("global")}>Global skills</button>
            <button className={`tab-item ${scope === "project" ? "active" : ""}`} onClick={() => setScope("project")}>Project skills</button>
          </div>
          {scope === "project" && <div className="workspace-scope-row"><span>Workspace</span><WorkspacePicker value={workspacePath} onChange={setWorkspacePath} className="workspace-picker" /></div>}
          <div className="field-group">
            <label htmlFor="skill-test-prompt">Test prompt</label>
            <textarea id="skill-test-prompt" value={prompt} onChange={(event) => setPrompt(event.target.value)} rows={5} placeholder="Try: Help me debug this Rust panic..." />
          </div>
          <div className="row"><button className="primary" onClick={() => void runPreview()} disabled={running || !prompt.trim()}>{running ? "Matching..." : "Run trigger test"}</button><button onClick={() => { setPrompt(""); setMatches([]); }}>Clear</button></div>
          {draftMatches.length > 0 && <p className="ok">Unsaved skill trigger match: {draftMatches.join(", ")}</p>}
          <section className="skill-preview-results">
            <h3>Auto-matched skills {matches.length > 0 && `(${matches.length})`}</h3>
            {matches.length === 0 ? <p className="hint">No saved enabled skills matched yet. Trigger matching is deterministic and does not call a model.</p> : matches.map(({ skill, content }) => <article className="skill-preview-card" key={skill.id}><div className="skill-name-row"><strong>{skill.name}</strong><span className={`skill-badge ${skill.source}`}>{skill.source}</span></div><p className="hint small">Matched by {skill.triggers.length ? skill.triggers.join(", ") : "built-in keyword"}</p><details><summary>Dry-run loaded content</summary><pre className="skill-content">{content}</pre></details></article>)}
          </section>
        </div>
      )}
      {notice && <p className="ok">{notice}</p>}
      {error && <p className="fail">{error}</p>}
    </div>
  );
}

function SkillEditor({ draft, setDraft, save, saving, onCancel }: { draft: Draft; setDraft: (draft: Draft) => void; save: () => void; saving: boolean; onCancel: () => void }) {
  return <section className="skill-editor"><div className="skills-section-head"><h3>{draft.id ? "Edit skill" : "New skill"}</h3><button onClick={onCancel}>Close</button></div><input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} placeholder="Name" /><input value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} placeholder="One-line description" /><input value={draft.triggers} onChange={(event) => setDraft({ ...draft, triggers: event.target.value })} placeholder="Trigger words or phrases, comma-separated" /><textarea value={draft.content} onChange={(event) => setDraft({ ...draft, content: event.target.value })} rows={18} placeholder="Full skill instructions in Markdown" /><button className="primary" onClick={save} disabled={saving || !draft.name.trim() || !draft.content.trim()}>{saving ? "Saving..." : "Save skill"}</button></section>;
}
