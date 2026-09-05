# ADR 0012: Live repository refresh

## Problem

Diffo reads Git once at startup. It reads again only after its own stage action.
External edits, `git add`, commits, pulls, and branch changes never reach the
UI.

## Decision

Add one refresh service outside the UI thread:

```text
worktree or Git event -> debounce -> collect full snapshot -> app message -> render
```

- Watch the worktree recursively.
- Resolve and watch the repository Git directory and common Git directory.
- Treat events only as refresh requests. Do not interpret event paths as Git
  state.
- Debounce bursts for 100 ms.
- Use one worker. Never run Git collection in rendering or input code.
- If events arrive during collection, mark the service dirty and collect once
  more.
- Number requests and results. Never apply an older result after a newer result.
- Send only complete `RepositorySnapshot` values to `diffo-app`.
- Keep the last good snapshot when collection fails. Present the failure through
  the shared acknowledgement modal from ADR 0084.
- Stop and join the watcher and worker during normal shutdown.

Use `diffo-repository-service` for the filesystem watcher adapter, debounce,
generations, and the single serialized repository worker. The same worker
executes application commands so snapshot collection cannot race a repository
mutation; the workbench remains the only owner of command scheduling and
lifecycle. Keep path discovery and repository implementation in `diffo-git`. The
`diffo` binary owns the service and forwards its events to the workbench.

Mock mode has no filesystem watcher. Its stage actions still refresh its
in-memory snapshot.

## Event loop

Drain refresh results before every draw. While a refresh is active, poll input
for at most 16 ms. Otherwise poll for at most 50 ms. Input and terminal restore
must not wait for Git.

Own stage actions and watcher refreshes may race. The newest numbered snapshot
wins. Selection stays on the same `FileKey` when it still exists.
