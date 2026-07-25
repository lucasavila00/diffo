# ADR 0097: Keep syntect's line convenience API

Status: Rejected

Evaluates
[Do not allocate spans for hidden look-behind lines](../highlight-performance/12-do-not-allocate-hidden-spans.md).

## Problem

`HighlightLines::highlight_line` returns a vector of styled spans for every line.
Diffo discards those vectors for hidden look-behind lines and copies visible spans
into its own result.

We replaced the convenience call with syntect's parser and highlight iterators. The
experiment advanced identical state but did not collect hidden spans or build an
intermediate visible vector.

## Result

Syntax tests and snapshots remained unchanged. Performance did not:

- Rust deep window: no statistically significant change.
- TypeScript deep window: no statistically significant change.

The result missed the required 10% improvement. Regular-expression parsing
dominates the cost of these lines.

## Decision

Keep `HighlightLines::highlight_line`. Its simpler state handling is preferable when
the lower-level API does not produce a measurable gain.
