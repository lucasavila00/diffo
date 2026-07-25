# ADR 0088: Keep parallel old and new highlighting

Status: Rejected

Evaluates
[Remove per-call thread creation](../highlight-performance/02-remove-per-call-thread-spawn.md).

## Problem

Each request starts two scoped threads so the old and new sides can be highlighted
at the same time. We wanted to know whether creating those threads cost more than it
saved for a small viewport.

## Result

We changed the request to highlight both sides one after the other. Every top-window
benchmark became slower:

- Rust regressed by about 40%.
- TypeScript regressed by about 55%.
- JSON regressed by about 67%.
- Markdown regressed by about 59%.

Parsing even 40 lines on both sides costs enough for the parallel work to repay the
thread setup.

## Decision

Keep the two scoped threads. Do not replace them with sequential highlighting.

A persistent worker pool remains a separate possible experiment, but it must beat
the current parallel implementation rather than the rejected sequential version.
