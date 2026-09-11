<img src="assets/logo-wordmark.svg" alt="Kestrel" width="280" />

An open-source, local-first coding & general-purpose AI agent desktop app.
Built on top of [OmniRoute](https://github.com/diegosouzapw/OmniRoute) for LLM
routing, with a [Hermes Agent](https://github.com/NousResearch/hermes-agent)-style
tool-calling loop underneath.

**Why "Kestrel"**: a kestrel hovers in place scanning the ground before it
commits to a fast, precise dive — which is roughly the shape of this app:
watch the tool-call stream while the agent works, approve or reject in
planning mode, let it move fast once you've seen enough. The logomark is
three blades fanning from one point — read it as a wing, a fast-forward
motif, or one task branching into parallel sub-agents. All three were
intentional.

This is an early scaffold, not a finished app. It's meant to be handed to the
agent itself once it can run, and grown from here. See `TODO.md` for what's
left, in rough priority order, plus a longer list of things worth
considering but not committed to yet.

## Status

Working right now:
- Tauri shell (Rust backend + React/TypeScript frontend)
- Modular first-run setup wizard
- OmniRoute connection config: local instance, or remote (URL + API key),
  editable later from Settings
- Workspace folder selection
- **The agent loop**: sessions persist to disk, messages go through
  OmniRoute's OpenAI-compatible endpoint, and the model can call tools in a
  loop until it produces a final answer
- **Planning mode**: any write, shell command, or mutating GitHub action
  pauses and streams to the UI as "awaiting approval" — nothing happens
  until you click Approve
- **Live tool-call stream** in the session view — each call appears the
  moment it starts and updates in place as it resolves
- **General AI mode**: a per-session toggle that sends no tools at all, so
  the same session/OmniRoute plumbing works as a plain assistant too
- **Skills**: a small built-in library (git workflow, debugging, testing,
  code review, refactoring, plus Python/TypeScript/Rust/Go references) the
  agent discovers via `list_skills`/`read_skill` tool calls rather than
  having everything loaded up front. Skills are viewable and toggleable
  from the Skills tab, and you can install more from a raw markdown URL
  (frontmatter `name`/`description`, body is the skill content).
- **GitHub, as real tool calls, not just a browser**: `github_list_issues`,
  `github_get_issue`, `github_list_issue_comments`, `github_comment_issue`,
  `github_close_issue`, `github_list_open_prs`, `github_get_pr`,
  `github_merge_pr` — the agent can use these mid-conversation. A session
  can be linked to `owner/repo` so it doesn't need to say which repo every
  time.
- **Persistent GitHub tool cards**: an issue/PR tool call in the live
  stream renders as a clickable card, not a text blob — clicking opens a
  modal with the full body and comments, plus action buttons (Close issue,
  Merge PR, "Prompt agent to fix/review" which drops a pre-filled message
  into the composer for you to send).
- **Baked-in system prompts** for coding mode and general mode
  (`src-tauri/src/prompts.rs`) — the coding prompt is what points the model
  at the skills library instead of trying to hardcode every language's
  conventions inline.
- **OmniRoute runs as part of the app, not alongside it.** In local mode,
  the app installs OmniRoute itself into its own data folder (a real
  ~450MB one-time `npm install`, with live progress in the sidebar), then
  launches it directly for fast, version-pinned startup on every run
  after. Status and Start/Stop live in the sidebar; the process tree is
  killed on exit, including the Windows-specific fix needed for that to
  actually work. See `OMNIROUTE_INTEGRATION.md` — it also explains why
  this is a managed npm install rather than a compiled sidecar binary
  (short version: OmniRoute runs via `tsx` and has native per-platform
  dependencies, so single-file packaging isn't a good fit for it
  specifically — a correction from what this doc said a pass ago, once
  the real package was actually pulled apart).
- **About page** with credits and GitHub links to OmniRoute and Hermes
  Agent — the two projects this one is built directly on top of.
- **Providers dashboard, embedded** — OmniRoute ships its own web UI for
  connecting providers, checking usage, and tuning routing. Rather than
  sending you to a browser tab for it, the Providers page in this app
  iframes it in directly (local or remote), with a "start it first" state
  when the engine isn't running yet and an "Open in browser" escape hatch
  in case OmniRoute's own server blocks being framed.
- **Sub-agents, on by default.** A coding-mode session gets a
  `delegate_to_subagent` tool the agent is told to prefer for bulky or
  exploratory work (reading many files, searching the codebase, running a
  test suite and summarizing failures) — the sub-agent runs with its own
  isolated message history, entirely separate from the parent's, and only
  its final summary comes back as the tool result. The parent's context
  never sees the sub-agent's intermediate file reads or command output.
  One level of nesting only (a sub-agent can't spawn further sub-agents).
  Toggle it off at session creation, or mid-session from the header badge
  — either way turns it off structurally; the agent also respects being
  told "don't delegate" in plain conversation. Sub-agent tool calls stream
  as their own nested, animated card (pulsing while running, auto-collapses
  to its summary once done) rather than flattening into the main list —
  same planning-mode approval gate applies to its calls as to the parent's.

The frontend has been type-checked and built successfully (`tsc --noEmit`,
`vite build`) in the environment this was assembled in. The Rust side has
**not** been compiled — no Rust toolchain was available there — so treat
`cargo check` on your machine as the first real test of it. See
`SETUP_WINDOWS.md`.

Not built yet:
- Swarms / multi-agent delegation
- Inline PR diff review (line comments, approve/request-changes) — only
  merge and a summary chat-based review are wired up so far
- Editable system prompts from the UI (currently fixed constants)
- Token-level streaming (responses arrive as one block per model turn —
  tool-call progress *is* already live between turns)
- Mobile client (shelved — see earlier planning notes)

## Using it to build itself

Once `cargo check` is clean and `npm run tauri dev` opens the app:
1. Finish setup, pointing the workspace at this repo's own folder
2. Start a coding-mode session with planning mode on
3. Ask it to implement one of the "not built yet" items above
4. Review and approve each proposed write/command as it comes in

Planning mode existing is exactly what makes this safe to try immediately —
nothing lands on disk without a click.

## Why no backend server

Earlier planning had this running against a VPS. That's gone — everything
runs locally in one app. The one piece that behaves like "a backend"
(OmniRoute itself, a Node process) is meant to be bundled as a Tauri sidecar
later so users never have to run or manage it separately. Until that's wired
up, local mode expects you to run OmniRoute yourself.

## Project layout

```
kestrel/
  src-tauri/          Rust backend (Tauri app, commands, config, setup logic)
    src/main.rs        command definitions + app bootstrap
    src/config.rs       persisted config (OmniRoute connection, workspace path)
    src/setup.rs        the modular setup-step registry (see below)
  src/                 React/TypeScript frontend
    setup/              first-run wizard + its steps
    App.tsx             checks for pending setup, otherwise shows main shell
```

## How the modular setup system works

`src-tauri/src/setup.rs` holds an ordered list of setup steps (`step_registry()`).
Each user's completed steps are saved locally in `setup_state.json`. On launch,
the app diffs the full registry against what that user has already completed
and only shows the difference.

This means: when a future version adds a new step (say, a GitHub auth step),
existing users who already finished setup will see *only* that new step, not
the whole wizard again. New users see everything in order.

To add a step:
1. Add an entry to `step_registry()` in `src-tauri/src/setup.rs` with a unique
   `id` and the next `order` value.
2. Add a matching component to `stepComponents` in
   `src/setup/stepRegistry.tsx`, keyed by the same `id`.
3. That's it — existing installs pick it up automatically on next launch.

If the frontend doesn't recognize a step id shipped by an older frontend
build (mismatch during upgrade), `Wizard.tsx` skips it instead of crashing.

## Contributing

MIT licensed. Issues and PRs welcome once this is public — the intent is
that this project ends up partly built by the agent it produces.

## Windows setup

See `SETUP_WINDOWS.md`.
