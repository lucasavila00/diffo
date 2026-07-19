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

- The Diffo application never reads CLI options. The executable launcher may accept only
  the fixed `update` maintenance argument; do not add any other public arguments.
- Diffo will never have configuration files or configurable key bindings.
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

## Crate documentation

- Every package under `crates/` must have a checked-in `README.md` declared by its
  `Cargo.toml` `readme` field.
- Treat the crate README as the source of truth for crate-level documentation. Include
  it from the crate root with `#![doc = include_str!("../README.md")]`; do not maintain
  a second copy in `//!` comments.
- Keep crate READMEs focused on purpose, responsibilities, and boundaries. Put API
  details on the Rust items they describe.
- Update the crate README when a change alters the crate's documented role or
  boundaries.
- Run `cargo doc --workspace --no-deps` after changing crate-level documentation.

## TUI conventions

- Always restore the terminal before returning from the application.
- Use the semantic layout and dark-gray chrome tokens from `diffo-ui` for structural
  borders, dividers, scrollbars, selection backgrounds, dimensions, gaps, and
  overlay bounds. Keep meaning-specific colors for semantic content, diffs, and
  syntax highlighting.
- Design for SSH use and treat terminal input, redraw work, and output as network
  costs at all times. Do not add hover-driven visual changes, hover-only state,
  passive mouse-movement handling solely for hover feedback, or redraws caused only
  by pointer movement; the resource cost provides too little user value.
- Keep terminal rendering, input handling, application state, and external commands in separate modules.
- Document key bindings in the interface and update them when controls change.
- Keep tests that reject uppercase character entries in the fixed key-binding table.
- Avoid blocking work in the rendering and input loop.
- Treat the displayed diff buffer and its viewport as one atomic commit. Keep the previous buffer unchanged until the replacement content, projections, hunk targets, scroll bounds, and initial position are ready to draw together.
- Treat visible syntax coverage as part of the atomic commit. File opens and uncached vertical jumps must not display a plain target and color it in a later frame.
- Bound syntax work by the visible viewport, fixed parser look-behind, and a fixed byte budget; never put full-file syntax work back on the file-opening critical path.
- Build only the requested diff projection on a cold path. Treat a view-mode change as an atomic prepared transition and keep the previously committed mode visible until it is ready.
- Keep the prepared file-and-mode cache at four entries unless a newer ADR changes
  that boundary.
- Preserve the strict 10,000-line syntax eligibility boundary and the sub-100 ms 9,999-line reference benchmark unless a newer ADR replaces that contract.
- Drain and install background diff results only during frame preparation. Rendering must consume committed state only, and stale results must never supply content, navigation targets, or scroll metrics.
- Keep the vertical scrollbar and hunk-marker rail visually and interactively separate; neither control may overwrite or capture the other control's cells.
- Add deterministic state-transition tests and a frame-traced PTY regression whenever changing asynchronous diff preparation, buffer opening, first-hunk navigation, or scrollbar markers. Never use sleeps or delay environment hooks to create the asynchronous boundary.
- Keep mock repository states in `crates/diffo-core/fixtures/`; do not add mock-only behavior to the real Git data path.

## Development workflows

- Run `make diffo` against the current Git repository.
- Run `make diffo-mock` against the mutable in-memory fixture at
  `crates/diffo-core/fixtures/repository-state.ron`. Its stage and unstage actions
  must not modify the fixture on disk.
- Keep generated large and stress-test payloads out of the repository. The mock
  application generates them on demand.
- Use `DIFFO_MOCK_FILE` only to preview another RON fixture during development and
  testing; it is not user configuration.
- Use `DIFFO_DUMP_PATH` only to write one repository snapshot and exit before the
  TUI starts; it is not user configuration.

## Release signing

- Stable releases use `v<major>.<minor>.<patch>` tags.
- Store the base64-encoded unencrypted PKCS#8 PEM Ed25519 private key in the
  `DIFFO_UPDATE_SIGNING_KEY` repository secret. Never commit the private key.
- Store the base64-encoded raw 32-byte public key in the
  `DIFFO_UPDATE_PUBLIC_KEY` repository variable.
- The release workflow must derive the public key from the private key and fail
  before building when the configured keys do not match.
- Embed the tag version in the binary and signed update metadata independently of
  the Cargo package version.

## Validation

`make all` is the only repository validation command. Always run it before
considering any change complete:

```sh
make all
```

Real Git behavior is snapshot-tested in `diffo-e2e` through `make all`.
`make e2e-review` reviews intentional snapshot changes with `cargo-insta`. Always
complete repository validation with `make all`.
