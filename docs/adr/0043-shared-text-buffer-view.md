# ADR 0043: One text-buffer surface

## Problem

Diff and Explorer both have a right-side text buffer. They implement it
separately. The UX already differs. More copies will drift more.

## Decision

Create a `diffo-text-view` crate. Diff, Explorer, and future text-buffer
surfaces must use it. No private replacement implementations.

The crate owns the whole read-only text surface:

- viewport and scroll state;
- scroll commands and key/mouse mapping;
- line, wheel, and page scroll speeds;
- vertical and horizontal bounds and clamping;
- visible-row horizontal overflow;
- scrollbar drawing, clicking, and dragging;
- fixed gutters and marker rails;
- text clipping and terminal-safe rendering;
- empty, loading, and error presentation; and
- atomic document, syntax coverage, viewport, and metrics commits.

This guarantees the same UX everywhere. The same command moves the same
distance. Wheel, arrows, page keys, scrollbar clicks, and drags behave the same.
Bounds, gutter behavior, loading behavior, and scroll speed cannot vary by
activity.

The crate accepts prepared, display-ready rows with styled text and optional
gutter or marker cells. It does not know about Git, paths, hunks, staged state,
or diff row kinds.

Diff keeps patch parsing, inline and side-by-side projection, hunk targets, and
diff styles. Explorer keeps file loading and change-marker projection. Both
convert their result into the shared prepared document.

`diffo-highlight` owns syntax detection, parsing, look-behind, styled spans,
plain-text fallback, the 10,000-line boundary, and byte budgets. Diff and
Explorer own background requests and stale-result rejection. `diffo-text-view`
only stores syntax coverage and renders prepared spans. It never performs
highlighting.

Rendering reads committed state only. A new document, its viewport, bounds,
targets, and visible syntax commit together during frame preparation. Until
then, keep the old document visible.

## Crate boundaries

`diffo-text-view` depends on `diffo-ui`, `diffo-highlight`, and Ratatui. It must
not depend on `diffo-app`, `diffo-diff`, `diffo-explorer`, `diffo-tui`, Git,
Crossterm, or the binary.

Move shared viewport, scrolling, scrollbar, gutter, and text rendering code out
of `diffo-tui` and `diffo-explorer`. Delete the old implementations after
migration.
