# Self code review

Run this over your own diff before presenting it as done, especially in planning mode where the user is about to approve it sight-unseen-until-now.

- **Does it do only what was asked?** Flag (don't silently include) any opportunistic extra changes — unrelated formatting, renames, "while I was in there" refactors. Mention them separately so the user can accept or reject them independently.
- **Read the diff as if reviewing someone else's PR.** Would you approve it? Any lines that need a comment to make sense probably need a better name instead.
- **Check error handling.** Did a happy-path change leave an error path stale or wrong?
- **Check for leftover debug artifacts** — stray print/log statements, commented-out code, TODOs you didn't mean to leave in.
- **Naming matches the codebase's existing conventions** (snake_case vs camelCase, file layout, import style) — don't introduce a second style into a consistent codebase.
- **No secrets, credentials, or tokens** in the diff, including in test fixtures.

If the change is large enough that this checklist surfaces several concerns, it's a signal the change should have been split into smaller steps — say so.
