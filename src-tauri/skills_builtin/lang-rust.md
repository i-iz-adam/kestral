# Rust

## Tooling
- `cargo check` first — much faster than `cargo build` for catching type errors while iterating.
- `cargo test`, `cargo clippy`, `cargo fmt` before considering a change done, if the project uses them (most do).
- Check `Cargo.toml` for the edition and existing dependencies before adding a new crate for something the project might already have a tool for.

## Idioms
- Prefer `Result<T, E>` and `?` over `unwrap()`/`expect()` in any code path that isn't a test or a genuinely-unreachable invariant — `unwrap()` in library or app code is a future panic waiting to happen.
- Borrow (`&T`) before you clone — reach for `.clone()` only when ownership genuinely needs to move or the borrow checker has no other reasonable answer.
- Use `impl Trait` or generics over `Box<dyn Trait>` unless dynamic dispatch is specifically needed.
- Prefer iterators (`.map()`, `.filter()`, `.collect()`) over manual index loops where it doesn't hurt readability.

## Common gotchas
- Holding a `std::sync::MutexGuard` across an `.await` point — it's not `Send`, and this is a frequent async-Rust compile error. Use `tokio::sync::Mutex` if the lock must survive an await, or drop the guard before awaiting.
- Forgetting `#[derive(Debug)]` — nearly every struct benefits from it for error messages and debugging.
- Fighting the borrow checker by cloning everywhere as a first instinct — usually a sign the ownership structure itself needs a rethink, not more clones.
