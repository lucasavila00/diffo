# ADR 0044: Render ready position changes in one frame

## Problem

Clicking a change marker or another in-document target can cause two renders:

1. move the viewport;
2. render its content.

This is needless when the committed buffer already has the target content.

## Decision

Position and content readiness are separate.

For a discrete position change inside the current document:

- clamp the target against committed metrics;
- if the target is ready, commit the viewport and full content in the next
  frame;
- otherwise prepare the target while keeping the current viewport visible, then
  commit the viewport and full content together.

This applies to change-marker clicks, hunk buttons, keyboard navigation, and
other absolute in-document jumps. Never show a blank target followed by its
content.

Scrolling is different. It needs continuous position feedback and may use its
lightweight interim rendering. A jump target is preparation metadata, not a
second viewport position.

File changes and projection-mode changes still commit atomically because they
replace the document, not just its position.

## Tests

- Clicking a ready change marker draws the target content in one frame.
- Ready keyboard and button jumps use the same path.
- Ready jumps schedule no syntax or projection work.
- An unready jump keeps the old viewport until one atomic target commit.
