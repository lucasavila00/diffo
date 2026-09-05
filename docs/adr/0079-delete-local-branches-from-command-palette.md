# ADR 0079: Delete local branches from the command palette

Builds on [ADR 0037](0037-git-checkout-to.md),
[ADR 0110](0110-queue-command-intents.md),
[ADR 0076](0076-recent-checkout-branches.md), and
[ADR 0077](0077-create-branch-from-command-palette.md).

## Context

Diffo can check out and create local branches, but it cannot delete one. Branch
deletion is destructive when the branch contains commits that Git does not
consider merged, so making every deletion require the same confirmation would
obscure the important unsafe case.

[VS Code's Git command](https://github.com/microsoft/vscode/blob/6b3e01a73b1b42485e1704bd95d417a30f586717/extensions/git/src/commands.ts#L3142-L3345)
opens a local-branch picker, omits the active branch, first asks Git to perform
a safe deletion, and offers a forced deletion only when Git reports that the
branch is not fully merged. Diffo should use the same workflow while retaining
its existing branch picker, command queue, and stale-result protections.

## Decision

Add this shared command to every activity:

```text
Git: Delete Branch...
```

It has no key binding. It opens a searchable picker titled `Delete branch` with
the prompt `Select a branch to delete`. Reuse the checkout branch picker's
filtering, keyboard and pointer controls, recent-commit ordering, relative tip
ages, loading presentation, safe text rendering, and empty-result presentation.
Show local branches only and omit the current local branch rather than
displaying it disabled. In detached `HEAD`, every local branch is eligible for
the picker. Selecting one branch closes the picker; Esc or clicking outside
cancels without an operation.

Load branches through the existing repository query lane. Give every load a
query ID, ignore stale results, and discard its data when the picker closes. A
load failure closes the picker and shows the shared acknowledgement modal.
Discovery is not a command and has no progress or success toast.

Store the selected branch's name, full `refs/heads/...` ref, and object ID,
never its display text. Selecting a branch enqueues one non-forced
`Delete branch` repository action through `CommandQueue`, labeled
`Deleting branch <name>`. The worker must recheck immediately before mutation
that the full ref still points to the selected object ID and that it is not the
current branch. A moved, missing, or newly current ref fails without deleting a
branch.

Attempt the safe deletion first, equivalent to:

```text
git branch -d -- <name>
```

Pass the branch name as a typed argument, never shell text. Let Git decide
whether the branch is fully merged into its configured upstream, or into `HEAD`
when it has no upstream. Do not precompute or cache that decision. Let Git also
authoritatively reject a branch checked out in another worktree.

When safe deletion fails specifically because the branch is not fully merged, do
not show the normal failure toast. Open a warning modal with:

```text
The branch "<name>" is not fully merged. Delete anyway?
```

Offer `Cancel` and `Delete branch`, with `Cancel` selected by default. Enter
activates the selected choice and Esc cancels. Cancelling closes the modal
without another command or toast. Confirming enqueues a forced `Delete branch`
action for the same captured full ref and object ID, equivalent to:

```text
git branch -D -- <name>
```

Recheck the captured identity and current branch again immediately before the
forced mutation. If either changed while the warning was open, fail rather than
deleting the replacement ref. Any safe-delete failure other than the
not-fully-merged condition uses the normal persistent error path and never
offers force.

Both actions use normal command cancellation. On success, return the deleted
local branch name and a complete repository snapshot together, show
`Deleted branch <name>`, and install the snapshot in one frame. Failure keeps
the previous committed snapshot and shows a persistent error. Successful
cancellation shows no result toast.

Put shared target, action, result, and failure types in `diffo-core`. Keep
picker and warning state in the workbench, Git failure classification and
mutation in `diffo-git`, and execution on the repository worker. Rendering never
runs Git.

Do not add remote-branch deletion, bulk deletion, deletion of the active branch,
automatic checkout, branch protection configuration, CLI arguments, environment
hooks, or a confirmation before an ordinary safe deletion.

## Consequences

Deleting a merged local branch takes one command and one selection. A branch
with unmerged commits requires a separate destructive confirmation, matching VS
Code's distinction between safe and forced deletion.

The picker remains consistent with checkout and create-from presentation, but
current branches differ intentionally: checkout displays the current branch
disabled, while delete omits it because it can never be a valid target.

Remote-tracking and remote-server branches remain unchanged. Deleting a local
branch does not delete, prune, or push anything on a remote.
