# ADR 0066: Keep syntax prefetch direction-neutral

## Context

Diff syntax windows are bounded so opening and scrolling remain responsive over
SSH. The existing window selection is directionally biased in two ways:

- every window starts at the requested viewport and extends only toward later
  rows;
- forward movement receives six or twelve viewports of coverage, while backward
  movement receives four.

Opening a file at its first change therefore prepares rows below the initial
viewport but not rows above it. A user who checks earlier context immediately
encounters syntax-skeleton frames, while the same movement toward later context
remains covered. Retained coverage makes revisits cheap, but it cannot help the
first upward visit.

Scroll direction is not evidence of future intent. In particular, the
first-change viewport is an anchor, not a prediction that the user will read
toward the end of the file.

## Decision

Never give earlier or later text rows preferential syntax preparation or
viewport behavior.

- Center every cold syntax window on the requested viewport.
- Use an odd total window size so the visible viewport has the same number of
  prefetched viewports before and after it.
- Preserve the total bounded window size when the centered window reaches the
  start or end of the projection. A document boundary may make the available
  content asymmetric, but the implementation must not introduce that asymmetry.
- Choose the window size from absolute movement magnitude, not movement sign:
  three viewports for an open or stationary request, seven for continuous line
  or wheel scrolling, and thirteen for page-sized or larger movement.
- Apply the same placement and sizing to inline and side-by-side projections and
  to initial opens, uncached scrolling, and discrete navigation preparation.
- Treat every vertical scroll target as an atomic prepared transition. If its
  visible syntax is not ready, keep the current viewport and its full text
  visible while preparing the target, then commit the target position and
  syntax-ready content in one frame.
- Accumulate repeated scroll input against the latest requested target while
  preparation is pending. Reversing direction changes that target immediately
  and follows the same readiness rules.
- Never render gutter-only or otherwise empty-looking syntax skeleton rows
  because a user scrolled beyond retained coverage. Earlier and later cold
  targets have the same fallback: the last committed viewport.

This supersedes the direction-specific prefetch sizes in ADR 0046. Its
retained-window, useful-stale-result, and worker-coalescing decisions remain in
force. It also supersedes ADR 0044's allowance for lightweight interim rendering
during scrolling; all vertical position changes now follow its atomic
position-and-content rule. [ADR 0086](0086-one-prepared-text-scrolling-state.md)
makes this one shared implementation for Diff, Explorer, and their full-screen
modes.

The strict 10,000-line syntax boundary, 256-line parser look-behind, 512 KiB
per-side byte budget, eight-window retained coverage, worker coalescing, and
syntax-skeleton fallback remain unchanged. These constants remain fixed product
behavior.

## Consequences

An initial open does the same amount of syntax work as before but divides its
spare coverage equally around the committed viewport. Continuous and page-sized
forward windows grow by one viewport so they can be centered exactly; backward
windows grow to the same bound. Work remains limited by the existing per-side
byte budget.

Direction changes no longer cause avoidable cold misses. Retained windows still
make previously visited regions warm. On an unavoidable first-visit miss, the
viewport waits on prepared syntax instead of exposing an empty-looking frame,
regardless of direction.
