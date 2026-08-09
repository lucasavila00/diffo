# ADR 0080: Back Explorer with the worktree filesystem

Refines [ADR 0012](0012-live-repository-refresh.md) and
[ADR 0035](0035-explorer-file-view.md).

## Problem

Explorer obtains its paths from `git ls-files`. That includes tracked files and
non-ignored untracked files, but hides files matched by Git ignore rules. The
worktree watcher sees filesystem events for those files, yet its result is only
a Git snapshot. When Git status is unchanged, Explorer does not request a new
tree or reload the selected file.

Explorer is a view of files on disk. Git state can annotate that view, but must
not decide which worktree files exist.

## Decision

Build the Explorer path list by walking the worktree filesystem. Include regular
files whether they are tracked, untracked, or ignored. Preserve the existing
support for file symlinks, but never follow symlinks while walking. Directories
appear only as ancestors of included files; empty directories and other special
filesystem entries do not appear.

Exclude the repository's Git control entry and do not descend into the resolved
Git directory or common Git directory when either is inside the worktree. Hidden
files have no special treatment. Keep paths relative to the worktree and do not
require them to be valid UTF-8.

Git remains the source of status and patch metadata only:

- tracked and non-ignored untracked files retain their existing Git-derived
  viewer gutter;
- ignored files and other files absent from Git status are neutral and have no
  synthetic all-added patch;
- removed paths remain visible in Diff but not in Explorer.

Keep filesystem discovery and file reads outside rendering and input handling. A
completed scan replaces the Explorer path list as one commit. Reconcile
expansion, selection, and scroll by stable relative path. If the selected path
disappears, select the nearest remaining row and clear its viewer only when the
replacement tree commits.

Treat worktree filesystem events as an Explorer invalidation independently of
Git snapshot changes:

```text
worktree event -> coalesce -> rescan paths + reread selected file -> Explorer outcomes
Git metadata event -> repository snapshot -> Git overlays
```

Do not mutate the tree directly from individual notification paths. Coalesce
event bursts and request a fresh filesystem scan. A worktree event also rereads
the selected file even when tree membership and Git status are unchanged. Keep
the previous tree and viewer visible until their replacements are ready.
Sequence scan and file outcomes, and reject stale results so an older read
cannot restore removed paths or old content.

The existing repository watcher may remain the operating-system adapter, but it
must distinguish worktree invalidation from Git-metadata-only invalidation.
Explorer refresh must not depend on a changed `RepositorySnapshot`. Mock mode
remains snapshot-driven and does not gain a real filesystem watcher.

## Alternatives

- Add ignored paths to the `git ls-files` query. Rejected because Git ignore
  policy would still define a filesystem view and Git would remain responsible
  for discovering ordinary files.
- Derive tree edits from notification paths. Rejected because notification
  streams may be coalesced, duplicated, or incomplete and cannot provide an
  authoritative tree.
- Put every filesystem path into `RepositorySnapshot`. Rejected because Diff's
  Git snapshot does not need unchanged or ignored files, and Explorer already
  owns its tree lifecycle.
- Refresh Explorer only after a changed Git snapshot. Rejected because
  ignored-file edits intentionally do not change Git state.

## Consequences

Explorer shows the worktree as it exists on disk, including ignored files. Large
ignored trees now contribute to Explorer scan cost, so scans remain asynchronous
and event bursts are coalesced. Git snapshot collection, Diff, staging, and
repository commands retain their existing boundaries.

Filesystem and Git results can arrive in either order. Explorer therefore treats
the path list as filesystem-owned data and Git state as a separately replaceable
overlay; neither result may roll back the other's newer generation or user
interaction state.
