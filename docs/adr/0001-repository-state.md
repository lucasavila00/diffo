# ADR 0001: Represent Git state as immutable snapshots

- Status: Proposed
- Date: 2026-07-17

## Context

Diffo needs to display staged and unstaged changes, tracked and untracked files,
recent commits, and the relationship between the local branch and its upstream.
It must update as the working tree changes and support deterministic mocked states
for UI development and debugging.

A raw unified diff cannot represent the complete repository state. It does not
contain sufficient information about untracked files, branches, recent commits,
or upstream status.

## Decision

The application will render an immutable, structured repository snapshot:

```rust
struct RepositorySnapshot {
    branch: BranchState,
    files: Vec<FileState>,
    recent_commits: Vec<Commit>,
    upstream: Option<UpstreamState>,
    generation: u64,
}

struct FileState {
    path: PathBuf,
    old_path: Option<PathBuf>,
    kind: ChangeKind,
    staged: Option<FileDiff>,
    unstaged: Option<FileDiff>,
}
```

Staged and unstaged changes are modeled separately because a single file can
contain both at the same time. `FileDiff` will contain structured hunks and lines,
not only raw diff text.

Snapshot collection will be hidden behind a source interface:

```rust
trait RepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot>;
}
```

Two implementations are planned:

- `GitRepositorySource` reads a real repository using Git commands.
- `FixtureRepositorySource` loads structured fixture files for development and tests.

The initial Git implementation will use:

- `git status --porcelain=v2 --branch -z` for branch and file status.
- `git diff --no-ext-diff --no-color` for unstaged changes.
- `git diff --cached --no-ext-diff --no-color` for staged changes.
- A bounded `git log` query for recent commits.
- Comparisons between `HEAD` and its upstream for ahead and behind commits.

Git output that supports NUL delimiters will be requested with `-z` so unusual but
valid paths are parsed correctly.

## Live updates

A filesystem watcher will monitor the working tree and relevant Git metadata. Its
events are invalidation signals rather than direct descriptions of Git changes.
Events will be debounced for approximately 50–150 ms, after which a background
collector will build a complete new snapshot.

Each refresh request receives an increasing generation number. Results older than
the latest requested generation are discarded. The UI loop only receives complete
snapshots and remains responsible for input and rendering, not Git commands.

The first implementation should favor complete refreshes. More selective refreshes
and caching should only be added after profiling demonstrates a need.

## Mocking

Mock states will use structured fixtures, likely RON or JSON, containing the same
data as `RepositorySnapshot`. This allows fixtures to represent staged and unstaged
changes, untracked files, commits, and upstream state together.

`make diffo-mock` will select `FixtureRepositorySource`. It may later replay a
sequence of snapshots to simulate edits, staging, commits, and pushes without
modifying a real repository.

Raw `.diff` fixtures may still be used for focused parser tests, but they will not
serve as the application's complete mocked repository state.

## Consequences

- Rendering is independent of Git and filesystem access.
- Real and mocked repository states use the same application path.
- Complete immutable snapshots simplify concurrency and avoid partially updated UI state.
- Git parsing and snapshot construction require more work than displaying raw command output.
- Large repositories may eventually require caching or selective invalidation.

## Implementation sequence

1. Define the snapshot types and `RepositorySource` interface.
2. Add structured fixture serialization and loading.
3. Parse porcelain v2 status output.
4. Parse staged and unstaged diffs into files and hunks.
5. Add recent commits and upstream comparison.
6. Add debounced filesystem watching and background refreshes.
7. Profile before introducing incremental collection or caching.
