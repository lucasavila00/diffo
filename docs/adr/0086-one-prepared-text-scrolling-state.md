# ADR 0086: Use one prepared scrolling state for text viewers

## Context

Diff and Explorer use the shared scroll math in `diffo-ui`, but they separately
owned the decision to expose a vertical target before its syntax coverage was
ready. They also selected their syntax windows separately. Those copies
diverged: one direction or one viewer could move onto a gutter-only syntax
skeleton while another kept full text visible.

Direction-neutral window sizes are insufficient when the viewport commit policy
is duplicated. The same input, readiness, and fallback state must have one
implementation for every syntax-backed text viewer.

## Decision

`diffo-ui::text_view` owns prepared vertical scrolling for Diff, Explorer, and
their full-screen modes.

- One prepared-scroll state resolves line, page, wheel, scrollbar, and absolute
  targets.
- Repeated input accumulates from the latest requested target. Reversing
  direction updates that target immediately.
- The requested target remains separate from the committed viewport.
- A ready target commits during frame preparation. A cold target requests syntax
  work while the previous full-text viewport remains committed.
- A document or projection replacement cancels its pending target.
- Rendering never uses a syntax skeleton as a scrolling fallback.
- One shared centered-window function places bounded syntax coverage. Equal
  movement magnitude selects equal odd window sizes in either direction: three
  viewports for nearby work, seven for medium jumps, and thirteen for far jumps.
- One shared `SyntaxCoverage` abstraction owns coverage readiness,
  adjacent-window merging, the eight-window bound, and matching style eviction
  for every syntax-backed text surface.

Syntax work retains a fixed 256-line parser look-behind and 512 KiB per-side
budget, and stops before a 10,000-line file. Coverage merges useful results from
the same document even when their original target became stale, coalesces worker
requests, and evicts spans with their matching window. The shared text component
owns viewport math and pure committed rendering; activities own document I/O and
mapping.

Diff and Explorer continue to own document-specific syntax readiness, worker
requests, stale result rejection, and rendering. Their document models may use
one coverage instance for plain text or one per diff side, but they may not
reimplement the shared target state, coverage cache, window placement, or atomic
commit policy.

This extends the common scroll core in ADR 0050 and applies the same prepared
surface contract to every text viewer.

## Consequences

A cold scroll may briefly keep the previous viewport stationary, but it never
flashes an empty or partially styled target. Warm scrolling remains immediate.
Diff and Explorer have the same target accumulation, reversal, bounds, prefetch
placement, and readiness behavior, so a future directional change cannot be made
in only one viewer.
