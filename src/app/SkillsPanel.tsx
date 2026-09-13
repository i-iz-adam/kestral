import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type { Skill, SkillProposal } from "../types";
import Modal from "./Modal";

export default function SkillsPanel() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [proposals, setProposals] = useState<SkillProposal[]>([]);
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

  const load = () => {
    invoke<Skill[]>("list_skills").then(setSkills);
    invoke<SkillProposal[]>("list_skill_proposals").then(setProposals);
  };

  useEffect(load, []);

  // The reflection pass (see reflect.rs) runs in the background, well
  // after any tool-call UI has settled — a live listener is how a newly
  // proposed skill actually shows up without the user having to reopen
  // this panel to notice it.
  useEffect(() => {
    const unlisten = listen("agent://skill-proposed", () => load());
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const toggle = async (skill: Skill) => {
    await invoke("toggle_skill", { id: skill.id, enabled: !skill.enabled });
    load();
  };

  const view = async (skill: Skill) => {
    const c = await invoke<string>("get_skill_content", { id: skill.id });
    setContent(c);
    setViewing(skill);
  };

  const remove = async (skill: Skill) => {
    try {
      await invoke("delete_skill", { id: skill.id });
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

  const builtin = skills.filter((s) => s.source === "builtin");
  const installed = skills.filter((s) => s.source === "installed");
  const learned = skills.filter((s) => s.source === "learned");

  const renderRow = (s: Skill, deletable: boolean) => (
    <div key={s.id} className="skill-row">
      <div className="skill-info">
        <span className="skill-name">
          {s.name}
          {s.overridden && <span className="badge">modified</span>}
        </span>
        <span className="skill-desc">{s.description}</span>
      </div>
      <div className="skill-actions">
        <button onClick={() => view(s)}>View</button>
        <label className="toggle">
          <input type="checkbox" checked={s.enabled} onChange={() => toggle(s)} />
        </label>
        {(deletable || s.overridden) && (
          <button onClick={() => remove(s)}>{s.overridden && !deletable ? "Revert" : "Delete"}</button>
        )}
      </div>
    </div>
  );

  return (
    <div className="settings-view">
      <h2>Skills</h2>
      <p className="hint">
        Skills are short, focused instructions the agent can pull in when
        relevant — it checks the list itself via a tool call rather than
        having everything loaded up front, so adding more doesn't bloat
        every conversation. The agent can also write and refine skills
        itself: directly when you ask it to, or — for something it noticed
        on its own — as a proposal below that only takes effect once you
        approve it.
      </p>

      {proposals.length > 0 && (
        <section>
          <h3>Proposed by Kestrel ({proposals.length})</h3>
          <p className="hint small">
            Noticed on its own during a session — nothing here has any
            effect until you accept it.
          </p>
          <div className="skill-list">
            {proposals.map((p) => (
              <div key={p.id} className="skill-row">
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
        {error && <p className="fail">{error}</p>}
      </section>

      <section>
        <h3>Built in</h3>
        <div className="skill-list">{builtin.map((s) => renderRow(s, false))}</div>
      </section>

      <section>
        <h3>Learned</h3>
        <p className="hint small">Written by the agent itself, either directly or via an accepted proposal.</p>
        <div className="skill-list">
          {learned.map((s) => renderRow(s, true))}
          {learned.length === 0 && <p className="hint small">None yet.</p>}
        </div>
      </section>

      <section>
        <h3>Installed</h3>
        <div className="skill-list">
          {installed.map((s) => renderRow(s, true))}
          {installed.length === 0 && <p className="hint small">None installed yet.</p>}
        </div>
      </section>

      {viewing && (
        <Modal title={viewing.name} onClose={() => setViewing(null)}>
          <pre className="skill-content">{content}</pre>
        </Modal>
      )}

      {reviewing && (
        <Modal title={`Review proposal: ${draft.name || reviewing.target_id}`} onClose={() => setReviewing(null)}>
          <p className="hint small">
            <strong>Why Kestrel proposed this:</strong> {reviewing.rationale}
          </p>
          {reviewing.previous_content && (
            <>
              <p className="hint small"><strong>Current content:</strong></p>
              <pre className="skill-content">{reviewing.previous_content}</pre>
            </>
          )}
          <p className="hint small"><strong>Proposed content</strong> (editable before accepting):</p>
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
            <button className="primary" onClick={accept}>Accept</button>
            <button onClick={() => reject(reviewing)}>Dismiss</button>
          </div>
        </Modal>
      )}
    </div>
  );
}
