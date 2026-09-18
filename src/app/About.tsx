import { useState } from "react";
import UpdaterModal from "./UpdaterModal";
import CustomInstallerModal from "./CustomInstallerModal";
import { open } from "@tauri-apps/plugin-shell";
import { useUpdaterStore } from "./updaterStore";

export default function About() {
  const [updaterOpen, setUpdaterOpen] = useState(false);
  const [installerOpen, setInstallerOpen] = useState(false);
  const { result: updateResult } = useUpdaterStore();
  const openLink = (url: string) => open(url);

  const currentVersion = updateResult?.current_version || "1.1.1";

  return (
    <div className="settings-view">
      <h2>About</h2>
      <p className="hint">
        This app is an open-source coding and general-purpose AI agent,
        built directly on top of the projects below — neither the routing
        engine nor the original agent design are ours.
      </p>

      <section>
        <h3>Updates & Installation</h3>
        <p className="modal-text">
          Current Version: <strong>v{currentVersion}</strong>
          {updateResult?.has_update && (
            <span
              style={{
                marginLeft: 10,
                padding: "2px 8px",
                borderRadius: 12,
                fontSize: 12,
                backgroundColor: "rgba(80, 250, 123, 0.2)",
                color: "#50fa7b",
                border: "1px solid rgba(80, 250, 123, 0.4)",
              }}
            >
              Update Available (v{updateResult.latest_version})
            </span>
          )}
        </p>
        <div className="row" style={{ marginTop: 10 }}>
          <button className="primary" onClick={() => setUpdaterOpen(true)}>
            Check for Updates
          </button>
          <button onClick={() => setInstallerOpen(true)}>
            Run Custom Installer
          </button>
        </div>
      </section>

      <section>
        <h3>OmniRoute</h3>
        <p className="modal-text">
          Every model call this app makes goes through OmniRoute, a free,
          local-first AI gateway — one endpoint, hundreds of providers, and
          automatic fallback across subscription, API-key, cheap, and free
          tiers. When set to local mode, this app launches and supervises
          OmniRoute itself so there's nothing separate to run — all the
          routing intelligence underneath is entirely OmniRoute's.
        </p>
        <button
          className="primary"
          onClick={() => openLink("https://github.com/diegosouzapw/OmniRoute")}
        >
          View OmniRoute on GitHub
        </button>
      </section>

      <section>
        <h3>Hermes Agent</h3>
        <p className="modal-text">
          The tool-calling agent loop, skills system, and planning-mode
          approval flow in this app were inspired by Hermes Agent, the
          open-source self-improving agent from Nous Research.
        </p>
        <button
          onClick={() => openLink("https://github.com/NousResearch/hermes-agent")}
        >
          View Hermes Agent on GitHub
        </button>
      </section>

      <section>
        <h3>License</h3>
        <p className="hint small">
          This project is MIT licensed — see LICENSE in the repository.
        </p>
      </section>

      <UpdaterModal isOpen={updaterOpen} onClose={() => setUpdaterOpen(false)} />
      <CustomInstallerModal isOpen={installerOpen} onClose={() => setInstallerOpen(false)} />
    </div>
  );
}
