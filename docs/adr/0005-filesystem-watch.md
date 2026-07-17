# ADR 0005: Watch the filesystem

Status: Superseded by [ADR 0012](0012-live-repository-refresh.md)

## Decision

Watch the worktree and Git metadata. Treat every event as "refresh state."

```text
filesystem event -> debounce -> collect snapshot -> send to UI
```

- Debounce events for about 100 ms.
- Collect state outside the UI thread.
- Send only complete snapshots to the UI.
- Number refreshes. Discard a result when a newer refresh has started.
- Do not convert filesystem events directly into Git changes.

Start with full snapshot refreshes. Add partial refreshes only after profiling.

## Done when

- File edits refresh the view.
- Staging and commits refresh the view.
- Event bursts cause one refresh.
- Slow old results cannot replace newer state.
