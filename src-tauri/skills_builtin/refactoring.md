# Safe refactoring

Restructuring code without changing its behavior, in small reversible steps.

## Ground rules
- Never mix a refactor with a behavior change in the same step. If you notice a bug while refactoring, note it and fix it separately.
- Prefer refactors that keep the code working at every intermediate step over a single large rewrite — if something breaks, you want a small diff to blame, not the whole change.
- Run existing tests before starting (to know the baseline) and after each step (to catch regressions early).

## Common safe moves
- Extract a function/method with the same signature and body, then call it from the original site, then verify, then repeat elsewhere if duplicated.
- Rename in one pass using project-wide search (`run_shell` grep) rather than editing files one at a time from memory — partial renames are a common source of breakage.
- Introduce a new interface alongside the old one, migrate call sites incrementally, then remove the old one once nothing references it.

## When there are no tests
Say so, and either write a minimal characterization test first (capture current behavior, even if it's not the "correct" behavior) or proceed more conservatively with smaller, more frequently verified steps — don't treat the absence of tests as license to move faster.
