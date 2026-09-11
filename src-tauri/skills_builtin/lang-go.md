# Go

## Tooling
- `go build ./...` and `go vet ./...` before considering a change done.
- `gofmt`/`goimports` — Go's formatting isn't optional style, run it, don't hand-format.
- `go test ./...` for the whole module, not just the package touched, since Go's type system won't catch every cross-package break.

## Idioms
- Explicit error checks (`if err != nil { return err }`) after nearly every call that can fail — don't ignore an error return, even to "clean up" the code.
- Wrap errors with context on the way up: `fmt.Errorf("doing X: %w", err)`, so a failure deep in a call stack is still diagnosable at the top.
- Small interfaces, defined at the point of use (consumer side), not alongside the implementing type.
- Prefer composition over embedding-as-inheritance unless the embedding genuinely models an "is-a" relationship.

## Common gotchas
- Loop variable capture in closures/goroutines before Go 1.22 — each iteration reused the same variable. Go 1.22+ fixed this per-iteration, but check the module's Go version before relying on it.
- Nil interface vs nil concrete value — an interface holding a nil pointer is not itself `== nil`. A common source of confusing bugs when returning a typed error.
- Unbuffered channel deadlocks — sending on a channel nobody's reading blocks forever; make sure there's always a receiver before a send, or use a buffered channel deliberately.
