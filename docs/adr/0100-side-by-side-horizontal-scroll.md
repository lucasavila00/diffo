# ADR 0100: Scroll side-by-side code with one shared controller

## Problem

Side-by-side diffs were originally fitted to the viewport. Each code cell was
clipped before rendering, the projection reported no horizontal overflow, and
every horizontal input was clamped to zero. Long old or new lines could not be
reviewed.

## Decision

Apply visible-slice horizontal scrolling to side-by-side diffs:

- Measure the widest old or new code text in the visible vertical slice.
- Show one horizontal scrollbar when that width exceeds either code viewport.
- Pan both code columns by the same offset so paired text remains aligned.
- Keep both line-number gutters and the center divider fixed while code moves.
- Route arrows, trackpad events, scrollbar clicks, and drags through the
  existing horizontal viewport state.
- Hide the scrollbar and clamp the offset to zero when the visible slice fits.

Inline mode follows the same visible-slice contract. Measure only rows in the
vertical position committed for that frame, derive the maximum from them, and
hide and clamp the scrollbar atomically when they fit. The control may appear or
disappear during vertical scrolling because it describes only visible content.

One shared controller is intentional. Independent offsets would make paired
lines harder to compare and require extra state without improving access to
either side.
