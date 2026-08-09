# ADR 0001: Git state snapshots

## State

Diffo renders one immutable snapshot:

```rust
struct RepositorySnapshot {
    branch: BranchState,
    files: Vec<FileState>,
    recent_commits: Vec<Commit>,
    upstream: Option<UpstreamState>,
}

struct FileState {
    path: PathBuf,
    staged: Option<FileDiff>,
    unstaged: Option<FileDiff>,
}
```

A file can have staged and unstaged changes at the same time. Keep both.

## Sources

```rust
trait RepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot>;
}
```

- Real source runs `git status`, `git diff`, `git diff --cached`, and `git log`.
- Mock source loads the same snapshot from a JSON or RON fixture.
- Raw diff fixtures are only for parser tests. They cannot describe full Git
  state.

## Live updates

Watch the worktree and `.git`. Debounce events for about 100 ms. Build a new
snapshot in a background thread. Send the complete snapshot to the UI.

Events mean "refresh." Do not try to turn each filesystem event into a Git
change. Discard an old refresh result if a newer refresh started.

## Build order

1. Add snapshot types and sources. See [ADR 0002](0002-snapshot-and-source.md).
2. Add structured mock fixtures. See [ADR 0003](0003-mock-fixtures.md).
3. Parse real Git state and test it end to end. See
   [ADR 0004](0004-real-git-state.md).
4. Add filesystem watching. See [ADR 0005](0005-filesystem-watch.md).
5. Optimize only if profiling says it is slow.
