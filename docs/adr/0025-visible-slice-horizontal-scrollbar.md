# ADR 0025: Show horizontal scrolling only for visible content

## Problem

The inline diff used the widest row in the entire file to decide whether to show
its horizontal scrollbar. One long line far above or below the viewport
therefore left a scrollbar visible while every on-screen row fit. The control
consumed attention and suggested that the current content could be scrolled when
it could not.

## Decision

Derive horizontal overflow from the currently rendered vertical slice.

- In inline mode, show the horizontal scrollbar only when at least one visible
  row is wider than the diff viewport.
- Compute the horizontal maximum from those visible rows, not from the complete
  file.
- During an atomic buffer transition, use the vertical position that will be
  committed and rendered in that frame when calculating visible-row widths.
- When vertical navigation moves to rows that all fit, hide the scrollbar and
  clamp horizontal scroll back to zero in the same frame.
- Allow the scrollbar to appear again when a vertically visible row requires it.
- Side-by-side mode remains fitted to the viewport and does not use horizontal
  scrolling.

The scrollbar may therefore appear and disappear during vertical scrolling. That
is intentional: it describes the content the user can currently see and interact
with.

## Tests

- A long off-screen row does not show or enable horizontal scrolling.
- Scrolling the long row into view enables a non-zero horizontal maximum.
- Scrolling it out of view hides the control and clamps the horizontal offset to
  zero.
- Existing drag coverage still reaches the right edge of a visible long line.
