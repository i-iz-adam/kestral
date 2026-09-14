# Changes: long-running-session hardening

Verified with `cargo check` (rustc/cargo 1.91) against the original, unmodified `Cargo.lock` — no dependency changes were needed for any of this.

## 1. Context compaction — `src-tauri/src/context.rs` (new)
- `context::maybe_compact(cfg, messages, force)` estimates token size (~chars/4) of a message vector and, once it crosses a 60k-token trigger, folds everything before a safe cut point (the last ~40 messages are always kept verbatim, and the cut always lands on a `user`-role message boundary so a tool_calls/tool-result pair is never split) into one summary message via a fast-model call. A prior summary at the front gets rolled into the new one instead of re-summarized from scratch.
- Called once per step (not once per turn) in `agent.rs`'s turn loop and in `subagent.rs`'s loop, since a single turn/task can itself run for hundreds of steps.
- `context::is_context_length_error(&str)` is a substring heuristic for the handful of ways an OpenAI-compatible backend phrases "too long"; on that error, `agent.rs`/`subagent.rs` force a compaction regardless of the estimate and retry the model call exactly once before giving up.

## 2. `run_shell` timeout + kill — `src-tauri/src/tools.rs`
- New `tools::run_shell_async` uses `tokio::process::Command` (`kill_on_drop(true)`) instead of the old synchronous `std::process::Command::output()`.
- Real timeout: `timeout_seconds` is now a model-settable argument on the `run_shell` tool schema (default 300s, capped at 1800s). On timeout **or** on the user hitting Stop (via `SessionStop::sleep_till_stop_or`, already in the codebase but previously unused for this), the child is actually killed (`child.start_kill()` + `wait()`), not just abandoned.
- Dispatched from `agent.rs::execute_tool` as a special case before the rest of the tool dispatch, since it needs real async cancellation that the rest of `tools::execute` doesn't.
- The rest of `tools::execute` (file I/O, search_code, etc.) is now run via `tokio::task::spawn_blocking` from `agent.rs`, so a slow directory walk or big write can't stall the async runtime either.

## 3. Output size caps — `src-tauri/src/tools.rs`
- `MAX_TOOL_OUTPUT_CHARS = 40_000`.
- `read_file`: truncates with a note ("truncated after N of M lines... pass start_line: X to continue"); new optional `start_line`/`num_lines` args let the model page through a large file instead of re-reading from the top every time.
- `run_shell`: stdout/stderr are each capped, keeping **head and tail** (`cap_head_tail`) rather than just the head, since a build error is often at the end of the log.

## 4. Retry/backoff on the model call — `src-tauri/src/omniroute.rs`
- Both `chat_completion` and `chat_completion_stream` now retry up to 3× with exponential backoff (500ms × 2^n) on connection errors and retryable HTTP statuses (429/500/502/503/504).
- Retries only ever happen *before* anything from the response has been used — no success status yet for `chat_completion`, no delta emitted yet for `chat_completion_stream` — so a retry never risks resending a request whose partial output the user already saw.

## 5. Durable plan/task tracker — `src-tauri/src/plan.rs` (new)
- `update_plan` tool (full-list-replace semantics, same shape as Claude Code's TodoWrite) persists a small per-session JSON file under the app's config dir, entirely outside `session.messages` — so it survives context compaction by construction, and even a crash mid-turn.
- Re-read fresh and injected as a system message every step, in both the top-level agent loop and sub-agent loop, so a sub-agent picking up a long task mid-flight sees the same plan the parent does.
- Added to both system prompts (`prompts.rs`) with guidance to use it for anything with more than a handful of steps.

## 6. Shell sandboxing — `src-tauri/src/tools.rs`, `sessions.rs`, `main.rs`
- New per-session `sandbox_shell` / `sandbox_network` fields (default off, to not require Docker for normal use), with `sessions::set_sandbox_shell`/`set_sandbox_network` and matching Tauri commands (`set_session_sandbox_shell`, `set_session_sandbox_network`) — **no frontend toggle was added**; the React/TS UI wasn't part of the files reviewed for this pass, so wiring a switch into the settings panel is a follow-up.
- When on, `run_shell` runs inside a `docker run --rm --network none --cap-drop ALL --memory 2g --cpus 2 ...` container (network re-enabled only if `sandbox_network` is also on) instead of directly on the host. Checks `docker info` first and fails loudly (not silently unsandboxed) if Docker isn't available.
- System prompt tells the model to suggest turning this on for a task involving compiling/running code from an untrusted source.

## 7. Decompilation skill — `src-tauri/skills_builtin/decompile-jar.md` (new)
- Registered as a builtin skill (`skills.rs`), with keyword triggers (`decompile`, `.jar`, `obfuscated`, `cfr`, `vineflower`, `bytecode`, etc.) so it auto-loads on a relevant request.
- Covers: when/why to turn on sandboxing for this specific workflow, choosing between Vineflower/CFR, the unpack → decompile → build-file → iterate-on-compile-errors → verify-against-original loop, and using `update_plan` to keep a long multi-phase run coherent.

## Notes / things worth a second look
- Step caps (1000/500) and the loop detector were confirmed fine as-is, per the original assessment — untouched.
- `context.rs`'s token estimate is a cheap `chars/4` heuristic, not a real tokenizer — conservative on purpose, but worth tuning against whatever model OmniRoute actually routes `auto/coding` to if compaction fires either much earlier or much later than expected in practice.
- The sandbox image (`debian:stable-slim`) has no JDK/build tools preinstalled by design (see the comment in `tools.rs`) — the decompile skill tells the model to install what it needs inside the sandbox with `sandbox_network` on first. This keeps the sandboxed image small/generic but means the first sandboxed step of a decompile task is a toolchain-install step, not the decompile itself.
