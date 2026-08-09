# ADR 0004: Read and test real Git state

## Decision

`GitRepositorySource` builds a snapshot with:

- `git status --porcelain=v2 --branch -z` for files and branch state.
- `git diff` for unstaged changes.
- `git diff --cached` for staged changes.
- `git log` for recent, ahead, and behind commits.

Use NUL-delimited output when Git supports it. Parse output into snapshot types.

## End-to-end tests

The `diffo-e2e` crate uses Insta snapshot tests. It builds and runs the real
`diffo` binary. Tests:

1. Clone or create temporary repositories.
2. Make staged, unstaged, untracked, committed, and unpushed changes.
3. Run Diffo with `DIFFO_DUMP_PATH` set.
4. Compare the full snapshot with a checked-in RON snapshot.

Temporary repositories are deleted after each test. Use `cargo insta test` to
run and review snapshot changes.

`DIFFO_DUMP_PATH` makes Diffo write one RON snapshot and exit before starting
the TUI. It is only for debugging and E2E tests. Diffo still has no command-line
arguments.

## Done when

- All Git state is parsed into a snapshot.
- E2E tests cover each state listed above.
- Dumps are stable and human-readable.
