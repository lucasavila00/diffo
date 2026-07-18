# ADR 0024: Atomic diff buffer transitions

Status: Accepted

## Problem

Opening a full-file diff requires parsing, projection, change-target discovery, and
sometimes syntax highlighting in a background worker. The selected file used to
change before that work finished. The old buffer could then be rendered with the new
selection's scroll state, and a worker result could be installed during either frame
preparation or rendering.

That created a partial open. Depending on when the result arrived, the view first
showed row zero and later jumped to the first change, or missed the jump entirely.
Results for files that were no longer selected could also supply scroll bounds and
change-navigation targets.

## Decision

Treat the rendered diff buffer and its viewport as one committed unit.

This is an architectural invariant, not a best-effort rendering optimization. There
must never be a frame containing new-buffer content with an old or provisional
viewport, nor a frame containing an uncommitted buffer's navigation targets or scroll
metrics.

- Keep the last committed buffer visible while a replacement is prepared. On startup,
  keep the pane empty until the first buffer is ready.
- Identify requests by path, staged or unstaged area, patch contents, and conflict
  mode. A path alone is not a buffer identity.
- Drain worker results and commit buffers only during frame preparation. Rendering
  reads committed state and never polls or installs preparation results.
- Commit the buffer, projections, change targets, scroll bounds, and initial viewport
  before one draw. A newly opened file starts at its first change and horizontal row
  zero.
- Preserve a visible-row anchor when refreshed content belongs to the same exact file
  area. A staged-to-unstaged switch is a new open, not a refresh.
- Ignore stale results. When selections change faster than preparation completes, the
  newest requested buffer is the only result that may be committed.
- While a replacement is pending, scrolling and change navigation continue to use the
  displayed buffer. They cannot read targets or bounds from an uncommitted buffer.

The application model remains the owner of numeric scroll state. Frame preparation
returns a complete vertical and horizontal viewport transition, and the main loop
applies and clamps it before drawing.

## Consequences

Large buffers can still take time to prepare, but users never see a half-open buffer
or a delayed first-change jump. The file-list selection may lead the diff pane while
preparation runs; the pane itself remains internally consistent.

Frame traces record requested and displayed identities plus the viewport transition.
A developer-only delay hook makes the asynchronous boundary deterministic in PTY
tests. This decision refines the background preparation in ADR 0009 and the frame
transaction in ADR 0014.

## Tests

- A replacement stays invisible until its first-change position is ready.
- The buffer and viewport change in the same traced frame.
- Rendering cannot install a worker result.
- Staged and unstaged buffers for one path have distinct identities.
- Stale and out-of-order results never become displayed buffers.
- Same-buffer refresh preserves the visible row.
- Any future change to preparation or rendering retains a delayed PTY test that proves
  the displayed identity and viewport transition change in the same frame.
