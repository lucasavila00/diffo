# ADR 0080: Complete the everyday Git operation loop

Status: Accepted

Builds on [ADR 0037](0037-git-checkout-to.md),
[ADR 0070](0070-rebase-unpushed-work-when-syncing.md),
[ADR 0077](0077-create-branch-from-command-palette.md), and
[ADR 0079](0079-delete-local-branches-from-command-palette.md).

## Context

"Complete Git coverage" cannot mean exposing every Git command. Git also includes
repository creation, server administration, patch transport, history surgery,
submodules, worktrees, and plumbing. Turning Diffo into a command launcher for all of
them would remove its fixed product behavior and move important policy back into
arguments and Git configuration.

Diffo's intended workflow is narrower: inspect a repository, prepare and correct
local work, move between topic branches, publish a branch, and reconcile it with its
upstream. Coverage is complete when that loop has no routine state that requires a
different Git client.

The current operation set already includes:

- stage and unstage one file or all files;
- commit staged changes;
- fetch;
- sync, which always fetches and then fast-forwards, rebases local-only commits, or
  pushes according to [ADR 0070](0070-rebase-unpushed-work-when-syncing.md);
- check out local and remote branches;
- create a branch from `HEAD` or another branch; and
- safely or forcibly delete a local branch under
  [ADR 0079](0079-delete-local-branches-from-command-palette.md).

Fetch is therefore not missing. Pull is absent deliberately. Git
[documents pull](https://git-scm.com/docs/git-pull) as fetch followed by a configurable
integration choice. Diffo's Sync keeps those steps separate so it can inspect the
fetched tips and apply one fixed policy. Adding `Git: Pull` beside Sync would provide
a second, configuration-dependent route to the same state and would contradict
ADR 0070.

The real gaps are abandoning or shelving work, correcting the latest local commit,
reversing published history safely, renaming a branch, and publishing a newly created
branch. In particular, Diffo currently creates an untracked branch that Sync then
rejects because it has no upstream.

## Decision

Define complete operation coverage as the following matrix. Existing rows remain.
Every missing row is required before Diffo claims complete everyday Git coverage.

| Area | Required outcome | Product action | Coverage |
| --- | --- | --- | --- |
| Inspect | Refresh status, diffs, commits, and branches | Automatic repository refresh | Existing |
| Index | Stage or unstage one file or all files | Existing file and group actions | Existing |
| Worktree | Abandon unstaged work in one file | `Discard Changes` file action | Missing |
| Worktree | Abandon all unstaged and untracked work | `Git: Discard All Changes...` | Missing |
| Shelf | Put tracked and untracked work aside | `Git: Stash Changes...` | Missing |
| Shelf | Restore a saved state without deleting it | `Git: Apply Stash...` | Missing |
| Shelf | Delete a saved state | `Git: Drop Stash...` | Missing |
| Commit | Create a commit | Existing Commit control | Existing |
| Commit | Replace the latest unpublished commit | `Git: Amend Last Commit...` | Missing |
| Commit | Remove the latest unpublished commit but keep its changes staged | `Git: Undo Last Commit...` | Missing |
| Commit | Reverse one published non-merge commit with a new commit | `Git: Revert Commit...` | Missing |
| Branch | Check out an existing branch | `Git: Checkout to...` | Existing |
| Branch | Create and check out a branch | Existing Create Branch commands | Existing |
| Branch | Rename the current local branch | `Git: Rename Branch...` | Missing |
| Branch | Delete a local branch | `Git: Delete Branch...` | Existing |
| Remote | Refresh remote-tracking refs without integrating | `Git: Fetch` | Existing |
| Remote | Create the current branch on a remote and set its upstream | `Git: Publish Branch...` | Missing |
| Remote | Reconcile and publish a tracked branch | Existing Sync control and `Git: Sync` | Existing |

These are product outcomes, not promises to invoke a same-named Git porcelain command.
Diffo may compose lower-level Git operations when that is necessary to preserve the
fixed behavior and atomic snapshot rules.

## Required behavior

### Discard

Discarding one tracked path restores only its unstaged worktree content from the
index. It does not unstage the path. Discarding one untracked path deletes it. Both
forms require a warning that names the exact path and has Cancel selected first.

`Git: Discard All Changes...` restores all tracked worktree content from the index and
deletes all untracked, non-ignored paths. It preserves the index, so abandoning staged
work remains the explicit sequence Unstage All, then Discard All. It never removes
ignored files. The confirmation states the tracked and untracked path counts and has
Cancel selected first.

Do not provide hunk-level discard in this coverage decision. File and repository
scope close the workflow without adding a patch editor.

### Stash

`Git: Stash Changes...` saves the index, tracked worktree changes, and untracked files,
then leaves a clean worktree. Ignored files are never included. An optional message is
the only input; stash selection and behavior do not come from Git configuration.

`Git: Apply Stash...` opens a searchable picker and applies one captured stash object,
including its saved index state. Apply keeps the stash entry even on success. If it
conflicts, keep the stash entry, install the resulting conflicted snapshot, and tell
the user that nothing was dropped.

`Git: Drop Stash...` uses the same picker and deletes one captured stash entry only
after confirmation. Recheck the selected stash object immediately before applying or
dropping it. A changed or missing entry fails instead of acting on the stash that now
occupies the old display index. Do not add Pop: Apply followed by an explicit Drop is
the same successful outcome without coupling restoration and deletion.

### Correct and reverse commits

`Git: Amend Last Commit...` opens the commit editor with the current `HEAD` message.
It may replace the message, include currently staged changes, or do both. Enable it
only when `HEAD` is a non-merge commit that is local-only relative to the configured
upstream, or when the branch has no upstream. With no authoritative remote
destination, Diffo treats the branch as unpublished; a later Publish still rejects a
non-fast-forward destination instead of forcing it. Amend never targets an arbitrary
older commit.

`Git: Undo Last Commit...` moves the current local branch to the first parent of
`HEAD` and leaves the removed commit's tree changes staged. It has a confirmation with
Cancel selected first. Apply the same unpublished, non-merge, and ref-identity checks
as Amend. Never use a hard or mixed reset for this action.

`Git: Revert Commit...` picks one non-merge commit reachable from the current branch
and creates a new commit that reverses it without rewriting existing history. Require
a clean index and worktree. Use Git's existing commit message without opening an
editor. If the revert conflicts, abort it automatically and leave the pre-operation
branch and worktree unchanged. Merge-commit mainline selection and multi-commit
reverts are outside this decision.

The local-only check for Amend and Undo must use commit reachability, not the displayed
ahead count. Neither action ever enables a force-push path.

### Rename and publish branches

`Git: Rename Branch...` renames only the current local branch. Use the same fixed name
cleanup and validation as Create Branch. Capture and recheck `HEAD` and the current
full ref before mutation. Never overwrite an existing ref.

Renaming an unpublished branch preserves its no-upstream state. Renaming a tracked
branch clears its upstream after confirmation and does not rename or delete any
remote branch. The result therefore cannot make a later Sync silently push the new
local name to the old remote name. The user may publish the renamed branch explicitly.

`Git: Publish Branch...` is available only on an existing local branch with no
upstream. If the repository has one remote, show that remote in the confirmation. If
it has more than one, open a searchable remote picker. With no remotes, fail without
mutation and explain that Diffo does not create remotes.

Publish performs one normal push from the captured local tip to a same-named branch
on the selected remote and sets that branch as the upstream only after the push
succeeds. It never overwrites a non-fast-forward remote ref. Apply the protected
`main` and `master` confirmation from
[ADR 0079](0079-confirm-protected-branch-pushes.md) to the destination. A failed or
cancelled publish leaves local branch configuration unchanged.

Do not add a standalone Push action. Sync is the push path for tracked branches;
Publish is the one-time path that creates the upstream required by Sync.

## Shared operation rules

Every added action follows the existing command architecture:

- expose fixed commands through the shared command palette and add no new public CLI
  arguments, environment controls, configuration, or character shortcuts;
- pass paths, refs, object IDs, messages, and remote names as typed process arguments,
  never shell text or display labels;
- discover picker data on the repository query lane, identify it by immutable Git
  object or ref identity, and reject stale selections;
- serialize mutations through `CommandQueue`, support cancellation where Git can be
  stopped safely, and keep input and rendering responsive;
- install each successful result and its complete repository snapshot atomically;
- keep the previously committed snapshot on a failure that makes no repository
  change, and install the resulting snapshot when Git reports a partial state such as
  a conflicted stash apply;
- use persistent errors and explicit confirmations for data loss or history changes;
  confirmations always select Cancel first; and
- never force-push, delete a remote ref, overwrite an existing branch, or discard
  ignored files.

Detailed interaction or failure policy that does not fit these rules requires a
follow-up ADR before that operation is implemented. Operations may land separately,
but the coverage claim remains incomplete until every Missing row is implemented and
verified.

## Boundaries

Diffo does not add `Git: Pull`. Sync already supplies the fast-forward and rebase
integration outcomes with a fixed, visible plan. Diffo also does not add standalone
Push for tracked branches.

This coverage boundary excludes clone, init, remote creation or editing, remote-branch
deletion, force-push, tags, notes, submodules, worktrees, sparse checkout, Git LFS,
bisect, blame, patch and email workflows, reflog editing, arbitrary reset, interactive
rebase, cherry-pick, local merge, general rebase, and sequencer Continue or Abort
commands. Diffo's branch-and-pull-request workflow does not require those operations.
They remain available through Git itself and need a new ADR to enter the product.

External merge, rebase, cherry-pick, or revert states remain visible as repository
state. Diffo may stage conflict resolutions, but it does not own those externally
started workflows.

## Consequences

The operation list has a testable end. A user can review changes, keep or abandon
them, temporarily move them aside, create and correct commits, work on local branches,
publish a topic branch, and synchronize it without leaving Diffo.

The command palette does not mirror Git's command list. Pull and Push remain product
outcomes expressed by Sync, while destructive or expert workflows stay explicit
non-goals.

The largest new safety surface is local data removal. Discard, Drop Stash, Undo Last
Commit, and tracked-branch rename therefore require dedicated deterministic state
tests and real-Git tests before implementation is complete.

## Verification

- Keep one coverage test that enumerates every command in the matrix and proves it is
  available only in valid repository states.
- Use real Git repositories to prove Discard preserves the index and ignored files,
  while Stash preserves index state and never drops an entry during Apply.
- Prove Amend and Undo reject published, merge, moved, detached, and unborn `HEAD`
  states and never make a force-push necessary.
- Prove Revert creates a new commit, and a conflicting revert restores the exact
  pre-operation branch, index, and worktree.
- Prove Rename never overwrites a ref or changes a remote ref, and clears a tracked
  branch's upstream only after confirmation.
- Prove Publish uses the captured tip, rejects a non-fast-forward destination, sets
  upstream only after success, and applies protected-branch confirmation.
- Add deterministic state-transition and frame-traced PTY regressions for every
  asynchronous mutation and atomic snapshot installation. Do not use sleeps or delay
  environment hooks.
- Keep tests that reject uppercase character entries in the fixed key-binding table.
- Complete every implementation change with `make all`.
