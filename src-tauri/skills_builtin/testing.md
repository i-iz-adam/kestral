# Writing and running tests

## Deciding what's worth testing
- Business logic and anything with edge cases: yes.
- Trivial getters/setters, or a one-line wrapper around a well-tested library call: usually not worth a dedicated test.
- Bug fixes should almost always come with a regression test — write the failing test first, confirm it fails for the right reason, then fix the code until it passes.

## Loop
1. Write (or find) a test that fails for the reason you expect.
2. Make the smallest change that passes it.
3. Run the full relevant test suite, not just the new test — check nothing else broke.
4. Only then move to the next case.

## Running tests
Check for the project's actual test command before assuming — don't guess `pytest` on a project that uses `unittest`, or `npm test` on one that uses a different runner. Look at `package.json` scripts, a `Makefile`, `tox.ini`, or similar before running anything via `run_shell`.

## What good tests look like
- One logical assertion focus per test — a test with an unclear name and five unrelated assertions is hard to trust when it fails.
- Descriptive names that state the expected behavior, not the implementation: `rejects_expired_token`, not `test_1`.
- Deterministic — no reliance on real time, network, or ordering unless that's specifically what's being tested.
