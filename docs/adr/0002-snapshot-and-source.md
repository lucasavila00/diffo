# ADR 0002: Snapshot and source

## Decision

The UI reads one immutable `RepositorySnapshot`. It carries explicit head and
repository-operation state, staged and unstaged file state, recent commits, and
upstream information. Keep the concrete Rust shape in `diffo-core`; do not copy
it into this ADR as a second schema.

Each file has separate staged and unstaged diffs. A file can have both.

State comes from one interface:

```rust
trait RepositorySource {
    fn snapshot(&self) -> Result<RepositorySnapshot>;
}
```

The UI does not know if state came from Git or a fixture.
