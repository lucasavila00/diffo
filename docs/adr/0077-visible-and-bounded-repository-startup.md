# ADR 0077: Keep one startup draw and bound snapshot concurrency

Builds on [ADR 0004](0004-real-git-state.md),
[ADR 0009](0009-diff-preparation.md),
[ADR 0041](0041-repeatable-cpu-measurement.md), and
[ADR 0073](0073-require-a-git-repository.md).

## Context

Interactive startup synchronously performs repository discovery, watch-path
lookup, complete snapshot collection, repository-service startup, and workbench
construction before entering terminal mode. This delays the first output, but it
also lets Diffo enter the alternate screen once and commit one complete
application frame.

Real snapshot collection first runs porcelain status and then collects a
full-context patch for each changed side of each tracked file. These patches are
independent, but were collected serially. A local release measurement on the
same host increased from about 33 ms for one changed tracked file to 192 ms for
100 files and 913 ms for 500 files. This near-linear growth identified serial
per-file Git subprocesses as the changed-file bottleneck.

The stress mock exposes another cost. `MutableFixtureRepository` owns its
snapshot, `Repository::snapshot` returns an owned clone, and `Workbench::new`
cloned that full snapshot again for Explorer. Explorer only used file paths and
change kinds, so large diff strings were duplicated without serving its
responsibility.

## Decision

### Keep a single terminal commit

Keep repository discovery, complete snapshot collection, service startup, and
workbench construction before terminal initialization. Then enter terminal mode,
configure mouse capture, and draw the committed application state once.

A static `Loading repository…` frame was implemented and measured during this
work, then rejected after interactive review. It required a loading draw
followed by a purge/clear and the application draw. On local terminals the short
interval appeared as flashing; over SSH it also added terminal bytes without
adding actionable state. The prior single draw produced a calmer and more atomic
startup UX.

Preserve ADR 0073: launching outside a worktree prints the stable plain-text
error without entering terminal mode. Keep `DIFFO_DUMP_PATH` and
`DIFFO_WATCH_DUMP_PATH` headless.

### Bound real-Git snapshot work

Collect changed-file states with at most eight scoped workers. Each worker
retains the existing per-file behavior: full-context staged and unstaged diffs,
untracked and conflicted worktree reads, rename context, UTF-8 validation, and
contextual errors. Reassemble results in porcelain-status order before
constructing the snapshot.

Eight is a fixed product implementation boundary, not configuration. It bounds
Git process and memory pressure while overlapping independent subprocess
latency. A single changed file stays on the direct path without starting a
worker.

Do not batch every file into one patch command in this change. Splitting a
combined patch back into exact status entries must correctly handle Git path
quoting, renames, copies, binary patches, and content resembling patch headers;
bounded concurrency preserves the already-tested per-file semantics.

### Stop retaining diff bodies in Explorer

Make the Diff model the workbench's sole owner of the complete repository
snapshot. Explorer derives and retains only a `path -> ChangeKind` map. Initial
construction and repository updates pass the snapshot to Explorer by reference.
Explorer rebuilds its tree and requests paths only when that status map changes;
a diff-body-only update no longer duplicates content or invalidates the tree.

The mutable mock repository and the Diff model still require separate owned
snapshots under the current `Repository` interface. Changing that interface or
sharing mutable snapshot internals is outside this decision.

## Consequences

Users continue to wait without an intermediate frame, but the alternate screen
does not flash through disposable startup state. `first_output_ms` therefore
remains an honest approximation of readiness and should stay close to `ready_ms`
in ADR 0076's measurement.

Repositories with many changed tracked files trade bounded extra concurrent Git
work for shorter wall time. Output ordering and snapshot semantics remain
stable. Large mock snapshots avoid one complete clone and retain substantially
less steady-state memory in Explorer.
