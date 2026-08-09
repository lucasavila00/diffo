# ADR 0081: Let Sync handle dirty files and missing upstreams

Refines [ADR 0070](0070-rebase-unpushed-work-when-syncing.md). Replaces the
Publish Branch flow from [ADR 0080](0080-complete-git-operation-coverage.md).

## Before

Sync stopped before fetching when:

- any staged, unstaged, or untracked file existed; or
- the current branch had no upstream.

This blocked safe pulls and pushes. A new branch also needed a separate Publish
Branch command before Sync worked.

## Now

Sync fetches first, then applies the rule for the actual branch state:

| Branch state           | Sync behavior                                                                  |
| ---------------------- | ------------------------------------------------------------------------------ |
| Local and remote equal | Finish. Keep local file changes.                                               |
| Local ahead            | Push commits. Keep local file changes.                                         |
| Remote ahead           | Try a fast-forward. Git stops it only if local files overlap.                  |
| Both ahead             | Rebase only if staged and tracked files are clean. Untracked files may remain. |

Sync never creates a stash and never force-pushes.

## Missing upstream

Sync now sets one automatically.

Remote choice:

1. Use `origin` if present.
2. Otherwise use the only remote.
3. Otherwise show a remote picker.
4. If there is no remote, stop with an error.

Sync uses the current branch name on that remote and fetches it first.

- Remote branch exists: reconcile with it, then set it as upstream.
- Remote branch does not exist: push it, then set it as upstream.
- Histories are unrelated: stop.
- Any operation fails or is cancelled: leave the upstream unset.

Pushing `main` or `master` still requires the confirmation from
[ADR 0079](0079-confirm-protected-branch-pushes.md).

Remove the separate Publish Branch command. Sync now handles first push too.

## Still blocked

Sync still stops for detached or unborn HEAD, an active
merge/rebase/cherry-pick, local merge commits that would need rebasing, rebase
conflicts, and rejected pushes.

## Result

Users can Sync with unrelated local edits. Users can also Sync a branch
immediately after a devcontainer rebuild, even when its upstream configuration
was lost.

Covered by Real-Git, state-transition, and frame-traced PTY tests. `make all`
passes.
