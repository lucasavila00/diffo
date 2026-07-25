# ADR 0089: Keep simple diff-side collection

Status: Rejected

Evaluates
[Avoid collecting complete diff sides](../highlight-performance/03-avoid-full-side-collection.md).

## Problem

Before parsing, the highlighter gathers old and new diff lines into temporary
lists. Those lists include lines outside the requested viewport.

We tried filtering the lists to the viewport and its look-behind while walking the
diff.

## Result

Rust top and deep windows had no statistically significant change. TypeScript's top
window became about 20% slower, which exceeded the allowed 5% regression.

The change reduced temporary list entries, but syntect parsing remained the dominant
cost. Extra range checks in the traversal did not pay for themselves.

## Decision

Keep the simple full-side collection. Revisit traversal only if a profile shows it
becoming important after parser costs are reduced.
