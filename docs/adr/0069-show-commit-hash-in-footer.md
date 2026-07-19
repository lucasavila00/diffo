# ADR 0069: Show the commit hash in the footer

Status: Accepted

Builds on [ADR 0036](0036-git-branch-status.md).

## Problem

The repository footer identifies a named branch but not the exact revision checked
out on that branch. Branch names can point to different commits across repositories
and can move after refreshes and Git operations, so the branch alone is insufficient
when a user needs to confirm the revision being reviewed.

Detached HEAD already displays an abbreviated commit hash. Named HEAD snapshots also
contain the commit hash, but the footer discards it.

## Decision

Display the seven-character abbreviated `HEAD` commit hash after every named branch:

```text
 branch main · a1b2c3d · clean
```

Use the commit stored in the same immutable `RepositorySnapshot` as the branch and
file state. Do not run Git from the renderer or introduce another cache. This keeps
the branch and hash atomic across refreshes and repository operations.

Keep detached HEAD's existing seven-character display unchanged. An unborn branch
has no commit, so continue to display `(unborn)` without a hash.

Treat the branch and hash as one head label for styling, mouse interaction, and
narrow-width truncation. The existing footer priority remains unchanged: omit
divergence, repository state, command help, and transient detail before truncating
the head label. At widths too narrow for the complete label, truncate the combined
branch-and-hash text with an ellipsis.

## Alternatives

- Show the full commit hash. Rejected because it consumes too much of the footer and
  competes with repository state, operation feedback, and command help.
- Show the hash only for detached HEAD. Rejected because it leaves named branches
  unable to identify the exact checked-out revision.
- Query `git rev-parse` while rendering. Rejected because rendering must consume
  committed state only and must not perform blocking external work.

## Acceptance

- A named HEAD displays its branch and seven-character commit hash in every activity.
- Detached and unborn HEAD displays remain distinct and correct.
- Branch and hash come from one committed repository snapshot.
- Narrow footers remain bounded and truncate the combined head label safely,
  including branch names containing wide Unicode characters.
