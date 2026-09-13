<img src="assets/logo-wordmark.svg" alt="Kestrel" width="280" />

# Kestrel

An open-source, local-first coding & general-purpose AI agent desktop app.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/Built%20with-Tauri-4f46e5?logo=tauri)](https://tauri.app)
[![Built on OmniRoute](https://img.shields.io/badge/Built%20on-OmniRoute-16a34a?logo=openai)](https://github.com/diegosouzapw/OmniRoute)
[![Inspired by Hermes Agent](https://img.shields.io/badge/Inspired%20by-Hermes%20Agent-9333ea?logo=research)](https://github.com/NousResearch/hermes-agent)

---

## ✦ Why Kestrel?

A kestrel hovers in place scanning the ground before it commits to a fast, precise dive — which is roughly the shape of this app: **watch the tool-call stream while the agent works, approve or reject in planning mode, let it move fast once you've seen enough.**

Kestrel is built for developers who want an AI agent that works *with* them, not around them. It runs entirely locally, routes through [OmniRoute](https://github.com/diegosouzapw/OmniRoute), and uses a [Hermes Agent](https://github.com/NousResearch/hermes-agent)-style tool-calling loop underneath.

---

## ✦ Features

### 🤖 Agent Loop
- **Sessions persist to disk** with full message history
- **Tool-calling loop** — model calls tools until it produces a final answer
- **Live streaming** — each tool call appears the moment it starts and updates in place as it resolves
- **General AI mode** — per-session toggle that sends no tools, works as a plain assistant

### 🛡️ Planning Mode
- Write operations, shell commands, and GitHub actions **pause and stream to the UI as "awaiting approval"**
- Nothing happens until you click **Approve**
- Makes it safe to use immediately — nothing lands on disk without a click

### 🌐 OmniRoute Integration
- Routes every model call through OmniRoute's OpenAI-compatible endpoint
- **Embedded providers dashboard** — manage API keys, check usage, tune routing without leaving the app
- Supports **local** (app manages OmniRoute) or **remote** (URL + API key) modes
- In local mode, OmniRoute installs itself on first run and launches automatically

### ⚡ Sub-Agents
- Coding-mode sessions get a `delegate_to_subagent` tool for bulky or exploratory work
- Sub-agents run with **isolated message history** — the parent's context never sees intermediate file reads or command output
- Tool calls stream as nested, animated cards (pulsing while running, auto-collapse on completion)
- One level of nesting only; toggle on/off at session creation or mid-session

### 🛠️ Built-in Tools
| Category | Tools |
|----------|-------|
| **GitHub** | `github_list_issues`, `github_get_issue`, `github_list_issue_comments`, `github_comment_issue`, `github_close_issue`, `github_list_open_prs`, `github_get_pr`, `github_merge_pr` |
| **Shell** | `run_shell` — execute commands in the workspace |
| **Files** | `read_file`, `write_file`, `edit_file`, `apply_patch` — full file manipulation |
| **Code Search** | `search_code`, `find_files` — search across the workspace |
| **Delegation** | `delegate_to_subagent` — spawn isolated sub-agents for complex tasks |

### 📚 Skills System
- Small built-in library: git workflow, debugging, testing, code review, refactoring, plus Python/TypeScript/Rust/Go references
- Agent discovers skills via `list_skills`/`read_skill` tool calls
- Skills are viewable and toggleable from the Skills tab
- Install more from a raw markdown URL (frontmatter `name`/`description`, body is the skill content)

### 💻 GitHub Integration
- **Persistent GitHub tool cards** — issue/PR calls render as clickable cards, not text blobs
- Clicking opens a modal with full body, comments, and action buttons (Close issue, Merge PR)
- Sessions can be linked to `owner/repo` so the agent doesn't need to specify which repo every time

### 🏗️ Modular First-Run Setup
- Ordered setup wizard on first launch
- Completed steps persist locally — future updates only show new steps
- Steps: Welcome → OmniRoute config → Workspace selection → Defaults → Finish

---

## ✦ Screenshots

> _Coming soon — screenshots of the main session view, planning mode approval flow, and embedded providers dashboard._

---

## ✦ Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) v22+ with npm
- Windows, macOS, or Linux

### Build & Run

```bash
# Clone the repository
git clone https://github.com/your-org/kestrel.git
cd kestrel

# Install frontend dependencies
npm install

# Run the development server
npm run tauri dev
```

> **Note:** The Rust side hasn't been compiled in the development environment — treat `cargo check` on your machine as the first real test. See [SETUP_WINDOWS.md](SETUP_WINDOWS.md) for platform-specific setup notes.

### First Launch

1. Complete the setup wizard:
   - Configure OmniRoute (local or remote mode)
   - Select your workspace folder
   - Choose default settings
2. Start a **coding mode** session with **planning mode on**
3. Ask the agent to help with a task — approve or reject each action as it comes in

---

## ✦ Project Structure

```
kestrel/
├── src/                          # React/TypeScript frontend
│   ├── App.tsx                   # Entry point — checks for pending setup
│   ├── app/                      # Main app components
│   │   ├── AppShell.tsx          # Main shell with sidebar and content areas
│   │   ├── Sidebar.tsx           # Session list, engine status, navigation
│   │   ├── SessionView.tsx       # Chat interface and tool-call stream
│   │   ├── Settings.tsx          # OmniRoute config, workspace, preferences
│   │   ├── SkillsPanel.tsx       # Skills library management
│   │   ├── GithubPanel.tsx       # GitHub repo linking and actions
│   │   ├── About.tsx             # Credits and links
│   │   └── ProvidersPanel.tsx    # Embedded OmniRoute dashboard
│   ├── setup/                    # First-run wizard
│   │   ├── Wizard.tsx            # Setup orchestrator
│   │   ├── stepRegistry.tsx     # Step registry and component mapping
│   │   └── steps/               # Individual setup step components
│   │       ├── Welcome.tsx
│   │       ├── OmniRouteConfig.tsx
│   │       ├── Workspace.tsx
│   │       ├── Defaults.tsx
│   │       └── Finish.tsx
│   └── styles.css                # Global styles
│
├── src-tauri/                    # Rust backend (Tauri)
│   ├── src/
│   │   ├── main.rs               # App bootstrap and command definitions
│   │   ├── config.rs             # Persisted config (OmniRoute, workspace, etc.)
│   │   ├── setup.rs              # Modular setup step registry
│   │   ├── engine.rs             # OmniRoute process management
│   │   ├── prompts.rs            # System prompts (coding/general modes)
│   │   └── tools.rs              # Tool definitions and execution
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── index.html
├── package.json
├── vite.config.ts
├── tsconfig.json
├── TODO.md                       # Feature roadmap and known gaps
├── OMNIROUTE_INTEGRATION.md      # How OmniRoute is embedded and why
└── SETUP_WINDOWS.md              # Windows-specific setup notes
```

---

## ✦ How the Setup System Works

`src-tauri/src/setup.rs` holds an ordered list of setup steps (`step_registry()`). Each user's completed steps are saved locally in `setup_state.json`. On launch, the app diffs the full registry against what that user has already completed and only shows the difference.

### Adding a New Setup Step

1. Add an entry to `step_registry()` in `src-tauri/src/setup.rs` with a unique `id` and next `order` value
2. Add a matching component to `stepComponents` in `src/setup/stepRegistry.tsx`, keyed by the same `id`
3. Existing installs automatically see only the new step on next launch

If the frontend doesn't recognize a step id (mismatch during upgrade), `Wizard.tsx` skips it instead of crashing.

---

## ✦ Roadmap

See [TODO.md](TODO.md) for the full feature list, known gaps, and future considerations.

### In Progress
- [ ] `cargo check` — Rust backend hasn't been compiled yet
- [ ] Click through OmniRoute install flow for real
- [ ] Run a real sub-agent delegation and verify nested card behavior
- [ ] Point a coding session at this repo and give it a real task

### Planned
- [ ] Automated tests (Rust backend, React frontend)
- [ ] CI/CD with GitHub Actions
- [ ] Parallel swarms / multi-agent delegation
- [ ] Inline PR diff review (line comments, approve/request-changes)
- [ ] Editable system prompts from UI
- [ ] Token-level streaming (live streaming of model responses)

---

## ✦ Contributing

Contributions are welcome! This project is MIT licensed.

### Guidelines
- Issues and PRs welcome — the intent is that this project ends up partly built by the agent it produces
- Use the app to build the app: with planning mode on, you can safely guide the agent through implementation tasks
- Follow existing code style and conventions

### Security Notes
- `run_shell` has no sandboxing — planning mode gates it behind approval, but there's no allowlist/denylist
- Secrets are stored in plaintext — OS keychain integration is planned before broad release

---

## ✦ Credits

Kestrel is built directly on the shoulders of giants:

| Project | Description |
|---------|-------------|
| [OmniRoute](https://github.com/diegosouzapw/OmniRoute) | Local AI gateway for routing, provider management, and OpenAI-compatible API — MIT licensed |
| [Hermes Agent](https://github.com/NousResearch/hermes-agent) | Open-source self-improving agent that inspired this project's tool loop, skills system, and planning-mode approval flow — MIT licensed |
| [Tauri](https://tauri.app) | Rust-based desktop app framework with web frontend |
| [React](https://react.dev) | UI library for the frontend |
| [TypeScript](https://www.typescriptlang.org) | Type-safe JavaScript |

---

## ✦ License

MIT License — see [LICENSE](LICENSE)

---

## ✦ Related

- [OmniRoute GitHub](https://github.com/diegosouzapw/OmniRoute)
- [Hermes Agent GitHub](https://github.com/NousResearch/hermes-agent)
- [Tauri Documentation](https://tauri.app/v1/guides/)