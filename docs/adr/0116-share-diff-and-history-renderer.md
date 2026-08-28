# ADR 0116: Add History review in three pull requests

This ADR changes [ADR 0115](0115-review-checkout-history.md).

## Context

The Diff activity has a staging view on the right side. The `r` key changes this
view between inline and side-by-side mode.

History needs the same right-side view. It must also show a complete commit as a
hunk view. Do not make a new renderer for History.

## Decision

Do the work in three pull requests.

### Pull request 1: Add hunk mode

Add hunk mode to the current right-side staging view. The `r` key changes
between inline, side-by-side, and hunk mode. Keep the current file picker and
Git actions. This pull request does not add History file selection.

### Pull request 2: Sync file and hunk selection

Add one selection state for the selected file and the hunk view. When the user
selects a file, show that file. When the user selects the complete change, show
the hunk view. Keep the two views in sync. Do not show new data until it is
ready. Keep old data on the screen until then.

### Pull request 3: Add the History picker

Add the History left-side picker. It has a commit list and a file picker for the
selected commit. History uses the same right-side modes from pull request 1.
History is read-only. Diff keeps its staging and commit actions.

## Consequences

Each pull request is small and can be checked by itself. Diff and History keep
different data and actions. They use the same right-side review modes.
