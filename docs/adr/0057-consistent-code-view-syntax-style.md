# ADR 0057: Use one syntax style across code views

Refines [ADR 0008](0008-diff-colors.md) and
[ADR 0035](0035-explorer-file-view.md).

## Context

Diff and Explorer both use `diffo-highlight` with the bundled syntax definitions
and the Monokai Extended theme. They do not render the resulting styles in the
same way. Diff keeps only each token's foreground color, as required by
ADR 0008. Explorer's regular file viewer also applies the theme's bold, italic,
and underline attributes. In some terminals those attributes make Rust keywords
appear to have a background, so the same source looks different when switching
between the regular and diff views.

The syntax theme describes code, not the surface displaying it. A
renderer-specific interpretation lets the two views drift even though they share
a highlighter.

## Decision

Use one foreground-only syntax style contract for every code view.

- `diffo-highlight` remains the single owner of the bundled syntax definitions
  and Monokai Extended token foreground colors.
- Code renderers use only those token foreground colors. They do not apply
  syntax- theme backgrounds, bold, italic, underline, or other terminal
  modifiers.
- An unchanged Rust line has the same token foregrounds and plain background in
  the regular Explorer viewer and on a context row in Diff.
- Diff may add its existing backgrounds for added, removed, and conflict rows
  after applying the shared syntax style. Its existing contrast correction
  remains local to those semantic backgrounds.
- Explorer continues to show changes in its separate gutter. It does not add a
  code- row background.

Enforce the foreground-only rule at the shared syntax-style boundary rather than
maintaining separate lists of attributes to discard in Diff and Explorer. Keep
the theme fixed in code; do not add theme or terminal-specific configuration.

## Consequences

Switching between regular and diff views preserves the code palette and removes
the keyword background artifact. Terminal font attributes cannot make syntax
categories look different between the views.

Diff rows still communicate additions, removals, and conflicts with their
accepted background colors. Explorer's gutter remains the only change-color
surface beside regular file contents.

The foreground-only contract intentionally gives up font emphasis supplied by
the upstream theme. Diffo already makes this tradeoff in Diff to keep rendering
stable across local terminals, multiplexers, and SSH sessions.
