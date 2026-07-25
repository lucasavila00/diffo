# ADR 0095: Stop highlighting after the requested range

Status: Accepted

## Problem

Diff lines are ordered by their old and new line numbers. The highlighting loop
still checked every later line after it had passed the requested range.

## Result

The loop now stops at the first line after the range. Syntax tests and the multiline
look-behind regression test remain unchanged.

Top-window results were:

- Rust: 10.7% faster.
- TypeScript: 9.0% faster.
- Markdown: 7.5% faster.
- JSON: no statistically significant change.

Rust deep-window highlighting also improved by 7.1%. The other deep cases had no
statistically significant change.

The experiment did not fully meet its original target because JSON did not improve
by 5%. It did improve three top cases and one deep case without a measured
regression.

## Decision

Keep the early break. It is a small change that follows from ordered line numbers
and avoids work that cannot affect the result.
