# ADR 0020: Operation toasts

Status: Proposed

## Goal

Show short results after repository actions.

Examples:

- `Pulled 3 commits`
- `Already up to date`
- `Pushed a1b2c3d to origin/main`
- `Committed a1b2c3d`
- `Fetch complete`
- `Pull failed: no network`

## Operation results

Do not parse Git's display text. It changes by Git version and locale.

Change repository actions from `Result<()>` to a structured result:

```text
OperationResult
  Commit { hash }
  Fetch { updated_refs }
  Pull { old_head, new_head, commits }
  Push { hash, remote, branch }
  Stage
  Unstage
```

Failures are also structured:

```text
OperationFailure { action: RepositoryAction, kind, summary, detail }

FailureKind
  PullRequired
  PushRejected
  Authentication
  Network
  MergeConflict
  DirtyWorktree
  HookRejected
  NoRemote
  Unknown
```

Git gets this data with stable commands after the action:

- `git rev-parse HEAD` for the commit hash;
- `git rev-list --count OLD..NEW` for pulled commit count;
- configured upstream data for remote and branch;
- refs before and after Fetch to count updates.

Use `git push --porcelain` for stable push status. Use repository state before the
command to detect known blocked actions. Keep raw stderr only as sanitized detail
for unknown failures.

Use short seven-character hashes in the UI. Keep full hashes in the result.

The refresh service must return separate events:

```text
RepositoryChanged(snapshot)
ActionCompleted(result, snapshot)
ActionFailed(failure, snapshot)
```

A watcher refresh never creates a toast.

## Blocked and failed actions

Do not silently ignore a primary action.

- Ahead and behind: `Push blocked: pull and merge required`.
- Behind: `Push blocked: pull required`.
- Non-fast-forward rejection after a remote race: `Push rejected: remote changed`.
- Merge conflict during Pull: `Pull stopped: resolve conflicts`.
- Missing credentials: `Push failed: authentication required`.
- Missing remote: `Fetch failed: no remote configured`.
- Remote hook rejection: `Push rejected by remote` plus safe detail.
- Network failure: `Pull failed: network unavailable`.

Never run `--force`, `--force-with-lease`, an automatic merge, or conflict
resolution. A failure toast explains the next required user action.

`Push + Pull` remains a blocked state for now. Clicking it creates the divergence
toast and performs no repository mutation. The same rule applies if Push is
requested from another UI path while the branch is behind.

Sanitize failure detail. Never show credentials, credential-bearing URLs, tokens,
or environment values.

## Toast state

Keep toast state in `diffo-app`:

```text
Toast { id, kind, title, detail }
ToastKind = Success | Info | Error
```

Keep at most three toasts. New toasts go on top. Duplicate messages replace the
older copy.

The runtime owns time. It sends `DismissToast(id)` after four seconds. The pure
model does not read the clock. Errors and blocked-action toasts stay until dismissed
or replaced.

## UI

Render toasts above the footer in the bottom-right corner.

- Success: green border.
- Info: cyan border.
- Error: red border.
- Keep the current diff visible behind them.
- Click a toast or press Esc when it is focused to dismiss it.
- Network loading remains visible until the action result arrives. Then replace it
  with the result toast.

Long text wraps inside a fixed maximum width. Toasts must not change pane layout.
Use xterm-256 colors over SSH.

## Failures

Keep the action name in every failure: `Push failed: ...`.
Never show success before both the Git command and result-data collection succeed.
If result metadata cannot be collected, show a generic success such as
`Push complete`; do not report a false hash or count.

## Tests

- Pure tests cover queue order, duplicate replacement, maximum size, and dismissal.
- Git tests cover structured Commit, Fetch, Pull, and Push results.
- Git tests cover non-fast-forward, hook rejection, missing remote, conflict,
  authentication, network, and unknown failures without leaking secrets.
- Refresh tests prove watcher snapshots cannot create action results.
- Renderer tests cover position, wrapping, colors, and three stacked toasts.
- Compiled PTY tests perform real local Commit, Fetch, Pull, and Push operations and
  assert their exact visible messages.
- A compiled PTY test waits for automatic dismissal.
- A compiled PTY test verifies a failed action names the action and stays visible.
- Compiled PTY tests create a diverged branch and verify Push is blocked without
  changing either ref.
- A compiled PTY test advances the remote after the local snapshot, attempts Push,
  and verifies the non-fast-forward rejection toast.
- A compiled PTY test uses a rejecting local remote hook and shows its safe detail.
- Failure tests use local repositories or invalid local remotes. They never depend
  on public network access.
- All E2E waits keep the existing five-second timeout.
