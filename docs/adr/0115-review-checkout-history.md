# ADR 0115: Review checkout history in a third activity

Refines [ADR 0039](0039-independent-app-modes.md) and
[ADR 0024](0024-atomic-diff-buffer-transitions.md).

## Context

Diffo can review working-tree and index changes in Diff and browse repository
files in Explorer, but it cannot review the commits that produced the current
checkout. Doing that in either existing activity would mix committed history
with mutable files and staging state.

A commit review does not need the full-file projection used to understand and
stage current changes. It needs a compact view of the patch recorded by one
commit and a quick way to move through the checkout's history.

## Decision

Add History as the third long-lived workbench activity. The fixed activity order
is Explorer, Diff, History. This replaces the unimplemented standalone Search
activity named by ADR 0039; search remains a modal interaction where it is
already used.

History has two panes:

- The left pane is a flat, newest-first list of commits reachable from `HEAD`.
  It represents the current checkout only and does not draw graph lanes or
  provide a branch, tag, or reflog browser. Each row identifies the commit and
  shows its subject.
- The right pane shows the selected commit as one read-only unified patch. Show
  only file headers, hunk headers, context lines, removed lines, added lines,
  and patch metadata needed to understand the change. Do not build the Diff
  activity's full-file, inline, or side-by-side projections.

Clicking a commit subject selects that commit and opens its patch. A normal
commit is compared with its first parent. A root commit is compared with the
empty tree. A merge commit is also compared with its first parent; History does
not add a combined-diff or parent-selection mode.

History owns its list selection, scrolling, prepared patch, input handling, and
background requests. It does not reuse or mutate Diff activity state. Reuse the
shared pane layout, text surface, scrolling behavior, syntax and diff styles,
and dark-gray chrome tokens, while keeping the hunk-only projection local to
History.

Query commit metadata and the selected patch on demand through the repository
service. Do not add complete history or commit patches to every repository
snapshot, and do not preload patches for unselected commits. Git reads, patch
parsing, projection, and bounded visible syntax work stay outside the input and
rendering loop.

Treat the selected row and its right-hand patch as one atomic commit. Until the
new patch and visible syntax coverage are ready, keep the previous selection and
patch visible. Install background results only during frame preparation and
discard results for an obsolete selection, checkout, or repository generation.
When `HEAD` changes, reload the reachable list; preserve the selected commit if
it remains reachable, otherwise select the newest commit.

## Alternatives

- Add commit history to Diff. Rejected because committed history has different
  data, selection, and loading lifecycles from working-tree and index changes.
- Show a commit graph with branches and tags. Rejected because the requested
  view is the history of the current checkout, not a repository-wide graph
  browser.
- Reuse the full-file Diff projection. Rejected because it loads content and
  builds navigation structures that a compact historical review does not need.
- Preload every commit patch. Rejected because repository history is unbounded
  and only one patch is visible at a time.

## Consequences

Users can move from current changes and file exploration to committed history
without leaving Diffo. Selecting a commit presents its recorded change in a
small, stable hunk-only view, including deterministic behavior for root and
merge commits.

Large histories and patches do not block terminal input or inflate the shared
repository snapshot. The activity requires new Git history and commit-patch
queries, a History-owned model and preparation path, and deterministic coverage
for checkout changes and stale background results.
