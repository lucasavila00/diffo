# AGENTS.md

Behavioral guidelines to reduce common LLM coding mistakes. Merge with project-specific instructions as needed.

**Tradeoff:** These guidelines bias toward caution over speed. For trivial tasks, use judgment.

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

---

**These guidelines are working if:** fewer unnecessary changes in diffs, fewer rewrites due to overcomplication, and clarifying questions come before implementation rather than after mistakes.


# Repository guidelines

This repository is a Rust workspace containing small command-line utilities. Each utility lives in its own package under `crates/`.

## Workspace structure

- Keep shared package metadata, dependency versions, and lint configuration in the root `Cargo.toml`.
- Add each utility as a separate package under `crates/<name>` and register it in the workspace members.
- Prefer focused modules over putting all application logic in `main.rs`.
- Keep `main.rs` responsible for startup, shutdown, and top-level orchestration.

## Product constraints

- Diffo will never have CLI arguments, configuration files, or configurable key bindings.
- Keep controls and product behavior fixed in code.
- Never add uppercase character shortcuts. Character shortcuts must be lowercase and must not require Shift.
- Environment variables are developer and test hooks only. Do not turn them into user configuration.

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
- Keep tests that reject uppercase character entries in the fixed key-binding table.
- Avoid blocking work in the rendering and input loop.
- Treat the displayed diff buffer and its viewport as one atomic commit. Keep the previous buffer unchanged until the replacement content, projections, hunk targets, scroll bounds, and initial position are ready to draw together.
- Treat visible syntax coverage as part of the atomic commit. File opens and uncached vertical jumps must not display a plain target and color it in a later frame.
- Bound syntax work by the visible viewport, fixed parser look-behind, and a fixed byte budget; never put full-file syntax work back on the file-opening critical path.
- Build only the requested diff projection on a cold path. Treat a view-mode change as an atomic prepared transition and keep the previously committed mode visible until it is ready.
- Preserve the strict 10,000-line syntax eligibility boundary and the sub-100 ms 9,999-line reference benchmark unless a newer ADR replaces that contract.
- Drain and install background diff results only during frame preparation. Rendering must consume committed state only, and stale results must never supply content, navigation targets, or scroll metrics.
- Keep the vertical scrollbar and hunk-marker rail visually and interactively separate; neither control may overwrite or capture the other control's cells.
- Add deterministic state-transition tests and a delayed PTY regression whenever changing asynchronous diff preparation, buffer opening, first-hunk navigation, or scrollbar markers.
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
