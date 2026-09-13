---
name: skill-creator
description: In-depth methodology for writing, editing, and improving skills — read this before authoring or updating one.
---

# Writing skills

A skill is a piece of procedural knowledge you'd otherwise have to
re-derive: a convention, a checklist, a tool's sharp edges, a house style.
It exists so a future session — yours or another one's — starts from
"here's how this works" instead of from scratch. This skill covers how to
write one that actually earns its place in the library, and how the
create_skill / edit_skill / propose_skill tools fit together.

## Before writing anything: does this deserve to be a skill?

Ask two questions.

**Will this come up again, for someone other than me, right now?** A fact
about this one file, this one bug, or this one user's one-off phrasing is
not a skill — it's just the task. "The auth module in this repo validates
tokens in `middleware/auth.rs`" is not a skill. "This codebase puts all
middleware validation in `middleware/`, named `<concern>.rs`" might be,
*if* you'd expect that pattern to hold for the next five files someone
touches here too, not just the one you happened to be in.

**Would writing this down have saved real effort — not just been mildly
convenient?** If discovering it took you one obvious read of a file, it's
not worth a skill; you'd rediscover it just as fast next time. If it took
several failed attempts, a surprising error message, or knowledge that
isn't visible from just looking at the code (a team convention, a tool's
undocumented flag behavior, a reason something looks wrong but isn't),
that's the signal.

When in doubt, propose it anyway (see "Which tool" below) — proposing
costs nothing and a human filters it. The failure mode to actually avoid
is a library cluttered with narrow, low-value entries that make
list_skills noisier without making anyone's next session faster. Quality
over coverage.

## Check for overlap first

Always call `list_skills` before writing anything. If something close
already exists:

- If it's basically the same thing with a gap, **edit it** — add the
  missing piece, don't create `git-workflow-2`.
- If it's adjacent but distinct (e.g. an existing `testing` skill and you
  learned something specific to one test framework), consider whether the
  existing skill should just get a new section, versus whether this is
  genuinely a separate concern someone would want to load independently.
  Default to folding in unless the result would become unfocused.
- Never create two skills that would both plausibly fire for the same
  situation and give contradictory advice. If you're not sure whether
  yours would win, that's a sign to edit the existing one instead.

A library where every skill is unambiguously the entry point for what it
covers beats one with more entries but overlapping scope.

## Structure of a skill

```
---
name: short-display-name
description: One line — what this covers and when it's relevant.
triggers: keyword one, another phrase, .ext
---

Body in markdown. The actual instructions.
```

**`name`** — short, human-readable. Doesn't need to match the id.

**`description`** — this is what shows up in `list_skills` output, which
is the *only* thing a future session sees before deciding whether to read
the whole skill. Write it so someone skimming a list of thirty of these
would recognize their situation in it. "Guidance for X" is weak. "What to
check before opening a PR: branch naming, commit format, and the tests
that must pass first" tells you exactly when to reach for it. Bad
descriptions are the single biggest reason a good skill never gets used.

**`triggers`** (optional) — comma-separated words/phrases. If a user's
message contains one, this skill's full content gets auto-loaded into
context without anyone needing to call `list_skills`/`read_skill` first.
This matters because those calls are optional from the model's point of
view — a skill that depends entirely on the model remembering to go look
for it will get missed sometimes, especially under a large tool surface.

Trigger design rules:
- Prefer specific, low-collision terms: a language name, a file
  extension, a tool's exact name, a distinctive phrase from how people
  actually describe the situation ("pull request", not "request").
- A single common English word (e.g. "test" alone is borderline; it's in
  the existing `testing` skill's table because it's a genuine keyword for
  that domain, but adding a bare word like "code" or "file" to a narrow
  skill would fire on nearly everything and drown out more relevant
  skills).
- Multi-word phrases and anything with a `.` match as substrings; bare
  single words match as whole tokens only (so a trigger `go` fires on "go
  fix this" but not on "background task"). Lean on multi-word phrases when
  a single word would be too broad.
- Skip triggers entirely for something genuinely rare or hard to phrase as
  a keyword — that's fine, it just means this one relies on
  `list_skills`/`read_skill` instead, same as most hand-authored skills
  before triggers existed at all.

**Body** — write it as instructions for future-you, in the imperative or
as clear declarative rules ("when X, do Y, because Z"), not as a log of
what happened in this session. Include the *why* behind a rule when it's
not obvious — "why" is what lets a future session apply the same
principle to a slightly different situation it wasn't literally written
for, instead of pattern-matching too literally or too loosely.

Keep it focused. A skill that tries to cover an entire language or an
entire discipline in one file becomes a wall of text nobody reads in
full — better to be the specific thing that's actually non-obvious, and
trust general capability for the rest. Compare the existing `lang-*`
skills (deliberately short: tooling, idioms, common gotchas) against how
long a "complete guide to Python" would have to be to be comprehensive —
the short version is the one that actually gets read.

## Choosing an id

Lowercase, kebab-case, descriptive: `git-worktrees`, not `skill-3` or
`GitWorktrees`. If you don't pass an explicit id to create_skill, one is
slugified from the name automatically — that's usually fine.

Never reuse an existing skill's id for something unrelated. If you want to
change what an id means, that's an edit to the existing skill, not a new
one shadowing it.

## Editing an existing skill — including a builtin

`edit_skill` works exactly like `edit_file`: exact search-and-replace
against the skill's current content (read it first with `read_skill` so
your `old_string` matches exactly). This is the right tool whenever a
skill you actually consulted this session turned out to be wrong,
outdated, or missing a case you had to work around by hand — fix it in
place rather than leaving the same gap for the next session to hit.

Editing a **builtin** id (one from `list_skills` with source "builtin")
doesn't touch the app's shipped files — it writes an override that
supersedes the builtin's compiled-in content from then on, for every
session, until someone deletes the override (which reverts to the
original). This is always safe to do when you're confident the
correction is right: nothing is destroyed, and it's exactly as reversible
as any other file edit. It's *not* the right move for a change that's
really project-specific rather than a genuine correction — a
project-specific convention on top of a general builtin (e.g. "this repo
additionally requires X in every commit message") belongs in a new,
narrower skill of its own, not folded into overriding the general one for
every future project too.

## Which tool: create_skill / edit_skill vs. propose_skill

- **create_skill / edit_skill** apply immediately (subject to planning-mode
  approval, same as any file write). Use these when the user explicitly
  asked, in this conversation, for something to be saved or remembered as
  a skill. That's a direct instruction, not a judgment call.
- **propose_skill** queues a create or update for a human to review before
  it has any effect on any session, including this one. Use this for
  anything *you* noticed as reusable on your own initiative, without being
  asked. This is deliberately the higher-friction, safer default for
  self-directed learning: it costs nothing to propose, so there's no
  reason to talk yourself out of a good instinct just because you're not
  100% sure — let the review step be the judgment call, not your own
  restraint. Always write a specific, honest `rationale`; that's the main
  thing the human reviewer has to go on.

A good rule of thumb: if you'd be surprised the user knew you did it,
it should have gone through propose_skill.

## Testing a skill before considering it done

A skill is only as good as whether following it actually produces the
right outcome. Before finishing:
- Re-read it as if you'd never seen the situation before — does it stand
  alone, or does it assume context only this session had?
- Would it have actually changed what you did, if you'd read it at the
  start instead of learning the lesson the hard way partway through? If
  the answer is no, it's not capturing the right thing yet.
- Check it doesn't contradict another enabled skill. If it might, that's
  a sign to fold the two together instead of leaving conflicting guidance
  for whichever one happens to load.

## Anti-patterns

- **Padding.** Don't invent detail to make a skill sound more complete —
  an accurate, three-sentence skill beats a confident-sounding paragraph
  with guessed specifics.
- **Narrating instead of instructing.** "In this session I found that
  the tests were in `tests/`" is a diary entry. "Tests live under
  `tests/`, one file per module, named `test_<module>.py`" is a skill.
- **Overriding a builtin for a one-off.** If the correction is really
  "for this repo" rather than "the builtin guidance is actually wrong",
  write a new project-scoped skill instead of overriding the general one.
- **Skipping the overlap check.** Always `list_skills` first.
