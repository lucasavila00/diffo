# ADR 0037: Check out branches from Diffo

Status: Proposed

Depends on [ADR 0036](0036-git-branch-status.md) and refines
[ADR 0013](0013-command-and-file-actions.md),
[ADR 0039](0039-independent-app-modes.md), and
[ADR 0055](0055-command-queue.md).

## Context

Diffo shows the current branch and runs repository commands from every activity, but
it cannot change branches. Checkout needs two distinct choices: first the command,
then a branch from the repository. Treating text typed into the command palette as a
ref would hide available branches and would allow unvalidated input to cross the Git
boundary.

The application architecture has changed since this decision was first drafted. The
workbench now owns global overlays and the FIFO application command queue;
`diffo-repository-service` owns the serialized background repository lane; and
completed repository commands return one result with one complete snapshot. Checkout
must use those owners instead of adding an activity-specific worker, operation state,
or snapshot cache.

## Decision

Add this exact shared command to every activity's command palette:

```text
Git: Checkout to...
```

The command has no fixed key binding. Selecting it closes the command palette and
opens a workbench-owned branch picker over the active activity. Clicking the branch
status control specified by ADR 0036 opens the same picker through the same workbench
transition.

This command covers local and remote branches only. It does not cover tags, arbitrary
commits, detached checkout, new branch names, fetch, stash, force checkout, or branch
deletion. In particular, picker text is only a filter and is never sent to Git.

## Branch picker

The picker uses the prompt `Select a branch to checkout` and has these states:

```text
Closed
Loading { request_id }
Ready { request_id, catalog, query, selection, offset }
```

Opening the picker immediately shows `Loading branches...` and requests a fresh
catalog. Esc closes it in both loading and ready states. A discovery failure closes
the picker and creates a persistent workbench error toast. The picker does not retain
its catalog after closing, so every opening observes current refs.

With an empty query, render two fixed sections:

1. `Local branches`, ordered by branch name;
2. `Remote branches`, ordered by qualified name.

Display local branches as `feature/search` and remote branches as
`origin/feature/search`. Exclude symbolic remote HEAD entries such as `origin/HEAD`.
Mark the current local branch with `current`; it remains visible but is disabled. Also
disable a remote row when its conventional local branch is the current branch and
already tracks that remote. An unborn or detached HEAD does not disable another row.

Typing performs case-insensitive fuzzy filtering with the command palette's existing
scoring rules. A remote row matches both its qualified name and the name without its
remote prefix; use its better score. Break equal scores by local-before-remote section
order and then branch name. Reset selection to the first enabled result after a query
change. Section headings are not selectable, and disabled rows are skipped by
keyboard and mouse selection. Show `No matching branches` when filtering returns no
rows.

Use the established overlay controls: Up and Down move selection, Enter executes the
selected enabled row, Esc closes, Backspace edits the query, ordinary character input
extends it, the mouse selects a visible enabled row, and wheel input scrolls long
results. Uppercase text is valid input; the lowercase-only rule applies to fixed
shortcuts. Render names through the shared terminal-safe text boundary, and use the
shared dialog geometry, chrome, enabled, disabled, selection, and scrollbar styles.
Pointer movement alone does not change state or request a redraw.

The workbench owns the picker model, input priority, layout, rendering, and hit
targets because the picker is a global modal entry point. Keep this implementation in
focused branch-picker modules rather than adding it to an activity or extending the
file picker with non-file behavior. Modal input takes priority over activity switching,
palettes, toasts, and ordinary activity input.

## Catalog discovery

Add transport-neutral branch catalog and target types to `diffo-core`, and add a
read-only catalog operation to the `Repository` interface. A target preserves whether
the selected row is local or remote and carries the discovered full ref name; it is
not reconstructed from display text.

`diffo-git` reads `refs/heads/` and `refs/remotes/` with one local
`git for-each-ref` invocation. Request machine-delimited full ref names and upstream
ref names and parse those fields instead of parsing `git branch` presentation. Ref
discovery never fetches, contacts a remote, or mutates the repository.

Run discovery on `diffo-repository-service`'s existing worker lane so it is serialized
with watcher snapshots and repository commands. Discovery is modal preparation, not an
application command: it does not enter `CommandQueue`, animate command progress, or
produce a success toast. If another repository command owns the lane, the picker stays
in its responsive loading state until discovery can run.

Each opening receives a monotonically increasing picker request ID. Install a catalog
only when its ID belongs to the currently open loading picker. Closing and reopening
the picker makes the previous result stale. Repository generation remains reserved for
snapshot and command ordering; do not overload it with picker lifetime. Rendering
reads only installed picker state and never invokes Git.

## Checkout command

Selecting a row closes the picker and enqueues one explicit repository action:

```text
BranchTarget
  Local { ref_name, name }
  Remote { ref_name, remote, name }

RepositoryAction::Checkout(BranchTarget)
OperationResult::Checkout { branch }
```

The existing workbench `CommandQueue` assigns the command ID, serializes checkout
with every other application command, exposes cancellation, and projects
`Checking out <target>` as command progress. The repository service executes the
action and collects a complete `RepositorySnapshot` only after Git reports success.

For a local target, check out that local branch. For a remote target, use the remote
branch path as the conventional local name. If that local branch does not exist at
execution time, create it, configure the selected remote branch as its upstream, and
check it out. If it already exists and tracks the selected remote branch, check it out
without recreating it. If it exists with no upstream or a different upstream, fail
with an actionable error. Never reset, rename, or overwrite the existing branch.

Refs may change after discovery. Revalidate the target during execution and fail
safely when it no longer exists or no longer has the expected kind. Let Git decide
whether working-tree changes can move to the target. If checkout would overwrite
local changes, classify the failure as `DirtyWorktree`; do not stash, discard, force,
or otherwise modify those changes to make checkout succeed.

On success, the repository service returns the resulting local branch name and the
complete post-checkout snapshot in one command-completion event. The runtime applies
the existing repository generation check, the workbench acknowledges the queued
command, distributes the snapshot to every activity that consumes repository state,
and shows `Checked out <branch>` through the workbench toast queue. Branch, files,
diff projections, navigation targets, and scroll bounds become visible through the
activities' existing prepared-state commits; no surface may render the new branch
against old repository content.

On failure or cancellation, keep the last committed snapshot. Failure removes command
progress and creates the normal persistent error toast; successful cancellation creates
no result toast. A watcher result or stale catalog result must never retry a checkout,
restore an older branch, or supply a target.

## Rejected alternatives

- Put all branches in `RepositorySnapshot`. Branches are needed only while this picker
  is open and would enlarge every watcher and command snapshot.
- Use a separate branch worker. Concurrent repository reads would weaken the single-lane
  ordering that already protects commands and watcher refreshes.
- Put discovery in `CommandQueue`. Listing refs is preparation for choosing a command,
  not a user-requested repository mutation, and should not create progress or result
  toasts.
- Reuse `diffo-file-picker`. Its rows, menus, tree projection, and ownership are specific
  to files; sharing it would couple unrelated domain behavior.
- Execute `git checkout <query>`. Query text is not a selected ref and could name a tag,
  commit, path, or revision expression outside this command's scope.
- Fetch before listing. Opening a local picker must not perform network work or mutate
  refs.

## Verification

- Command-palette tests find the exact shared label in Diff, Explorer, and Search and
  prove that both the command and branch-status control open the same workbench picker.
- Deterministic picker tests cover loading, close-and-reopen staleness, discovery
  failure, section order, symbolic remote HEAD exclusion, fuzzy score ties, remote
  short-name matching, disabled current branch, empty results, scrolling, keyboard,
  mouse, uppercase input, modal priority, and terminal-safe rendering.
- Repository-service tests prove catalog requests share the serialized lane with
  refreshes and commands, use independent request IDs, and never create command queue
  or toast state.
- Real Git tests cover local checkout, remote tracking-branch creation, reuse of an
  existing matching tracking branch, conflicting local names, a ref deleted after
  discovery, detached and unborn starting states, cancellation, and dirty-worktree
  refusal without data loss.
- Prepared-state tests prove a successful checkout installs the returned branch and
  repository content together and that stale repository generations cannot restore the
  previous branch.
- A delayed PTY test opens the picker, observes loading, filters similarly named local
  and remote branches, performs checkout, and observes the new branch and file state in
  one committed frame. A second delayed PTY test proves a checkout blocked by local
  changes leaves HEAD, index, working tree, and the displayed snapshot unchanged.
