# ADR 0116: Share the renderer between Diff and History

Refines [ADR 0115](0115-review-checkout-history.md),
[ADR 0021](0021-full-file-diffs-and-change-navigation.md), and
[ADR 0024](0024-atomic-diff-buffer-transitions.md).

## Context

Diff reviews mutable working-tree and index changes. History reviews immutable
commits from the current checkout. They are different activities: their source
data, leading lists, selection lifecycles, and available actions must remain
independent.

Both activities nevertheless need the same two ways to inspect a change:

- a unified hunk projection, useful for reviewing a complete change; and
- a rich file projection, useful for understanding one file in inline or
  side-by-side form with syntax coverage, scrolling, change navigation, and
  rails.

ADR 0115 kept History hunk-only and local to its own preparation path. That
would make the rich projection unavailable for historical files and duplicate
the renderer as the two activities grow.

## Decision

Keep Diff and History as separate long-lived activities. Do not merge their
lists, actions, state, input handling, background requests, or selection and
scroll ownership.

Extract one shared prepared renderer and projection pipeline for their
right-hand review surface. It accepts an activity-owned document identity and a
prepared projection. It supports both unified hunks and rich files, including
the fixed inline and side-by-side controls, bounded syntax coverage, scrolling,
change navigation, scrollbar, and hunk-marker rail. Explorer remains outside
this decision.

Diff continues to present unstaged and staged file lists and owns staging,
unstaging, commit composition, and working-tree refresh behavior. Its data comes
from mutable repository state.

History remains read-only. Its leading pane is split evenly between a
newest-first list of commits reachable from `HEAD` and the changed paths of the
selected commit. Selecting a commit initially preserves an all-files hunk review
of that commit. Selecting one changed path opens that path through the same rich
file projection as Diff. History resolves the commit and parent blobs on demand,
comparing root commits with the empty tree and merge commits with their first
parent. It does not preload history blobs or add them to repository snapshots.

Each activity prepares and atomically installs its own selection, list content,
projection, navigation targets, scroll bounds, and visible syntax coverage.
Until a replacement is ready, it keeps its prior committed renderer state
visible. Stale results cannot provide content, targets, or metrics. The shared
prepared file-and-mode cache remains limited to four entries, and the strict
10,000-line syntax boundary remains in force.

## Alternatives

- Merge Diff and History into one activity. Rejected because mutable staging and
  commit operations do not belong to checkout-history review.
- Keep History hunk-only. Rejected because a historical file needs the same rich
  inspection tools as a working-tree file.
- Give History a separate rich renderer. Rejected because it duplicates the
  projection, scrolling, syntax, rail, and atomic-transition contracts.
- Preload every historical blob. Rejected because history is unbounded and only
  the selected commit or path is visible.

## Consequences

History gains rich file review without inheriting Diff's mutable controls, and
Diff keeps its existing workflow. The implementation must separate common
renderer inputs from activity-specific repository queries and state, while
preserving deterministic prepared-transition tests for both activities.
