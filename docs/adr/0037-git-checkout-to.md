# ADR 0037: Add `Git: Checkout to...`

Status: Proposed

Depends on [ADR 0036](0036-git-branch-status.md).

## Problem

Diffo's command palette can run an action immediately, but checkout needs another
piece of user input: the target branch. A plain text prompt would require exact
branch names, hide useful candidates, and make local and remote branches difficult
to distinguish.

VS Code exposes this workflow as `Git: Checkout to...` and follows the command with
a searchable branch picker. Diffo should use the same palette name and interaction
model so that the command is familiar and remains usable in repositories with many
branches.

## Scope

Add exactly this command label to the command palette:

```text
Git: Checkout to...
```

This decision covers named local and remote branches. It does not add tags,
detached checkout, branch creation, automatic fetch, stash, force checkout, or
configurable sorting. Those require separate commands or decisions. In particular,
do not add VS Code's `Git: Checkout to (Detached)...` or branch-creation suggestions
as hidden behavior of this command.

## Interaction

Selecting the command replaces the command palette with a branch picker in the same
overlay position:

```text
Command palette
  └─ Git: Checkout to...
       └─ Branch picker: Loading
            ├─ load failed → close picker and show error
            └─ Ready(query, matches, selection)
                 ├─ Esc → close without an action
                 └─ Enter/click → checkout selected target
```

Use the prompt `Select a branch to checkout`. Show a loading row immediately and
collect refs outside the input and rendering loop. The picker remains responsive to
Esc while refs are loading.

When ready, show these fixed sections for an empty query:

1. `Suggestions`: branches checked out successfully earlier in this Diffo process,
   most recent first, followed by the most recently committed local branches;
2. `Local branches`: all local branches, alphabetically;
3. `Remote branches`: all remote branches, alphabetically by qualified name.

Deduplicate suggestions from the later sections visually, but keep every branch
searchable. Exclude symbolic remote HEAD refs such as `origin/HEAD`. Mark the current
branch as `current` and disable it instead of turning it into a no-op action. Show a
local branch as `feature/search` and a remote branch as `origin/feature/search`.
Include its seven-character commit and upstream, when present, as secondary text.

Typing performs case-insensitive fuzzy matching over both the displayed qualified
name and, for remote refs, the branch name without the remote prefix. Prefer exact
prefixes, then consecutive matches at path-component boundaries, then other
subsequence matches. Break equal scores by section priority and alphabetical name
so results never jump between frames. Separators do not participate in selection.
Reset selection to the first enabled match whenever the query changes. Show
`No matching branches` for an empty result; never treat arbitrary input as a ref to
execute.

The picker accepts normal text, including uppercase characters, because the
lowercase-only rule applies to fixed shortcuts rather than user input. Arrow keys,
Enter, Esc, Backspace, and mouse selection behave like the existing command
palette. Query edits and selection changes are pure application state transitions.

## Ref discovery

Read local and remote refs with one bounded-background Git command based on
`git for-each-ref`, requesting full ref name, object id, commit time, and upstream.
Parse machine-selected fields instead of display-formatted `git branch` output.
Branch discovery reads only local refs and never contacts a remote.

Give each request a generation. Install a catalog only when its generation still
belongs to the open picker. Closing and reopening the picker, repository refresh,
or shutdown makes an older result stale. Rendering consumes only the installed
catalog and only formats the visible rows.

Session suggestions live in application memory. Do not create a history or
configuration file. A passive repository refresh invalidates the catalog so the
next invocation sees externally created or deleted branches.

## Checkout effect

Use an explicit target rather than passing picker text through to Git:

```text
BranchTarget
  Local { name }
  Remote { remote, name }

RepositoryAction::Checkout(BranchTarget)
OperationResult::Checkout { branch }
```

Run checkout through the existing repository worker. Disable branch and other
repository actions while it is pending, keep input and Ctrl+C responsive, and
collect a complete snapshot after Git succeeds.

For a local target, check out that local branch. For a remote target, check out an
existing local branch that tracks the selected remote branch. If none exists,
create a local branch with the remote branch's short name and configure it to track
the selected remote. If that local name already exists but tracks something else,
fail with an actionable error instead of selecting, resetting, or overwriting it.

Let Git reject a checkout that would overwrite local changes. Do not automatically
stash, migrate, discard, or force. Report a structured `DirtyWorktree` failure and
leave both HEAD and the working tree unchanged. Missing or externally deleted refs
also fail without changing the committed snapshot.

On success, close the picker, add the resulting local branch to the in-process
suggestion history, and atomically install the returned snapshot. Show
`Checked out <branch>` as the operation toast. On failure, close the picker, keep the
previous snapshot, and show `Checkout failed: <safe reason>`.

## Alternatives

- Reuse the command palette query as the branch query. Rejected because command and
  target are separate choices with different data, loading, and empty states.
- Put every branch in `RepositorySnapshot`. Rejected because the diff UI does not
  need the catalog on every watcher refresh.
- Run `git checkout <typed text>`. Rejected because it bypasses discovery, can select
  a non-branch ref, and cannot provide stable suggestions or remote semantics.
- Fetch before listing branches. Rejected because opening a local picker must not
  perform network work or mutate refs.

## Acceptance

- The palette contains the exact, case-sensitive label `Git: Checkout to...`, and
  its fuzzy search can find it.
- Deterministic model tests cover loading, stale results, filtering, score ties,
  section order, disabled current branch, empty matches, keyboard selection, mouse
  selection, and Esc.
- Git tests cover local checkout, an existing tracking branch, creation of a new
  tracking branch, ambiguous local names, a ref deleted after discovery, detached
  and unborn starting states, and dirty-worktree refusal without data loss.
- A successful result installs branch, files, projections, navigation targets, and
  scroll bounds as one prepared transition. Stale snapshot or catalog results never
  restore a prior branch or drive a checkout.
- A delayed PTY test opens the command, observes loading, searches among similarly
  named local and remote branches, checks one out, and sees the new branch and its
  file state appear together.
- A second delayed PTY test attempts a checkout blocked by local changes and proves
  that HEAD, index, working tree, and displayed snapshot remain unchanged.
