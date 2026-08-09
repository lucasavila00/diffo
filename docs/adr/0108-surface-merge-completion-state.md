# ADR 0108: Show unfinished merges

Status: Accepted

Refines [ADR 0036](0036-git-branch-status.md),
[ADR 0082](0082-unpushed-commits-panel.md), and
[ADR 0101](0101-merge-branches-and-tags.md).

## Problem

After all merge conflicts are resolved and staged, Git is still merging until the
merge commit is created. Diffo already knows this because `MERGE_HEAD` appears as
`RepositoryOperationState::Merge`, but the UI can show `No unpushed commits` and
look as if there is nothing left to do.

The Commit control can also be enabled while conflicts remain, or disabled when a
valid merge resolution has no staged diff.

## Decision

Keep an in-progress merge visible until `MERGE_HEAD` disappears.

- Show `merge conflicts` in the footer while conflicted files remain.
- Show `merge ready` after every conflict is staged.
- Replace the Unpushed panel with a Merge panel that says either
  `Resolve and stage N conflicted files` or `All conflicts resolved · complete the merge`.
- Rename the Commit control to `Complete merge` during a merge.
- Disable it while conflicts remain. Enable it when the merge is ready, even when
  there is no staged diff.
- Keep a typed commit message. When the field is empty, use `Complete merge`.

Use the existing `RepositoryAction::Commit`; Git will create the merge commit from
`MERGE_HEAD`. Do not add another Git command or count the unfinished merge as an
unpushed commit.

Derive all of this from one committed `RepositorySnapshot`. Completion, abort, and
refresh must replace the merge status, files, and Unpushed panel atomically.

## Verification

Test unresolved, resolved, no-staged-diff, completed, aborted, failed, and externally
started merges. Completing a ready merge must create a two-parent commit and remove
`MERGE_HEAD`. Finish with `make all`.
