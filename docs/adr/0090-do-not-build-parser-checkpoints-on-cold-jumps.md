# ADR 0090: Do not build parser checkpoints on cold jumps

Status: Rejected

Evaluates
[Cache parser-state checkpoints](../highlight-performance/04-parser-state-checkpoints.md).

## Problem

A saved syntect parser state could let highlighting resume near a deep viewport.
However, a cold file open has no saved state at that position.

## Result

The design spike found two ways to create checkpoints, neither of which measures or
improves the cold path we care about:

1. Parsing from the start of the file creates an accurate checkpoint, but puts
   full-file syntax work back on the opening path.
2. Reusing checkpoints from an earlier request helps only warmed revisits. Diffo
   already caches prepared highlighted windows for those revisits.

Letting Criterion repeat one document until checkpoints are warm would report a
cache-hit improvement while hiding the first uncached jump.

## Decision

Do not add parser checkpoints to `diffo-highlight`. Keep cold syntax work bounded by
the viewport, fixed look-behind, and byte budget.

Reconsider this only with a benchmark that separately reports checkpoint creation
and a product design that builds checkpoints outside the cold opening path.
