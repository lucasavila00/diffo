# Repository guidelines

This repository is a Rust workspace containing small command-line utilities. Each utility lives in its own package under `crates/`.

## Workspace structure

- Keep shared package metadata, dependency versions, and lint configuration in the root `Cargo.toml`.
- Add each utility as a separate package under `crates/<name>` and register it in the workspace members.
- Prefer focused modules over putting all application logic in `main.rs`.
- Keep `main.rs` responsible for startup, shutdown, and top-level orchestration.

## Rust conventions

- Use the workspace Rust edition and minimum supported Rust version.
- Inherit dependencies and package metadata from the workspace where possible.
- Avoid `unsafe` code; it is forbidden by the workspace lint configuration.
- Handle recoverable errors with `Result` and add context at system boundaries.
- Do not panic for expected user or environment errors.
- Add unit tests for state transitions and non-trivial application logic.

## TUI conventions

- Always restore the terminal before returning from the application.
- Keep terminal rendering, input handling, application state, and external commands in separate modules.
- Document key bindings in the interface and update them when controls change.
- Avoid blocking work in the rendering and input loop.
- Keep mock repository states in `crates/diffo-core/fixtures/`; do not add mock-only behavior to the real Git data path.

## Validation

Run these commands before considering a change complete:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Real Git behavior is snapshot-tested in `diffo-e2e`. Use `make e2e` to check it and
`make e2e-review` to review intentional snapshot changes with `cargo-insta`.

When changing a single package, targeted commands are useful during development, but the complete workspace checks should still pass before handoff.
