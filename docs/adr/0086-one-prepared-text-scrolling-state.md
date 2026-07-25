# ADR 0086: Use one prepared scrolling state for text viewers

Status: Accepted

## Context

Diff and Explorer use the shared scroll math in `diffo-ui`, but they separately owned the
decision to expose a vertical target before its syntax coverage was ready. They also selected
their syntax windows separately. Those copies diverged: one direction or one viewer could move
onto a gutter-only syntax skeleton while another kept full text visible.

Direction-neutral window sizes are insufficient when the viewport commit policy is duplicated.
The same input, readiness, and fallback state must have one implementation for every
syntax-backed text viewer.

## Decision

`diffo-ui::text_view` owns prepared vertical scrolling for Diff, Explorer, and their full-screen
modes.

- One prepared-scroll state resolves line, page, wheel, scrollbar, and absolute targets.
- Repeated input accumulates from the latest requested target. Reversing direction updates that
  target immediately.
- The requested target remains separate from the committed viewport.
- A ready target commits during frame preparation. A cold target requests syntax work while the
  previous full-text viewport remains committed.
- A document or projection replacement cancels its pending target.
- Rendering never uses a syntax skeleton as a scrolling fallback.
- One shared centered-window function places bounded syntax coverage. Equal movement magnitude
  selects equal odd window sizes in either direction.

Diff and Explorer continue to own document-specific syntax readiness, worker requests, stale
result rejection, and rendering. They may not replace the shared target state, window placement,
or atomic commit policy.

This realizes the shared text-surface ownership in ADR 0043, extends the common scroll core in
ADR 0050, and supersedes ADR 0044's allowance for interim scrolling skeletons. ADR 0066 retains
the direction-neutral prefetch sizes and is generalized by this decision to every text viewer.

## Consequences

A cold scroll may briefly keep the previous viewport stationary, but it never flashes an empty
or partially styled target. Warm scrolling remains immediate. Diff and Explorer have the same
target accumulation, reversal, bounds, prefetch placement, and readiness behavior, so a future
directional change cannot be made in only one viewer.

## Tests

- Shared state tests cover bounds, repeated input, reversal, readiness, centered windows, and
  equal upward and downward prefetch sizing.
- Diff and Explorer state tests request cold targets on both sides of a committed viewport and
  require full text until the target commits.
- Frame-traced PTY tests cross cold coverage in both directions in both viewers and reject every
  scroll-input frame that exposes a syntax skeleton or moves before readiness.
