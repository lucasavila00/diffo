# ADR 0066: Keep diff syntax prefetch direction-neutral

Status: Accepted

## Context

Diff syntax windows are bounded so opening and scrolling remain responsive over SSH. The
existing window selection is directionally biased in two ways:

- every window starts at the requested viewport and extends only toward later rows;
- forward movement receives six or twelve viewports of coverage, while backward movement
  receives four.

Opening a file at its first change therefore prepares rows below the initial viewport but not
rows above it. A user who checks earlier context immediately encounters syntax-skeleton frames,
while the same movement toward later context remains covered. Retained coverage makes revisits
cheap, but it cannot help the first upward visit.

Scroll direction is not evidence of future intent. In particular, the first-change viewport is
an anchor, not a prediction that the user will read toward the end of the file.

## Decision

Never give earlier or later diff rows preferential syntax preparation.

- Center every cold syntax window on the requested viewport.
- Use an odd total window size so the visible viewport has the same number of prefetched
  viewports before and after it.
- Preserve the total bounded window size when the centered window reaches the start or end of
  the projection. A document boundary may make the available content asymmetric, but the
  implementation must not introduce that asymmetry.
- Choose the window size from absolute movement magnitude, not movement sign: three viewports
  for an open or stationary request, seven for continuous line or wheel scrolling, and thirteen
  for page-sized or larger movement.
- Apply the same placement and sizing to inline and side-by-side projections and to initial
  opens, uncached scrolling, and discrete navigation preparation.

This supersedes the direction-specific prefetch sizes in ADR 0046. Its retained-window,
useful-stale-result, and worker-coalescing decisions remain in force.

The strict 10,000-line syntax boundary, 256-line parser look-behind, 512 KiB per-side byte
budget, eight-window retained coverage, worker coalescing, and syntax-skeleton fallback remain
unchanged. These constants remain fixed product behavior.

## Required fixes

1. Replace target-to-end projection ranges with viewport-centered, boundary-clamped ranges.
2. Replace the backward special case with absolute-distance window sizing and odd window sizes.
3. Add deterministic tests for centered initial coverage and equal sizing in both directions.
4. Add a delayed PTY regression proving that paging either way immediately after a middle-file
   open uses prepared syntax coverage.
5. Retire the empty `scrolling-up-has-no-pre-cache` todo when these changes land.

## Consequences

An initial open does the same amount of syntax work as before but divides its spare coverage
equally around the committed viewport. Continuous and page-sized forward windows grow by one
viewport so they can be centered exactly; backward windows grow to the same bound. Work remains
limited by the existing per-side byte budget.

Direction changes no longer cause avoidable cold misses. Retained windows still make previously
visited regions warm, but correctness and first-visit readiness do not depend on which direction
was visited first.

## Tests

- A renderer test checks that initial first-change coverage includes source rows above the
  committed viewport.
- A sizing test pairs equal upward and downward distances and requires equal odd window sizes.
- A real-Git PTY test delays background diff preparation, opens a Rust change in the middle of a
  file, pages up and down, and requires syntax-ready input frames in both directions.
