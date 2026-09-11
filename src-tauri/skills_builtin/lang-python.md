# Python

## Environment & tooling
- Prefer a virtual environment (`venv` or the project's existing one, e.g. Poetry/uv) over installing into the system Python — check for `pyproject.toml`, `requirements.txt`, or `poetry.lock` to see what's already in use before picking a tool.
- Formatting/linting: check for `ruff`, `black`, or `flake8` config before assuming which one the project uses.
- Testing: check for `pytest.ini`/`pyproject.toml [tool.pytest]` vs plain `unittest` before running tests.

## Idioms
- Type hints on public functions, even in an otherwise untyped codebase — they cost little and catch real bugs.
- List/dict comprehensions over manual loops for simple transforms; drop back to a loop once the comprehension needs a conditional inside a conditional.
- Context managers (`with open(...) as f:`) for anything that needs cleanup — files, locks, connections.
- f-strings for formatting, not `%` or `.format()`, unless matching existing style.
- `pathlib.Path` over raw string path manipulation for new code.

## Common gotchas
- Mutable default arguments (`def f(x=[]):`) — shared across calls, almost never what's intended.
- Late-binding closures in loops (`[lambda: i for i in range(3)]` all return 2) — capture with a default arg if needed.
- `is` vs `==` — `is` for `None`/identity, `==` for value equality.
- Circular imports from overly clever package `__init__.py` re-exports.
