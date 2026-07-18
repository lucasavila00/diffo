# CLI utilities

A Rust workspace for small command-line utilities.

## Utilities

- [`diffo`](crates/diffo): browse the current repository's Git state in a terminal UI.

## Development

```sh
make diffo
make diffo-mock
make diffo-mock-ro
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
```

`make diffo` reads the current Git repository. `make diffo-mock` loads a mutable,
in-memory repository from `crates/diffo-core/fixtures/repository-state.ron`. Stage,
unstage, and stage-all work for the life of the process without changing the fixture.
It also generates large changes on demand: a 20,000-line Rust file, 5,000 JSON
items, and a 25,000-byte line. These payloads are not stored in the repo.
It also includes generated 5k, 50k, 500k, and 5,000k-line stress patches.
`make diffo-mock-ro` loads the same state in read-only mode. The fixture covers staged
and unstaged changes, untracked files, recent commits, and commits that have not been
pushed.

Set `DIFFO_MOCK_FILE` directly to preview another RON fixture without changing the normal
application behavior.

For debugging, `DIFFO_DUMP_PATH=state.ron make diffo` writes one repository snapshot
and exits without opening the TUI. Diffo has no command-line arguments.

## TUI architecture invariants

Diff-buffer changes are atomic. While a selected file is being prepared, Diffo keeps
the last committed buffer and viewport unchanged. It commits the replacement's
content, projections, hunk targets, scroll bounds, and initial position together
before one draw. Rendering must never poll or install background preparation results,
and stale results must never become visible. See
[`ADR 0024`](docs/adr/0024-atomic-diff-buffer-transitions.md).

The vertical scrollbar and hunk overview are separate controls. The scrollbar owns
the inner track; hunk markers own the adjacent right-border rail. Neither control may
paint over or capture clicks intended for the other.
