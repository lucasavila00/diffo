# ADR 0108: Overlay change warnings and scrollbars

Status: Accepted

Refines [ADR 0022](0022-large-hunk-navigation-targets.md),
[ADR 0025](0025-visible-slice-horizontal-scrollbar.md),
[ADR 0074](0074-static-footer-branch-info.md),
[ADR 0079](0079-color-change-navigation-by-target.md),
[ADR 0100](0100-side-by-side-horizontal-scroll.md), and
[ADR 0103](0103-stable-diff-navigation-rails.md).

## Context

The previous and next change controls permanently take two diff rows. The horizontal
scrollbar takes another row when it appears, shifting the viewport.

## Decision

Render previous and next change labels as non-clickable warnings over the first and
last diff rows. When a warning is absent, that row shows diff text. Keep `p` and `n`
as the actions. Keep the semantic destination background and use regular-weight text;
ADR 0074 reserves bold for mouse targets. Place a later navigation target one row
below a visible top warning.

Draw Diff and Explorer horizontal scrollbars over the bottom pane border when visible
text overflows. The track remains draggable but never consumes a content row.

Full-screen views keep their existing control-free behavior.

## Consequences

Warnings may temporarily cover edge text, but no control permanently reduces the
viewport. Warning clicks do nothing. Scrollbar and warning visibility cannot move
content, vertical rails, or markers.

## Verification

- Warning visibility keeps content and rail geometry fixed.
- Warning clicks are inert, labels are not bold, and `p` and `n` still navigate.
- Inline Diff, side-by-side Diff, and Explorer keep the same viewport when the
  horizontal scrollbar appears.
- `make all` passes.
