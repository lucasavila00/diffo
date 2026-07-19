# ADR 0070: Rebase local-only commits during sync

Status: Accepted

## Problem

A local branch and its upstream remote-tracking ref are two refs. For example, `main`
is a local branch and `origin/main` is the last fetched position of its upstream
branch. The names look related, but their tips can differ.

When the local and remote branches both move, push is rejected. This is correct. Push
must not erase remote work.

Git then makes the user choose merge, rebase, or stop. Pull hides this choice inside
one command. Git configuration can change it. Merge can open a commit-message editor.

This is too much policy for a normal sync.

Most changes do not conflict. Different files combine. Different parts of one file
often combine. Git knows how to do this.

Diffo should handle the normal case. It should use one rule. It should tell the user
that rule before changing local history.

## Terminology

**Sync** is the single Diffo action that reconciles the current local branch with its
configured upstream and, when needed, publishes local commits. Use **sync** everywhere
the product names this complete action. A sync always fetches. It may then
fast-forward, rebase, push, or finish without another Git operation.

**Fetch**, **fast-forward**, **rebase**, and **push** name the individual Git operations
within a sync. Use those terms when describing a plan, progress, or result.

**Pull** names Git's combined fetch-and-integrate command. It is not another name for
Diffo's sync action, and this workflow does not run `git pull`.

The **local branch** is the checked-out branch, such as `main`. Its **upstream ref** is
the local remote-tracking ref configured for that branch, such as `origin/main`. A
fetch updates the upstream ref from the corresponding branch on the remote.

A **local-only commit** is reachable from the local branch but not from its upstream
ref. An **upstream-only commit** is reachable from the upstream ref but not from the
local branch.

## Decision

Sync has fixed behavior.

### Sync algorithm

Every sync runs this algorithm:

1. Check the preconditions in [Stop cases](#stop-cases). Stop before fetch if the
   current repository state is unsupported.
2. Fetch from the remote that owns the configured upstream ref. This is required even
   when the previously displayed ahead and behind counts are zero.
3. Read the local branch tip and the refreshed upstream tip.
4. Compute the local-only and upstream-only commit sets from those tips.
5. Select exactly one plan from the table below and show it to the user.
6. Run that plan. Never substitute merge for rebase and never force-push.

The fetch in step 2 is part of every row. The table states what happens after that
fetch:

| Local-only commits | Upstream-only commits | Local effect after fetch | Remote effect after fetch |
| --- | --- | --- | --- |
| None | None | None. The branches already have the same tip. | None. |
| None | One or more | Fast-forward the local branch to the upstream tip. | None. |
| One or more | None | None. | Push the local tip normally, advancing the upstream branch. |
| One or more | One or more | Rebase the local-only commits onto the upstream tip, giving those commits new IDs. | Push the rebased tip normally, advancing the upstream branch. |

The second row is the case a user may informally call “just pull.” If the remote does
not move during the operation, fetch followed by fast-forward produces the same final
branch tip as `git pull --ff-only`.

Diffo does not run `git pull`, because pull combines two algorithm steps that sync
must keep separate. Sync first fetches so it can inspect the refreshed tips, select a
plan, and show that exact plan to the user. It then fast-forwards to the upstream tip
it inspected. Running pull after selecting the plan would fetch a second time. That
fetch could discover a different upstream tip, so the operation would no longer match
the commit counts and plan already shown. A plain `git pull` can also merge or rebase
according to arguments and Git configuration, while sync must always apply the fixed
case table above.

The product action is therefore still **sync**. In this row, its explicit Git
operations are fetch and fast-forward.

The third row is the case a user may informally call “just push.” Diffo still calls
the product action **sync**. Its explicit Git operations are fetch and push.

In this algorithm, fetch reads remote state and a normal push advances a remote ref.
No row deletes a remote ref, replaces remote commits, or force-pushes. Any future sync
behavior that can make such a destructive remote change requires a separate decision.

Rebase keeps commit order and messages. It gives the commits new IDs. It does not
change upstream commits.

Git combines clean changes. Diffo does not ask about each clean file or commit.

If the rebase finishes, the new local tip contains the fetched upstream tip. The
normal push in the fourth row therefore needs no force.

If Git finds a conflict, run `git rebase --abort`. This restores the branch to its
pre-rebase tip. Stop the sync and do not push.

Diffo never creates a merge commit or force-pushes during sync.

## Communication

Call the user action `Sync` in controls, commands, and documentation. Do not expose
separate `Pull` and `Push` product actions for this workflow.

After fetch, show the chosen plan before fast-forwarding, rebasing, or pushing.

Example:

```text
origin/main has 3 upstream-only commits.
main has 2 local-only commits.
Plan: rebase 2 commits onto origin/main, then push.
```

Show the active step while it runs. Use `Fetching`, `Fast-forwarding main`,
`Rebasing 2 commits`, or `Pushing`.

While a sync is running, do not report only `Syncing`. Name the current Git operation.

On conflict: `Rebase conflicted in 1 file and was aborted. Nothing was pushed.`

On success, name the operations that ran: `Rebased 2 commits and pushed main.` Do not
describe this result as `Pulled`.

## Confirmation and abort boundaries

Diffo does not ask for confirmation for any plan in the sync table. Fetch,
fast-forward, a conflict-free rebase of local-only commits, and a normal push are all
automatic parts of sync.

A rebase can be aborted only while Git still considers it in progress. Therefore,
Diffo automatically runs `git rebase --abort` when a rebase reports a conflict. It
does not leave the repository in a paused rebase for the user to resolve.

Once a rebase completes successfully, `git rebase --abort` can no longer undo it. If
the following push is rejected, Diffo leaves the successfully rebased commits on the
local branch and reports that nothing was pushed. It does not reset the branch to
simulate an abort.

Destructive remote operations are outside this algorithm. If sync would require a
remote ref deletion, force-push, or other non-fast-forward update, stop without
performing it. Adding such an operation requires a separate decision, including its
confirmation policy.

## Stop cases

Every unsupported starting state and execution failure has a fixed result:

| Condition | Result |
| --- | --- |
| No configured upstream | Stop before fetch and explain that sync requires an upstream. |
| Detached HEAD or unborn branch | Stop before fetch and explain that sync requires an existing local branch. |
| Merge, rebase, or cherry-pick already in progress | Stop before fetch and tell the user to finish or abort it first. |
| Worktree state is not supported by the later dirty-worktree decision | Stop before fetch and leave the worktree unchanged. |
| The selected plan requires rebase and the local-only history contains merge commits | Stop after fetch, before rebase or push. |
| Rebase reports a conflict | Abort the rebase, restore the pre-rebase local tip, and stop. Do not push. |
| Push is rejected before any rebase | Stop with the local branch unchanged. Do not force-push or automatically retry. |
| Push is rejected after a completed rebase | Stop with the rebased commits on the local branch. Do not force-push, reset, or automatically retry. |

Do not guess. Do not silently switch to merge. Do not retry forever.

## Consequences

Sync becomes one predictable path.

Clean changes need no manual work. Diverged history stays linear. Sync does not create
surprise merge commits or commit-message editors.

Rebased commits get new IDs. This is allowed because they are not on the configured
upstream. Diffo must say that a rebase will happen.

Merge is still a valid Git workflow. It is not part of Diffo's sync workflow.

Dirty-worktree handling and the conflict-resolution interface need separate decisions.

## Verification

Use real local and remote repositories. Test same, behind, ahead, clean divergence,
and conflicting divergence.
Test different files and different hunks in one file.
Prove clean divergence creates no merge commit and uses no force-push.
Prove conflict stops before push and can still be aborted.
Prove the shown plan matches the Git operation that runs.
