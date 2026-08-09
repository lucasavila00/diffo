# ADR 0083: Open files with Quick Open

Refines [ADR 0035](0035-explorer-file-view.md),
[ADR 0049](0049-shared-file-picker.md), and
[ADR 0080](0080-filesystem-backed-explorer.md).

## Problem

Opening a known file requires switching to Explorer, expanding its ancestors,
and locating it in the tree. This is slow in a large worktree and makes the tree
carry both browsing and direct-navigation responsibilities.

Diffo needs a keyboard-first file finder like VS Code's Quick Open, while
retaining fixed controls and avoiding another source of repository paths.

## Decision

Add a global Quick Open modal. Pressing the lowercase, unmodified `o` key opens
it from Diff or Explorer. Document `o: Quick Open` in the help interface. Do not
add a CLI option, configuration setting, environment variable, alternate
uppercase shortcut, or configurable binding.

Quick Open lists the regular files and file symlinks in Explorer's latest
committed filesystem path set. It therefore follows ADR 0080's inclusion and
exclusion rules, including ignored and hidden files, and does not run a second
filesystem walk or a Git query. Show worktree-relative paths and match the typed
query fuzzily against both the file name and the whole relative path. Rank any
file-name match above a path-only match, then compare fuzzy score, and use path
order to break equal scores. A query containing a path separator naturally
matches only the whole relative path. Select the first match. An empty query
shows every file in path order. Reuse `diffo-ui`'s searchable picker behavior
and semantic modal layout for text input, selection, scrolling, mouse
activation, Enter, and Esc.

While the initial Explorer path scan has no committed result, open the modal
immediately with a loading state. Install only the newest completed path result
and retain the user's query when it arrives. Filesystem refreshes replace the
available items without closing the modal; reconcile selection by stable
relative path and reject stale results.

Activating a result closes Quick Open, switches to Explorer, expands the file's
ancestors, selects and reveals its row, and requests that file for the Explorer
viewer. This is one navigation action even when Quick Open was launched from
Diff. Keep the previously committed viewer visible until the selected file's
content, change gutter, scroll bounds, and visible syntax are ready to commit
together. If the path disappears before it opens, keep Explorer's prior
committed selection and viewer and refresh the path set; do not open a
replacement path.

Once Quick Open is open, character keys, including `o`, edit its query instead
of invoking global actions. Other modals continue to capture their own input, so
typing `o` in a command or searchable picker does not open Quick Open.

## Alternatives

- Search only tracked files or changed files. Rejected because Quick Open
  navigates the filesystem-backed Explorer, whose file membership is defined by
  ADR 0080.
- Open a file directly inside Diff. Rejected because Explorer owns full-file
  reads, viewing, and filesystem navigation; Diff remains a view of Git changes.
- Start a filesystem scan each time the modal opens. Rejected because Explorer
  already owns an asynchronously refreshed authoritative path set.

## Consequences

Any committed worktree file is reachable without manually traversing the
Explorer tree, and Quick Open has the same view of the filesystem as Explorer.
Opening a result always makes the destination and ownership visible by switching
to Explorer.

Quick Open adds the otherwise-unassigned `o` global shortcut.
