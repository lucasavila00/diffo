# ADR 0103: Keep diff navigation rails stable

Refines [ADR 0022](0022-large-hunk-navigation-targets.md).

## Problem

The previous- and next-change buttons appear and disappear while the diff
scrolls. Their rows currently move the scrollbar and change-marker dots, which
is distracting and also moves their mouse targets.

The current `diffo-mock` fixture has no long file with two separated change
blocks, so it cannot reproduce the problem.

## Decision

Always reserve one row above and one row below the diff for the change buttons.
Draw a button only when its action is available; otherwise leave its row blank
and inert.

Keep the diff viewport, scrollbar, and marker rail fixed when button
availability changes. Reserve the horizontal scrollbar separately below the
bottom button. Apply the same rule to inline and side-by-side views.

Add one modified file to `crates/diffo-core/fixtures/repository-state.ron` with
a change near the start, enough short unchanged lines to overflow the viewport,
and a second change near the end. This must reproduce next-only, both, and
previous-only button states in `make diffo-mock` without mock-only application
behavior.

## Verification

- Rendering tests keep the viewport, scrollbar, marker rail, markers, and hit
  targets fixed across all three button states. Blank button rows are inert.
- Cover inline, side-by-side, and narrow layouts.
- A frame-traced PTY test scrolls through the mock case without sleeps or delay
  hooks.
- `make all` passes.
