# ADR 0029: Split `diffo-git/src/lib.rs`

## Decision

Keep `lib.rs` for `GitRepositorySource`, constructors, and trait wiring.

- `command.rs`: Git process execution.
- `snapshot.rs`: watch paths, diffs, file content, commits, snapshots.
- `status.rs`: porcelain status parser and change mapping.
- `operation.rs`: repository actions, results, failure classification.

Keep parsers pure. Keep command context at the process boundary. Move tests
beside their owner. No Git command or public API change.
