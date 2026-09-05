# ADR 0114: Make change-navigation overlays clickable links

Refines [ADR 0022](0022-large-hunk-navigation-targets.md),
[ADR 0038](0038-remove-button-hover-changes.md),
[ADR 0118](0118-use-terminal-defaults-for-ui-surfaces.md), and
[ADR 0108](0108-overlay-change-warnings-and-scrollbars.md).

## Context

The previous- and next-change labels communicate that changed content exists
outside the viewport, but ADR 0108 made them inert and regular-weight when they
moved from dedicated rows onto diff content. They look like navigation controls
and retain large, stable rectangles, yet pointer users must switch to the
keyboard to activate them.

Diffo already uses bold text as the fixed affordance for clickable text
controls. Reusing that affordance with an explicit light foreground makes the
labels legible on their dark diff backgrounds without hover state.

## Decision

Render each visible previous- or next-change overlay with the shared
mouse-target style: bold primary text over its existing semantic destination
background. Treat the complete rendered overlay rectangle as a left-click target
for the same atomic, non-wrapping change jump used by lowercase `p` or `n`.

Keep the overlays on the first and last diff rows, including the split layout
when only one row is available. Do not add hover styling, passive mouse-movement
handling, pointer-driven redraws, new shortcuts, or configuration.

## Alternatives

- Keep the labels inert. Rejected because their wording, arrows, and stable
  placement already present them as controls while withholding the expected
  pointer interaction.
- Underline the labels or add a new link color. Rejected because the shared bold
  mouse-target style already distinguishes actions from content and chrome.

## Consequences

Pointer and keyboard users can activate the same directional navigation from the
visible label. The bold text identifies the overlaid row as an action while its
existing background continues to preview the destination's diff meaning.

Clicks still use committed projection targets and preserve atomic viewport and
syntax transitions. SSH sessions incur no passive pointer traffic or cosmetic
redraws.
