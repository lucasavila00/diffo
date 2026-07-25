# ADR 0091: Keep owned strings in syntax spans

Status: Rejected

Evaluates
[Reduce styled-span allocation](../highlight-performance/05-reduce-span-allocation.md).

## Problem

Each highlighted token is copied into an owned `String`. We tested whether using a
smaller `Box<str>` value would reduce enough allocation and bookkeeping work to
matter.

## Result

The 9,999-line Rust benchmark changed from about 365.6 ms to 370.2 ms. Criterion
reported no statistically significant difference.

A boxed string makes the span record smaller, but it still allocates and copies each
token. Syntect parsing remains the dominant cost.

## Decision

Keep `String` in `StyledSpan`. It is familiar to callers and the alternative did not
meet the 15% improvement target.

A range-based representation would require the highlighted result to retain or own
the original source text. Treat that as a separate design change rather than another
container substitution.
