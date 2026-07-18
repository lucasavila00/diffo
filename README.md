# CLI utilities

A Rust workspace for small command-line utilities.

## Utilities

- [`diffo`](crates/diffo): browse the current repository's Git state in a terminal UI.

## Development

```sh
make diffo
make diffo-mock
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
```

`make diffo` reads the current Git repository. `make diffo-mock` loads a mutable,
in-memory repository from `crates/diffo-core/fixtures/repository-state.ron`. Stage,
unstage, and stage-all work for the life of the process without changing the fixture.
It also generates large changes on demand: a 20,000-line Rust file, 5,000 JSON
items, and a 25,000-byte line. These payloads are not stored in the repo.
It also includes generated 5k, 50k, 500k, and 5,000k-line stress patches. The fixture
covers staged and unstaged changes, untracked files, recent commits, and commits that
have not been pushed.

Set `DIFFO_MOCK_FILE` directly to preview another RON fixture without changing the normal
application behavior.

For debugging, `DIFFO_DUMP_PATH=state.ron make diffo` writes one repository snapshot
and exits without opening the TUI. Diffo has no command-line arguments.

## Crate documentation

Every package under `crates/` has its own `README.md`. That file is the source of
truth for the package overview and is included verbatim as the crate-level rustdoc,
so the repository and generated API documentation cannot drift. API-specific details
remain on the Rust items they describe.

Build the workspace documentation with:

```sh
cargo doc --workspace --no-deps
```

When a crate's purpose or boundaries change, update its README rather than adding a
duplicate crate overview to `lib.rs` or `main.rs`. See
[`ADR 0051`](docs/adr/0051-crate-documentation.md) for the decision and tradeoffs.

## Keyboard controls

Diffo's keyboard shortcuts are fixed and always use lowercase characters. Uppercase
characters are never assigned as shortcuts, so no action requires holding Shift.
Non-character keys such as arrows, function keys, Enter, and Escape may still be
used where they fit the interaction.

## TUI architecture invariants

Diffo is designed for use over SSH, so terminal input and output must always be
treated as network traffic. Buttons and other controls keep a stable appearance as
the pointer moves over them: hover-only state and redraws consume network and CPU
resources for little value, particularly on slow, high-latency, or metered
connections. Mouse clicks, drags, and wheel actions remain supported. See
[`ADR 0038`](docs/adr/0038-remove-button-hover-changes.md).

Diff-buffer changes are atomic. While a selected file is being prepared, Diffo keeps
the last committed buffer and viewport unchanged. It commits the replacement's
content, projections, hunk targets, scroll bounds, and initial position together
before one draw. Rendering must never poll or install background preparation results,
and stale results must never become visible. See
[`ADR 0024`](docs/adr/0024-atomic-diff-buffer-transitions.md).

Syntax preparation is viewport-bounded but remains part of that atomic commit. A
9,999-line Rust fixture previously took about 3.45–3.56 seconds in debug and 640 ms
in release because both complete file versions were highlighted. The same cold open
now measures 72–98 ms in debug and 31 ms in release by highlighting the first visible
window with bounded parser context and parallel old/new sides. The full measurements
and tradeoffs are in [`ADR 0032`](docs/adr/0032-bounded-syntax-windows.md).

Only the active inline or side-by-side projection is built for a cold open. Switching
modes is itself an atomic prepared transition, and returning to a recently prepared
file or mode uses a four-entry cache.

Uncached vertical jumps also wait for their colored target window; the current
viewport remains unchanged until content, colors, targets, bounds, and position can
commit together. Syntax remains enabled below 10,000 file lines.

The vertical scrollbar and hunk overview are separate controls. The scrollbar owns
the inner track; hunk markers own the adjacent right-border rail. Neither control may
paint over or capture clicks intended for the other.
