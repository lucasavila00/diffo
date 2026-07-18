# ADR 0044: Show skeleton frames while syntax catches up

Status: Accepted

## Problem

An uncached vertical scroll waits for syntax highlighting. Diffo keeps the old
viewport visible. Fast scrolling looks frozen and drops visual feedback.

## Options

- Keep the old viewport. Rejected. Correct, but feels broken.
- Show plain text, then add color. Rejected. Content flashes between two visual
  states and syntax is no longer atomic.
- Highlight synchronously. Rejected. Blocks input and rendering.
- Show a lightweight skeleton at the requested viewport. Chosen.

## Decision

Scrolling inside the committed document moves immediately, even when target syntax
is not ready.

During that gap, render:

- the correct line numbers for the requested viewport;
- the correct vertical and horizontal scrollbars;
- the normal border, title, and status; and
- no text content, syntax color, diff background, or change markers.

The blank content area is intentional. Do not render stale rows. Do not render plain
rows and color them later.

Use the committed document's row mapping and metrics. Clamp the requested viewport
immediately. Scrollbar position and line numbers must match the requested position,
not the last syntax-ready position.

Request syntax for the newest viewport. Older scroll requests may finish, but cannot
replace the skeleton or move the viewport. When the newest target is ready, replace
the skeleton with full content at the same viewport in one frame.

Rapid scrolling may show many skeleton frames. Each frame must remain cheap: no
projection, syntax parsing, full-document width scan, or background-result install
during rendering.

This rule applies to Diff and Explorer through `diffo-text-view`. Arrow, wheel, page,
scrollbar click, and scrollbar drag all use it.

File opens, file replacements, and Diff projection-mode changes stay atomic. Keep
the previous committed document visible until the new document, initial viewport,
targets, metrics, and visible syntax are ready together. Skeletons are only for
movement inside one committed document identity.

This ADR replaces the uncached-scroll rule in ADR 0024 and ADR 0032. Their document
commit and stale-result rules remain.

## Tests

- A delayed uncached scroll commits the new viewport immediately.
- Its interim frame contains correct line numbers and scrollbars, but no text or
  change markers.
- Rapid input follows the newest requested viewport without waiting for workers.
- Stale syntax results never replace the newest skeleton.
- Full content appears at the same viewport when syntax becomes ready.
- File opens and projection-mode changes never use a skeleton from another document.
- Diff and Explorer pass the same skeleton-frame contract tests.
