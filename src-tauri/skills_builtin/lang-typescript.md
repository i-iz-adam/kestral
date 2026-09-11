# JavaScript / TypeScript

## Environment & tooling
- Check `package.json` for the package manager already in use (npm/pnpm/yarn — look for the lockfile) before running installs with a different one.
- Check for existing ESLint/Prettier config before introducing a different style.
- Testing: check for Jest, Vitest, or another runner in `package.json` scripts before assuming.

## Idioms
- Prefer `const`/`let`, never `var`.
- Prefer `async`/`await` over raw `.then()` chains for anything beyond a single call.
- In TypeScript, avoid `any` — use `unknown` plus a narrowing check when the type genuinely isn't known yet, and prefer precise types over broad ones.
- Destructuring for props/parameters over repeated `obj.field` access.
- Optional chaining (`?.`) and nullish coalescing (`??`) over manual null checks.

## Common gotchas
- `==` vs `===` — always `===` unless there's a specific, commented reason not to.
- Floating promises — an `async` call without `await` or `.catch()` silently swallows errors. Most linters can catch this; fix rather than suppress.
- Array/object mutation inside React state — always create a new array/object rather than mutating in place, or state updates won't trigger a re-render.
- `this` binding in callbacks outside arrow functions/class fields.
