# ADR 0067: Skip fully visible changes

Refines [ADR 0021](0021-full-file-diffs-and-change-navigation.md),
[ADR 0022](0022-large-hunk-navigation-targets.md).

## Problem

ADR 0021 made Diff a full-file view. One viewport can contain several separate
changes. Diff highlights changed rows, but it does not select one change as the
current item. The useful review unit is therefore the visible screen, not one
Git hunk.

Next change and previous change still navigate from change starts. They can
re-anchor the viewport on a change that was already fully readable. The screen
moves only a few rows and reveals no new changed content. This feels like
stepping through hidden Git structure instead of reviewing the visible file.

The edge buttons and other inputs invoke the same semantic actions. They must
agree on the target and on whether a target exists.

## Decision

A navigation item remains one contiguous region of changed rows. Its first and
last rows come from the committed inline or side-by-side projection.

A change is fully visible only when all its rows are inside the content
viewport. The content viewport excludes the button rows and horizontal
scrollbar.

- Next change picks the nearest region with changed rows below the viewport.
- Previous change picks the nearest region with changed rows above the viewport.
- Fully visible regions are skipped.
- A region crossing the relevant viewport edge is not skipped. It still has
  hidden changed rows.
- If a region spans the whole viewport, move through it one content viewport at
  a time. Every action must move in its direction and reveal hidden changed
  rows.
- Do not wrap. The action is unavailable when no target exists in its direction.

All input paths invoke these actions. They do not pick targets themselves. A
change button is visible exactly when its action is available.

Use only committed projection and viewport state. If the target needs syntax
work, keep the old viewport visible until the target is ready. Preserve the
atomic jump from [ADR 0044](0044-single-frame-position-changes.md).

Change-marker clicks still jump to the clicked change. Opening a file still
starts at its first change. This ADR changes directional actions only.

## Consequences

One action normally reveals changed content that was not on screen. Complete
changes already on screen are treated as reviewed.

Prepared diff state must keep both ends of each change region. Inline and
side-by-side bounds can differ. Button availability may change when a region
crosses a viewport edge even if its first row is already visible.

## Tests

- Multiple fully visible changes are skipped in both directions.
- Regions crossing the top or bottom edge remain targets.
- A region taller than the viewport makes progress in both directions.
- First and last targets do not wrap.
- Inline and side-by-side modes use their own bounds.
- Every input path gets the same action target and availability.
- A delayed PTY regression proves the target commits atomically.

Run `make all` when implementing.
