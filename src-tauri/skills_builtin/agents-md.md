# Writing and maintaining AGENTS.md

AGENTS.md is a plain-markdown file at a project's root that gives any
coding agent working in that repo — this one included — standing
instructions specific to that codebase. Kestrel reads it automatically
into every coding-session turn for a workspace that has one, the same
convention a growing number of other coding agents follow. Use this
skill whenever a user asks you to create, review, or update a project's
AGENTS.md, or when you notice a repo has none and would clearly benefit
from one.

## What AGENTS.md is for (and isn't)

It's the answer to: "what would I tell a new, competent engineer on
their first day, before they touch any code?" Things that are true about
*this* project specifically, that aren't visible just by reading the
code, and that would otherwise have to be rediscovered — or worse,
guessed wrong — every session.

It is **not**:
- A copy of the README (that's for humans discovering the project; this
  is operational instructions for an agent already working in it).
- A place for one-off task notes, a changelog, or TODOs — those belong
  in issues, commit messages, or a project's own docs, not here.
- A dumping ground for everything the codebase *could* tell you. A
  bloated AGENTS.md that repeats what's obvious from `package.json` or
  `Cargo.toml` costs context on every single turn for no benefit —
  keep it tight.

## What actually belongs in it

Roughly in order of how often it saves real work:

1. **Build, test, and lint commands.** The exact ones, not "run the
   tests" — `npm run test:unit` vs `npm test` vs a script that wraps
   both matters, and guessing wrong wastes a round-trip.
2. **Project-specific conventions that aren't inferable from the code
   alone.** Naming patterns, where new code of a given kind goes, a
   house style the linter doesn't enforce.
3. **Things that look like a good idea but are actually wrong here.** A
   library that's deliberately not used despite being the "obvious"
   choice, a directory that looks editable but is generated, a pattern
   that was tried and reverted. This category has the highest
   value-per-line — it's exactly the kind of mistake an agent (or a new
   hire) would otherwise make once, painfully, before learning better.
4. **How to verify a change actually worked**, if it's not just "run the
   tests" — a manual smoke-test step, a service that needs to be running
   locally, environment variables a dev server expects.
5. **Deploy/release process**, if the agent might ever be asked to run
   it — exact commands, what environment they target, anything
   irreversible that needs a human's explicit go-ahead first.
6. **Pointers, not copies.** If there's a CONTRIBUTING.md or a docs/
   folder with real depth on something, link to it rather than
   duplicating it here — AGENTS.md should stay skimmable.

## Structure

There's no required schema, but a predictable shape helps both humans
skimming it and an agent looking for one specific thing under time
pressure. A reasonable default:

```markdown
# AGENTS.md

## Setup
(install/bootstrap commands, required env vars)

## Build & test
(exact commands — build, unit tests, integration tests, lint, typecheck)

## Conventions
(naming, structure, house style not caught by linters)

## Gotchas
(things that look right but aren't — the highest-value section)

## Deploy
(if applicable — exact steps, what's irreversible, what needs a human)
```

Skip any section that has nothing genuinely worth saying — an empty
"Gotchas" heading with "none yet" under it is better than nothing (it
signals the file was maintained deliberately, not abandoned), but don't
pad every section just to fill the template.

## Writing it

- Use read_file/find_files/search_code to check what's actually true
  (the real test command, the real lint config) rather than guessing
  from the project's language or framework — a wrong command in
  AGENTS.md is worse than no AGENTS.md, since it'll be trusted and
  retried before anyone thinks to doubt it.
- Write it the way you'd want to receive it: short, declarative
  sentences, no filler ("This project uses TypeScript" tells an agent
  nothing it can't see from the file extensions — "Run `npm run
  typecheck` before considering a TS change done; the build step alone
  doesn't catch type errors" tells it something real).
- If the user already has strong opinions about their process, capture
  those verbatim rather than smoothing them into generic advice —
  AGENTS.md is supposed to be *this project's* voice, not a template.

## Keeping it current

Treat a stale or wrong line in AGENTS.md the same way you'd treat a
wrong instruction in any other skill: fix it in place via edit_file the
moment you notice it's out of date, rather than working around it
silently and leaving the next session to hit the same wrong command. If
something you learned this session feels durable but is really about
*this specific project* rather than something generalizable across
projects, it belongs here — not as a create_skill/propose_skill call,
which are for cross-project knowledge. AGENTS.md is itself the
project-scoped equivalent of a skill; a genuinely reusable pattern
you'd want in *other* projects too is what create_skill (optionally
with `scope: "project"` if it's still specific to this one) is for
instead.

## When there isn't one yet

If a user asks you to create AGENTS.md for their project and it doesn't
exist, don't ask them to dictate it — read the repo (README, package
manifest, CI config if any, a few representative files) to draft one
inferring the setup/build/test commands, then show the draft and ask
what to correct rather than starting from a blank page. Getting 80% of
it right from the repo itself and having the user fix the last 20% is
almost always faster than the reverse.
