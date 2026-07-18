# ADR 0037: Check out branches from Diffo

Status: Proposed

Depends on [ADR 0036](0036-git-branch-status.md). Refines
[ADR 0013](0013-command-and-file-actions.md),
[ADR 0039](0039-independent-app-modes.md), and
[ADR 0055](0055-command-queue.md).

## Context

Diffo shows the current branch. It cannot change branches.

Checkout needs two choices: the command, then a known branch. Typed filter text is
not a Git ref.

The workbench owns overlays and the command queue. `diffo-repository-service` owns
the single repository worker. Checkout must use them. Do not add an activity worker,
operation state, or snapshot cache.

## Decision

Add this command to every activity:

```text
Git: Checkout to...
```

It has no key binding. It closes the command palette and opens a workbench branch
picker. The branch status control opens the same picker.

The picker lists local and remote branches only. No tags, commits, detached checkout,
new names, fetch, stash, force, or delete. Filter text never goes to Git.

## Branch picker

Prompt: `Select a branch to checkout`.

States:

```text
Closed
Loading { request_id }
Ready { request_id, catalog, query, selection, offset }
```

Open shows `Loading branches...` and loads a fresh catalog. Esc closes the picker.
Failure closes it and shows a persistent workbench error toast. Closing drops the
catalog.

An empty query shows:

1. `Local branches`, sorted by branch name.
2. `Remote branches`, sorted by qualified name.

Show local names as `feature/search`. Show remote names as
`origin/feature/search`. Hide symbolic remote HEAD refs such as `origin/HEAD`.

Mark the current local branch `current` and disable it. Also disable a remote branch
when the current local branch already tracks it. Detached or unborn HEAD disables
nothing else.

Use the command palette's case-insensitive fuzzy match. Match a remote branch by its
full or short name. Use the better score. Break ties by local before remote, then
branch name. A query change selects the first enabled result. Headings and disabled
rows cannot be selected. Show `No matching branches` when empty.

Use normal overlay controls: Up, Down, Enter, Esc, Backspace, text input, mouse click,
and wheel. Uppercase text is valid input. Render names with the terminal-safe text
boundary. Use shared dialog and scrollbar styles. Pointer movement alone does
nothing.

The workbench owns picker state, input, layout, rendering, and hit targets. Put this
in branch-picker modules. Do not put it in an activity or the file picker. Picker
input wins over activity switching, palettes, toasts, and activity input.

## Catalog discovery

Add branch catalog and target types to `diffo-core`. Add a read-only catalog operation
to `Repository`. A target stores its local or remote kind and full discovered ref. Do
not rebuild it from display text.

`diffo-git` runs one local `git for-each-ref` over `refs/heads/` and
`refs/remotes/`. Request and parse machine-delimited full ref and upstream ref fields.
Do not parse `git branch` output. Do not fetch, contact a remote, or change the repo.

Run discovery on the existing repository-service worker. It waits behind other work.
The picker stays responsive while loading. Discovery does not enter `CommandQueue`,
show progress, or show a success toast.

Each open gets a rising request ID. Install a result only when that ID still belongs
to the open loading picker. A close or reopen makes old results stale. Do not use
repository generation for picker lifetime. Rendering never runs Git.

## Checkout command

Selecting a row closes the picker and queues one action:

```text
BranchTarget
  Local { ref_name, name }
  Remote { ref_name, remote, name }

RepositoryAction::Checkout(BranchTarget)
OperationResult::Checkout { branch }
```

`CommandQueue` assigns the ID, serializes the action, supports cancellation, and
shows `Checking out <target>`. The repository service runs it. It builds a full
snapshot only after Git succeeds.

For a local target, check out that branch.

For a remote target, use the remote branch path as the local name:

- If no local branch exists, create it, set the selected upstream, and check it out.
- If it exists and tracks that remote branch, check it out.
- If it has no upstream or a different one, fail with a useful error.

Never reset, rename, or overwrite an existing branch.

Refs can change after discovery. Recheck the target before checkout. Fail if it is
gone or changed kind. Let Git decide if working-tree changes can move. Map overwrite
refusal to `DirtyWorktree`. Never stash, discard, or force.

On success, return the local branch name and full post-checkout snapshot in one event.
Apply the repository generation check. Acknowledge the queued command. Send the
snapshot to every repository activity. Show `Checked out <branch>`. Commit branch,
files, diffs, navigation, and scroll state through existing prepared-state commits.
Never show a new branch with old content.

On failure or cancellation, keep the last committed snapshot. Failure removes
progress and shows the normal persistent error toast. Successful cancellation shows
no result toast. Stale catalog or watcher results never retry checkout or change its
target.

## Rejected alternatives

- Store branches in `RepositorySnapshot`: every snapshot would grow for picker-only
  data.
- Add a branch worker: it would break single-lane ordering.
- Put discovery in `CommandQueue`: discovery is preparation, not a mutation.
- Reuse `diffo-file-picker`: its model is for files.
- Run `git checkout <query>`: filter text may name unsupported Git objects.
- Fetch first: opening the picker must not use the network or change refs.

## Verification

- Palette tests find the exact command in Diff, Explorer, and Search. The command and
  branch status control open the same picker.
- Picker tests cover loading, stale results, failure, sorting, remote HEAD exclusion,
  fuzzy ties, remote short names, disabled rows, empty results, scrolling, keyboard,
  mouse, uppercase input, modal priority, and safe text rendering.
- Repository-service tests prove discovery shares the worker lane, uses separate
  request IDs, and creates no command or toast state.
- Real Git tests cover local checkout, remote branch creation and reuse, name conflict,
  deleted refs, detached and unborn HEAD, cancellation, and dirty-tree refusal without
  data loss.
- Prepared-state tests prove branch and content install together. Stale generations
  cannot restore the old branch.
- Delayed PTY tests cover loading, filtering similar local and remote names, atomic
  checkout display, and dirty-tree refusal with no repository or display change.
