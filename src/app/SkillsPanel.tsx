import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { Skill } from "../types";
import Modal from "./Modal";

export default function SkillsPanel() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [installUrl, setInstallUrl] = useState("");
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewing, setViewing] = useState<Skill | null>(null);
  const [content, setContent] = useState<string>("");

  const load = () => {
    invoke<Skill[]>("list_skills").then(setSkills);
  };

  useEffect(load, []);

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

  const builtin = skills.filter((s) => s.source === "builtin");
  const installed = skills.filter((s) => s.source === "installed");

  return (
    <div className="settings-view">
      <h2>Skills</h2>
      <p className="hint">
        Skills are short, focused instructions the agent can pull in when
        relevant — it checks the list itself via a tool call rather than
        having everything loaded up front, so adding more doesn't bloat
        every conversation.
      </p>

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
        <div className="skill-list">
          {builtin.map((s) => (
            <div key={s.id} className="skill-row">
              <div className="skill-info">
                <span className="skill-name">{s.name}</span>
                <span className="skill-desc">{s.description}</span>
              </div>
              <div className="skill-actions">
                <button onClick={() => view(s)}>View</button>
                <label className="toggle">
                  <input
                    type="checkbox"
                    checked={s.enabled}
                    onChange={() => toggle(s)}
                  />
                </label>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section>
        <h3>Installed</h3>
        <div className="skill-list">
          {installed.map((s) => (
            <div key={s.id} className="skill-row">
              <div className="skill-info">
                <span className="skill-name">{s.name}</span>
                <span className="skill-desc">{s.description}</span>
              </div>
              <div className="skill-actions">
                <button onClick={() => view(s)}>View</button>
                <label className="toggle">
                  <input
                    type="checkbox"
                    checked={s.enabled}
                    onChange={() => toggle(s)}
                  />
                </label>
                <button onClick={() => remove(s)}>Delete</button>
              </div>
            </div>
          ))}
          {installed.length === 0 && (
            <p className="hint small">None installed yet.</p>
          )}
        </div>
      </section>

      {viewing && (
        <Modal title={viewing.name} onClose={() => setViewing(null)}>
          <pre className="skill-content">{content}</pre>
        </Modal>
      )}
    </div>
  );
}
