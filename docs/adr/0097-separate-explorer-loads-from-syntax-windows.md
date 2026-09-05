# ADR 0097: Separate Explorer loads from syntax windows

## Problem

Explorer used one job for two things:

- load a file from Git;
- highlight a new scroll window.

A cold scroll loaded the whole file again. That read could also trigger a
filesystem access event. Explorer treated the event as a file change and loaded
the file again. The new load then made the pending syntax result stale.

Diff and Explorer also had separate code for syntax coverage and cache eviction.

## Decision

- A file load reads filesystem content and Git patch metadata through the
  repository source, then creates a document ID and immutable line buffer.
- A syntax-window job uses that ID and buffer. It does not read Git or rebuild
  the viewer.
- Results from an old document ID are ignored.
- Access-only filesystem events are ignored. Real changes still refresh the
  file.
- `diffo-ui::text_view` owns prepared vertical scrolling, window sizing, syntax
  coverage, the eight-window limit, and style eviction.
- Explorer uses one shared `SyntaxCoverage`. Diff uses two, one per side.
- Explorer and Diff keep separate workers. Their I/O and line mapping are
  different.
- A result requests a redraw only when committed visible state changes.
- Existing syntax limits and the atomic viewport rule do not change.

This extends [ADR 0086](0086-one-prepared-text-scrolling-state.md).

## Result

Cold Explorer scrolling does bounded syntax work on text already in memory. It
does not reload the file or its Git metadata. Diff and Explorer use the same
scrolling and coverage rules.

Tests cover file-read counts, stale results, watcher events, shared coverage
behavior, and cold scrolling in both directions. The five-second PTY guard stays
unchanged. `make all` must pass.
