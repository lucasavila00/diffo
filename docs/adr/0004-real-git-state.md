# ADR 0004: Read and test real Git state

## Decision

`GitRepositorySource` builds a snapshot with:

- `git status --porcelain=v2 --branch -z` for files and branch state.
- `git diff` for unstaged changes.
- `git diff --cached` for staged changes.
- `git log` for recent, ahead, and behind commits.

Use NUL-delimited output when Git supports it. Parse output into snapshot types.
