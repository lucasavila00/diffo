# ADR 0003: Structured mock fixtures

## Decision

Mock state uses checked-in RON files that deserialize into `RepositorySnapshot`.

A fixture can contain:

- Staged and unstaged changes.
- Untracked files.
- Recent commits.
- Ahead and behind commits.

`make diffo-mock` loads a fixture through `FixtureRepositorySource`.

Raw `.diff` files are only for diff parser tests. They cannot hold full
repository state.
