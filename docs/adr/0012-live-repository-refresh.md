# ADR 0012: Live repository refresh

Status: Accepted

Supersedes [ADR 0005](0005-filesystem-watch.md).

## Problem

Diffo reads Git once at startup. It reads again only after its own stage action.
External edits, `git add`, commits, pulls, and branch changes never reach the UI.

## Decision

Add one refresh service outside the UI thread:

```text
worktree or Git event -> debounce -> collect full snapshot -> app message -> render
```

- Watch the worktree recursively.
- Resolve and watch the repository Git directory and common Git directory.
- Treat events only as refresh requests. Do not interpret event paths as Git state.
- Debounce bursts for 100 ms.
- Use one worker. Never run Git collection in rendering or input code.
- If events arrive during collection, mark the service dirty and collect once more.
- Number requests and results. Never apply an older result after a newer result.
- Send only complete `RepositorySnapshot` values to `diffo-app`.
- Keep the last good snapshot when collection fails. Show the error in the status bar.
- Stop and join the watcher and worker during normal shutdown.

Use `diffo-repository-service` for the filesystem watcher adapter, debounce,
generations, and the single serialized repository worker. The same worker executes
application commands so snapshot collection cannot race a repository mutation; the
workbench remains the only owner of command scheduling and lifecycle. Keep path
discovery and repository implementation in `diffo-git`. The `diffo` binary owns the
service and forwards its events to the workbench.

Mock mode has no filesystem watcher. Its stage actions still refresh its in-memory
snapshot.

## Event loop

Drain refresh results before every draw. While a refresh is active, poll input for at
most 16 ms. Otherwise poll for at most 50 ms. Input and terminal restore must not wait
for Git.

Own stage actions and watcher refreshes may race. The newest numbered snapshot wins.
Selection stays on the same `FileKey` when it still exists.

## Regression tests

Add deterministic unit tests with fake events and a fake snapshot collector:

- A burst causes one collection.
- An event during collection causes one later collection.
- An old result cannot replace a newer result.
- Collection failure keeps the last snapshot and reports an error.
- Shutdown joins all worker threads.

Add a real filesystem integration test with a temporary Git repository. Verify edits
to the worktree and Git metadata both request refreshes.

Add a black-box integration test to the `diffo` binary crate. Cargo provides the
compiled no-argument binary to the test. Use developer-only `DIFFO_WATCH_DUMP_PATH`
to run the same watcher and refresh pipeline without a terminal. Every accepted
snapshot is written atomically as RON.
The test:

1. Waits for the initial clean snapshot.
2. Modifies a tracked file and creates an untracked file.
3. Waits until both appear as unstaged.
4. Runs `git add` and waits until they appear as staged.
5. Commits and waits for a clean snapshot with the new commit.
6. Sends SIGTERM and checks clean process shutdown.

Poll the dump with a deadline. Do not use fixed sleeps. Write to a temporary file and
rename it so the test never reads partial RON.

## Done when

- `make diffo` updates after external file and Git commands without restart.
- Refresh work never blocks `q`, Ctrl-C, or drawing.
- The black-box live-update test runs in normal workspace CI.
- Existing real Git snapshot tests still pass.
