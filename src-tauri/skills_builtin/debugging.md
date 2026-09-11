# Systematic debugging

A repeatable process for isolating and fixing a bug instead of guessing at fixes.

1. **Reproduce it first.** Don't touch code until you can trigger the bug reliably — a failing test, a specific command, or a specific input. If you can't reproduce it, say so and ask for the exact repro steps rather than fixing speculatively.
2. **Read the actual error.** The full stack trace / error message, not a summary of it. The root cause is usually named explicitly somewhere in there.
3. **Localize before you fix.** Use `read_file` and `list_dir` to trace the call path. Add a temporary print/log via `run_shell` if the codebase doesn't already have enough visibility — remove it once you're done.
4. **Form one hypothesis at a time.** Don't change five things and hope one works — you'll never know which fix actually mattered, and you may have introduced new bugs.
5. **Verify the fix against the original repro**, not just "it compiles now."
6. **Check for the same bug elsewhere.** If it was a copy-pasted pattern, search for other occurrences with `run_shell` (grep/ripgrep) before calling it done.

If a bug resists two or three focused hypotheses, stop and summarize what's been ruled out so far — that's more useful to the user than continuing to guess.
