# ADR 0046: Reuse bounded syntax windows and install useful stale work

Status: Accepted

## Context

ADR 0045 added text-readiness measurement and identified repeated skeleton frames while
scrolling. The first implementation still had three problems:

- Diff replaced its syntax coverage on every syntax-only result. Scrolling down and then
  back up therefore recomputed a previously prepared viewport.
- Diff accepted a syntax result only when its requested scroll offset exactly matched the
  newest offset. A result whose prefetched window already covered the newest viewport was
  discarded as stale.
- Explorer replaced its file coverage and allowed multiple queued viewport requests for the
  same file. Revisited viewports missed the cache and queued requests repeated file work.

The original readiness workload also used `DIFFO_MOCK_FILE` for Explorer. That repository is
intentionally snapshot-only and cannot return Explorer file contents, so it measured a
permanent text skeleton rather than scrolling readiness. Startup skeletons were also included
in the workload interval.

## Decision

Use a deterministic real Git fixture for text-readiness measurement. Each run creates the same
3,000-line Rust file, commits its baseline, changes every line, opens it in a release Diffo
inside a 100x30 PTY, waits for cold preparation, and marks the measured interval in the frame
trace before sending input.

Keep up to eight syntax coverage windows per document in both Diff and Explorer. Merge adjacent
or overlapping windows and prepared spans. When the bound is exceeded, evict the oldest window
and its spans together. Do not treat gaps between disjoint windows as covered.

For Diff:

- prefetch three viewports without movement, six for continuous forward scrolling, twelve for
  page-sized forward movement, and four when moving backward into an uncached region;
- queue viewport requests without blocking the input loop, and have the worker drain queued
  requests to the newest viewport before starting work;
- install every completed syntax result for the current document, even when its original scroll
  target is no longer current. Navigation still commits only after the current target is covered.

For Explorer:

- merge same-document highlighted spans and coverage instead of replacing them;
- coalesce queued file requests to the newest viewport;
- preserve the existing worker-side stale request check.

The parser look-behind, per-side highlight byte budget, strict 10,000-line eligibility boundary,
atomic document commit, and skeleton fallback remain unchanged.

## Measurement

Representative release runs of `make measure-text-readiness` on the fixed 100x30 PTY fixture:

| Surface and workload | Before | After |
| --- | ---: | ---: |
| Diff slow wheel | 0 skeleton frames | 0 skeleton frames |
| Diff fast wheel | 0 skeleton frames | 0 skeleton frames |
| Diff repeated page down | 37 frames, 525 ms episode | 7 frames, 97 ms episode |
| Diff scrollbar drag | 5 frames, 73 ms episode | 5 frames, 73 ms episode |
| Diff hunk jump | 0 skeleton frames | 0 skeleton frames |
| Explorer slow wheel | 0 skeleton frames | 0 skeleton frames |
| Explorer fast wheel | 0 skeleton frames | 0 skeleton frames |

Repeated page-down skeleton time in Diff fell by about 81%. Explorer page and scrollbar timing
varies with worker scheduling and has no timing assertion. Deterministic state-transition tests
instead verify that an earlier Explorer window remains ready after preparing a later window, that
returning upward submits no request, and that queued file requests retain only the newest
viewport.

## Consequences

Previously visited nearby regions render with syntax immediately in both surfaces. Same-document
work that is stale only by viewport can still reduce a later miss. Fast forward movement spends
more of the fixed highlight byte budget ahead of the viewport, while backward movement uses a
smaller window because retained coverage is expected to satisfy most revisits.

Memory remains bounded by eight coverage windows and their associated spans per committed
document. Diff still repeats parse and projection work inside a syntax-only worker request; that
is the next optimization if worker timing, rather than request selection, remains the dominant
cost in future measurements.
