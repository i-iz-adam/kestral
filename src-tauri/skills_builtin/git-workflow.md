# Git workflow

Use this before making commits or opening branches/PRs on behalf of the user.

## Branching
- Branch off the default branch unless told otherwise: `git checkout -b <type>/<short-description>`
- Types: `feat`, `fix`, `chore`, `refactor`, `docs`, `test`. Example: `fix/session-approval-race`.
- Keep branches scoped to one logical change. If a task grows into two unrelated changes, split it.

## Commits
- Write commit subjects in the imperative mood: "Add skill install endpoint", not "Added" or "Adds".
- Subject under ~72 chars. Body (if needed) explains *why*, not a restatement of the diff.
- Prefer several small, reviewable commits over one giant one, each of which leaves the repo in a working state.
- Never commit secrets, tokens, or `.env` files. Check `git status` before committing if unsure what's staged.

## Before opening a PR
- Run the project's tests and linter if they exist (check `list_skills`/`read_skill` for a language-specific skill, or look for `package.json`/`Cargo.toml`/`pyproject.toml` scripts).
- Write a PR description with: what changed, why, and how it was verified. Link the issue it closes if there is one (`Closes #N`).
- Keep the PR focused — reviewers trust small PRs more, and they merge faster.

## Never
- Force-push a shared branch without being asked explicitly.
- Merge your own PR unless the user has clearly asked for that.
