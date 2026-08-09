# Sync algorithm

Sync makes the current local branch and one remote branch agree.

It does not run `git pull`. It does not stash. It does not force-push.

## 1. Check local state

Stop before fetch when:

| State                   | Result |
| ----------------------- | ------ |
| Detached HEAD           | Stop.  |
| Unborn branch           | Stop.  |
| Merge in progress       | Stop.  |
| Rebase in progress      | Stop.  |
| Cherry-pick in progress | Stop.  |

Dirty files do not stop Sync here.

## 2. Find the remote branch

Configured upstream:

| State                                         | Remote and target                                                             |
| --------------------------------------------- | ----------------------------------------------------------------------------- |
| Upstream branch matches the local branch name | Use its remote and branch.                                                    |
| Upstream branch has a different name          | Use its remote with the local branch name. Repair the upstream after success. |
| Upstream config is invalid                    | Stop.                                                                         |

No configured upstream:

| Remotes                    | Choice        |
| -------------------------- | ------------- |
| `origin` exists            | Use `origin`. |
| One remote exists          | Use it.       |
| Several exist, no `origin` | Ask the user. |
| None exist                 | Stop.         |

The target branch name is always the current local branch name. A differently
named configured upstream selects only the remote; it is never the Sync or push
target.

Example: local `topic` plus remote `origin` means target `origin/topic`.

Example: local `topic` configured with upstream `origin/master` still means
target `origin/topic`. Sync never pushes `topic` to `origin/master`.

## 3. Fetch

Fetch the selected remote.

Fetch failure: stop.

Same-named target branch after fetch when its upstream must be established or
repaired:

| Remote branch                 | Result                            |
| ----------------------------- | --------------------------------- |
| Exists with related history   | Use it as a provisional upstream. |
| Exists with unrelated history | Stop.                             |
| Does not exist                | Plan first publication.           |

## 4. Count commits

Compare `HEAD` with the fetched target.

| Local-only commits | Remote-only commits | Plan              |
| -----------------: | ------------------: | ----------------- |
|                  0 |                   0 | Equal             |
|                 1+ |                   0 | Push              |
|                  0 |                  1+ | Fast-forward      |
|                 1+ |                  1+ | Rebase, then push |

Missing target branch: push, then set or repair the upstream.

## 5. Check dirty state for the plan

| Plan              | Allowed local file state                                                                |
| ----------------- | --------------------------------------------------------------------------------------- |
| Equal             | Staged, unstaged, and untracked files allowed.                                          |
| Push              | Staged, unstaged, and untracked files allowed.                                          |
| Fast-forward      | Dirty files allowed. Git rejects files it would overwrite.                              |
| Rebase, then push | Index and tracked files must be clean. Untracked files allowed unless Git rejects them. |

Do not guess file overlap. Run Git. Let Git decide.

A rebase plan also stops when local-only history contains a merge commit.

## 6. Confirm protected pushes

A push to `main` or `master` needs confirmation.

After confirmation, verify that the branch, tips, upstream config, remotes, and
local file state still match the plan. If anything changed: stop and ask the
user to Sync again.

## 7. Execute the plan

| Plan              | Git work                                     |
| ----------------- | -------------------------------------------- |
| Equal             | No branch movement.                          |
| Push              | Normal push.                                 |
| Fast-forward      | `merge --ff-only` to the fetched target.     |
| Rebase, then push | Rebase onto the fetched target. Normal push. |
| First publication | Normal push with `--set-upstream`.           |

If a rebase conflicts: abort the rebase. Do not push.

If a push is rejected: stop. Do not retry or force-push.

## 8. Set or repair upstream

Set a missing upstream, or replace a differently named upstream, only after the
plan succeeds.

| Successful plan   | Action                      |
| ----------------- | --------------------------- |
| Equal             | Set upstream locally.       |
| Fast-forward      | Set upstream locally.       |
| Push              | Push with `--set-upstream`. |
| Rebase, then push | Push with `--set-upstream`. |
| First publication | Push with `--set-upstream`. |

Failure or cancellation: leave a missing upstream unset and a mismatched
upstream unchanged.

## Guarantees

- Never stash automatically.
- Never force-push.
- Never create a remote.
- Never use a remote branch name that differs from the current local branch
  name.
- Never rebase dirty tracked work.
- Never push after a failed or conflicted rebase.
- Fetch may update remote-tracking refs even when a later step stops.

Decision records: [ADR 0081](../adr/0081-broaden-sync-preconditions.md) and
[ADR 0085](../adr/0085-repair-mismatched-sync-upstreams.md).
