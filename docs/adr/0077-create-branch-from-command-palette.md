# ADR 0077: Create and check out branches from the command palette

Status: Accepted

Builds on [ADR 0037](0037-git-checkout-to.md),
[ADR 0055](0055-command-queue.md), and
[ADR 0076](0076-recent-checkout-branches.md).

## Context

Diffo can check out an existing local or remote branch, but it cannot create a new
local branch. ADR 0037 intentionally prevents checkout filter text from becoming a
Git ref, so creation needs explicit commands and a branch-name confirmation step.

VS Code provides separate `Git: Create Branch...` and
`Git: Create Branch From...` commands. The first creates from `HEAD`; the second
selects a base ref before asking for the new name. Diffo should keep the same split
while reusing its existing checkout branch picker for explicit base selection.

## Decision

Add these shared commands to every activity:

```text
Git: Create Branch...
Git: Create Branch From...
```

Neither has a key binding. `Git: Create Branch...` opens the branch-name input and
uses the committed `HEAD` as its base. `Git: Create Branch From...` first opens the
same searchable branch picker used by `Git: Checkout to...`, titled
`Create branch from`. It shows the same local and remote branches, aliases,
selection controls, and recent-commit metadata. All branches, including the current
local branch and its tracked remote, are enabled because selecting a base does not
check it out directly. Selecting a branch opens the same branch-name input; Esc or
clicking outside either step cancels without an operation.

The text-input modal is titled `Create branch` with a `Branch name` placeholder, a
visible cursor, and `Enter: create and checkout` and `Esc: cancel` help. Uppercase
and lowercase text are both valid branch-name input; this does not add an uppercase
shortcut. An empty submission closes the modal without an operation.

Use fixed VS Code-style name cleanup: trim surrounding whitespace, remove leading
hyphens, and replace whitespace and characters or components forbidden in a Git
branch name with `-`. When cleanup changes non-empty input, show
`The new branch will be <name>` and submit the displayed name, never the raw text.
Reject a cleaned name that is empty, already names a local branch, or fails the
rules enforced by `git check-ref-format --branch`. Apply the same rules in
application state so invalid input keeps the modal open and shows the validation
error beside the field. Do not add configuration, an environment hook, or a random
name generator for naming behavior.

Load branch names through the existing repository query lane. The current-`HEAD`
command shows `Loading branches...` in the name modal while it discovers duplicate
local names. The explicit-base command shows the checkout picker's existing loading
state and carries its loaded branch list into the name modal without a second query.
Give every load a query ID, ignore stale results, and drop loaded data when its modal
closes. A load failure closes the modal and shows a persistent error toast. This
discovery is not a command and has no progress or success toast.

Submitting a valid name closes the modal and enqueues one `Create branch` repository
action through `CommandQueue`. For the current-`HEAD` command, capture the committed
`HEAD` identity and object ID shown when the name modal becomes ready. For the
explicit-base command, capture the selected branch kind, full ref, and object ID.
The worker must recheck the captured start point immediately before mutation and
fail without changing the repository if it moved. An unborn `HEAD` cannot supply a
base commit for the current-`HEAD` command. Detached `HEAD` is valid there.

Create and check out the local branch from the exact captured commit in one Git
invocation, equivalent to:

```text
git checkout -q -b <name> --no-track <captured-commit>
```

Pass the name and object ID as typed arguments, never shell text. Recheck name
validity in the worker and let Git authoritatively reject a name created after the
modal's discovery. Never reset, force, stash, discard changes, create an upstream,
fetch, or push.

While queued or running, label the command `Creating branch <name>`. Allow normal
command cancellation. On success, return the created local name and complete
repository snapshot together, show `Created and checked out <name>`, and commit the
new branch, content, projections, and scroll bounds in one frame. Failure keeps the
previous committed snapshot and shows a persistent error. Successful cancellation
shows no result toast.

Put shared action and result types in `diffo-core`. Keep modal state and validation
presentation in the workbench, Git validation and mutation in `diffo-git`, and
execution on the repository worker. Rendering never runs Git.

Do not add branch publishing, branch renaming, branch deletion, tag selection, or
arbitrary commit entry in this decision.

## Consequences

Creating the usual topic branch requires one command, one name, and Enter. Creating
from another local or remote branch adds one explicit picker selection. Both flows
leave the created branch checked out and untracked.

The explicit-base flow deliberately shares checkout's branch presentation and
navigation. Changes to branch sorting, aliases, metadata, or picker controls apply
to both workflows rather than drifting between two implementations.

## Verification

- Test modal input priority, cursor editing, cleanup preview, empty input, existing
  names, Git-invalid names, detached and unborn heads, cancellation, and stale branch
  loads.
- Test that the explicit-base command reuses checkout picker selection, enables the
  current branch, preserves selected identity across refreshes, and carries the
  selected object ID into the queued action.
- Test that every activity exposes both shared commands and that the fixed
  key-binding table still rejects uppercase character shortcuts.
- Use real Git tests to prove both start-point forms create the branch at the
  captured commit, make it current without an upstream, and preserve staged,
  unstaged, and untracked changes when Git permits the checkout.
- Use real Git tests for duplicate-name races, a changed `HEAD`, and a changed
  selected branch; all must fail before mutation.
- Add a deterministic frame-traced PTY regression proving that explicit base
  selection and success install the new branch and its snapshot atomically. Use an
  explicit Git-proxy gate before mutation for cancellation and ordering; do not use
  sleeps or delay environment hooks.
- Complete repository validation with `make all`.
