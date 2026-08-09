# ADR 0082: Show unpushed commits in the Diff pane

Refines [ADR 0001](0001-repository-state.md),
[ADR 0036](0036-git-branch-status.md), and
[ADR 0070](0070-rebase-unpushed-work-when-syncing.md).

## Problem

The Diff activity shows unstaged and staged file changes, but it does not show
the local commits that Sync would publish. The footer's ahead count says how
many such commits exist without identifying them. A user therefore cannot see
all unpublished work from the normal review screen.

Uncommitted file changes and unpushed commits are different states. The existing
`Changes` and `Staged` panels cover the former. The new panel must describe the
latter without implying that committed work can be staged again.

## Decision

Add a read-only `Unpushed` panel to the leading pane of the Diff activity. Keep
the commit composer first, then show `Unpushed`, `Staged`, and `Changes` from
top to bottom. The panel is informational: its rows have no selection state,
keyboard binding, mouse action, or effect on the displayed file diff.

An unpushed commit is a local-only commit as defined by ADR 0070: it is
reachable from the current local branch and not reachable from that branch's
configured upstream ref. Use the same immutable repository snapshot and upstream
comparison as the footer ahead count and Sync planning. Do not run Git during
rendering and do not maintain a second divergence cache.

List at most the three newest local-only commits, newest first. Each row
contains the seven-character commit ID followed by its one-line subject.
Sanitize terminal control characters and truncate the subject to the panel's
inner width with one ellipsis; do not wrap a commit onto another row.

When more than three local-only commits exist, add one final row:

```text
... and 4 more
```

The number is the total local-only count minus three. Do not add that row when
three or fewer commits exist. When the branch has an upstream but no local-only
commits, show `No unpushed commits`. When the current head is unborn or
detached, or the named branch has no configured upstream, show `No upstream`
because Diffo cannot classify commits as pushed or unpushed.

The footer ahead count and this panel must agree because they come from the same
snapshot. The panel may show three commit rows while the footer reports a larger
count; the final `... and N more` row accounts for the difference.

## Layout and refresh

Give `Unpushed` only the height required for its border and up to four content
rows: three commits and the remaining-count row. An empty or unavailable state
uses one content row. Divide the rest of the existing file-list area equally
between `Staged` and `Changes`; do not make the unpushed panel consume an equal
third when it has less content.

At heights where all three panels cannot retain their semantic minimums,
preserve the existing `Staged` and `Changes` minimum heights and reduce
`Unpushed` first. Clip whole rows from the bottom and keep the newest visible
commit; do not wrap, overlap, or move the panel into the trailing diff pane.

Install the unpushed list atomically with the branch, upstream divergence, and
file state from a repository refresh. While refresh or Sync is running, retain
the last committed list. A stale result must not restore commits from an older
branch or upstream tip.

Use the semantic layout and dark-gray chrome tokens from `diffo-ui` for the
panel. Commit IDs and subjects are ordinary informational text, not mouse
targets, and must not use bold.

## Alternatives

- Expand only the footer ahead count. Rejected because a count does not identify
  the unpublished commits.
- Show every local-only commit in a scrollable panel. Rejected because this is a
  compact summary and another interactive list would compete with file
  navigation.
- Derive unpushed commits from all remotes. Rejected because Sync and the
  existing ahead count use the configured upstream; a different comparison would
  disagree with the action the panel is meant to explain.
- Treat commits as a third selectable diff source. Rejected because this
  decision is about visibility of unpublished work, not browsing historical
  commit diffs.
