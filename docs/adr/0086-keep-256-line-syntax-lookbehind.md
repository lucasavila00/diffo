# ADR 0086: Keep the 256-line syntax look-behind

Status: Rejected

Evaluates
[Reduce parser look-behind](../highlight-performance/06-smaller-lookbehind.md).

## Problem

An uncached deep jump parses 256 hidden lines before the visible viewport. This
work gives syntect enough earlier context for constructs such as multiline
comments, but it is a large part of deep-jump latency.

We tested reducing the look-behind from 256 lines to 64.

## Result

The smaller window was substantially faster:

- Rust deep window: about 11.6 ms to 4.4 ms, a 62% improvement.
- TypeScript deep window: about 46.7 ms to 20.9 ms, a 55% improvement.

It was not correct. A Rust block comment beginning 200 lines before the visible
line was rendered as ordinary code because syntect could no longer see the start
of the comment.

## Decision

Keep the 256-line look-behind. Speed is not a win when visible syntax colors are
wrong.

Keep the multiline-comment regression test so future changes must preserve the
same visible result as highlighting the complete document.
