import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";

interface Issue {
  number: number;
  title: string;
  html_url: string;
}

export default function GithubPanel() {
  const [token, setToken] = useState("");
  const [connected, setConnected] = useState<string | null>(null);
  const [owner, setOwner] = useState("");
  const [repo, setRepo] = useState("");
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<string | null>("get_github_token").then((t) => {
      if (t) {
        setToken(t);
        setBusy(true);
        invoke<string>("test_github_token", { token: t })
          .then((login) => setConnected(login))
          .catch(() => setConnected(null))
          .finally(() => setBusy(false));
      }
    });
  }, []);

  const connect = async () => {
    if (!token.trim()) return;
    setError(null);
    setBusy(true);
    try {
      const login = await invoke<string>("test_github_token", { token });
      await invoke("save_github_token", { token });
      setConnected(login);
    } catch (e) {
      setError(String(e));
      setConnected(null);
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    setError(null);
    setBusy(true);
    try {
      await invoke("save_github_token", { token: "" });
      setToken("");
      setConnected(null);
      setIssues([]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const loadIssues = async () => {
    setError(null);
    setBusy(true);
    try {
      const result = await invoke<Issue[]>("list_github_issues", {
        token,
        owner,
        repo,
      });
      setIssues(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="github-settings-panel">
      <section>
        <h3>Connection</h3>
        {connected ? (
          <div className="github-connected-card">
            <div className="github-status-badge">
              <span className="pulsing-dot" />
              <span>
                Connected as <strong>{connected}</strong>
              </span>
            </div>
            <button className="button-disconnect" onClick={disconnect} disabled={busy}>
              {busy ? "Disconnecting..." : "Disconnect"}
            </button>
          </div>
        ) : (
          <div>
            <div className="field-group">
              <label>Personal access token</label>
              <input
                value={token}
                onChange={(e) => setToken(e.target.value)}
                type="password"
                placeholder="ghp_..."
              />
            </div>
            <button className="primary" onClick={connect} disabled={busy || !token.trim()}>
              {busy ? "Connecting..." : "Connect"}
            </button>
          </div>
        )}
        {error && <p className="fail" style={{ marginTop: 8 }}>{error}</p>}
      </section>

      <section>
        <h3>Browse issues</h3>
        <div className="row">
          <input
            value={owner}
            onChange={(e) => setOwner(e.target.value)}
            placeholder="owner"
            style={{ width: 120 }}
          />
          <input
            value={repo}
            onChange={(e) => setRepo(e.target.value)}
            placeholder="repo"
            style={{ width: 120 }}
          />
          <button onClick={loadIssues} disabled={busy || !token}>
            Load issues
          </button>
        </div>
        <div className="issue-list">
          {issues.map((i) => (
            <div key={i.number} className="issue-row">
              <span>#{i.number}</span>
              <span>{i.title}</span>
            </div>
          ))}
          {issues.length === 0 && (
            <p className="hint small">No issues loaded yet.</p>
          )}
        </div>
      </section>

      <p className="hint small">
        This is read-only for now — the agent opening PRs and responding to
        review comments is planned next, once the coding-mode tool loop has
        had more real use.
      </p>
    </div>
  );
}
