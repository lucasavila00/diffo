# ADR 0096: Skip unrequested highlight sides

Status: Accepted

## Problem

An added or deleted file can request highlighting for only one side. The highlighter
still gathered both complete side lists and entered the two-thread path.

The original benchmark suite always requested both sides, so it could not measure
this cost.

## Result

The harness now includes one-sided Rust top and deep windows. The engine skips line
collection for a missing range and highlights a lone side directly. It keeps the
parallel path when both sides are requested.

Results were:

- One-sided top window: 1.59 ms to 1.27 ms, 18.9% faster.
- One-sided deep window: 10.63 ms to 9.07 ms, 14.7% faster.
- Existing two-sided Rust windows: no statistically significant change.

## Decision

Keep the one-sided path and its benchmarks. Do not start a worker or gather lines
for a side that cannot produce output.
