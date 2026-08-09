# ADR 0002: Snapshot and source

## Decision

The UI reads one immutable `RepositorySnapshot`:

```rust
struct RepositorySnapshot {
    branch: BranchState,
    files: Vec<FileState>,
    recent_commits: Vec<Commit>,
    upstream: Option<UpstreamState>,
}
```

Each file has separate staged and unstaged diffs. A file can have both.

State comes from one interface:

```rust
trait RepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot>;
}
```

The UI does not know if state came from Git or a fixture.

## Done when

- Snapshot types exist.
- Real and mock sources implement the same trait.
- The UI only accepts snapshots.
