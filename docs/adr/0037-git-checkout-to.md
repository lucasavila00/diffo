# ADR 0037: Check out branches from Diffo

Status: Proposed

Depends on [ADR 0036](0036-git-branch-status.md). Refines
[ADR 0013](0013-command-and-file-actions.md),
[ADR 0039](0039-independent-app-modes.md), and
[ADR 0055](0055-command-queue.md).

## Context

Diffo shows branches but cannot change them. Users must pick a known branch. Filter
text must never become a Git ref.

## Decision

Add this command to every activity:

```text
Git: Checkout to...
```

It has no key binding. It opens the same workbench modal as the branch status control.
The picker supports local and remote branches only. No tags, commits, detached HEAD,
new names, fetch, stash, force, or delete.

Show `Loading branches...` while loading. Esc closes. Errors close and show a
persistent toast. Each load gets a new ID. Ignore stale results. Drop data on close.

Show locals first, then remotes. Sort by name. Show remotes as `origin/name`. Hide
remote HEAD refs. Disable the current branch and its tracked remote.

Use command-palette fuzzy search. Match remote full and short names. On ties, locals
win. Select the first enabled row. Show `No matching branches` when empty.

Support Up, Down, Enter, Esc, Backspace, text, click, and wheel. Uppercase text is
valid. Pointer movement does nothing. Use shared modal styles and safe text.

Read refs with one machine-delimited `git for-each-ref`. Use the existing repository
worker. Do not fetch or change the repo. Discovery is not a command. It shows no
progress or success toast.

Store the selected kind and full ref. Never use display text. Run checkout through
`CommandQueue`. Show `Checking out <target>`. Allow cancellation.

For a local target, check out that branch. For a remote target:

- No local branch: create it and set the upstream.
- Same upstream: reuse it.
- No or different upstream: fail.

Recheck the ref before checkout. Never reset, rename, overwrite, stash, discard, or
force. Map blocked local changes to `DirtyWorktree`.

On success, return the local name and full snapshot together. Show
`Checked out <branch>`. Commit branch and content in one frame.

On failure or cancellation, keep the old snapshot. Failure shows a persistent error.
Successful cancellation shows no result toast.

Put shared types in `diffo-core`. Keep picker code out of activities and
`diffo-file-picker`. Rendering never runs Git.

## Verification

Test picker behavior, stale loads, worker ordering, and no discovery toast. Use real
Git tests for checkout, conflicts, changed refs, HEAD states, cancellation, and dirty
trees. Use delayed PTY tests for atomic display and no lost work.
