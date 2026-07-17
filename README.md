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

`make diffo` reads the current Git repository. `make diffo-mock` loads a deterministic
repository snapshot from `crates/diffo-core/fixtures/repository-state.ron`, which is
useful while developing the UI. The fixture covers staged and unstaged changes,
untracked files, recent commits, and commits that have not been pushed.

Set `DIFFO_MOCK_FILE` directly to preview another RON fixture without changing the normal
application behavior.

For debugging, `DIFFO_DUMP_PATH=state.ron make diffo` writes one repository snapshot
and exits without opening the TUI. Diffo has no command-line arguments.
