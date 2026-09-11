import { open } from "@tauri-apps/api/shell";

export default function About() {
  const openLink = (url: string) => open(url);

  return (
    <div className="settings-view">
      <h2>About</h2>
      <p className="hint">
        This app is an open-source coding and general-purpose AI agent,
        built directly on top of the projects below — neither the routing
        engine nor the original agent design are ours.
      </p>

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
    </div>
  );
}
