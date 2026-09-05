# ADR 0020: Structured operation results

## Goal

Produce truthful structured results after repository actions. ADR 0084 owns
their current toast or acknowledgement-modal presentation.

Examples:

- `Sync complete`
- `Already up to date`
- `Synced a1b2c3d to origin/main`
- `Committed a1b2c3d`
- `Fetch complete`
- `Sync failed: no network`

## Operation results

Do not parse Git's display text. It changes by Git version and locale.

Change repository actions from `Result<()>` to a structured result:

```text
OperationResult
  Commit { hash }
  Fetch { updated_refs }
  Sync { plan }
  Stage
  Unstage
  ...one typed variant per supported repository action
```

Stage and Unstage results do not create toasts. Their effect is already
immediate and visible in the file lists.

Failures are also structured:

```text
OperationFailure { action: RepositoryAction, kind, detail }

FailureKind
  PushRejected
  Authentication
  Network
  RebaseConflict
  DirtyWorktree
  HookRejected
  NoRemote
  Unknown
```

Git gets this data with stable commands after the action:

- `git rev-parse HEAD` for the commit hash;
- the selected sync plan for local and upstream commit counts;
- configured upstream data for remote and branch;
- refs before and after Fetch to count updates.

Use `git push --porcelain` for stable push status. Use repository state before
the command to detect known blocked actions. Keep raw stderr only as sanitized
detail for unknown failures.

Use short seven-character hashes in the UI. Keep full hashes in the result.

The refresh service must distinguish watcher snapshots from command outcomes:

```text
Snapshot(snapshot)
CommandCompleted(id, action, result, snapshot)
CommandFailed(id, failure, optional_snapshot)
CommandCancelled(id, action, snapshot)
```

A watcher refresh never creates a toast.

## Blocked and failed actions

Do not silently ignore an action. Classify known failures, keep the action name,
and sanitize every diagnostic. Never expose credentials, credential-bearing
URLs, tokens, secret-sensitive streams, or environment values. Preserve stderr
and stdout separately and apply the bounded diagnostic contract in ADR 0105.

Sync owns fetch/rebase/push planning and divergence handling under ADRs 0070,
0081, and 0085. Never force-push, retry a rejected push automatically, stash
implicitly, or resolve conflicts without an explicit product decision.

## Failures

Keep the action name in every failure: `Sync failed: ...`. Never show success
before both the Git command and result-data collection succeed. If result
metadata cannot be collected, show a generic success such as `Sync complete`; do
not report a false hash or count.
