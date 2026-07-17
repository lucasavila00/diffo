# CLI utilities

A Rust workspace for small command-line utilities.

## Utilities

- [`git-diff-tui`](crates/git-diff-tui): browse the current repository's Git diff in a terminal UI.

## Development

```sh
make diffo
make diffo-mock
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
```

`make diffo` reads the current Git repository. `make diffo-mock` loads a deterministic
repository snapshot from `crates/git-diff-tui/fixtures/repository-state.ron`, which is
useful while developing the UI. The fixture covers staged and unstaged changes,
untracked files, recent commits, and commits that have not been pushed.

Set `DIFFO_MOCK_FILE` directly to preview another RON fixture without changing the normal
application behavior.
