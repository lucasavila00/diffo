# ADR 0045: Measure and reduce skeleton frames

## Problem

Skeleton frames keep scrolling responsive, but appear too often.

Likely causes:

- syntax coverage is one window; useful old windows are replaced;
- fixed prefetch does not follow scroll direction or speed;
- Diff repeats parse and projection work for syntax-only misses;
- Explorer repeats file-window work;
- superseded worker jobs still consume time;
- text readiness and syntax readiness are not measured separately.

We are guessing. Measure first.

## Decision

Extend `DIFFO_TRACE_FRAMES`. For every text surface frame record:

- surface and document revision;
- viewport and requested range;
- render mode: full, syntax skeleton, or text skeleton;
- coverage before and after;
- request id, queue wait, worker time, and install time;
- parsed, projected, highlighted, and rendered lines and bytes;
- cache hit, coalesced request, and stale discarded result.

Report per fixed PTY workload:

- skeleton frames / total frames;
- skeleton episodes, p50/p95 duration, and longest episode;
- coverage misses / viewport changes;
- work completed but discarded;
- input-to-full-content latency.

Add `make measure-text-readiness`. Use release Diffo, a 100x30 PTY, fixed Diff
and Explorer fixtures, and fixed slow wheel, fast wheel, page, scrollbar-drag,
and hunk jump workloads. Print raw counts and medians. Do not add a flaky CI
timing limit.

Use these measurements diagnostically when changing the prepared text pipeline.
ADR 0086 owns its current centered windows, retained coverage, coalescing, and
no-skeleton commit policy. Keep memory, parser look-behind, byte budgets, the
10,000-line boundary, and render work bounded. Do not highlight full files or
block the input or render loop.
