# ADR 0101: Merge branches and tags

Builds on [ADR 0037](0037-git-checkout-to.md) and
[ADR 0110](0110-queue-command-intents.md). Replaces the local-merge exclusion in
[ADR 0080](0080-complete-git-operation-coverage.md).

## Context

Diffo already manages branches and displays merge conflicts, but users must
leave the application to start or abort a merge.

[VS Code](https://github.com/microsoft/vscode/blob/18e5dfaf6ec3c285da350423457d73e3f1f175bb/extensions/git/src/commands.ts#L3096-L3124)
offers one Merge command for local branches, remote-tracking branches, and tags,
plus Abort Merge when a merge is in progress. Diffo should follow that model.

## Decision

Add these shared commands with no key bindings:

```text
Git: Merge...
Git: Abort Merge
```

`Git: Merge...` opens the existing branch picker with the prompt
`Select a branch or tag to merge from`. Show local branches, remote-tracking
branches, and tags. Omit the current branch, its tracked remote, and remote HEAD
refs. Reuse the existing filtering, ordering, loading, and stale-result
behavior.

Store the selected full ref and object ID, not its label. Also capture `HEAD`.
Recheck both immediately before running the merge so a moved ref or changed
destination fails without mutation.

Run the merge through `CommandQueue`, equivalent to:

```text
git merge --no-edit <selected-full-ref>
```

Use Git's normal fast-forward or merge-commit behavior. Do not fetch, stash,
force, squash, rebase, choose a strategy, or ask for a merge message.

On success, install the new repository snapshot atomically and show
`Merged <name>`. If Git reports conflicts, keep and install the conflicted state
instead of treating it as an ordinary failure. The user resolves and stages
files, then finishes through the existing Commit control.

Show `Git: Abort Merge` only while `MERGE_HEAD` exists, including for merges
started outside Diffo. Run `git merge --abort` through `CommandQueue`, install
the resulting snapshot, and show `Merge aborted`. A failed abort leaves the
actual repository state visible and uses the shared error modal.

Always install the state Git actually left behind after a conflict, failure, or
late cancellation. Cancellation must never imply an automatic abort.

Keep shared types in `diffo-core`, Git work in `diffo-git`, picker state in the
workbench, and execution on the repository worker. Do not add CLI options,
configuration, environment hooks, or character shortcuts.

## Consequences

Users can merge without leaving Diffo. Clean merges finish immediately;
conflicted merges use the existing review, staging, and commit workflow and can
be explicitly aborted.

Sync keeps its current policy and still refuses to rebase local merge commits.
