# ADR 0004: Read and test real Git state

Status: Accepted

## Decision

`GitRepositorySource` builds a snapshot with:

- `git status --porcelain=v2 --branch -z` for files and branch state.
- `git diff` for unstaged changes.
- `git diff --cached` for staged changes.
- `git log` for recent, ahead, and behind commits.

Use NUL-delimited output when Git supports it. Parse output into snapshot types.

## End-to-end scripts

Scripts will:

1. Clone or create temporary repositories.
2. Make staged, unstaged, untracked, committed, and unpushed changes.
3. Run Diffo's collector.
4. Dump the snapshot.
5. Compare the dump with expected state.

The scripts must clean up their temporary repositories. Dumps may also become mock
fixtures.

## Done when

- All Git state is parsed into a snapshot.
- E2E scripts cover each state listed above.
- Dumps are stable and human-readable.
