# ADR 0100: Scroll side-by-side code with one shared controller

Status: Accepted

## Problem

ADR 0025 kept side-by-side diffs fitted to the viewport. Each code cell was clipped
before rendering, the projection reported no horizontal overflow, and every
horizontal input was clamped to zero. Long old or new lines could not be reviewed.

## Decision

Apply visible-slice horizontal scrolling to side-by-side diffs:

- Measure the widest old or new code text in the visible vertical slice.
- Show one horizontal scrollbar when that width exceeds either code viewport.
- Pan both code columns by the same offset so paired text remains aligned.
- Keep both line-number gutters and the center divider fixed while code moves.
- Route arrows, trackpad events, scrollbar clicks, and drags through the existing
  horizontal viewport state.
- Hide the scrollbar and clamp the offset to zero when the visible slice fits.

One shared controller is intentional. Independent offsets would make paired lines
harder to compare and require extra state without improving access to either side.

This supersedes ADR 0025's side-by-side fitted-width exception. Its visible-slice
rules remain unchanged for inline diffs.

## Verification

- A visible long line enables a non-zero horizontal bound and scrollbar.
- Trackpad scrolling and dragging to the track endpoint reveal the hidden line end.
- Returning to zero reveals the line start.
- Gutters and the divider remain fixed throughout horizontal movement.
