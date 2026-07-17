# ADR 0006: Split Diffo into crates

Status: Accepted

## Problem

`git-diff-tui` holds state, Git commands, fixtures, UI, and startup code. These parts
change for different reasons.

## Crates

```text
diffo-core   Snapshot types and RepositorySource trait
diffo-app    Pure Model, Message, update, and Effect state machine
diffo-git    Real Git source
diffo-tui    App state and terminal UI
diffo        Binary, source choice, startup, shutdown
```

`diffo-core` has no Git or TUI dependencies. `diffo-app` depends only on
`diffo-core`. `diffo-git` and `diffo-tui` stay outside the pure state layer. The
`diffo` binary runs effects and wires the crates together.

Keep fixture loading in `diffo-core` for now. Move it only if it grows.

## Rules

- Dependencies point toward `diffo-core`.
- Crates do not depend on the `diffo` binary.
- The UI only reads snapshots.
- Git code does not know about terminal code.
- App state does not know about Git, Crossterm, Ratatui, or screen coordinates.
- Keep one workspace version for all Diffo crates.

## Move order

1. Move snapshot types and fixture source to `diffo-core`.
2. Move `GitRepositorySource` to `diffo-git`.
3. Move app state and rendering to `diffo-tui`.
4. Leave `main.rs` in the `diffo` binary crate.
5. Run unit tests and the real Git E2E script after each move.

## Done when

- Each crate has one job.
- `make diffo` and `make diffo-mock` still work.
- Workspace tests, Clippy, and E2E pass.
